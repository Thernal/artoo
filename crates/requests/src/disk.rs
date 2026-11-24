//! The open list and the Inbox's rows kept on disk, per project, for the next start (audit step 8): a restart shows
//! the last list and the last notices at once, with the time they were read, while the host is asked again.
//!
//! Owner-only files, bounded: `MOST_PROJECTS` projects, each `MOST_ROWS` rows. Written from the background executor.

use artoo_core::storage;
use serde::{Serialize, de::DeserializeOwned};

/// The open requests, by project (`crate::comment::key`'s project key).
pub const LIST_FILE: &str = "requests-open.json";
/// The Inbox's notices, by the host's base address.
pub const NOTICES_FILE: &str = "notices-kept.json";
/// Projects kept in a file; the one read longest ago goes first.
const MOST_PROJECTS: usize = 20;
/// Rows kept per project; the newest (the last) stay.
const MOST_ROWS: usize = 300;

#[derive(serde::Deserialize)]
struct Entry<T> {
    at: i64,
    rows: Vec<T>,
}

/// Project `key`'s rows in `text`, with the time they were read; none when there is nothing, or it cannot be read.
pub fn entry_of<T: DeserializeOwned>(text: &str, key: &str) -> Option<(i64, Vec<T>)> {
    let mut all: serde_json::Map<String, serde_json::Value> = serde_json::from_str(text).ok()?;
    let entry: Entry<T> = serde_json::from_value(all.remove(key)?).ok()?;
    Some((entry.at, entry.rows))
}

/// `text` with project `key`'s rows replaced by `rows` read at `at`; the other projects stay, up to `MOST_PROJECTS`.
pub fn with_entry<T: Serialize>(text: &str, key: &str, at: i64, rows: &[T]) -> String {
    let mut all: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(text).unwrap_or_default();
    let rows = &rows[rows.len().saturating_sub(MOST_ROWS)..];
    if let Ok(v) = serde_json::to_value(serde_json::json!({ "at": at, "rows": rows })) {
        all.insert(key.to_owned(), v);
    }
    while all.len() > MOST_PROJECTS {
        let oldest = (all.iter())
            .filter(|(k, _)| k.as_str() != key)
            .min_by_key(|(_, v)| v.get("at").and_then(|a| a.as_i64()).unwrap_or(0))
            .map(|(k, _)| k.clone());
        match oldest {
            Some(k) => all.remove(&k),
            None => break,
        };
    }
    serde_json::Value::Object(all).to_string()
}

/// Project `key`'s rows as `file` holds them.
pub fn read<T: DeserializeOwned>(file: &str, key: &str) -> Option<(i64, Vec<T>)> {
    entry_of(&storage::read(file), key)
}

/// Keeps `rows` of project `key`, read at `at`, in `file`. Blocks on the disk: call it from the background executor.
pub fn write<T: Serialize>(file: &str, key: &str, at: i64, rows: &[T]) {
    storage::save_private(file, with_entry(&storage::read(file), key, at, rows));
}

#[cfg(test)]
mod tests {
    use super::*;
    use artoo_core::mr::{Mr, MrState};

    fn request(iid: u64) -> Mr {
        Mr {
            iid,
            title: format!("Request {iid}"),
            reviewers: vec!["ember".into()],
            state: MrState::Opened,
            updated: 1_700_000_000 + iid as i64,
            ..Default::default()
        }
    }

    #[test]
    fn the_list_comes_back_as_it_was_kept_with_its_time() {
        let list = vec![request(1), request(2)];
        let text = with_entry("", "proj", 42, &list);
        assert_eq!(entry_of::<Mr>(&text, "proj"), Some((42, list)));
        assert_eq!(entry_of::<Mr>(&text, "other"), None);
    }

    #[test]
    fn another_projects_rows_stay_and_garbage_is_nothing() {
        let text = with_entry(
            &with_entry("", "a", 1, &[request(1)]),
            "b",
            2,
            &[request(2)],
        );
        assert_eq!(entry_of::<Mr>(&text, "a").unwrap().1[0].iid, 1);
        assert_eq!(entry_of::<Mr>(&text, "b").unwrap().1[0].iid, 2);
        assert_eq!(entry_of::<Mr>("{not json", "a"), None);
        // A file from the next version, with fields this one has no use for, still reads.
        let odd = r#"{"a":{"at":5,"rows":[{"iid":9,"later":true}]}}"#;
        assert_eq!(entry_of::<Mr>(odd, "a").unwrap().1[0].iid, 9);
    }

    #[test]
    fn rows_and_projects_are_bounded() {
        let many: Vec<Mr> = (0..MOST_ROWS as u64 + 50).map(request).collect();
        let text = with_entry("", "p", 1, &many);
        let (_, rows) = entry_of::<Mr>(&text, "p").unwrap();
        assert_eq!(rows.len(), MOST_ROWS);
        assert_eq!(rows.last().unwrap().iid, MOST_ROWS as u64 + 49);
        let mut text = String::new();
        for n in 0..MOST_PROJECTS as i64 + 5 {
            text = with_entry(&text, &format!("p{n}"), n + 1, &[request(1)]);
        }
        let all: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&text).unwrap();
        assert_eq!(all.len(), MOST_PROJECTS);
        // The ones read longest ago went, the newest stayed.
        assert!(!all.contains_key("p0") && all.contains_key(&format!("p{}", MOST_PROJECTS + 4)));
    }
}
