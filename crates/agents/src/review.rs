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
