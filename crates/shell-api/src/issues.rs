//! Issue keys (#217): a tracker — a plugin, Tasks — names the keys of its issues (`PAY-412`), and every text the
//! tab draws through the kit's markdown shows them as links, a key's hover shows its card, and its click opens the
//! issue under the tracker's space. The tracker says which words are its keys, what an issue looks like for the card
//! (`IssueCard`, which also says when the tracker could not be reached, and what was read last) and how to open it.
//! With no tracker offered a key stays plain text; nothing else changes.

use gpui::{App, Global};
use std::ops::Range;
use std::rc::Rc;

pub use artoo_ui_kit::hover::IssueCard;

/// The address a found key is followed by: `artoo-issue:PAY-412`, which the tab answers (`open`), never the browser.
pub const SCHEME: &str = artoo_ui_kit::markdown::ISSUE_SCHEME;

/// A tracker, as a plugin offers it (`add`).
pub trait Tracker {
    /// Its name: `Jira`.
    fn name(&self) -> String;

    /// The keys of its issues in `text`, by their bytes, in order and apart (`scan` finds the usual shape).
    fn find(&self, text: &str, cx: &App) -> Vec<Range<usize>>;

    /// What the card of issue `key` shows, from what the tracker has read; none when it does not know the issue (the
    /// key then has no card). When the tracker could not be reached the card says so (`IssueCard::stale`) and shows
    /// what was read last. Called while the card is drawn: it reads, it does not fetch.
    fn card(&self, key: &str, cx: &App) -> Option<IssueCard>;

    /// Opens issue `key` under the tracker's space.
    fn open(&self, key: &str, cx: &mut App);
}

#[derive(Default)]
struct Trackers(Vec<Rc<dyn Tracker>>);

impl Global for Trackers {}

/// Offers a tracker (its owner's `install`).
pub fn add(cx: &mut App, tracker: Rc<dyn Tracker>) {
    cx.default_global::<Trackers>().0.push(tracker);
}

fn all(cx: &App) -> Vec<Rc<dyn Tracker>> {
    cx.try_global::<Trackers>()
        .map(|t| t.0.clone())
        .unwrap_or_default()
}

/// The keys in `text` as links: each one's bytes and its address (`SCHEME` and the key); none without a tracker.
pub fn links(cx: &App, text: &str) -> Vec<(Range<usize>, String)> {
    let mut found: Vec<(Range<usize>, String)> = Vec::new();
    for tracker in all(cx) {
        for range in tracker.find(text, cx) {
            let taken = found
                .iter()
                .any(|(r, _)| r.start < range.end && range.start < r.end);
            if !taken && let Some(key) = text.get(range.clone()) {
                found.push((range, format!("{SCHEME}{key}")));
            }
        }
    }
    found.sort_by_key(|(r, _)| r.start);
    found
}

/// The key a link's address names, if it is one of a tracker's.
pub fn key_of(address: &str) -> Option<&str> {
    address.strip_prefix(SCHEME)
}

/// The card of issue `key`, from the first tracker that knows it.
pub fn card(cx: &App, key: &str) -> Option<IssueCard> {
    all(cx).iter().find_map(|t| t.card(key, cx))
}

/// Opens issue `key` in the first tracker that has its card; false when none does (nothing happens).
pub fn open(cx: &mut App, key: &str) -> bool {
    let Some(tracker) = all(cx).into_iter().find(|t| t.card(key, cx).is_some()) else {
        return false;
    };
    tracker.open(key, cx);
    true
}

/// `PAY-123`: capitals (a digit may follow the first), a dash, a number.
pub fn is_key(word: &str) -> bool {
    let Some((project, number)) = word.split_once('-') else {
        return false;
    };
    project.len() >= 2
        && project.starts_with(|c: char| c.is_ascii_uppercase())
        && project
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        && !number.is_empty()
        && number.chars().all(|c| c.is_ascii_digit())
}

/// The words of `text` that are keys of the usual shape (`is_key`), by their bytes: what a tracker's `find` may
/// answer with when its keys look like `PAY-412`.
pub fn scan(text: &str) -> Vec<Range<usize>> {
    let mut found = Vec::new();
    let mut start = None;
    let end = text.len();
    for (i, c) in text.char_indices().chain(std::iter::once((end, ' '))) {
        let word = c.is_ascii_alphanumeric() || c == '-';
        match (word, start) {
            (true, None) => start = Some(i),
            (false, Some(from)) => {
                start = None;
                if is_key(&text[from..i]) {
                    found.push(from..i);
                }
            }
            _ => {}
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_found_by_their_bytes_and_other_words_are_left() {
        let text = "Fixes PAY-412: and (AB1-7), not pay-1 or X-1 or PAY-";
        let words: Vec<&str> = scan(text).into_iter().map(|r| &text[r]).collect();
        assert_eq!(words, ["PAY-412", "AB1-7"]);
    }

    struct Fixed;

    impl Tracker for Fixed {
        fn name(&self) -> String {
            "Jira".into()
        }
        fn find(&self, text: &str, _: &App) -> Vec<Range<usize>> {
            scan(text)
        }
        fn card(&self, key: &str, _: &App) -> Option<IssueCard> {
            (key == "PAY-412").then(IssueCard::default)
        }
        fn open(&self, _: &str, _: &mut App) {}
    }

    #[test]
    fn without_a_tracker_a_key_is_text_and_with_one_a_link() {
        gpui::TestAppContext::single().update(|cx| {
            assert!(links(cx, "Fixes PAY-412").is_empty());
            add(cx, Rc::new(Fixed));
            let found = links(cx, "Fixes PAY-412");
            assert_eq!(found, [(6..13, "artoo-issue:PAY-412".to_owned())]);
            assert_eq!(key_of(&found[0].1), Some("PAY-412"));
            assert!(open(cx, "PAY-412"));
            assert!(!open(cx, "PAY-999"));
        });
    }
}
