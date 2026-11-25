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

/// Width of the time column every list row shares, wide enough for ``Nov 21` at 11 px.
pub const SHORT_TIME_WIDTH: f32 = 44.;

/// A list row's time, short and tabular: `now`, `12m`, `2h`, `3d` (under a week), then `Nov 3` within the year,
/// then the year alone (`2025`). The long form is `ago`, the full date the tooltip's.
pub fn short_ago(time: i64) -> String {
    short_at(now(), time)
}

fn short_at(now: i64, time: i64) -> String {
    let secs = (now - time).max(0);
    match secs {
        0..60 => "now".to_owned(),
        60..3600 => format!("{}m", secs / 60),
        3600..86400 => format!("{}h", secs / 3600),
        86400..604_800 => format!("{}d", secs / 86400),
        604_800..31_536_000 => {
            let (_, m, d) = civil_from_days((time + local_offset(time)).div_euclid(86400));
            format!("{} {d}", MONTHS[m as usize - 1])
        }
        _ => date(time)[..4].to_owned(),
    }
}

/// A time in words for a row with room for them: `ago` under an hour, `today`, the weekday within a week, then
/// `Nov 3` within the year and the year alone.
pub fn when_word(time: i64) -> String {
    when_at(now(), time)
}

fn when_at(now: i64, time: i64) -> String {
    let day = |t: i64| (t + local_offset(t)).div_euclid(86400);
    match (now - time).max(0) {
        secs @ 0..3600 => match secs / 60 {
            0 => "just now".to_owned(),
            m => format!("{m} min ago"),
        },
        _ if day(time) == day(now) => "today".to_owned(),
        // 1970-01-01 was a Thursday.
        _ if day(now) - day(time) < 7 => {
            const DAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
            DAYS[day(time).rem_euclid(7) as usize].to_owned()
        }
        _ => short_at(now, time),
    }
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// The time cell of a list row: `short_ago`, right-aligned in the column every row shares.
pub fn time_cell(time: i64) -> gpui::Div {
    use gpui::{ParentElement, Styled, div, px};
    div()
        .flex_none()
        .w(px(SHORT_TIME_WIDTH))
        .flex()
        .justify_end()
        .whitespace_nowrap()
        .child(short_ago(time))
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

/// A comment on one line, for a folded thread or a list: without its markdown when `rich`.
pub fn one_line(body: &str, rich: bool) -> String {
    if rich {
        artoo_core::markdown::plain(body)
    } else {
        body.replace('\n', " ")
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

/// `12.3 KB`.
pub fn bytes(n: usize) -> String {
    match n {
        n if n < 1024 => format!("{n} B"),
        n if n < 1024 * 1024 => format!("{:.1} KB", n as f64 / 1024.),
        n => format!("{:.1} MB", n as f64 / (1024. * 1024.)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rows_time_in_words_goes_from_minutes_to_a_date() {
        // Noon of a day in local time, so no step below crosses midnight by the zone.
        let noon = 20_000 * 86400 + 43200;
        let now = noon - local_offset(noon);
        assert_eq!(when_at(now, now - 20), "just now");
        assert_eq!(when_at(now, now - 35 * 60), "35 min ago");
        assert_eq!(when_at(now, now - 5 * 3600), "today");
        // Day 20 000 is a Friday.
        assert_eq!(when_at(now, now - 3 * 86400), "Tue");
        assert_eq!(
            when_at(now, now - 9 * 86400),
            short_at(now, now - 9 * 86400)
        );
    }

    #[test]
    fn short_times_cross_each_threshold() {
        let now = 1_795_000_000;
        let at = |ago: i64| short_at(now, now - ago);
        assert_eq!(at(0), "now");
        assert_eq!(at(59), "now");
        assert_eq!(at(60), "1m");
        assert_eq!(at(12 * 60), "12m");
        assert_eq!(at(3599), "59m");
        assert_eq!(at(3600), "1h");
        assert_eq!(at(86399), "23h");
        assert_eq!(at(86400), "1d");
        assert_eq!(at(172_800), "2d");
        assert_eq!(at(604_799), "6d");
        // A week and more: month and day, then the year alone past a year.
        let time = now - 604_800;
        let want = chrono::DateTime::from_timestamp(time, 0)
            .unwrap()
            .with_timezone(&chrono::Local)
            .format("%b %-d")
            .to_string();
        assert_eq!(at(604_800), want);
        assert_eq!(at(31_536_000), date(now - 31_536_000)[..4]);
        // The future reads as now.
        assert_eq!(short_at(now, now + 500), "now");
    }

    #[test]
    fn plurals_read_naturally() {
        assert_eq!(plural(1, "file"), "1 file");
        assert_eq!(plural(3, "commit"), "3 commits");
        assert_eq!(plural(2, "reply"), "2 replies");
        assert_eq!(plural(2, "day"), "2 days");
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
