//! Coming back to a request after a push (#187, Design A1): the version of it you read — the push your newest mark
//! of it was made at, when a newer one came since —, and what each of its ranges shows: `All` its whole diff, `Since`
//! that version against the latest, `Commits` one commit at a time, from the first after it.

use crate::history::{History, Selection};
use artoo_branches_api::{Range, RangeOn};
use artoo_core::route::{Place, Route};
use artoo_requests_api::{RequestCommitRoute, RequestRoute, RequestVersionsRoute};
use artoo_review_api::diff::Header;
use artoo_review_api::{Review, Seen};
use gpui::App;

/// The version you read of a request, kept while it stays open: (its number, the version's tip).
pub type KeptRead = (u64, git2::Oid);

impl History {
    /// The version of the open request you read (its index in `versions`), when a newer one came since: the one
    /// kept when it opened (`keep_read`), else the head your newest mark of it was made at.
    pub fn read_version(&self, review: &dyn Review, cx: &App) -> Option<usize> {
        let iid = self.open_request.as_ref()?.2;
        let latest = self.versions.len().checked_sub(1).filter(|&l| l > 0)?;
        let tip = match self.kept_read {
            Some((of, tip)) if of == iid => tip,
            _ => {
                let head = Some(self.versions[latest].tip);
                if review.request_seen(iid, head, cx) != Seen::Earlier {
                    return None;
                }
                review.reviewed_head(iid, head, cx)?
            }
        };
        self.versions
            .iter()
            .position(|v| v.tip == tip)
            .filter(|&ix| ix < latest)
    }

    /// The version you read of the open request kept as it is now, so reading what changed since it does not take
    /// its `Since` away while the request stays open.
    pub fn keep_read(&mut self, review: &dyn Review, cx: &App) {
        let iid = self.open_request.as_ref().map(|r| r.2);
        self.kept_read = None;
        let read = self.read_version(review, cx);
        self.kept_read = iid.zip(read.map(|ix| self.versions[ix].tip));
    }

    /// What the open request shows of itself on `route` (its page, one under its Changes, a commit under its
    /// Commits), as the diff shown says: the range, and the number of the version you read.
    pub fn range_on(&self, route: &Route, review: &dyn Review, cx: &App) -> Option<RangeOn> {
        let iid = self.open_request.as_ref()?.2;
        let diff = review.shown(cx)?;
        let read = self.read_version(review, cx);
        let on = if let Some(r) = RequestRoute::from_route(route) {
            let whole = matches!(diff.header, Header::Compare { request: Some(n), .. } if n == iid);
            (r.number == iid && whole).then_some(Range::All)?
        } else if let Some(r) = RequestVersionsRoute::from_route(route) {
            let since = match &diff.header {
                Header::Versions { from, to, .. } => {
                    read.and_then(|ix| self.versions.get(ix))
                        .is_some_and(|v| v.tip == from.tip)
                        && self.versions.last().is_some_and(|v| v.tip == to.tip)
                }
                _ => false,
            };
            (r.number == iid && since && read == Some(r.from)).then_some(Range::Since)?
        } else {
            let r = RequestCommitRoute::from_route(route)?;
            (r.number == iid && matches!(diff.header, Header::Commit(_)))
                .then_some(Range::Commits)?
        };
        Some(RangeOn {
            on,
            read: read.map(|ix| self.versions[ix].number),
        })
    }

    /// What `range` of the open request shows: `Since` the version you read against the latest, `Commits` the first
    /// commit after that version, else its first. None for `All` (the request's own page), and where there is none.
    pub fn range_pick(&self, range: Range, review: &dyn Review, cx: &App) -> Option<Selection> {
        self.open_request.as_ref()?;
        let read = self.read_version(review, cx);
        match range {
            Range::All => None,
            Range::Since => Some(Selection::Versions {
                from: read?,
                to: self.versions.len() - 1,
            }),
            Range::Commits => {
                let oldest = self.commits.len().checked_sub(1)?;
                // The commits are newest first: those after the version read are above its tip.
                let after = read
                    .and_then(|ix| self.versions.get(ix))
                    .and_then(|v| self.commits.iter().position(|c| c.id == v.tip))
                    .and_then(|at| at.checked_sub(1));
                Some(Selection::Commit(after.unwrap_or(oldest)))
            }
        }
    }

    /// The paths of the open request's whole diff, read once for its sides: what `unchanged` counts against.
    pub fn read_whole(&mut self) {
        let (Some(root), Some((base, head, _))) = (self.root.as_ref(), self.open_request.as_ref())
        else {
            return;
        };
        let key = (base.1, head.1);
        if self.whole.as_ref().is_some_and(|(k, _)| *k == key) {
            return;
        }
        let paths = artoo_core::git::Repo::open(root).and_then(|r| r.paths_since(base.1, head.1));
        self.whole = paths.ok().map(|p| (key, p.len()));
    }

    /// How many files of the open request's whole diff are as they were at the version you read, when the diff shown
    /// (`files` listed) is what changed since it.
    pub fn unchanged(&self, files: usize, review: &dyn Review, cx: &App) -> Option<usize> {
        let (base, head, _) = self.open_request.as_ref()?;
        let Selection::Versions { from, to } = self.selection else {
            return None;
        };
        if self.read_version(review, cx) != Some(from) || to + 1 != self.versions.len() {
            return None;
        }
        let (key, whole) = self.whole.as_ref()?;
        (*key == (base.1, head.1)).then(|| whole.saturating_sub(files))
    }
}
