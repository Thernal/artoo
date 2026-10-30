//! Small text helpers for the views.

use std::time::{SystemTime, UNIX_EPOCH};

/// Now, in seconds since the epoch.
pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// "just now", "5 min ago", "3 h ago", "2 d ago", then the date.
pub fn ago(time: i64) -> String {
    let secs = (now() - time).max(0);
    match secs {
        0..60 => "just now".to_owned(),
        60..3600 => format!("{} min ago", secs / 60),
        3600..86400 => format!("{} h ago", secs / 3600),
        86400..2_592_000 => format!("{} d ago", secs / 86400),
        _ => date(time),
    }
}

/// `YYYY-MM-DD` in local time.
pub fn date(time: i64) -> String {
    date_at(time + local_offset(time))
}

/// `HH:MM` in local time.
pub fn clock(time: i64) -> String {
    let secs = (time + local_offset(time)).rem_euclid(86400);
    format!("{:02}:{:02}", secs / 3600, secs % 3600 / 60)
}

/// `YYYY-MM-DD` of `time` as it is.
pub fn date_at(time: i64) -> String {
    let (y, m, d) = civil_from_days(time.div_euclid(86400));
    format!("{y:04}-{m:02}-{d:02}")
}

/// Seconds the local time zone is ahead of UTC at `time` (X61).
pub fn local_offset(time: i64) -> i64 {
    use chrono::Offset;
    chrono::DateTime::from_timestamp(time, 0).map_or(0, |t| {
        i64::from(
            t.with_timezone(&chrono::Local)
                .offset()
                .fix()
                .local_minus_utc(),
        )
    })
}

/// Howard Hinnant's days-to-civil conversion.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

/// `text` cut in the middle to `n` characters: long branch names keep both ends.
pub fn mid(text: &str, n: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= n || n < 5 {
        return text.to_owned();
    }
    let head = n.div_ceil(2) - 1;
    let tail = n - head - 1;
    let mut out: String = chars[..head].iter().collect();
    out.push('…');
    out.extend(&chars[chars.len() - tail..]);
    out
}

/// An object id cut to its first 7 characters, as git shows it (a `git2::Oid`, without the kit knowing git).
pub fn short(id: impl std::fmt::Display) -> String {
    id.to_string()[..7].to_owned()
}

/// `reason` of a version, without the subject that follows the colon.
pub fn reason(reason: &str) -> &str {
    let kind = reason.split(':').next().unwrap_or_default().trim();
    match kind {
        "" => "unknown",
        k if k.starts_with("commit (amend)") => "amend",
        k if k.starts_with("rebase") => "rebase",
        k if k.starts_with("fetch") || k.starts_with("pull") => "fetch",
        k if k.starts_with("reset") => "reset",
        k if k.starts_with("update by push") => "push",
        k if k.starts_with("branch") => "created",
        k if k.starts_with("commit") => "commit",
        k => k,
    }
}

/// `path` with the home directory written as `~`.
pub fn home_relative(path: &std::path::Path) -> String {
    match std::env::home_dir().and_then(|home| path.strip_prefix(home).ok().map(|p| p.to_owned())) {
        Some(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

/// "1 file", "3 commits", "2 replies".
pub fn plural(n: usize, noun: &str) -> String {
    if n == 1 {
        return format!("1 {noun}");
    }
    // "reply" → "replies", not "replys".
    match noun.strip_suffix('y') {
        Some(stem) if stem.ends_with(|c: char| !"aeiou".contains(c)) => format!("{n} {stem}ies"),
        _ => format!("{n} {noun}s"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plurals_read_naturally() {
        assert_eq!(plural(1, "file"), "1 file");
        assert_eq!(plural(3, "commit"), "3 commits");
        assert_eq!(plural(2, "reply"), "2 replies");
        assert_eq!(plural(2, "day"), "2 days");
    }

    #[test]
    fn long_branch_names_keep_both_ends() {
        assert_eq!(mid("main", 10), "main");
        let cut = mid("feature/login-state-sealed-interface-and-error-text", 20);
        assert_eq!(cut.chars().count(), 20);
        assert!(cut.starts_with("feature/l") && cut.ends_with("-text") && cut.contains('…'));
    }

    #[test]
    fn dates() {
        assert_eq!(date_at(0), "1970-01-01");
        assert_eq!(date_at(1_791_417_600), "2026-10-08");
    }

    #[test]
    fn reasons() {
        assert_eq!(reason("commit (amend): fix"), "amend");
        assert_eq!(reason("rebase (finish): refs/heads/x onto 123"), "rebase");
        assert_eq!(reason("fetch: forced-update"), "fetch");
    }
}
