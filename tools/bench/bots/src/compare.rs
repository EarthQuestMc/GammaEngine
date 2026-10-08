//! `gamma-bots compare <before> <after>`: the before and after table of two results, in Markdown,
//! ready for a task report (French labels, decimal comma). Each side is a result of `run` (all its
//! repetitions), one repetition, a bare server recording (`gammaengine/bench/<name>-<date>/`), or
//! several of them joined by commas. With more than one repetition a side shows the median and,
//! in brackets, the smallest and largest values.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use gammaengine_protocol::json::Json;

use crate::util;

const USAGE: &str = "\
Usage: gamma-bots compare <before> <after> [--output <file.md>]

<before> and <after> are each a result directory of `gamma-bots run`, one of its rep<n>
directories, or a server recording (gammaengine/bench/<name>-<date>), or several of them joined
by commas; repetitions are summed up by their median and range.
";

pub fn main(raw: impl Iterator<Item = String>) -> ExitCode {
    let mut sides = Vec::new();
    let mut output = None;
    let mut raw = raw.peekable();
    while let Some(arg) = raw.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            "--output" | "-o" => match raw.next() {
                Some(path) => output = Some(PathBuf::from(path)),
                None => return usage_error("--output: missing value"),
            },
            _ if arg.starts_with("--") => return usage_error(&format!("unknown option '{arg}'")),
            _ => sides.push(arg),
        }
    }
    if sides.len() != 2 {
        return usage_error("expected two results: <before> <after>");
    }
    let table = Side::load(&sides[0])
        .and_then(|before| Ok((before, Side::load(&sides[1])?)))
        .map(|(before, after)| markdown(&before, &after));
    match table {
        Ok(table) => {
            print!("{table}");
            if let Some(path) = output {
                if let Err(e) = fs::write(&path, &table) {
                    eprintln!("error: cannot write {}: {e}", path.display());
                    return ExitCode::from(2);
                }
            }
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::from(2)
        }
    }
}

fn usage_error(message: &str) -> ExitCode {
    eprintln!("error: {message}\n\n{USAGE}");
    ExitCode::from(2)
}

/// What one repetition measured. Missing values (older recordings, no bots) stay `None`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Sample {
    pub mspt_mean: Option<f64>,
    pub mspt_p50: Option<f64>,
    pub mspt_p95: Option<f64>,
    pub mspt_p99: Option<f64>,
    pub mspt_max: Option<f64>,
    pub tps: Option<f64>,
    pub ticks: Option<f64>,
    pub gc_pauses: Option<f64>,
    pub gc_pause_total_ms: Option<f64>,
    pub gc_pause_max_ms: Option<f64>,
    pub gc_cycles: Option<f64>,
    pub gc_cycle_total_ms: Option<f64>,
    pub cost_per_player_ms: Option<f64>,
    pub players_mean: Option<f64>,
    pub players_max: Option<f64>,
    pub bots_planned: Option<f64>,
    pub bots_at_end: Option<f64>,
    pub bots_kicked: Option<f64>,
    pub bots_failed: Option<f64>,
    pub attribution: Option<bool>,
    pub java_version: Option<String>,
    pub jvm_args: Option<String>,
    pub server_version: Option<String>,
}

fn number(json: &Json, path: &[&str]) -> Option<f64> {
    let mut value = json;
    for key in path {
        value = value.get(key)?;
    }
    value.as_f64()
}

impl Sample {
    pub fn from_summary(text: &str) -> Result<Sample, String> {
        let json = Json::parse(text).map_err(|e| format!("summary.json: {e}"))?;
        let n = |path: &[&str]| number(&json, path);
        Ok(Sample {
            mspt_mean: n(&["mspt", "mean"]),
            mspt_p50: n(&["mspt", "p50"]),
            mspt_p95: n(&["mspt", "p95"]),
            mspt_p99: n(&["mspt", "p99"]),
            mspt_max: n(&["mspt", "max"]),
            tps: n(&["tps"]),
            ticks: n(&["ticks"]),
            gc_pauses: n(&["gc", "count"]),
            gc_pause_total_ms: n(&["gc", "total_ms"]),
            gc_pause_max_ms: n(&["gc", "max_ms"]),
            gc_cycles: n(&["gc", "concurrent_cycles", "count"]),
            gc_cycle_total_ms: n(&["gc", "concurrent_cycles", "total_ms"]),
            cost_per_player_ms: n(&["cost_per_player_ms"]),
            players_mean: n(&["players", "mean"]),
            players_max: n(&["players", "max"]),
            attribution: json
                .get("attribution")
                .and_then(|a| a.get("enabled"))
                .and_then(Json::as_bool),
            java_version: json
                .get("java_version")
                .and_then(Json::as_str)
                .map(str::to_owned),
            jvm_args: json.get("jvm_args").and_then(Json::as_array).map(|args| {
                args.iter()
                    .filter_map(Json::as_str)
                    .collect::<Vec<_>>()
                    .join(" ")
            }),
            server_version: json
                .get("server_version")
                .and_then(Json::as_str)
                .map(str::to_owned),
            ..Sample::default()
        })
    }

    /// Bot counts from `bots.csv` (header of `report::CSV_HEADER`).
    pub fn add_bots(&mut self, csv: &str) {
        let mut lines = csv.lines();
        let Some(header) = lines.next() else {
            return;
        };
        let columns = util::csv_fields(header);
        let column = |name: &str| columns.iter().position(|c| c == name);
        let (Some(at_end), Some(kick), Some(error)) = (
            column("connected_at_end"),
            column("kick_reason"),
            column("error"),
        ) else {
            return;
        };
        let (mut planned, mut present, mut kicked, mut failed) = (0.0, 0.0, 0.0, 0.0);
        for line in lines.filter(|l| !l.is_empty()) {
            let fields = util::csv_fields(line);
            let field = |i: usize| fields.get(i).map_or("", String::as_str);
            planned += 1.0;
            if field(at_end) == "yes" {
                present += 1.0;
            } else if !field(kick).is_empty() {
                kicked += 1.0;
            } else if !field(error).is_empty() {
                failed += 1.0;
            }
        }
        self.bots_planned = Some(planned);
        self.bots_at_end = Some(present);
        self.bots_kicked = Some(kicked);
        self.bots_failed = Some(failed);
    }
}

/// One side of the comparison: its repetitions and what `run.json` says about them.
#[derive(Debug, Default)]
pub struct Side {
    pub label: String,
    pub samples: Vec<Sample>,
    pub run: Vec<Json>,
}

fn repetition_dirs(dir: &Path) -> Result<Vec<PathBuf>, String> {
    if dir.join("summary.json").is_file() {
        return Ok(vec![dir.to_path_buf()]);
    }
    let entries = fs::read_dir(dir).map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
    let mut reps: Vec<(u32, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let index = name.strip_prefix("rep")?.parse().ok()?;
            let path = e.path();
            path.join("summary.json").is_file().then_some((index, path))
        })
        .collect();
    reps.sort();
    if reps.is_empty() {
        return Err(format!(
            "{}: no summary.json, and no rep<n>/summary.json",
            dir.display()
        ));
    }
    Ok(reps.into_iter().map(|(_, path)| path).collect())
}

impl Side {
    pub fn load(spec: &str) -> Result<Side, String> {
        let mut side = Side {
            label: spec.to_owned(),
            ..Side::default()
        };
        for part in spec.split(',').filter(|p| !p.is_empty()) {
            let dir = Path::new(part);
            if let Ok(text) = fs::read_to_string(dir.join("run.json")) {
                side.run.push(
                    Json::parse(&text)
                        .map_err(|e| format!("{}: {e}", dir.join("run.json").display()))?,
                );
            }
            for rep in repetition_dirs(dir)? {
                let path = rep.join("summary.json");
                let text = fs::read_to_string(&path)
                    .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
                let mut sample =
                    Sample::from_summary(&text).map_err(|e| format!("{}: {e}", rep.display()))?;
                if let Ok(csv) = fs::read_to_string(rep.join("bots.csv")) {
                    sample.add_bots(&csv);
                }
                side.samples.push(sample);
            }
        }
        if side.samples.is_empty() {
            return Err(format!("{spec}: nothing to compare"));
        }
        Ok(side)
    }

    /// One `run.json` value, when every run of this side agrees on it.
    fn run_text(&self, path: &[&str]) -> Option<String> {
        let mut values = self.run.iter().map(|run| {
            let mut value = run;
            for key in path {
                value = value.get(key)?;
            }
            match value {
                Json::String(s) => Some(s.clone()),
                Json::Number(x) => Some(format_number(*x, if x.fract() == 0.0 { 0 } else { 2 })),
                Json::Bool(b) => Some(if *b { "oui" } else { "non" }.to_owned()),
                _ => None,
            }
        });
        let first = values.next()??;
        values
            .all(|v| v.as_deref() == Some(&first))
            .then_some(first)
    }

    fn sample_text(&self, get: impl Fn(&Sample) -> Option<String>) -> Option<String> {
        let first = get(&self.samples[0])?;
        self.samples
            .iter()
            .all(|s| get(s).as_deref() == Some(&first))
            .then_some(first)
    }
}

/// Median of the values, and their range; `None` when no repetition has the value.
pub fn median_range(values: &[f64]) -> Option<(f64, f64, f64)> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let mid = sorted.len() / 2;
    let median = if sorted.len() % 2 == 0 {
        (sorted[mid - 1] + sorted[mid]) / 2.0
    } else {
        sorted[mid]
    };
    Some((median, sorted[0], sorted[sorted.len() - 1]))
}

/// French number: decimal comma, a space every three digits from 10 000 up.
pub fn format_number(value: f64, decimals: usize) -> String {
    let text = format!("{:.*}", decimals, value.abs());
    let (whole, fraction) = text.split_once('.').unwrap_or((&text, ""));
    let mut grouped = String::new();
    if whole.len() > 4 {
        for (i, c) in whole.chars().enumerate() {
            if i > 0 && (whole.len() - i) % 3 == 0 {
                grouped.push(' ');
            }
            grouped.push(c);
        }
    } else {
        grouped.push_str(whole);
    }
    let negative = value < 0.0 && text.bytes().any(|b| b.is_ascii_digit() && b != b'0');
    let sign = if negative { "-" } else { "" };
    if fraction.is_empty() {
        format!("{sign}{grouped}")
    } else {
        format!("{sign}{grouped},{fraction}")
    }
}

/// Reads one line of the context table (scenario, commit, Java...) for a side.
type Describe = fn(&Side) -> Option<String>;

struct Metric {
    label: &'static str,
    decimals: usize,
    get: fn(&Sample) -> Option<f64>,
}

const METRICS: &[Metric] = &[
    Metric {
        label: "MSPT moyen (ms)",
        decimals: 2,
        get: |s| s.mspt_mean,
    },
    Metric {
        label: "MSPT p50 (ms)",
        decimals: 2,
        get: |s| s.mspt_p50,
    },
    Metric {
        label: "MSPT p95 (ms)",
        decimals: 2,
        get: |s| s.mspt_p95,
    },
    Metric {
        label: "MSPT p99 (ms)",
        decimals: 2,
        get: |s| s.mspt_p99,
    },
    Metric {
        label: "MSPT max (ms)",
        decimals: 1,
        get: |s| s.mspt_max,
    },
    Metric {
        label: "TPS",
        decimals: 2,
        get: |s| s.tps,
    },
    Metric {
        label: "Coût par joueur (ms)",
        decimals: 3,
        get: |s| s.cost_per_player_ms,
    },
    Metric {
        label: "Joueurs (moyenne)",
        decimals: 1,
        get: |s| s.players_mean,
    },
    Metric {
        label: "Joueurs (max)",
        decimals: 0,
        get: |s| s.players_max,
    },
    Metric {
        label: "Pauses GC",
        decimals: 0,
        get: |s| s.gc_pauses,
    },
    Metric {
        label: "Pauses GC, total (ms)",
        decimals: 0,
        get: |s| s.gc_pause_total_ms,
    },
    Metric {
        label: "Pauses GC, max (ms)",
        decimals: 0,
        get: |s| s.gc_pause_max_ms,
    },
    Metric {
        label: "Cycles GC concurrents",
        decimals: 0,
        get: |s| s.gc_cycles,
    },
    Metric {
        label: "Cycles GC concurrents, total (ms)",
        decimals: 0,
        get: |s| s.gc_cycle_total_ms,
    },
    Metric {
        label: "Ticks mesurés",
        decimals: 0,
        get: |s| s.ticks,
    },
    Metric {
        label: "Bots prévus",
        decimals: 0,
        get: |s| s.bots_planned,
    },
    Metric {
        label: "Bots en jeu à la fin",
        decimals: 0,
        get: |s| s.bots_at_end,
    },
    Metric {
        label: "Bots expulsés",
        decimals: 0,
        get: |s| s.bots_kicked,
    },
    Metric {
        label: "Bots en échec",
        decimals: 0,
        get: |s| s.bots_failed,
    },
];

/// Counts are whole, but the median of an even number of repetitions may fall in between.
fn shown(value: f64, decimals: usize) -> String {
    let decimals = if decimals == 0 && value.fract() != 0.0 {
        1
    } else {
        decimals
    };
    format_number(value, decimals)
}

/// `+` before a positive value that does not round to zero.
fn signed(value: f64, decimals: usize) -> String {
    let text = shown(value, decimals);
    if value > 0.0 && text.bytes().any(|b| (b'1'..=b'9').contains(&b)) {
        format!("+{text}")
    } else {
        text
    }
}

fn cell(values: &[f64], decimals: usize) -> String {
    match median_range(values) {
        None => "n/d".into(),
        Some((median, low, high)) if values.len() > 1 => format!(
            "{} ({} – {})",
            shown(median, decimals),
            shown(low, decimals),
            shown(high, decimals)
        ),
        Some((median, _, _)) => shown(median, decimals),
    }
}

fn values(side: &Side, get: fn(&Sample) -> Option<f64>) -> Vec<f64> {
    side.samples.iter().filter_map(get).collect()
}

pub fn markdown(before: &Side, after: &Side) -> String {
    let mut out = String::new();
    let count = |side: &Side| match side.samples.len() {
        1 => "1 répétition".to_owned(),
        n => format!("{n} répétitions, médiane (min – max)"),
    };
    let _ = writeln!(out, "Avant : `{}` ({})  ", before.label, count(before));
    let _ = writeln!(out, "Après : `{}` ({})\n", after.label, count(after));

    // What was played, so that a reader sees at once whether the two sides compare.
    let context: [(&str, Describe); 9] = [
        ("Scénario", |s| s.run_text(&["scenario"])),
        ("Commit", |s| s.run_text(&["git_commit"])),
        ("Jar", |s| s.run_text(&["server_jar"])),
        ("Java", |s| s.sample_text(|x| x.java_version.clone())),
        ("Arguments JVM", |s| s.sample_text(|x| x.jvm_args.clone())),
        ("Bots", |s| s.run_text(&["bots", "count"])),
        ("Comportement", |s| s.run_text(&["bots", "behaviour"])),
        ("Mesure (s)", |s| s.run_text(&["measure_s"])),
        ("Attribution (niveau 2)", |s| {
            s.sample_text(|x| x.attribution.map(|a| if a { "oui" } else { "non" }.into()))
        }),
    ];
    let _ = writeln!(out, "| | Avant | Après |");
    let _ = writeln!(out, "| --- | --- | --- |");
    for (label, get) in context {
        let (b, a) = (get(before), get(after));
        if b.is_none() && a.is_none() {
            continue;
        }
        let show = |v: Option<String>| v.map_or("n/d".into(), |v| format!("`{v}`"));
        let mark = if b != a { " ≠" } else { "" };
        let _ = writeln!(out, "| {label}{mark} | {} | {} |", show(b), show(a));
    }
    out.push('\n');

    let _ = writeln!(out, "| Mesure | Avant | Après | Écart | Écart (%) |");
    let _ = writeln!(out, "| --- | ---: | ---: | ---: | ---: |");
    for metric in METRICS {
        let (b, a) = (values(before, metric.get), values(after, metric.get));
        if b.is_empty() && a.is_empty() {
            continue;
        }
        let (delta, percent) = match (median_range(&b), median_range(&a)) {
            (Some((mb, _, _)), Some((ma, _, _))) => {
                let d = ma - mb;
                let percent = if mb != 0.0 {
                    format!("{} %", signed(d / mb.abs() * 100.0, 1))
                } else if d == 0.0 {
                    format!("{} %", format_number(0.0, 1))
                } else {
                    "n/d".into()
                };
                (signed(d, metric.decimals), percent)
            }
            _ => ("n/d".into(), "n/d".into()),
        };
        let _ = writeln!(
            out,
            "| {} | {} | {} | {delta} | {percent} |",
            metric.label,
            cell(&b, metric.decimals),
            cell(&a, metric.decimals)
        );
    }
    let attribution = |s: &Side| s.sample_text(|x| x.attribution.map(|a| a.to_string()));
    if attribution(before) != attribution(after) {
        out.push_str(
            "\nAttention : l'attribution (niveau 2) n'est pas la même des deux côtés ; son coût \
             fausse la comparaison du MSPT.\n",
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SUMMARY: &str = r#"{
  "name": "spawn-groupe-rep1",
  "server_version": "1.7.10-dev-e0e28627 (MC: 1.7.10)",
  "java_version": "1.8.0_491",
  "jvm_args": ["-Xms1G", "-Xmx1G"],
  "duration_s": 60.0100,
  "ticks": 1200,
  "tps": 19.9967,
  "mspt": {"mean": 4.0000, "p50": 3.5000, "p95": 8.0000, "p99": 12.0000, "max": 80.5000},
  "players": {"mean": 20.0000, "max": 20},
  "cost_per_player_ms": 0.2000,
  "budget_per_player_ms": 2.5000,
  "attribution": {"enabled": false},
  "gc": {"count": 4, "total_ms": 120, "max_ms": 45, "concurrent_cycles": {"count": 0, "total_ms": 0, "max_ms": 0}}
}"#;

    #[test]
    fn summary_fields() {
        let s = Sample::from_summary(SUMMARY).unwrap();
        assert_eq!(s.mspt_mean, Some(4.0));
        assert_eq!(s.mspt_max, Some(80.5));
        assert_eq!(s.tps, Some(19.9967));
        assert_eq!(s.gc_pauses, Some(4.0));
        assert_eq!(s.gc_pause_max_ms, Some(45.0));
        assert_eq!(s.gc_cycles, Some(0.0));
        assert_eq!(s.cost_per_player_ms, Some(0.2));
        assert_eq!(s.players_max, Some(20.0));
        assert_eq!(s.attribution, Some(false));
        assert_eq!(s.jvm_args.as_deref(), Some("-Xms1G -Xmx1G"));
        // An older recording without attribution or concurrent cycles, and with no MSPT.
        let old = Sample::from_summary(
            r#"{"tps": 20.0, "mspt": null, "gc": {"count": 1, "total_ms": 5, "max_ms": 5}}"#,
        )
        .unwrap();
        assert_eq!(old.mspt_mean, None);
        assert_eq!(old.gc_cycles, None);
        assert_eq!(old.attribution, None);
        assert!(Sample::from_summary("{").is_err());
    }

    #[test]
    fn bot_counts() {
        let mut s = Sample::default();
        s.add_bots(&format!(
            "{}\n\
             bot1,yes,1,120,,59.0,yes,,,,30,2000,2100,1,2,1,0,0,1.00,64.00,1.00\n\
             bot2,yes,1,120,,10.0,no,\"Kicked, flying\",,,5,2000,2100,1,2,1,0,0,,,\n\
             bot3,no,3,,,0.0,no,,connect: refused,,0,,,0,0,0,0,0,,,\n",
            crate::report::CSV_HEADER
        ));
        assert_eq!(s.bots_planned, Some(3.0));
        assert_eq!(s.bots_at_end, Some(1.0));
        assert_eq!(s.bots_kicked, Some(1.0));
        assert_eq!(s.bots_failed, Some(1.0));
    }

    #[test]
    fn medians() {
        assert_eq!(median_range(&[]), None);
        assert_eq!(median_range(&[3.0]), Some((3.0, 3.0, 3.0)));
        assert_eq!(median_range(&[5.0, 1.0, 3.0]), Some((3.0, 1.0, 5.0)));
        assert_eq!(median_range(&[4.0, 1.0, 2.0, 3.0]), Some((2.5, 1.0, 4.0)));
    }

    #[test]
    fn french_numbers() {
        assert_eq!(format_number(4.0, 2), "4,00");
        assert_eq!(format_number(-0.126, 2), "-0,13");
        assert_eq!(format_number(-0.001, 2), "0,00");
        assert_eq!(format_number(2484.0, 0), "2484");
        assert_eq!(format_number(12345.678, 1), "12 345,7");
        assert_eq!(format_number(1234567.0, 0), "1 234 567");
        assert_eq!(format_number(-98765.0, 0), "-98 765");
        // The median of two whole counts may fall in between.
        assert_eq!(shown(2.5, 0), "2,5");
        assert_eq!(shown(2.0, 0), "2");
        assert_eq!(signed(0.5, 0), "+0,5");
        assert_eq!(signed(0.001, 2), "0,00");
        assert_eq!(signed(-1.5, 1), "-1,5");
    }

    fn side(label: &str, means: &[f64]) -> Side {
        Side {
            label: label.into(),
            samples: means
                .iter()
                .map(|&m| Sample {
                    mspt_mean: Some(m),
                    tps: Some(20.0),
                    attribution: Some(false),
                    ..Sample::default()
                })
                .collect(),
            run: vec![
                Json::parse(r#"{"scenario": "spawn-groupe", "bots": {"count": 20}}"#).unwrap(),
            ],
        }
    }

    #[test]
    fn table() {
        let table = markdown(&side("a", &[4.0]), &side("b", &[3.0, 3.5, 2.0]));
        assert!(table.contains("Avant : `a` (1 répétition)"), "{table}");
        assert!(
            table.contains("Après : `b` (3 répétitions, médiane (min – max))"),
            "{table}"
        );
        assert!(
            table.contains("| Scénario | `spawn-groupe` | `spawn-groupe` |"),
            "{table}"
        );
        assert!(table.contains("| Bots | `20` | `20` |"), "{table}");
        assert!(
            table.contains("| MSPT moyen (ms) | 4,00 | 3,00 (2,00 – 3,50) | -1,00 | -25,0 % |"),
            "{table}"
        );
        assert!(
            table.contains("| TPS | 20,00 | 20,00 (20,00 – 20,00) | 0,00 | 0,0 % |"),
            "{table}"
        );
        // Metrics neither side has are left out.
        assert!(!table.contains("Pauses GC"), "{table}");
        assert!(!table.contains("Attention"), "{table}");

        let mut other = side("c", &[5.0]);
        other.samples[0].attribution = Some(true);
        other.run = vec![Json::parse(r#"{"scenario": "disperses"}"#).unwrap()];
        let table = markdown(&side("a", &[4.0]), &other);
        assert!(
            table.contains("| Scénario ≠ | `spawn-groupe` | `disperses` |"),
            "{table}"
        );
        assert!(table.contains("| +1,00 | +25,0 % |"), "{table}");
        assert!(table.contains("Attention"), "{table}");
    }

    #[test]
    fn loads_result_directories() {
        let root = std::env::temp_dir().join(format!("gamma-bots-cmp-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        for rep in ["rep1", "rep2", "rep10"] {
            fs::create_dir_all(root.join(rep)).unwrap();
            fs::write(root.join(rep).join("summary.json"), SUMMARY).unwrap();
        }
        fs::create_dir_all(root.join("rep3")).unwrap(); // failed repetition: no summary
        fs::write(root.join("run.json"), r#"{"scenario": "spawn-groupe"}"#).unwrap();
        let side = Side::load(root.to_str().unwrap()).unwrap();
        assert_eq!(side.samples.len(), 3);
        assert_eq!(side.run.len(), 1);
        let single = Side::load(root.join("rep2").to_str().unwrap()).unwrap();
        assert_eq!(single.samples.len(), 1);
        let joined = format!(
            "{},{}",
            root.join("rep1").display(),
            root.join("rep2").display()
        );
        assert_eq!(Side::load(&joined).unwrap().samples.len(), 2);
        assert!(Side::load(root.join("rep3").to_str().unwrap()).is_err());
        fs::remove_dir_all(&root).unwrap();
    }
}
