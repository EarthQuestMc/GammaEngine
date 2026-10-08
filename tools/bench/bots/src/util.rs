//! Small helpers of the orchestrator: UTC time stamps, JSON and CSV text, file trees.

use std::fs;
use std::io;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Civil date of a day count since 1970-01-01 (H. Hinnant's `civil_from_days`).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// UTC date and time of a Unix time: (year, month, day, hour, minute, second).
pub fn utc_fields(unix_secs: i64) -> (i64, u32, u32, u32, u32, u32) {
    let (year, month, day) = civil_from_days(unix_secs.div_euclid(86_400));
    let secs = unix_secs.rem_euclid(86_400);
    (
        year,
        month,
        day,
        (secs / 3600) as u32,
        (secs / 60 % 60) as u32,
        (secs % 60) as u32,
    )
}

pub fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// `yyyyMMdd-HHmmss`, UTC: the stamp of a results directory.
pub fn stamp(unix_secs: i64) -> String {
    let (y, mo, d, h, mi, s) = utc_fields(unix_secs);
    format!("{y:04}{mo:02}{d:02}-{h:02}{mi:02}{s:02}")
}

/// ISO 8601, UTC, as in the server's `summary.json`.
pub fn iso(unix_secs: i64) -> String {
    let (y, mo, d, h, mi, s) = utc_fields(unix_secs);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

pub fn json_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

pub fn json_strings(items: &[String]) -> String {
    let items: Vec<String> = items.iter().map(|s| json_string(s)).collect();
    format!("[{}]", items.join(", "))
}

/// Splits one CSV line, with `"` quoting and `""` escapes. Fields may not contain line breaks.
pub fn csv_fields(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => fields.push(std::mem::take(&mut field)),
            _ => field.push(c),
        }
    }
    fields.push(field);
    fields
}

/// Copies a directory tree; `to` must not exist yet or be empty.
pub fn copy_dir(from: &Path, to: &Path) -> io::Result<u64> {
    fs::create_dir_all(to)?;
    let mut files = 0;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            files += copy_dir(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), &target)?;
            files += 1;
        }
    }
    Ok(files)
}

/// Removes everything in `dir` except the names in `keep`.
pub fn clear_dir_except(dir: &Path, keep: &[&str]) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name();
        if keep.iter().any(|k| name == *k) {
            continue;
        }
        if entry.file_type()?.is_dir() {
            fs::remove_dir_all(entry.path())?;
        } else {
            fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}

/// Removes ANSI escape sequences and Minecraft `§` colour codes from a console line.
pub fn plain_text(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            '\u{1b}' => {
                // CSI: ESC [ parameters final-byte (@ to ~).
                if chars.clone().next() == Some('[') {
                    chars.next();
                    for c in chars.by_ref() {
                        if ('@'..='~').contains(&c) {
                            break;
                        }
                    }
                }
            }
            '§' => {
                chars.next();
            }
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates() {
        assert_eq!(utc_fields(0), (1970, 1, 1, 0, 0, 0));
        // 2026-10-08T14:01:37Z
        assert_eq!(utc_fields(1_791_468_097), (2026, 10, 8, 14, 1, 37));
        // Leap day.
        assert_eq!(utc_fields(951_782_400), (2000, 2, 29, 0, 0, 0));
        assert_eq!(utc_fields(-1), (1969, 12, 31, 23, 59, 59));
        assert_eq!(stamp(1_791_468_097), "20261008-140137");
        assert_eq!(iso(1_791_468_097), "2026-10-08T14:01:37Z");
    }

    #[test]
    fn json_text() {
        assert_eq!(
            json_string("a\"b\\c\nd\u{1}é"),
            "\"a\\\"b\\\\c\\nd\\u0001é\""
        );
        assert_eq!(
            json_strings(&["-Xmx1G".into(), "nogui".into()]),
            "[\"-Xmx1G\", \"nogui\"]"
        );
        assert_eq!(json_strings(&[]), "[]");
    }

    #[test]
    fn csv_splitting() {
        assert_eq!(csv_fields("a,b,,c"), ["a", "b", "", "c"]);
        assert_eq!(
            csv_fields("bot1,\"Mod rejections [a, b]\",\"say \"\"hi\"\"\",x"),
            ["bot1", "Mod rejections [a, b]", "say \"hi\"", "x"]
        );
        assert_eq!(csv_fields(""), [""]);
    }

    #[test]
    fn console_text() {
        assert_eq!(
            plain_text("\u{1b}[0;32;1mRecording stopped: \u{1b}[0;37;1m400 ticks\u{1b}[m"),
            "Recording stopped: 400 ticks"
        );
        assert_eq!(plain_text("§aGreen §rplain"), "Green plain");
        assert_eq!(
            plain_text("[14:01:37] Done (1,644s)!"),
            "[14:01:37] Done (1,644s)!"
        );
    }

    #[test]
    fn tree_copy_and_clear() {
        let root = std::env::temp_dir().join(format!("gamma-bots-util-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let from = root.join("from");
        fs::create_dir_all(from.join("region")).unwrap();
        fs::write(from.join("level.dat"), b"x").unwrap();
        fs::write(from.join("region/r.0.0.mca"), b"y").unwrap();
        let to = root.join("to");
        assert_eq!(copy_dir(&from, &to).unwrap(), 2);
        assert_eq!(fs::read(to.join("region/r.0.0.mca")).unwrap(), b"y");

        fs::write(to.join("keep.txt"), b"k").unwrap();
        clear_dir_except(&to, &["keep.txt"]).unwrap();
        let left: Vec<_> = fs::read_dir(&to)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(left, ["keep.txt"]);
        fs::remove_dir_all(&root).unwrap();
    }
}
