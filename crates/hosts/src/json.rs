//! What GitLab's and GitHub's answers write alike: a time, a colour, a list of labels.

use artoo_hosts_api::Label;
use serde_json::Value;

/// `2026-10-08T12:34:56.789Z` (or with an offset) as seconds since the epoch; 0 if unreadable.
pub fn epoch(stamp: &str) -> i64 {
    let (date, time) = stamp.split_once('T').unwrap_or((stamp, "00:00:00"));
    let mut d = date.split('-').filter_map(|p| p.parse::<i64>().ok());
    let (Some(y), Some(m), Some(day)) = (d.next(), d.next(), d.next()) else {
        return 0;
    };
    let clock: String = time
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == ':')
        .collect();
    let mut t = clock.split(':').filter_map(|p| p.parse::<i64>().ok());
    let (h, mi, s) = (
        t.next().unwrap_or(0),
        t.next().unwrap_or(0),
        t.next().unwrap_or(0),
    );
    // Howard Hinnant's civil-to-days.
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    // An offset such as +03:00 after the clock.
    let offset = time
        .rfind(['+', '-'])
        .filter(|i| *i > 0)
        .and_then(|i| {
            let sign = if time.as_bytes()[i] == b'+' { 1 } else { -1 };
            let mut p = time[i + 1..]
                .split(':')
                .filter_map(|x| x.parse::<i64>().ok());
            Some(sign * (p.next()? * 3600 + p.next().unwrap_or(0) * 60))
        })
        .unwrap_or(0);
    days * 86400 + h * 3600 + mi * 60 + s - offset
}

/// `#rrggbb` (or `rrggbb`) as 0xRRGGBB.
pub fn hex_color(text: &str) -> Option<u32> {
    let hex = text.trim().trim_start_matches('#');
    (hex.len() == 6)
        .then(|| u32::from_str_radix(hex, 16).ok())
        .flatten()
}

/// A host's labels, named, in their colours; one without a name is left out.
pub fn parse_labels(value: &Value) -> Vec<Label> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .map(|l| Label {
            name: l["name"].as_str().unwrap_or_default().to_owned(),
            color: hex_color(l["color"].as_str().unwrap_or_default()),
        })
        .filter(|l| !l.name.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn labels_keep_their_colours_and_skip_the_nameless() {
        let labels = parse_labels(&json!([
            { "name": "auth", "color": "#428BCA" },
            { "name": "", "color": "#000000" },
            { "name": "refactor", "color": "" }
        ]));
        assert_eq!(
            labels,
            [
                Label {
                    name: "auth".to_owned(),
                    color: Some(0x428bca)
                },
                Label {
                    name: "refactor".to_owned(),
                    color: None
                }
            ]
        );
    }

    #[test]
    fn timestamps_become_epoch_seconds() {
        assert_eq!(epoch("1970-01-02T00:00:00Z"), 86400);
        assert_eq!(epoch("2026-10-08T00:00:00.123Z"), 1_791_417_600);
        assert_eq!(epoch("2026-10-08T03:00:00+03:00"), 1_791_417_600);
        assert_eq!(epoch("nonsense"), 0);
    }
}
