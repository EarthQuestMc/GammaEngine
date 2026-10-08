//! The per-bot CSV and the summary printed at the end.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::{self, Write};
use std::path::Path;
use std::time::Duration;

use crate::bot::BotStats;

pub const CSV_HEADER: &str = concat!(
    "name,connected,attempts,join_ms,first_chunk_ms,in_game_s,connected_at_end,",
    "kick_reason,error,first_failure,keepalives,keepalive_interval_ms,keepalive_interval_max_ms,",
    "bytes_received,chunk_packets,s08_received,setbacks,deaths,x,y,z",
);

fn csv_field(text: &str) -> Cow<'_, str> {
    if text.contains([',', '"', '\n', '\r']) {
        Cow::Owned(format!("\"{}\"", text.replace('"', "\"\"")))
    } else {
        Cow::Borrowed(text)
    }
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

fn opt<T: ToString>(value: Option<T>) -> String {
    value.map(|v| v.to_string()).unwrap_or_default()
}

pub fn csv_line(name: &str, s: &BotStats) -> String {
    [
        csv_field(name).into_owned(),
        yes_no(s.joined).into(),
        s.attempts.to_string(),
        opt(s.join_ms),
        opt(s.first_chunk_ms),
        format!("{:.1}", s.in_game_ms as f64 / 1000.0),
        yes_no(s.connected_at_end).into(),
        csv_field(s.kick_reason.as_deref().unwrap_or("")).into_owned(),
        csv_field(s.error.as_deref().unwrap_or("")).into_owned(),
        csv_field(s.first_failure.as_deref().unwrap_or("")).into_owned(),
        s.keepalives.to_string(),
        opt(s.keepalive_interval_ms().map(|ms| format!("{ms:.0}"))),
        if s.keepalive_intervals > 0 {
            format!("{:.0}", s.keepalive_interval_max_ms)
        } else {
            String::new()
        },
        s.bytes_received.to_string(),
        s.chunk_packets.to_string(),
        s.s08_received.to_string(),
        s.setbacks.to_string(),
        s.deaths.to_string(),
        opt(s.position.map(|p| format!("{:.2}", p.x))),
        opt(s.position.map(|p| format!("{:.2}", p.y))),
        opt(s.position.map(|p| format!("{:.2}", p.z))),
    ]
    .join(",")
}

pub fn write_csv(path: &Path, rows: &[(String, BotStats)]) -> io::Result<()> {
    let mut out = io::BufWriter::new(std::fs::File::create(path)?);
    writeln!(out, "{CSV_HEADER}")?;
    for (name, stats) in rows {
        writeln!(out, "{}", csv_line(name, stats))?;
    }
    out.flush()
}

/// Nearest-rank percentile of sorted values.
fn percentile(sorted: &[u64], p: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let rank = ((p / 100.0) * sorted.len() as f64).ceil() as usize;
    sorted[rank.clamp(1, sorted.len()) - 1]
}

fn distribution(mut values: Vec<u64>) -> Option<String> {
    if values.is_empty() {
        return None;
    }
    values.sort_unstable();
    Some(format!(
        "min {} p50 {} p95 {} max {}",
        values[0],
        percentile(&values, 50.0),
        percentile(&values, 95.0),
        values[values.len() - 1]
    ))
}

fn tally<'a>(items: impl Iterator<Item = &'a str>) -> BTreeMap<&'a str, usize> {
    let mut counts = BTreeMap::new();
    for item in items {
        *counts.entry(item).or_insert(0) += 1;
    }
    counts
}

pub fn summary(rows: &[(String, BotStats)], elapsed: Duration, title: &str) -> String {
    let all: Vec<&BotStats> = rows.iter().map(|(_, s)| s).collect();
    let started = all.iter().filter(|s| s.started).count();
    let joined = all.iter().filter(|s| s.joined).count();
    let at_end = all.iter().filter(|s| s.connected_at_end).count();
    let kicked = all.iter().filter(|s| s.kick_reason.is_some()).count();
    let errors = all.iter().filter(|s| s.error.is_some()).count();
    let secs = elapsed.as_secs_f64().max(1e-3);
    let bytes: u64 = all.iter().map(|s| s.bytes_received).sum();
    let chunks: u64 = all.iter().map(|s| s.chunk_packets).sum();
    let keepalives: u64 = all.iter().map(|s| s.keepalives).sum();
    let s08: u64 = all.iter().map(|s| s.s08_received).sum();
    let deaths: u64 = all.iter().map(|s| s.deaths).sum();
    let setbacks: u64 = all.iter().map(|s| s.setbacks).sum();
    let intervals: u64 = all.iter().map(|s| s.keepalive_intervals).sum();
    let interval_total: f64 = all.iter().map(|s| s.keepalive_interval_total_ms).sum();

    let mut text = String::new();
    let _ = writeln!(text, "=== gamma-bots: {title}, {:.1} s ===", secs);
    let _ = writeln!(
        text,
        "bots: {} planned, {started} started, {joined} joined, {at_end} still in game at the end",
        rows.len()
    );
    let retried = all.iter().filter(|s| s.attempts > 1).count();
    let _ = writeln!(
        text,
        "kicked: {kicked}, errors: {errors}, reconnected before joining: {retried}"
    );
    if let Some(d) = distribution(all.iter().filter_map(|s| s.join_ms).collect()) {
        let _ = writeln!(text, "join time (ms): {d}");
    }
    if let Some(d) = distribution(all.iter().filter_map(|s| s.first_chunk_ms).collect()) {
        let _ = writeln!(text, "first chunk after join (ms): {d}");
    }
    let longest = all
        .iter()
        .map(|s| s.keepalive_interval_max_ms)
        .fold(0.0, f64::max);
    let intervals = if intervals > 0 {
        format!(
            "interval mean {:.0} ms, longest {longest:.0} ms (2000 ms at 20 TPS)",
            interval_total / intervals as f64
        )
    } else {
        "no interval".into()
    };
    let _ = writeln!(text, "keep-alives: {keepalives} received, {intervals}");
    let _ = writeln!(
        text,
        "received: {:.1} MiB ({:.2} MiB/s), {chunks} chunk packets ({:.1}/s), {s08} S08 ({setbacks} setbacks), {deaths} deaths",
        bytes as f64 / 1048576.0,
        bytes as f64 / 1048576.0 / secs,
        chunks as f64 / secs,
    );
    for (label, reasons) in [
        (
            "kick reasons",
            tally(all.iter().filter_map(|s| s.kick_reason.as_deref())),
        ),
        (
            "errors",
            tally(all.iter().filter_map(|s| s.error.as_deref())),
        ),
        (
            "first connection failed",
            tally(all.iter().filter_map(|s| s.first_failure.as_deref())),
        ),
    ] {
        if !reasons.is_empty() {
            let _ = writeln!(text, "{label}:");
            for (reason, count) in reasons {
                let _ = writeln!(text, "  {count:>4} x {reason}");
            }
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_escaping() {
        assert_eq!(csv_field("plain"), "plain");
        assert_eq!(csv_field("a,b"), "\"a,b\"");
        assert_eq!(csv_field("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(csv_field("two\nlines"), "\"two\nlines\"");
    }

    #[test]
    fn csv_line_matches_header() {
        let stats = BotStats {
            started: true,
            joined: true,
            attempts: 1,
            join_ms: Some(120),
            kick_reason: Some("Mod rejections [a, b]".into()),
            keepalives: 3,
            keepalive_interval_total_ms: 4000.0,
            keepalive_intervals: 2,
            keepalive_interval_max_ms: 2500.0,
            ..BotStats::default()
        };
        let line = csv_line("bot1", &stats);
        assert_eq!(
            line,
            "bot1,yes,1,120,,0.0,no,\"Mod rejections [a, b]\",,,3,2000,2500,0,0,0,0,0,,,"
        );
        assert_eq!(CSV_HEADER.split(',').count(), 21);
    }

    #[test]
    fn percentiles() {
        let values: Vec<u64> = (1..=100).collect();
        assert_eq!(percentile(&values, 50.0), 50);
        assert_eq!(percentile(&values, 95.0), 95);
        assert_eq!(percentile(&values, 100.0), 100);
        assert_eq!(percentile(&[7], 95.0), 7);
        assert_eq!(percentile(&[], 50.0), 0);
    }

    #[test]
    fn summary_counts() {
        let ok = BotStats {
            started: true,
            joined: true,
            join_ms: Some(100),
            connected_at_end: true,
            ..Default::default()
        };
        let kicked = BotStats {
            started: true,
            kick_reason: Some("Server full".into()),
            ..Default::default()
        };
        let rows = vec![
            ("a".to_string(), ok.clone()),
            ("b".to_string(), ok),
            ("c".to_string(), kicked),
        ];
        let text = summary(&rows, Duration::from_secs(10), "3 bots, idle");
        assert!(
            text.contains("3 planned, 3 started, 2 joined, 2 still in game at the end"),
            "{text}"
        );
        assert!(text.contains("kicked: 1, errors: 0"));
        assert!(text.contains("1 x Server full"));
    }
}
