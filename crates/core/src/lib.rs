//! Artoo's core: git, highlighting, the platform layer and what is remembered between runs — nothing that draws (#165).

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
pub mod platform;
pub mod rank;
pub mod search;
pub mod storage;
pub mod structural;
pub mod symbols;
pub mod textedit;
pub mod themes;
pub mod worddiff;

/// The commit this build is made from (`build.rs`).
pub const COMMIT: &str = env!("ARTOO_COMMIT");

/// "Artoo 0.2.0 (7c5758f)": which build it is.
pub fn version() -> String {
    format!("Artoo {} ({COMMIT})", env!("CARGO_PKG_VERSION"))
}
