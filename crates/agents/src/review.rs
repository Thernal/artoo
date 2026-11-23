//! The comments for a coding agent, kept per repository on this Mac (never in the repository); their model is the
//! contract's (`artoo_agents_api::comment`).

pub use artoo_agents_api::comment::{
    Answered, Comment, Form, FormKind, Kind, Place, Reply, State, key, kind_of, place,
};
use std::path::Path;

fn store(root: &Path) -> String {
    format!("comments/{}.json", key(root))
}

pub fn load(root: &Path) -> Vec<Comment> {
    serde_json::from_str(&crate::storage::read(&store(root))).unwrap_or_default()
}

pub fn save(root: &Path, comments: &[Comment]) {
    if let Ok(body) = serde_json::to_string_pretty(comments) {
        crate::storage::save(&store(root), body);
    }
}

fn messages_store(root: &Path) -> String {
    format!("messages/{}.json", key(root))
}

/// The reviewer's messages to the sessions (#220), kept beside the comments.
pub fn load_messages(root: &Path) -> Vec<artoo_agents_api::conversation::Message> {
    serde_json::from_str(&crate::storage::read(&messages_store(root))).unwrap_or_default()
}

pub fn save_messages(root: &Path, messages: &[artoo_agents_api::conversation::Message]) {
    if let Ok(body) = serde_json::to_string_pretty(messages) {
        crate::storage::save(&messages_store(root), body);
    }
}

/// `text` as a message to `session`, after `kept`: its id one past the greatest kept. None for words that are
/// only blanks.
pub fn next_message(
    kept: &[artoo_agents_api::conversation::Message],
    session: &str,
    text: &str,
    at: i64,
) -> Option<artoo_agents_api::conversation::Message> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    Some(artoo_agents_api::conversation::Message {
        id: kept.iter().map(|m| m.id).max().unwrap_or(0) + 1,
        session: session.to_owned(),
        text: text.to_owned(),
        at,
        ..Default::default()
    })
}

/// The composer's Send: `text` kept as the reviewer's message to `session`, alone — the comments not sent stay in
/// Not sent. Whether there was anything to send.
pub fn send_message(root: &Path, session: &str, text: &str, at: i64) -> bool {
    let mut kept = load_messages(root);
    let Some(message) = next_message(&kept, session, text, at) else {
        return false;
    };
    kept.push(message);
    save_messages(root, &kept);
    true
}

/// Send all's message: `text` to go with `carried`, the comments sent together with it, kept as the reviewer's
/// message to `session`. Blank words keep no message, the comments go as they are.
pub fn send_all_message(root: &Path, session: &str, text: &str, carried: &[u64], at: i64) -> bool {
    let mut kept = load_messages(root);
    let Some(mut message) = next_message(&kept, session, text, at) else {
        return false;
    };
    message.carried = carried.to_vec();
    kept.push(message);
    save_messages(root, &kept);
    true
}

#[cfg(test)]
mod message_tests {
    use super::*;

    #[test]
    fn a_message_takes_the_next_id_and_blanks_are_none() {
        let first = next_message(&[], "s1", "  Take the login first.  ", 10).unwrap();
        assert_eq!(
            (first.id, first.text.as_str()),
            (1, "Take the login first.")
        );
        assert!(first.carried.is_empty(), "alone: no comment goes with it");
        let second =
            next_message(std::slice::from_ref(&first), "s1", "Then the logout.", 11).unwrap();
        assert_eq!(second.id, 2);
        assert!(next_message(&[first], "s1", " \n ", 12).is_none());
    }
}
