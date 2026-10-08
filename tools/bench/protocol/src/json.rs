//! A small JSON reader, enough for the status response and chat components.
//!
//! The server writes them with Gson, which escapes `<`, `>`, `=`, `&` and `'` as `\uXXXX`: the
//! escapes are decoded, surrogate pairs included.

use crate::{Error, Result};

const MAX_DEPTH: usize = 128;

#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Json>),
    /// Members in document order.
    Object(Vec<(String, Json)>),
}

impl Json {
    pub fn parse(text: &str) -> Result<Json> {
        let mut parser = Parser {
            bytes: text.as_bytes(),
            pos: 0,
        };
        parser.skip_whitespace();
        let value = parser.value(0)?;
        parser.skip_whitespace();
        if parser.pos != parser.bytes.len() {
            return Err(parser.error("trailing characters"));
        }
        Ok(value)
    }

    /// Member `key` of an object; the first one if the key is repeated.
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Object(members) => members.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Json::Number(n) => Some(*n),
            _ => None,
        }
    }

    /// The number if it is integral and fits an `i64`.
    pub fn as_i64(&self) -> Option<i64> {
        let n = self.as_f64()?;
        (n.fract() == 0.0 && n.abs() < 9.2e18).then_some(n as i64)
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Json::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Json]> {
        match self {
            Json::Array(items) => Some(items),
            _ => None,
        }
    }
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl Parser<'_> {
    fn error(&self, reason: &'static str) -> Error {
        Error::Json {
            offset: self.pos,
            reason,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }

    fn expect(&mut self, byte: u8, reason: &'static str) -> Result<()> {
        if self.peek() == Some(byte) {
            self.pos += 1;
            Ok(())
        } else {
            Err(self.error(reason))
        }
    }

    fn literal(&mut self, word: &[u8], value: Json) -> Result<Json> {
        if self.bytes[self.pos..].starts_with(word) {
            self.pos += word.len();
            Ok(value)
        } else {
            Err(self.error("unknown literal"))
        }
    }

    fn value(&mut self, depth: usize) -> Result<Json> {
        if depth > MAX_DEPTH {
            return Err(self.error("nesting too deep"));
        }
        match self.peek() {
            Some(b'{') => self.object(depth),
            Some(b'[') => self.array(depth),
            Some(b'"') => self.string().map(Json::String),
            Some(b't') => self.literal(b"true", Json::Bool(true)),
            Some(b'f') => self.literal(b"false", Json::Bool(false)),
            Some(b'n') => self.literal(b"null", Json::Null),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(_) => Err(self.error("unexpected character")),
            None => Err(self.error("unexpected end")),
        }
    }

    fn object(&mut self, depth: usize) -> Result<Json> {
        self.pos += 1;
        let mut members = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(Json::Object(members));
        }
        loop {
            self.skip_whitespace();
            if self.peek() != Some(b'"') {
                return Err(self.error("expected a member name"));
            }
            let key = self.string()?;
            self.skip_whitespace();
            self.expect(b':', "expected ':'")?;
            self.skip_whitespace();
            let value = self.value(depth + 1)?;
            members.push((key, value));
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(Json::Object(members));
                }
                _ => return Err(self.error("expected ',' or '}'")),
            }
        }
    }

    fn array(&mut self, depth: usize) -> Result<Json> {
        self.pos += 1;
        let mut items = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(Json::Array(items));
        }
        loop {
            self.skip_whitespace();
            items.push(self.value(depth + 1)?);
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b']') => {
                    self.pos += 1;
                    return Ok(Json::Array(items));
                }
                _ => return Err(self.error("expected ',' or ']'")),
            }
        }
    }

    fn number(&mut self) -> Result<Json> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        let digits = |p: &mut Self| {
            let from = p.pos;
            while matches!(p.peek(), Some(b'0'..=b'9')) {
                p.pos += 1;
            }
            p.pos > from
        };
        if !digits(self) {
            return Err(self.error("expected digits"));
        }
        if self.peek() == Some(b'.') {
            self.pos += 1;
            if !digits(self) {
                return Err(self.error("expected digits after '.'"));
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.pos += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            if !digits(self) {
                return Err(self.error("expected exponent digits"));
            }
        }
        // The slice is ASCII digits and signs, so both conversions succeed.
        let text = std::str::from_utf8(&self.bytes[start..self.pos]).unwrap_or("0");
        text.parse::<f64>()
            .map(Json::Number)
            .map_err(|_| self.error("invalid number"))
    }

    fn hex4(&mut self) -> Result<u32> {
        let digits = self
            .bytes
            .get(self.pos..self.pos + 4)
            .ok_or_else(|| self.error("short \\u escape"))?;
        let mut value = 0;
        for &d in digits {
            let nibble = (d as char)
                .to_digit(16)
                .ok_or_else(|| self.error("bad \\u escape"))?;
            value = value * 16 + nibble;
        }
        self.pos += 4;
        Ok(value)
    }

    fn string(&mut self) -> Result<String> {
        self.pos += 1;
        let mut out = String::new();
        loop {
            let run_start = self.pos;
            while !matches!(self.peek(), Some(b'"' | b'\\') | None) {
                if self.bytes[self.pos] < 0x20 {
                    return Err(self.error("control character in string"));
                }
                self.pos += 1;
            }
            // The input came from a `&str` and the run stops on ASCII, so it is valid UTF-8.
            out.push_str(std::str::from_utf8(&self.bytes[run_start..self.pos]).unwrap_or(""));
            match self.peek() {
                None => return Err(self.error("unterminated string")),
                Some(b'"') => {
                    self.pos += 1;
                    return Ok(out);
                }
                _ => {}
            }
            self.pos += 1;
            let escape = self
                .peek()
                .ok_or_else(|| self.error("unterminated escape"))?;
            self.pos += 1;
            match escape {
                b'"' => out.push('"'),
                b'\\' => out.push('\\'),
                b'/' => out.push('/'),
                b'b' => out.push('\u{8}'),
                b'f' => out.push('\u{c}'),
                b'n' => out.push('\n'),
                b'r' => out.push('\r'),
                b't' => out.push('\t'),
                b'u' => {
                    let unit = self.hex4()?;
                    let code = if (0xd800..0xdc00).contains(&unit)
                        && self.bytes[self.pos..].starts_with(b"\\u")
                    {
                        let save = self.pos;
                        self.pos += 2;
                        let low = self.hex4()?;
                        if (0xdc00..0xe000).contains(&low) {
                            0x10000 + ((unit - 0xd800) << 10) + (low - 0xdc00)
                        } else {
                            self.pos = save;
                            unit
                        }
                    } else {
                        unit
                    };
                    out.push(char::from_u32(code).unwrap_or('\u{fffd}'));
                }
                _ => return Err(self.error("unknown escape")),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalars() {
        assert_eq!(Json::parse("null"), Ok(Json::Null));
        assert_eq!(Json::parse(" true "), Ok(Json::Bool(true)));
        assert_eq!(Json::parse("false"), Ok(Json::Bool(false)));
        assert_eq!(Json::parse("-12.5e1"), Ok(Json::Number(-125.0)));
        assert_eq!(Json::parse("0").unwrap().as_i64(), Some(0));
        assert_eq!(Json::parse("5").unwrap().as_i64(), Some(5));
        assert_eq!(Json::parse("5.5").unwrap().as_i64(), None);
        assert_eq!(
            Json::parse("\"a\\\"b\\\\c\\/d\\n\"").unwrap().as_str(),
            Some("a\"b\\c/d\n")
        );
    }

    #[test]
    fn unicode_escapes() {
        // Gson escapes '=' and '<' by default.
        assert_eq!(
            Json::parse("\"a\\u003db\\u003c\"").unwrap().as_str(),
            Some("a=b<")
        );
        assert_eq!(
            Json::parse("\"\\ud83d\\ude00\"").unwrap().as_str(),
            Some("\u{1F600}")
        );
        assert_eq!(
            Json::parse("\"\\ud83dx\"").unwrap().as_str(),
            Some("\u{fffd}x")
        );
        assert_eq!(Json::parse("\"Été\"").unwrap().as_str(), Some("Été"));
    }

    #[test]
    fn nested_structures() {
        let doc = Json::parse(r#"{"a":[1,{"b":null},[]],"c":{},"a":2}"#).unwrap();
        let a = doc.get("a").unwrap().as_array().unwrap();
        assert_eq!(a.len(), 3);
        assert_eq!(a[1].get("b"), Some(&Json::Null));
        assert_eq!(doc.get("c"), Some(&Json::Object(vec![])));
        assert_eq!(doc.get("missing"), None);
        // First of repeated keys.
        assert_eq!(
            doc.get("a").and_then(Json::as_array).map(<[Json]>::len),
            Some(3)
        );
    }

    #[test]
    fn errors() {
        for bad in [
            "",
            "{",
            "[1,]",
            "{\"a\" 1}",
            "\"open",
            "tru",
            "01x",
            "1 2",
            "\"\\x\"",
            "{1:2}",
            "-",
        ] {
            assert!(Json::parse(bad).is_err(), "{bad:?} should fail");
        }
        let deep = "[".repeat(200) + &"]".repeat(200);
        assert!(Json::parse(&deep).is_err());
    }
}
