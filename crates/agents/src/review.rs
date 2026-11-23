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

#[allow(dead_code, reason = "the composer (#220) saves them")]
fn messages_store(root: &Path) -> String {
    format!("messages/{}.json", key(root))
}

/// The reviewer's messages to the sessions (#220), kept beside the comments.
#[allow(dead_code, reason = "the conversation's tab (#220) reads them")]
pub fn load_messages(root: &Path) -> Vec<artoo_agents_api::conversation::Message> {
    serde_json::from_str(&crate::storage::read(&messages_store(root))).unwrap_or_default()
}

#[allow(dead_code, reason = "the composer (#220) saves them")]
pub fn save_messages(root: &Path, messages: &[artoo_agents_api::conversation::Message]) {
    if let Ok(body) = serde_json::to_string_pretty(messages) {
        crate::storage::save(&messages_store(root), body);
    }
}
