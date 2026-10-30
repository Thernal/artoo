//! Merge, from the request's page (#164 B), the tab's side: what is in the way, listed in a menu with where each
//! is fixed; Merge's card opened with the keyboard (`artoo_requests::merging` draws it and keeps its message);
//! Rebase and Cancel asked of the host; and what follows a merge — a toast, the list read again.

use super::menu::{Act, Entry};
use super::typing::Input;
use super::{Workspace, px};
use crate::merge::{Fix, Merge, Stand};
use artoo_requests::merging::CardLook;
use gpui::{Context, KeyDownEvent, Pixels, Point, prelude::*};

/// The width of the list of reasons: each says what and why on one line.
const REASONS_WIDTH: f32 = 400.;

impl Workspace {
    /// The open repository's host is GitHub: its words in what an action writes, before a frame
    /// has said them (`words::github` is the workspace drawn last).
    pub(super) fn on_github(&self) -> bool {
        self.web.as_ref().is_some_and(|w| w.github)
    }

    /// A request's number as this repository's host writes it.
    fn reference(&self, iid: u64) -> String {
        artoo_requests::merging::reference(iid, self.on_github())
    }

    /// Reads where merging the current request stands again (`Requests::reload_merge`).
    pub(super) fn reload_merge(&mut self, cx: &mut Context<Self>) {
        let Some(host) = self.host(cx) else {
            return;
        };
        self.requests.update(cx, |rq, cx| rq.reload_merge(host, cx));
    }

    /// Merge's ▾: each reason in the way, and where it is fixed.
    pub(super) fn open_merge_reasons(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(Stand::InTheWay(blocks)) = self.requests.read(cx).merge_stand() else {
            return;
        };
        let pipeline = self
            .requests
            .read(cx)
            .pipeline
            .as_ref()
            .map(|p| p.url.clone());
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
        self.open_titled(at, vec![(Some(title), entries)], cx);
        if let Some(menu) = self.ctx_menu.as_mut() {
            menu.width = REASONS_WIDTH;
        }
    }

    /// Goes where a reason in the way is fixed.
    pub(super) fn go_fix(&mut self, fix: Fix, cx: &mut Context<Self>) {
        match fix {
            Fix::Nowhere => {}
            Fix::Discussions => self.set_tool(Some(super::tools::Tool::Discussions), cx),
            Fix::Page(url) => cx.open_url(&url),
        }
    }

    /// Merge… or ⌘K's Merge this merge request: the card, the project's defaults filled in; while
    /// something is in the way, the list of what.
    pub(super) fn open_merge(&mut self, cx: &mut Context<Self>) {
        let when = match self.requests.read(cx).merge_stand() {
            Some(Stand::Ready) => false,
            Some(Stand::Running) => true,
            Some(Stand::InTheWay(_)) => {
                let at = Point::new(px(self.win_width), px(84.));
                return self.open_merge_reasons(at, cx);
            }
            _ => return,
        };
        let github = self.on_github();
        if self
            .requests
            .update(cx, |rq, _| rq.open_merge_card(when, github))
        {
            self.take_keyboard(Input::Merge, cx);
            cx.notify();
        }
    }

    /// The card's keys (`Requests::merge_key`); ⌘↵ merges.
    pub(super) fn merge_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        if self.requests.update(cx, |rq, cx| rq.merge_key(event, cx)) {
            self.run_merge(cx);
        }
    }

    /// Sends the merge (`Requests::run_merge`).
    pub(super) fn run_merge(&mut self, cx: &mut Context<Self>) {
        let Some(host) = self.host(cx) else {
            return;
        };
        self.requests.update(cx, |rq, cx| rq.run_merge(host, cx));
    }

    /// A push came after the version on the page: the card goes, the request is read again and
    /// its branch fetched, so the new head shows as its version.
    pub(super) fn show_new_version(&mut self, cx: &mut Context<Self>) {
        self.requests.update(cx, |rq, _| {
            rq.merging = None;
            rq.merge = None;
        });
        self.load_requests(cx);
        self.fetch_now(cx);
        cx.notify();
    }

    /// The host merged it, or set it to merge when the pipeline succeeds: a toast says which, and
    /// the list is read again — a request merged moves to Merged, as one merged elsewhere does.
    pub(super) fn merged(&mut self, iid: u64, merge: &Merge, cx: &mut Context<Self>) {
        let here = self.requests.read(cx).current == Some(iid);
        if merge.when_pipeline_succeeds {
            let text = format!("{} merges when the pipeline succeeds", self.reference(iid));
            self.toast_info(text.into(), cx);
            self.requests.update(cx, |rq, _| {
                if let Some(host) = rq.merge.as_mut().filter(|_| here) {
                    host.auto_set = true;
                }
            });
            return self.reload_merge(cx);
        }
        let deletes = here
            && self
                .requests
                .read(cx)
                .merge
                .as_ref()
                .is_some_and(|h| h.deletes_itself);
        if let Some(m) = self.requests.read(cx).find(iid) {
            let deleted = match merge.delete_branch || deletes {
                true => format!(" · {} deleted", m.source),
                false => String::new(),
            };
            let text = format!("{} merged into {}{deleted}", self.reference(iid), m.target);
            self.toast_info(text.into(), cx);
        }
        if here {
            self.requests.update(cx, |rq, _| rq.merge = None);
        }
        self.load_requests(cx);
    }

    /// Rebase: GitLab puts the commits on top of the target, and the pipeline runs again.
    pub(super) fn rebase_request(&mut self, cx: &mut Context<Self>) {
        self.merge_call("Rebasing…", |host, iid, _| host.rebase(iid), cx);
    }

    /// Cancel: it no longer merges when the pipeline succeeds.
    pub(super) fn cancel_auto_merge(&mut self, cx: &mut Context<Self>) {
        self.merge_call(
            "Cancelling…",
            |host, iid, node| host.cancel_auto_merge(iid, node),
            cx,
        );
    }

    /// A call about merging the current request (`Requests::merge_call`).
    fn merge_call(
        &mut self,
        busy: &'static str,
        call: impl FnOnce(&dyn artoo_hosts_api::Host, u64, &str) -> anyhow::Result<()> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = self.host(cx) else {
            return;
        };
        self.requests
            .update(cx, |rq, cx| rq.merge_call(busy, host, call, cx));
    }

    /// The card before a merge goes, over everything (`Requests::render_merge_card`).
    pub(super) fn render_merge_card(&self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        let look = CardLook {
            commits: self.commits.len(),
            github: self.on_github(),
            active: self.has_keyboard(Input::Merge, cx),
        };
        let acts = self.request_acts(cx);
        self.requests.update(cx, |rq, cx| {
            rq.render_merge_card(look, acts, cx)
                .map(IntoElement::into_any_element)
        })
    }
}
