//! A page waiting for the branch open to be read (#71): a commit of it, a run of its commits, two of its versions
//! compared — of the merge request on it, when it is one's —, shown through the review's handle once what it asks
//! for is in. Where the page goes and what it keeps of it stay the tab's.

use crate::history::History;
use artoo_review_api::Review;
use git2::Oid;
use gpui::App;

/// What a page waiting asks of the branch or the request open.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pick {
    Commit(Oid),
    /// A run of commits, the newest and the oldest.
    Commits(Oid, Oid),
    /// Two versions compared, by their indexes.
    Versions(usize, usize),
}

/// A page waiting: what it asks, of the request whose number `request` is, or of the branch.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Waiting {
    pub request: Option<u64>,
    pub pick: Pick,
}

pub use artoo_branches_api::Finished;

impl History {
    /// Shows the page waiting once what it asks for is in: its request open, its commits listed, its versions
    /// read. `None` while it waits, when none does, or when its commits are gone (a push rewrote them): then its
    /// item's own page is shown instead, and nothing waits any more.
    pub fn finish_waiting(&mut self, review: &dyn Review, cx: &mut App) -> Option<Finished> {
        let waiting = self.waiting?;
        if waiting.request.is_some() && self.open_request.as_ref().map(|r| r.2) != waiting.request {
            return None;
        }
        let at = |id: Oid| self.commits.iter().position(|c| c.id == id);
        let finished = match waiting.pick {
            Pick::Commits(newest, oldest) => {
                let (Some(a), Some(b)) = (at(newest), at(oldest)) else {
                    self.waiting = None;
                    return None;
                };
                if a == b {
                    let commit = self.show_commit(a, None, review, cx);
                    Finished { commit }
                } else {
                    self.show_commits(a, b, None, review, cx);
                    Finished { commit: None }
                }
            }
            Pick::Commit(id) => {
                let Some(ix) = at(id) else {
                    self.waiting = None;
                    return None;
                };
                let commit = self.show_commit(ix, None, review, cx);
                Finished { commit }
            }
            Pick::Versions(from, to) if to < self.versions.len() => {
                self.show_versions(from, to, None, review, cx);
                Finished { commit: None }
            }
            Pick::Versions(..) => return None,
        };
        self.waiting = None;
        Some(finished)
    }
}
