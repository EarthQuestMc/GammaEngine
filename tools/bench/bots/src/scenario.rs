//! Scenario files (`bench/scenarios/*.toml`): what the orchestrator plays. Every key has a
//! default; an unknown key or table is an error, so a typo never runs a different scenario
//! than the one written.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::args::{self, Args};
use crate::behaviour::Behaviour;
use crate::toml::{self, Document, Entry, Value};

/// Settings the orchestrator writes itself: the server listens on this machine only, in offline
/// mode, on the scenario's port.
pub const FORCED_PROPERTIES: &[&str] = &["server-ip", "online-mode", "server-port"];

const TABLES: &[&str] = &[
    "",
    "server",
    "server.properties",
    "bots",
    "measure",
    "console",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gc {
    /// The JVM's own choice: Parallel on Java 8 server machines, G1 from Java 9.
    Default,
    G1,
    Zgc,
}

impl Gc {
    pub fn name(self) -> &'static str {
        match self {
            Gc::Default => "",
            Gc::G1 => "g1",
            Gc::Zgc => "zgc",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ServerSpec {
    /// Server folder, relative to the repository root unless absolute; emptied before each run.
    pub dir: PathBuf,
    /// Jar copied over the one setup takes from `build/distributions`.
    pub jar: Option<PathBuf>,
    pub java: String,
    /// `-Xms` and `-Xmx`.
    pub heap: String,
    pub gc: Gc,
    pub gc_log: bool,
    pub jvm_args: Vec<String>,
    pub port: u16,
    /// Starting world, a folder with `level.dat`, copied as `world/` before each start.
    pub world: Option<PathBuf>,
    /// Expected fingerprint of `world` (see `sha256::dir_hex`); the run refuses another world.
    pub world_sha256: Option<String>,
    pub startup_timeout: Duration,
    pub stop_timeout: Duration,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Scenario {
    pub name: String,
    pub description: String,
    pub repetitions: u32,
    pub server: ServerSpec,
    /// `server.properties` keys written over the test settings, in file order.
    pub properties: Vec<(String, String)>,
    /// The bots' settings; `count` may be 0 for a run without players.
    pub bots: Args,
    pub warmup: Duration,
    pub measure: Duration,
    pub attribution: bool,
    /// Console commands sent once the server is up, before the bots arrive.
    pub at_start: Vec<String>,
    /// Console commands sent once every bot started, before the warm-up.
    pub after_arrival: Vec<String>,
}

impl Scenario {
    /// Reads a scenario file, with `table.key=value` overrides applied on top.
    pub fn load(path: &Path, overrides: &[String]) -> Result<Scenario, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        Scenario::parse(&text, &stem, overrides).map_err(|e| format!("{}: {e}", path.display()))
    }

    pub fn parse(text: &str, default_name: &str, overrides: &[String]) -> Result<Scenario, String> {
        let mut doc = Document::parse(text)?;
        for o in overrides {
            apply_override(&mut doc, o)?;
        }
        if let Some(table) = doc.tables.iter().find(|t| !TABLES.contains(&t.as_str())) {
            return Err(format!(
                "unknown table [{table}] (known: [server], [server.properties], [bots], \
                 [measure], [console])"
            ));
        }
        let mut r = Reader { doc };

        let name = r.string("", "name", default_name)?;
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Err(format!(
                "name '{name}': use letters, digits, '-' and '_' only (it names the results)"
            ));
        }
        let description = r.string("", "description", "")?;
        let repetitions = r.integer("", "repetitions", 1, 1, 100)? as u32;

        let s = "server";
        let heap = r.string(s, "heap", "1G")?;
        let heap_ok = heap
            .strip_suffix(['k', 'K', 'm', 'M', 'g', 'G'])
            .unwrap_or(&heap)
            .parse::<u32>()
            .is_ok_and(|n| n > 0);
        if !heap_ok {
            return Err(format!(
                "[server] heap '{heap}': expected a size such as 1G or 2048M"
            ));
        }
        let gc = match r.string(s, "gc", "")?.to_ascii_lowercase().as_str() {
            "" | "default" => Gc::Default,
            "g1" => Gc::G1,
            "zgc" => Gc::Zgc,
            other => {
                return Err(format!(
                    "[server] gc '{other}': expected \"\", \"g1\" or \"zgc\""
                ))
            }
        };
        let jvm_args = r.strings(s, "jvm_args")?;
        if let Some(bad) = jvm_args
            .iter()
            .find(|a| a.starts_with("-Xmx") || a.starts_with("-Xms"))
        {
            return Err(format!("[server] jvm_args: '{bad}' clashes with heap"));
        }
        let server = ServerSpec {
            dir: PathBuf::from(r.string(s, "dir", "test-server-orch")?),
            jar: r.path(s, "jar")?,
            java: r.string(s, "java", "java")?,
            heap,
            gc,
            gc_log: r.boolean(s, "gc_log", false)?,
            jvm_args,
            port: r.integer(s, "port", 25570, 1, 65535)? as u16,
            world: r.path(s, "world")?,
            world_sha256: Some(r.string(s, "world_sha256", "")?.to_ascii_lowercase())
                .filter(|h| !h.is_empty()),
            startup_timeout: r.seconds(s, "startup_timeout", 300.0)?,
            stop_timeout: r.seconds(s, "stop_timeout", 120.0)?,
        };
        if server.dir.as_os_str().is_empty() {
            return Err("[server] dir must not be empty".into());
        }
        if server.java.is_empty() {
            return Err("[server] java must not be empty".into());
        }

        let mut properties = Vec::new();
        for entry in r.doc.take_table("server.properties") {
            if FORCED_PROPERTIES.contains(&entry.key.as_str()) {
                return Err(format!(
                    "line {}: [server.properties] {} is set by the orchestrator \
                     (127.0.0.1, offline mode, [server] port)",
                    entry.line, entry.key
                ));
            }
            if matches!(entry.value, Value::Array(_)) {
                return Err(type_error(&entry, "a string, a number or a boolean"));
            }
            properties.push((entry.key, entry.value.to_string()));
        }

        let b = "bots";
        let behaviour_name = r.string(b, "behaviour", "idle")?;
        let behaviour = Behaviour::parse(&behaviour_name).ok_or_else(|| {
            format!("[bots] behaviour '{behaviour_name}': expected idle, wander or explore")
        })?;
        let defaults = Args::default();
        let bots = Args {
            host: "127.0.0.1".into(),
            port: server.port,
            count: r.integer(b, "count", 20, 0, 100_000)? as u32,
            prefix: r.string(b, "prefix", &defaults.prefix)?,
            first: r.integer(b, "first", 1, 0, i64::from(u32::MAX))? as u32,
            rate: r.float(b, "rate", defaults.rate)?,
            behaviour,
            seed: r.integer(b, "seed", 1, 0, i64::MAX)? as u64,
            wander_radius: r.float(b, "wander_radius", defaults.wander_radius)?,
            wander_height: r.float(b, "wander_height", defaults.wander_height)?,
            fly_height: r.float(b, "fly_height", defaults.fly_height)?,
            join_timeout: r.seconds(b, "join_timeout", defaults.join_timeout.as_secs_f64())?,
            attempts: r.integer(b, "attempts", i64::from(defaults.attempts), 1, 100)? as u32,
            // The server answered its console: a short retry covers a slow first status.
            wait: Duration::from_secs(30),
            ..defaults
        };

        let m = "measure";
        let warmup = r.seconds(m, "warmup", 30.0)?;
        let measure = r.seconds(m, "duration", 60.0)?;
        if measure.is_zero() {
            return Err("[measure] duration must be positive".into());
        }
        let attribution = r.boolean(m, "attribution", false)?;

        let at_start = r.strings("console", "at_start")?;
        let after_arrival = r.strings("console", "after_arrival")?;
        if let Some(bad) = at_start
            .iter()
            .chain(&after_arrival)
            .find(|c| c.contains(['\n', '\r']) || c.trim().is_empty())
        {
            return Err(format!("[console] invalid command {bad:?}"));
        }
        r.finish()?;

        let mut bots = bots;
        bots.duration = warmup + measure;
        if bots.count > 0 {
            args::validate(&bots).map_err(|e| format!("[bots] {e}"))?;
        }
        Ok(Scenario {
            name,
            description,
            repetitions,
            server,
            properties,
            bots,
            warmup,
            measure,
            attribution,
            at_start,
            after_arrival,
        })
    }
}

/// `table.key=value`, the table being everything before the last dot of the left side. A value
/// that is not valid TOML is taken as a string: `--set server.heap=2G`.
fn apply_override(doc: &mut Document, text: &str) -> Result<(), String> {
    let (path, raw) = text
        .split_once('=')
        .ok_or_else(|| format!("--set {text}: expected table.key=value"))?;
    let path = path.trim();
    let (table, key) = path.rsplit_once('.').unwrap_or(("", path));
    if key.is_empty() {
        return Err(format!("--set {text}: missing key"));
    }
    let value = toml::parse_value(raw).unwrap_or_else(|_| Value::String(raw.trim().to_owned()));
    doc.set(table, key, value);
    Ok(())
}

/// `line 4: [bots] count`, or `--set: [bots] count` for an override.
fn place(entry: &Entry) -> String {
    let table = if entry.table.is_empty() {
        String::new()
    } else {
        format!("[{}] ", entry.table)
    };
    let at = if entry.line > 0 {
        format!("line {}: ", entry.line)
    } else {
        "--set: ".into()
    };
    format!("{at}{table}{}", entry.key)
}

fn type_error(entry: &Entry, expected: &str) -> String {
    format!(
        "{}: expected {expected}, found {}",
        place(entry),
        entry.value.type_name()
    )
}

/// Takes typed values out of the document; what is left at the end is unknown.
struct Reader {
    doc: Document,
}

impl Reader {
    fn string(&mut self, table: &str, key: &str, default: &str) -> Result<String, String> {
        match self.doc.take(table, key) {
            None => Ok(default.to_owned()),
            Some(Entry {
                value: Value::String(s),
                ..
            }) => Ok(s),
            Some(entry) => Err(type_error(&entry, "a string")),
        }
    }

    /// An empty string means no path.
    fn path(&mut self, table: &str, key: &str) -> Result<Option<PathBuf>, String> {
        let text = self.string(table, key, "")?;
        Ok((!text.is_empty()).then(|| PathBuf::from(text)))
    }

    fn integer(
        &mut self,
        table: &str,
        key: &str,
        default: i64,
        min: i64,
        max: i64,
    ) -> Result<i64, String> {
        match self.doc.take(table, key) {
            None => Ok(default),
            Some(Entry {
                value: Value::Integer(n),
                ..
            }) if (min..=max).contains(&n) => Ok(n),
            Some(
                entry @ Entry {
                    value: Value::Integer(n),
                    ..
                },
            ) => Err(format!(
                "{}: {n} is out of range (from {min} to {max})",
                place(&entry)
            )),
            Some(entry) => Err(type_error(&entry, "an integer")),
        }
    }

    /// Integers are accepted where a float is expected.
    fn float(&mut self, table: &str, key: &str, default: f64) -> Result<f64, String> {
        match self.doc.take(table, key) {
            None => Ok(default),
            Some(Entry {
                value: Value::Float(x),
                ..
            }) => Ok(x),
            Some(Entry {
                value: Value::Integer(n),
                ..
            }) => Ok(n as f64),
            Some(entry) => Err(type_error(&entry, "a number")),
        }
    }

    fn seconds(&mut self, table: &str, key: &str, default: f64) -> Result<Duration, String> {
        let entry = self.doc.get(table, key).cloned();
        let secs = self.float(table, key, default)?;
        match entry {
            Some(entry) if !(0.0..=1e7).contains(&secs) => Err(format!(
                "{}: {secs} s is out of range (from 0 to 1e7)",
                place(&entry)
            )),
            _ => Ok(Duration::from_secs_f64(secs)),
        }
    }

    fn boolean(&mut self, table: &str, key: &str, default: bool) -> Result<bool, String> {
        match self.doc.take(table, key) {
            None => Ok(default),
            Some(Entry {
                value: Value::Boolean(b),
                ..
            }) => Ok(b),
            Some(entry) => Err(type_error(&entry, "a boolean")),
        }
    }

    fn strings(&mut self, table: &str, key: &str) -> Result<Vec<String>, String> {
        let Some(entry) = self.doc.take(table, key) else {
            return Ok(Vec::new());
        };
        let Value::Array(items) = &entry.value else {
            return Err(type_error(&entry, "an array of strings"));
        };
        items
            .iter()
            .map(|item| match item {
                Value::String(s) => Ok(s.clone()),
                _ => Err(type_error(&entry, "an array of strings")),
            })
            .collect()
    }

    fn finish(self) -> Result<(), String> {
        match self.doc.entries.first() {
            None => Ok(()),
            Some(entry) => {
                let table = if entry.table.is_empty() {
                    "at the top".to_owned()
                } else {
                    format!("in [{}]", entry.table)
                };
                let at = if entry.line > 0 {
                    format!("line {}", entry.line)
                } else {
                    "--set".into()
                };
                Err(format!("{at}: unknown key '{}' {table}", entry.key))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPAWN: &str = include_str!("../../../../bench/scenarios/spawn-groupe.toml");
    const DISPERSED: &str = include_str!("../../../../bench/scenarios/disperses.toml");
    const EXPLORATION: &str = include_str!("../../../../bench/scenarios/exploration.toml");

    #[test]
    fn committed_scenarios_parse() {
        let spawn = Scenario::parse(SPAWN, "x", &[]).unwrap();
        assert_eq!(spawn.name, "spawn-groupe");
        assert_eq!(spawn.bots.behaviour, Behaviour::Idle);
        assert_eq!(spawn.server.port, 25570);
        assert_eq!(spawn.server.dir, PathBuf::from("test-server-orch"));
        assert_eq!(spawn.server.jar, None);
        assert_eq!(spawn.server.world, None);
        assert_eq!(spawn.properties, [("view-distance".into(), "8".into())]);
        assert_eq!(spawn.at_start.len(), 3);

        let dispersed = Scenario::parse(DISPERSED, "x", &[]).unwrap();
        assert_eq!(dispersed.bots.behaviour, Behaviour::Wander);
        assert_eq!(dispersed.after_arrival.len(), 1);

        let exploration = Scenario::parse(EXPLORATION, "x", &[]).unwrap();
        assert_eq!(exploration.bots.behaviour, Behaviour::Explore);
        for s in [&spawn, &dispersed, &exploration] {
            assert!(s.repetitions >= 1);
            assert!(!s.measure.is_zero());
            assert_eq!(s.bots.host, "127.0.0.1");
        }
    }

    #[test]
    fn defaults_and_name_from_file() {
        let s = Scenario::parse("", "mon-scenario", &[]).unwrap();
        assert_eq!(s.name, "mon-scenario");
        assert_eq!(s.repetitions, 1);
        assert_eq!(s.server.heap, "1G");
        assert_eq!(s.server.gc, Gc::Default);
        assert_eq!(s.server.port, 25570);
        assert_eq!(s.server.startup_timeout, Duration::from_secs(300));
        assert_eq!(s.bots.count, 20);
        assert_eq!(s.bots.port, 25570);
        assert_eq!(s.warmup, Duration::from_secs(30));
        assert_eq!(s.measure, Duration::from_secs(60));
        assert!(!s.attribution);
        assert!(s.at_start.is_empty());
    }

    #[test]
    fn overrides() {
        let s = Scenario::parse(
            SPAWN,
            "x",
            &[
                "bots.count=50".into(),
                "server.heap=2G".into(),
                "server.gc=\"zgc\"".into(),
                "server.properties.view-distance=10".into(),
                "repetitions=3".into(),
                "measure.attribution=true".into(),
                "server.jvm_args=[\"-XX:+AlwaysPreTouch\"]".into(),
            ],
        )
        .unwrap();
        assert_eq!(s.bots.count, 50);
        assert_eq!(s.server.heap, "2G");
        assert_eq!(s.server.gc, Gc::Zgc);
        assert_eq!(s.properties, [("view-distance".into(), "10".into())]);
        assert_eq!(s.repetitions, 3);
        assert!(s.attribution);
        assert_eq!(s.server.jvm_args, ["-XX:+AlwaysPreTouch"]);

        let zero = Scenario::parse(SPAWN, "x", &["bots.count=0".into()]).unwrap();
        assert_eq!(zero.bots.count, 0);
        // An override of an unknown key is as wrong as the key in the file.
        let error = Scenario::parse(SPAWN, "x", &["bots.cuont=5".into()]).unwrap_err();
        assert!(
            error.contains("--set: unknown key 'cuont' in [bots]"),
            "{error}"
        );
        assert!(Scenario::parse(SPAWN, "x", &["nothing".into()]).is_err());
    }

    #[test]
    fn errors() {
        let cases = [
            (
                "[bots]\nbehavior = \"idle\"\n",
                "line 2: unknown key 'behavior' in [bots]",
            ),
            ("[bot]\n", "unknown table [bot]"),
            (
                "[bots]\ncount = \"20\"\n",
                "line 2: [bots] count: expected an integer, found a string",
            ),
            ("[bots]\ncount = -1\n", "from 0 to 100000"),
            ("[bots]\nbehaviour = \"fly\"\n", "behaviour 'fly'"),
            ("[bots]\nprefix = \"a-b\"\n", "[bots] --prefix"),
            ("[server]\nheap = \"lots\"\n", "heap 'lots'"),
            ("[server]\ngc = \"cms\"\n", "gc 'cms'"),
            ("[server]\njvm_args = [\"-Xmx4G\"]\n", "clashes with heap"),
            (
                "[server]\njvm_args = \"-Xss1m\"\n",
                "expected an array of strings",
            ),
            ("[server]\nport = 70000\n", "from 1 to 65535"),
            (
                "[server.properties]\nonline-mode = true\n",
                "set by the orchestrator",
            ),
            (
                "[server.properties]\nmotd = [\"a\"]\n",
                "expected a string, a number or a boolean",
            ),
            ("[measure]\nduration = 0\n", "duration must be positive"),
            ("[measure]\nwarmup = -5\n", "out of range"),
            ("[console]\nat_start = [\"\"]\n", "invalid command"),
            ("name = \"a b\"\n", "name 'a b'"),
            ("repetitions = 0\n", "from 1 to 100"),
        ];
        for (text, expected) in cases {
            let error = Scenario::parse(text, "x", &[]).expect_err(text);
            assert!(error.contains(expected), "{text:?}: {error}");
        }
    }
}
