//! Coming back to a request after a push (#187, Design A1): its Changes open on what changed since the version you
//! read (`Since v2`), and the switch `All · Since v2 · Commits` takes it there and back; `s` toggles All and Since.
//! What each range shows is the branches' (`Branches::range_pick`); the tab goes to its page.

use super::Workspace;
use super::nav::{Page, Section};
use artoo_branches_api::{Range, RangeOn, Selection};
use artoo_review_api::SinceRead;
use gpui::{App, Context};

impl Workspace {
    /// What the request on screen shows of itself: its range, and the version you read.
    pub(super) fn range_on(&self, cx: &App) -> Option<RangeOn> {
        self.branches()?.range(self.nav_page(), cx)
    }

    /// The request's own Changes are on screen — its whole diff, or what changed since the version you read —: its
    /// bar's actions and its keys (`a`, ⇧⌘↵) are there.
    pub(super) fn on_request_changes(&self, cx: &App) -> bool {
        self.showing_request(cx) || self.range_on(cx).is_some_and(|r| r.on == Range::Since)
    }

    /// What the review is told of the diff in when it is what changed since the version you read: that version's
    /// number, and how many of the request's files it leaves as they were.
    pub(super) fn since_read(&self, cx: &App) -> Option<SinceRead> {
        let on = self.range_on(cx).filter(|r| r.on == Range::Since)?;
        let files = self.shown_diff(cx)?.files.len();
        Some(SinceRead {
            version: on.read?,
            unchanged: self.branches()?.unchanged(files, cx),
        })
    }

    /// Range `range` of the request on screen: All its Changes, Since the version you read against the latest, its
    /// files listed; Commits its first commit after that version.
    pub(super) fn show_range(&mut self, range: Range, cx: &mut Context<Self>) {
        let Some(on) = self.range_on(cx) else {
            return;
        };
        if on.on == range {
            return;
        }
        match range {
            Range::All => self.show_section(Section::Changes, cx),
            Range::Since | Range::Commits => {
                let Some(pick) = self.branches().and_then(|b| b.range_pick(range, cx)) else {
                    return;
                };
                self.pick(pick, cx);
                if let (Selection::Versions { .. }, Some(branches)) = (pick, self.branches()) {
                    branches.show_version_files(cx);
                }
            }
        }
        cx.notify();
    }

    /// `s`: All and Since the version you read, in turn, on a request's Changes.
    pub(super) fn toggle_since(&mut self, cx: &mut Context<Self>) {
        let Some(on) = self.range_on(cx).filter(|r| r.read.is_some()) else {
            return;
        };
        self.show_range(
            if on.on == Range::Since {
                Range::All
            } else {
                Range::Since
            },
            cx,
        );
    }

    /// A request opened anew on its Changes, once its whole diff and its versions are in: the version you read is
    /// kept, and when a push came after it, the page is what changed since — in the place of its whole diff, so
    /// Back goes where it was opened from.
    pub(super) fn offer_since(&mut self, cx: &mut Context<Self>) {
        let Some(iid) = self.since_offer else {
            return;
        };
        if self.shown_page() != Some(Page::Request(iid)) {
            self.since_offer = None;
            return;
        }
        let versions_in = (self.requests_api())
            .and_then(|r| r.versions(cx))
            .is_some_and(|(of, _)| of == iid);
        if !versions_in || !self.request_loaded(iid, cx) {
            return;
        }
        self.since_offer = None;
        let Some(branches) = self.branches() else {
            return;
        };
        branches.keep_read(cx);
        if self.page_section(cx) != Some(Section::Changes) {
            return;
        }
        let Some(pick @ Selection::Versions { .. }) = branches.range_pick(Range::Since, cx) else {
            return;
        };
        let whole = Page::Request(iid).route();
        self.pick(pick, cx);
        branches.show_version_files(cx);
        self.nav.forget_before(&whole);
        cx.notify();
    }
}
