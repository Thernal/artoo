//! Merge, from the request's page (#164 B), the tab's side: what is in the way, listed in a menu with where each
//! is fixed; Merge's card opened with the keyboard (`artoo_requests::merging` draws it and keeps its message);
//! Rebase and Cancel asked of the host; and what follows a merge — a toast, the list read again.

use super::menu::{Act, Entry};
use super::typing::Input;
use super::{Workspace, px};
use crate::mr::Merge;
use artoo_requests_api::{CardLook, Fix, Stand};
use gpui::{Context, KeyDownEvent, Pixels, Point};

/// The width of the list of reasons: each says what and why on one line.
const REASONS_WIDTH: f32 = 400.;

impl Workspace {
    /// The open repository's host is GitHub: its words in what an action writes, before a frame
    /// has said them (`words::github` is the workspace drawn last).
    pub(super) fn on_github(&self) -> bool {
        self.web.as_ref().is_some_and(|w| w.github)
    }

    /// Reads where merging the current request stands again (`Requests::reload_merge`).
    pub(super) fn reload_merge(&mut self, cx: &mut Context<Self>) {
        if let Some(requests) = self.requests_api() {
            requests.reload_merge(cx);
        }
    }

    /// Merge's ▾: each reason in the way, and where it is fixed.
    pub(super) fn open_merge_reasons(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(requests) = self.requests_api() else {
            return;
        };
        let Some(Stand::InTheWay(blocks)) = requests.merge_stand(cx) else {
            return;
        };
        let pipeline = requests.pipeline_url(cx);
        let github = self.on_github();
        let host = if github { "GitHub →" } else { "GitLab →" };
        let entries = blocks
            .into_iter()
            .map(|b| {
                let key = match &b.fix {
                    Fix::Nowhere => "",
                    Fix::Discussions => "Discussions →",
                    Fix::Page(url) if Some(url) == pipeline.as_ref() => "Pipeline →",
                    Fix::Page(_) => host,
                };
                Entry::new(b.what, Act::MergeFix(b.fix))
                    .detail(b.detail)
                    .key(key)
            })
            .collect();
        let title = if github {
            "In the way · GitHub asks again on every push"
        } else {
            "In the way · GitLab asks again on every push"
        };
        self.ctx_menu.update(cx, |m, cx| {
            m.open(at, vec![(Some(title), entries)], REASONS_WIDTH, cx)
        });
    }

    /// Goes where a reason in the way is fixed.
    pub(super) fn go_fix(&mut self, fix: Fix, cx: &mut Context<Self>) {
        match fix {
            Fix::Nowhere => {}
            Fix::Discussions => self.set_tool(Some(artoo_requests_api::DISCUSSIONS), cx),
            Fix::Page(url) => cx.open_url(&url),
        }
    }

    /// Merge… or ⌘K's Merge this merge request: the card, the project's defaults filled in; while
    /// something is in the way, the list of what.
    pub(super) fn open_merge(&mut self, cx: &mut Context<Self>) {
        let Some(requests) = self.requests_api() else {
            return;
        };
        let when = match requests.merge_stand(cx) {
            Some(Stand::Ready) => false,
            Some(Stand::Running) => true,
            Some(Stand::InTheWay(_)) => {
                let at = Point::new(px(self.win_width), px(84.));
                return self.open_merge_reasons(at, cx);
            }
            _ => return,
        };
        let github = self.on_github();
        if requests.open_merge_card(when, github, cx) {
            self.take_keyboard(Input::Merge, cx);
            cx.notify();
        }
    }

    /// The card's keys (`Requests::merge_key`); ⌘↵ merges.
    pub(super) fn merge_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        if self.requests_api().is_some_and(|r| r.merge_key(event, cx)) {
            self.run_merge(cx);
        }
    }

    /// Sends the merge (`Requests::run_merge`).
    pub(super) fn run_merge(&mut self, cx: &mut Context<Self>) {
        if let Some(requests) = self.requests_api() {
            requests.run_merge(cx);
        }
    }

    /// A push came after the version on the page: the card goes, the request is read again and
    /// its branch fetched, so the new head shows as its version.
    pub(super) fn show_new_version(&mut self, cx: &mut Context<Self>) {
        if let Some(requests) = self.requests_api() {
            requests.drop_merge(cx);
        }
        self.load_requests(cx);
        self.fetch_now(cx);
        cx.notify();
    }

    /// The host merged it, or set it to merge when the pipeline succeeds: a toast says which, and
    /// the list is read again — a request merged moves to Merged, as one merged elsewhere does.
    pub(super) fn merged(&mut self, iid: u64, merge: &Merge, cx: &mut Context<Self>) {
        let Some(requests) = self.requests_api() else {
            return;
        };
        if let Some(text) = requests.merged(iid, merge, self.on_github(), cx) {
            self.toast_info(text.into(), cx);
        }
        if merge.when_pipeline_succeeds {
            return self.reload_merge(cx);
        }
        self.load_requests(cx);
    }

    /// Rebase: GitLab puts the commits on top of the target, and the pipeline runs again.
    pub(super) fn rebase_request(&mut self, cx: &mut Context<Self>) {
        if let Some(requests) = self.requests_api() {
            requests.rebase(cx);
        }
    }

    /// Cancel: it no longer merges when the pipeline succeeds.
    pub(super) fn cancel_auto_merge(&mut self, cx: &mut Context<Self>) {
        if let Some(requests) = self.requests_api() {
            requests.cancel_auto_merge(cx);
        }
    }

    /// The card before a merge goes, over everything (`Requests::render_merge_card`).
    pub(super) fn render_merge_card(&self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        let look = CardLook {
            commits: self.commit_count(cx),
            github: self.on_github(),
            active: self.has_keyboard(Input::Merge, cx),
        };
        self.requests_api()?.merge_card(look, cx)
    }
}
