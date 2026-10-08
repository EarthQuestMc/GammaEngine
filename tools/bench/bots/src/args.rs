//! Command line.

use std::path::PathBuf;
use std::time::Duration;

use gammaengine_protocol::packets::login::MAX_NAME_LEN;

use crate::behaviour::Behaviour;

pub const USAGE: &str = "\
gamma-bots: load bots for a Minecraft 1.7.10 Forge server (protocol 5, offline mode)

Usage: gamma-bots [options]

Options:
  --host <host>          Server address (default 127.0.0.1)
  --port <port>          Server port (default 25565)
  --count <n>            Number of bots (default 1)
  --prefix <text>        Name prefix; bots are named <prefix><n> (default bot)
  --first <n>            Number of the first bot (default 1)
  --rate <n>             Connections per second, 0 for all at once (default 5)
  --behaviour <name>     idle | wander | explore (default idle)
  --duration <s>         Run time in seconds, counted from the first connection (default 60)
  --report <file>        CSV file with one line per bot (default: none)
  --seed <n>             Seed of the wander paths and explore headings (default 1)
  --wander-radius <b>    Wander radius around the spawn, in blocks (default 16)
  --wander-height <b>    Wander height above the spawn, in blocks (default 0)
  --fly-height <b>       Explore flight height above the world spawn, in blocks (default 100)
  --join-timeout <s>     Give up a connection that has not joined after this long (default 30)
  --attempts <n>         Connections a bot may open before it joins (default 3)
  --trace <n>            Print the first n packets the first bot receives (default 0)
  --wait <s>             Keep retrying the status ping this long if the server is down (default 0)
  -h, --help             This help

Ctrl-C stops the bots cleanly and still writes the report.
";

#[derive(Debug, Clone, PartialEq)]
pub struct Args {
    pub host: String,
    pub port: u16,
    pub count: u32,
    pub prefix: String,
    pub first: u32,
    pub rate: f64,
    pub behaviour: Behaviour,
    pub duration: Duration,
    pub report: Option<PathBuf>,
    pub seed: u64,
    pub wander_radius: f64,
    pub wander_height: f64,
    pub fly_height: f64,
    pub join_timeout: Duration,
    pub attempts: u32,
    pub trace: u32,
    pub wait: Duration,
}

impl Default for Args {
    fn default() -> Self {
        Args {
            host: "127.0.0.1".into(),
            port: 25565,
            count: 1,
            prefix: "bot".into(),
            first: 1,
            rate: 5.0,
            behaviour: Behaviour::Idle,
            duration: Duration::from_secs(60),
            report: None,
            seed: 1,
            wander_radius: 16.0,
            wander_height: 0.0,
            fly_height: 100.0,
            join_timeout: Duration::from_secs(30),
            attempts: 3,
            trace: 0,
            wait: Duration::ZERO,
        }
    }
}

impl Args {
    pub fn name(&self, offset: u32) -> String {
        format!(
            "{}{}",
            self.prefix,
            u64::from(self.first) + u64::from(offset)
        )
    }
}

#[derive(Debug, PartialEq)]
pub enum Command {
    Run(Args),
    Help,
}

fn number<T: std::str::FromStr>(option: &str, value: &str) -> Result<T, String> {
    value
        .parse()
        .map_err(|_| format!("{option}: invalid value '{value}'"))
}

fn seconds(option: &str, value: &str) -> Result<Duration, String> {
    let secs: f64 = number(option, value)?;
    if !(0.0..=1e9).contains(&secs) {
        return Err(format!("{option}: out of range '{value}'"));
    }
    Ok(Duration::from_secs_f64(secs))
}

pub fn parse(raw: impl IntoIterator<Item = String>) -> Result<Command, String> {
    let mut args = Args::default();
    let mut raw = raw.into_iter();
    while let Some(arg) = raw.next() {
        if arg == "-h" || arg == "--help" {
            return Ok(Command::Help);
        }
        let (option, inline) = match arg.split_once('=') {
            Some((option, value)) if arg.starts_with("--") => {
                (option.to_owned(), Some(value.to_owned()))
            }
            _ => (arg.clone(), None),
        };
        let mut value = || -> Result<String, String> {
            inline
                .clone()
                .or_else(|| raw.next())
                .ok_or_else(|| format!("{option}: missing value"))
        };
        match option.as_str() {
            "--host" => args.host = value()?,
            "--port" => args.port = number(&option, &value()?)?,
            "--count" => args.count = number(&option, &value()?)?,
            "--prefix" => args.prefix = value()?,
            "--first" => args.first = number(&option, &value()?)?,
            "--rate" => args.rate = number(&option, &value()?)?,
            "--behaviour" | "--behavior" => {
                let name = value()?;
                args.behaviour = Behaviour::parse(&name).ok_or_else(|| {
                    format!("{option}: unknown behaviour '{name}' (idle, wander, explore)")
                })?;
            }
            "--duration" => args.duration = seconds(&option, &value()?)?,
            "--report" => args.report = Some(PathBuf::from(value()?)),
            "--seed" => args.seed = number(&option, &value()?)?,
            "--wander-radius" => args.wander_radius = number(&option, &value()?)?,
            "--wander-height" => args.wander_height = number(&option, &value()?)?,
            "--fly-height" => args.fly_height = number(&option, &value()?)?,
            "--join-timeout" => args.join_timeout = seconds(&option, &value()?)?,
            "--attempts" => args.attempts = number(&option, &value()?)?,
            "--trace" => args.trace = number(&option, &value()?)?,
            "--wait" => args.wait = seconds(&option, &value()?)?,
            _ => return Err(format!("unknown option '{arg}'")),
        }
    }
    validate(&args)?;
    Ok(Command::Run(args))
}

fn validate(args: &Args) -> Result<(), String> {
    if args.count == 0 {
        return Err("--count must be at least 1".into());
    }
    if args.prefix.is_empty()
        || !args
            .prefix
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Err("--prefix: use letters, digits and '_' only".into());
    }
    let longest = args.name(args.count - 1);
    if longest.len() > MAX_NAME_LEN {
        return Err(format!(
            "names are limited to {MAX_NAME_LEN} characters, '{longest}' is too long"
        ));
    }
    if !args.rate.is_finite() || args.rate < 0.0 {
        return Err("--rate must be zero or positive".into());
    }
    if args.duration.is_zero() {
        return Err("--duration must be positive".into());
    }
    if args.join_timeout.is_zero() {
        return Err("--join-timeout must be positive".into());
    }
    if args.attempts == 0 {
        return Err("--attempts must be at least 1".into());
    }
    if !(args.wander_radius.is_finite() && args.wander_radius > 0.0) {
        return Err("--wander-radius must be positive".into());
    }
    if !(args.wander_height.is_finite() && args.wander_height >= 0.0) {
        return Err("--wander-height must be zero or positive".into());
    }
    if !(args.fly_height.is_finite() && args.fly_height >= 0.0) {
        return Err("--fly-height must be zero or positive".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_str(line: &str) -> Result<Command, String> {
        parse(line.split_whitespace().map(String::from))
    }

    #[test]
    fn defaults() {
        assert_eq!(parse_str(""), Ok(Command::Run(Args::default())));
    }

    #[test]
    fn full_command_line() {
        let Ok(Command::Run(args)) = parse_str(
            "--host 10.0.0.2 --port 25570 --count 20 --prefix bot --rate 5 \
             --behaviour wander --duration 120 --report bots.csv --seed=9 --first 101",
        ) else {
            panic!("should parse");
        };
        assert_eq!(args.host, "10.0.0.2");
        assert_eq!(args.port, 25570);
        assert_eq!(args.count, 20);
        assert_eq!(args.behaviour, Behaviour::Wander);
        assert_eq!(args.duration, Duration::from_secs(120));
        assert_eq!(args.report, Some(PathBuf::from("bots.csv")));
        assert_eq!(args.seed, 9);
        assert_eq!(args.name(0), "bot101");
        assert_eq!(args.name(19), "bot120");
    }

    #[test]
    fn errors() {
        assert!(parse_str("--count 0").is_err());
        assert!(parse_str("--count").is_err());
        assert!(parse_str("--behaviour fly").is_err());
        assert!(parse_str("--prefix bad-name").is_err());
        assert!(parse_str("--prefix abcdefghijklmno --count 10").is_err());
        assert!(parse_str("--prefix abcdefghijklmno --count 9").is_ok());
        assert!(parse_str("--rate -1").is_err());
        assert!(parse_str("--port 70000").is_err());
        assert!(parse_str("--frobnicate 1").is_err());
        assert_eq!(parse_str("--host x -h"), Ok(Command::Help));
    }
}
