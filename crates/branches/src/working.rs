//! The working tree as last looked at, polled while a repository is open.

use artoo_core::fswatch::Changes;
use std::rc::Rc;

/// The working tree's count and fingerprint, polled while a repository is open.
#[derive(Default)]
pub struct WorkingState {
    pub count: usize,
    pub hash: u64,
    pub only_unstaged: bool,
    pub task: Option<gpui::Task<()>>,
    /// What the file system says changed, so the poll asks git only then (#87).
    pub changes: Option<Rc<Changes>>,
}
