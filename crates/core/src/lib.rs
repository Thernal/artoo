//! Artoo's core: git, highlighting, the platform layer and what is remembered between runs — nothing that draws (#165).

pub mod anchor;
pub mod applog;
pub mod codeowners;
pub mod editor;
pub mod emoji;
pub mod files;
pub mod fswatch;
pub mod generated;
pub mod git;
pub mod highlight;
pub mod labels;
pub mod links;
pub mod markdown;
pub mod mddiff;
pub mod mr;
pub mod notice;
pub mod platform;
pub mod rank;
pub mod route;
pub mod rows;
pub mod search;
pub mod select;
pub mod storage;
pub mod structural;
pub mod symbols;
pub mod textedit;
pub mod themes;
pub mod worddiff;

/// The commit this build is made from, as the app said it when it started.
static COMMIT: std::sync::OnceLock<&'static str> = std::sync::OnceLock::new();

/// The app says which commit it is built from (its `build.rs`), before anything asks.
pub fn set_commit(commit: &'static str) {
    COMMIT.set(commit).ok();
}

/// The commit this build is made from; "unknown" where no app said it (a crate's own tests).
pub fn commit() -> &'static str {
    COMMIT.get().copied().unwrap_or("unknown")
}

/// "Artoo 0.2.0 (7c5758f)": which build it is.
pub fn version() -> String {
    format!("Artoo {} ({})", env!("CARGO_PKG_VERSION"), commit())
}
