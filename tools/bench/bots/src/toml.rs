//! The TOML subset the scenario files use: tables (`[a]`, `[a.b]`), bare or quoted keys, basic and
//! literal strings, integers, floats, booleans and arrays (one or several lines). Anything else
//! (inline tables, arrays of tables, dates, multi-line strings, dotted keys) is an error rather
//! than a silent misreading.

use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Array(Vec<Value>),
}

impl Value {
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::String(_) => "a string",
            Value::Integer(_) => "an integer",
            Value::Float(_) => "a float",
            Value::Boolean(_) => "a boolean",
            Value::Array(_) => "an array",
        }
    }
}

impl fmt::Display for Value {
    /// The value as it would be written in a properties file: strings without quotes.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::String(s) => f.write_str(s),
            Value::Integer(n) => write!(f, "{n}"),
            Value::Float(x) => write!(f, "{x}"),
            Value::Boolean(b) => write!(f, "{b}"),
            Value::Array(items) => {
                let items: Vec<String> = items.iter().map(Value::to_string).collect();
                write!(f, "{}", items.join(","))
            }
        }
    }
}

/// One `key = value` line, with the table it belongs to (`""` before the first header).
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub table: String,
    pub key: String,
    pub value: Value,
    pub line: usize,
}

/// The entries in file order. Tables are only names: `[server.properties]` is the table
/// `server.properties`, unrelated to `server` as far as this reader is concerned.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Document {
    pub tables: Vec<String>,
    pub entries: Vec<Entry>,
}

impl Document {
    pub fn parse(text: &str) -> Result<Document, String> {
        let mut parser = Parser {
            bytes: text.as_bytes(),
            pos: 0,
            line: 1,
        };
        let mut doc = Document::default();
        let mut table = String::new();
        loop {
            parser.skip_blank_lines();
            let Some(c) = parser.peek() else {
                break;
            };
            if c == b'[' {
                table = parser.table_header()?;
                if doc.tables.contains(&table) {
                    return Err(parser.error(&format!("table [{table}] defined twice")));
                }
                doc.tables.push(table.clone());
            } else {
                let line = parser.line;
                let key = parser.key()?;
                parser.skip_spaces();
                if parser.peek() == Some(b'.') {
                    return Err(parser.error("dotted keys are not supported; use a [table]"));
                }
                parser.expect(b'=')?;
                parser.skip_spaces();
                let value = parser.value()?;
                if doc.get(&table, &key).is_some() {
                    return Err(format!("line {line}: key '{key}' defined twice"));
                }
                doc.entries.push(Entry {
                    table: table.clone(),
                    key,
                    value,
                    line,
                });
            }
            parser.end_of_line()?;
        }
        Ok(doc)
    }

    pub fn get(&self, table: &str, key: &str) -> Option<&Entry> {
        self.entries
            .iter()
            .find(|e| e.table == table && e.key == key)
    }

    /// Removes and returns an entry: what is left once a reader took what it knows is unknown.
    pub fn take(&mut self, table: &str, key: &str) -> Option<Entry> {
        let index = self
            .entries
            .iter()
            .position(|e| e.table == table && e.key == key)?;
        Some(self.entries.remove(index))
    }

    /// Removes and returns every entry of a table, in file order.
    pub fn take_table(&mut self, table: &str) -> Vec<Entry> {
        let (taken, kept) = std::mem::take(&mut self.entries)
            .into_iter()
            .partition(|e| e.table == table);
        self.entries = kept;
        taken
    }

    /// Sets a value, replacing the existing entry if there is one.
    pub fn set(&mut self, table: &str, key: &str, value: Value) {
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|e| e.table == table && e.key == key)
        {
            entry.value = value;
            return;
        }
        if !table.is_empty() && !self.tables.iter().any(|t| t == table) {
            self.tables.push(table.to_owned());
        }
        self.entries.push(Entry {
            table: table.to_owned(),
            key: key.to_owned(),
            value,
            line: 0,
        });
    }
}

/// Parses a lone value, as given on a command line (`--set bots.count=50`).
pub fn parse_value(text: &str) -> Result<Value, String> {
    let mut parser = Parser {
        bytes: text.as_bytes(),
        pos: 0,
        line: 1,
    };
    parser.skip_spaces();
    let value = parser.value()?;
    parser.skip_spaces();
    if parser.pos != parser.bytes.len() {
        return Err(parser.error("trailing characters after the value"));
    }
    Ok(value)
}

fn is_bare_key_byte(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c == b'-'
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
    line: usize,
}

impl Parser<'_> {
    fn error(&self, message: &str) -> String {
        format!("line {}: {message}", self.line)
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn starts_with(&self, text: &str) -> bool {
        self.bytes[self.pos..].starts_with(text.as_bytes())
    }

    fn expect(&mut self, c: u8) -> Result<(), String> {
        if self.peek() == Some(c) {
            self.pos += 1;
            Ok(())
        } else {
            Err(self.error(&format!("expected '{}'", c as char)))
        }
    }

    fn skip_spaces(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t')) {
            self.pos += 1;
        }
    }

    fn skip_comment(&mut self) {
        if self.peek() == Some(b'#') {
            while !matches!(self.peek(), None | Some(b'\n')) {
                self.pos += 1;
            }
        }
    }

    /// Consumes one line break (`\n` or `\r\n`); false if there is none here.
    fn newline(&mut self) -> bool {
        if self.starts_with("\r\n") {
            self.pos += 2;
        } else if self.peek() == Some(b'\n') {
            self.pos += 1;
        } else {
            return false;
        }
        self.line += 1;
        true
    }

    fn skip_blank_lines(&mut self) {
        loop {
            self.skip_spaces();
            self.skip_comment();
            if !self.newline() {
                return;
            }
        }
    }

    /// Spaces and comments, then a line break or the end of the text.
    fn end_of_line(&mut self) -> Result<(), String> {
        self.skip_spaces();
        self.skip_comment();
        if self.peek().is_none() || self.newline() {
            Ok(())
        } else {
            Err(self.error("expected the end of the line"))
        }
    }

    fn table_header(&mut self) -> Result<String, String> {
        self.expect(b'[')?;
        if self.peek() == Some(b'[') {
            return Err(self.error("arrays of tables ([[...]]) are not supported"));
        }
        let mut parts = Vec::new();
        loop {
            self.skip_spaces();
            parts.push(self.key()?);
            self.skip_spaces();
            match self.peek() {
                Some(b'.') => self.pos += 1,
                Some(b']') => {
                    self.pos += 1;
                    return Ok(parts.join("."));
                }
                _ => return Err(self.error("expected '.' or ']' in the table name")),
            }
        }
    }

    fn key(&mut self) -> Result<String, String> {
        match self.peek() {
            Some(b'"') => self.basic_string(),
            Some(b'\'') => self.literal_string(),
            Some(c) if is_bare_key_byte(c) => {
                let start = self.pos;
                while self.peek().is_some_and(is_bare_key_byte) {
                    self.pos += 1;
                }
                Ok(String::from_utf8_lossy(&self.bytes[start..self.pos]).into_owned())
            }
            _ => Err(self.error("expected a key")),
        }
    }

    fn value(&mut self) -> Result<Value, String> {
        match self.peek() {
            Some(b'"') => self.basic_string().map(Value::String),
            Some(b'\'') => self.literal_string().map(Value::String),
            Some(b'[') => self.array(),
            Some(b'{') => Err(self.error("inline tables are not supported")),
            Some(_) if self.starts_with("true") => {
                self.pos += 4;
                self.after_word(Value::Boolean(true))
            }
            Some(_) if self.starts_with("false") => {
                self.pos += 5;
                self.after_word(Value::Boolean(false))
            }
            Some(c) if c.is_ascii_digit() || c == b'+' || c == b'-' => self.number(),
            _ => Err(self.error("expected a value")),
        }
    }

    /// A keyword must not run into more letters (`truest`).
    fn after_word(&self, value: Value) -> Result<Value, String> {
        if self.peek().is_some_and(is_bare_key_byte) {
            Err(self.error("invalid value"))
        } else {
            Ok(value)
        }
    }

    fn number(&mut self) -> Result<Value, String> {
        let start = self.pos;
        while self
            .peek()
            .is_some_and(|c| c.is_ascii_alphanumeric() || matches!(c, b'+' | b'-' | b'.' | b'_'))
        {
            self.pos += 1;
        }
        let text = std::str::from_utf8(&self.bytes[start..self.pos]).unwrap_or("");
        let invalid = || self.error(&format!("invalid number '{text}'"));
        // Underscores only between digits, as in TOML.
        let bytes = text.as_bytes();
        for (i, &c) in bytes.iter().enumerate() {
            if c == b'_'
                && !(i > 0
                    && bytes[i - 1].is_ascii_digit()
                    && bytes.get(i + 1).is_some_and(u8::is_ascii_digit))
            {
                return Err(invalid());
            }
        }
        let clean = text.replace('_', "");
        let digits = clean.trim_start_matches(['+', '-']);
        if digits.is_empty() || !digits.as_bytes()[0].is_ascii_digit() {
            return Err(invalid());
        }
        let leading_zero =
            digits.len() > 1 && digits.starts_with('0') && digits.as_bytes()[1].is_ascii_digit();
        if leading_zero {
            return Err(self.error(&format!("leading zeros are not allowed in '{text}'")));
        }
        if digits.contains(['.', 'e', 'E']) {
            // TOML wants digits on both sides of the point.
            if let Some((whole, fraction)) = digits.split_once('.') {
                if whole.is_empty() || !fraction.starts_with(|c: char| c.is_ascii_digit()) {
                    return Err(invalid());
                }
            }
            clean
                .parse::<f64>()
                .ok()
                .filter(|x| x.is_finite())
                .map(Value::Float)
                .ok_or_else(invalid)
        } else {
            clean
                .parse::<i64>()
                .map(Value::Integer)
                .map_err(|_| invalid())
        }
    }

    fn basic_string(&mut self) -> Result<String, String> {
        if self.starts_with("\"\"\"") {
            return Err(self.error("multi-line strings are not supported"));
        }
        self.expect(b'"')?;
        let mut out = Vec::new();
        loop {
            let Some(c) = self.peek() else {
                return Err(self.error("unterminated string"));
            };
            self.pos += 1;
            match c {
                b'"' => break,
                b'\n' | b'\r' => return Err(self.error("unterminated string")),
                b'\\' => {
                    let Some(escape) = self.peek() else {
                        return Err(self.error("unterminated string"));
                    };
                    self.pos += 1;
                    let decoded = match escape {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'n' => '\n',
                        b't' => '\t',
                        b'r' => '\r',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'u' => self.unicode_escape(4)?,
                        b'U' => self.unicode_escape(8)?,
                        _ => {
                            return Err(
                                self.error(&format!("unknown escape '\\{}'", escape as char))
                            )
                        }
                    };
                    let mut buf = [0u8; 4];
                    out.extend_from_slice(decoded.encode_utf8(&mut buf).as_bytes());
                }
                _ => out.push(c),
            }
        }
        String::from_utf8(out).map_err(|_| self.error("invalid UTF-8 in a string"))
    }

    fn unicode_escape(&mut self, digits: usize) -> Result<char, String> {
        let hex = self
            .bytes
            .get(self.pos..self.pos + digits)
            .and_then(|h| std::str::from_utf8(h).ok())
            .ok_or_else(|| self.error("truncated unicode escape"))?;
        let code =
            u32::from_str_radix(hex, 16).map_err(|_| self.error("invalid unicode escape"))?;
        self.pos += digits;
        char::from_u32(code).ok_or_else(|| self.error("invalid unicode scalar value"))
    }

    fn literal_string(&mut self) -> Result<String, String> {
        if self.starts_with("'''") {
            return Err(self.error("multi-line strings are not supported"));
        }
        self.expect(b'\'')?;
        let start = self.pos;
        loop {
            match self.peek() {
                Some(b'\'') => break,
                None | Some(b'\n' | b'\r') => return Err(self.error("unterminated string")),
                Some(_) => self.pos += 1,
            }
        }
        let text = std::str::from_utf8(&self.bytes[start..self.pos])
            .map_err(|_| self.error("invalid UTF-8 in a string"))?
            .to_owned();
        self.pos += 1;
        Ok(text)
    }

    /// Spaces, comments and line breaks inside an array.
    fn skip_array_space(&mut self) {
        loop {
            self.skip_spaces();
            self.skip_comment();
            if !self.newline() {
                return;
            }
        }
    }

    fn array(&mut self) -> Result<Value, String> {
        self.expect(b'[')?;
        let mut items = Vec::new();
        loop {
            self.skip_array_space();
            if self.peek() == Some(b']') {
                self.pos += 1;
                return Ok(Value::Array(items));
            }
            items.push(self.value()?);
            self.skip_array_space();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b']') => {
                    self.pos += 1;
                    return Ok(Value::Array(items));
                }
                _ => return Err(self.error("expected ',' or ']' in the array")),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(doc: &Document, table: &str, key: &str) -> Value {
        doc.get(table, key)
            .unwrap_or_else(|| panic!("{table}.{key} missing"))
            .value
            .clone()
    }

    #[test]
    fn tables_keys_and_scalars() {
        let doc = Document::parse(
            "# comment\n\
             name = \"spawn-groupe\" # trailing\n\
             repetitions = 3\n\
             \n\
             [server]\n\
             heap = '2G'\n\
             gc_log = true\n\
             rate = 2.5\n\
             big = 1_000_000\n\
             neg = -12\n\
             exp = 1e3\n\
             [ server . properties ]\n\
             view-distance = 8\n\
             \"level.seed\" = \"x\"\r\n",
        )
        .unwrap();
        assert_eq!(doc.tables, ["server", "server.properties"]);
        assert_eq!(
            value(&doc, "", "name"),
            Value::String("spawn-groupe".into())
        );
        assert_eq!(value(&doc, "", "repetitions"), Value::Integer(3));
        assert_eq!(value(&doc, "server", "heap"), Value::String("2G".into()));
        assert_eq!(value(&doc, "server", "gc_log"), Value::Boolean(true));
        assert_eq!(value(&doc, "server", "rate"), Value::Float(2.5));
        assert_eq!(value(&doc, "server", "big"), Value::Integer(1_000_000));
        assert_eq!(value(&doc, "server", "neg"), Value::Integer(-12));
        assert_eq!(value(&doc, "server", "exp"), Value::Float(1000.0));
        assert_eq!(
            value(&doc, "server.properties", "view-distance"),
            Value::Integer(8)
        );
        assert_eq!(
            value(&doc, "server.properties", "level.seed"),
            Value::String("x".into())
        );
        assert_eq!(doc.get("server", "view-distance"), None);
        assert_eq!(doc.get("server", "heap").unwrap().line, 6);
    }

    #[test]
    fn strings_and_escapes() {
        let doc = Document::parse(
            r#"a = "tab\there \"quoted\" back\\slash \u00e9\U0001F600"
b = 'C:\Users\no\escapes'
c = "é direct"
"#,
        )
        .unwrap();
        assert_eq!(
            value(&doc, "", "a"),
            Value::String("tab\there \"quoted\" back\\slash é😀".into())
        );
        assert_eq!(
            value(&doc, "", "b"),
            Value::String(r"C:\Users\no\escapes".into())
        );
        assert_eq!(value(&doc, "", "c"), Value::String("é direct".into()));
    }

    #[test]
    fn arrays_over_several_lines() {
        let doc = Document::parse(
            "empty = []\n\
             one = [\"-Xss1m\"]\n\
             many = [\n  \"a\", # first\n  'b',\n\n  \"c\",\n]\n\
             numbers = [1, 2.5, true]\n\
             after = 1\n",
        )
        .unwrap();
        assert_eq!(value(&doc, "", "empty"), Value::Array(vec![]));
        assert_eq!(
            value(&doc, "", "one"),
            Value::Array(vec![Value::String("-Xss1m".into())])
        );
        assert_eq!(
            value(&doc, "", "many"),
            Value::Array(vec![
                Value::String("a".into()),
                Value::String("b".into()),
                Value::String("c".into()),
            ])
        );
        assert_eq!(
            value(&doc, "", "numbers"),
            Value::Array(vec![
                Value::Integer(1),
                Value::Float(2.5),
                Value::Boolean(true)
            ])
        );
        assert_eq!(doc.get("", "after").unwrap().line, 10);
    }

    #[test]
    fn errors_name_the_line() {
        let cases = [
            ("a = 1\na = 2\n", "line 2: key 'a' defined twice"),
            ("[t]\n[t]\n", "line 2: table [t] defined twice"),
            ("a = \"open\n", "line 1: unterminated string"),
            ("a = 1 2\n", "line 1: expected the end of the line"),
            ("a.b = 1\n", "dotted keys"),
            ("a = {x = 1}\n", "inline tables"),
            ("[[t]]\n", "arrays of tables"),
            ("a = \"\"\"x\"\"\"\n", "multi-line strings"),
            ("a = 012\n", "leading zeros"),
            ("a = 1__0\n", "invalid number"),
            ("a = _1\n", "expected a value"),
            ("a = .5\n", "expected a value"),
            ("a = 5.\n", "invalid number"),
            ("a = truest\n", "invalid value"),
            ("a = [1 2]\n", "expected ',' or ']'"),
            ("a = \"\\q\"\n", "unknown escape"),
            ("= 1\n", "expected a key"),
            ("a 1\n", "expected '='"),
            ("\n\nb = nope\n", "line 3: expected a value"),
            ("a = 99999999999999999999\n", "invalid number"),
        ];
        for (text, expected) in cases {
            let error = Document::parse(text).expect_err(text);
            assert!(error.contains(expected), "{text:?}: {error}");
        }
    }

    #[test]
    fn take_set_and_lone_values() {
        let mut doc = Document::parse("[bots]\ncount = 20\nrate = 5\n[x]\ny = 1\n").unwrap();
        doc.set("bots", "count", Value::Integer(50));
        doc.set("measure", "warmup", Value::Integer(10));
        assert_eq!(value(&doc, "bots", "count"), Value::Integer(50));
        assert!(doc.tables.contains(&"measure".to_string()));
        assert_eq!(doc.take("bots", "rate").unwrap().value, Value::Integer(5));
        assert_eq!(doc.take("bots", "rate"), None);
        let bots = doc.take_table("bots");
        assert_eq!(bots.len(), 1);
        assert_eq!(doc.entries.len(), 2);

        assert_eq!(parse_value("50"), Ok(Value::Integer(50)));
        assert_eq!(parse_value(" \"2G\" "), Ok(Value::String("2G".into())));
        assert_eq!(
            parse_value("[\"a\", \"b\"]"),
            Ok(Value::Array(vec![
                Value::String("a".into()),
                Value::String("b".into())
            ]))
        );
        assert!(parse_value("2G").is_err());
        assert!(parse_value("1 2").is_err());
    }

    #[test]
    fn display_for_properties() {
        assert_eq!(Value::String("x y".into()).to_string(), "x y");
        assert_eq!(Value::Integer(8).to_string(), "8");
        assert_eq!(Value::Boolean(false).to_string(), "false");
        assert_eq!(Value::Float(0.5).to_string(), "0.5");
    }
}
