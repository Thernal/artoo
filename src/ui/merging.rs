//! Merge, from the request's page (#164 B), the tab's side: what is in the way, listed in a menu with where each
//! is fixed; Merge's card opened with the keyboard (`artoo_requests::merging` draws it and keeps its message);
//! Rebase and Cancel asked of the host; and what follows a merge — a toast, the list read again.

use super::menu::{Act, Entry};
use super::typing::Input;
use super::{Workspace, px};
use crate::mr::Merge;
use artoo_requests_api::{CardLook, Fix, Stand};
use artoo_ui_kit::menu::Lead;
use artoo_ui_kit::theme;
use gpui::{Context, KeyDownEvent, Pixels, Point};

/// The width of the list of reasons: each says what and why on one line.
const REASONS_WIDTH: f32 = 320.;

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
        let entries = blocks
            .into_iter()
            .map(|b| {
                let (icon, value) = reason_look(b.what, &b.detail);
                // A failing pipeline is a pill in red; the others are quiet facts.
                let tone = (b.what == "Pipeline").then(theme::removed);
                let go = match (&b.fix, b.what) {
                    (Fix::Nowhere, _) => None,
                    (Fix::Discussions, _) => Some(format!("Show the {value} discussions")),
                    (Fix::Page(_), "Pipeline") => Some(format!("Open the {value} job")),
                    (Fix::Page(_), _) => Some(format!("Open the {}", b.what.to_lowercase())),
                };
                let entry = Entry::new(b.what, {
                    let fix = b.fix;
                    Act::new(move |this, _, cx| this.go_fix(fix.clone(), cx))
                })
                .lead(Lead::Icon(icon, theme::muted()))
                .value(icon, value, b.detail, tone);
                match go {
                    Some(tip) => entry.go(tip),
                    None => entry.go_blank(),
                }
            })
            .collect();
        // A list of facts under its heading, a glyph and a short value each (Design M1).
        self.ctx_menu.update(cx, |m, cx| {
            m.open_plain(
                at,
                vec![(Some("In the way of merging"), entries)],
                REASONS_WIDTH,
                "The host asks again on every push",
                cx,
            )
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

    /// The host merged it, or set it to merge when the pipeline succeeds: a toast says which. A request merged keeps
    /// its page and its row as they are, only its state changing (round 41); the next read moves it on.
    pub(super) fn merged(&mut self, iid: u64, merge: &Merge, cx: &mut Context<Self>) {
        let Some(requests) = self.requests_api() else {
            return;
        };
        if let Some(text) = requests.merged(iid, merge, self.on_github(), cx) {
            self.toast_info(text.into(), cx);
        }
        if merge.when_pipeline_succeeds {
            self.reload_merge(cx);
        }
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

/// A reason in the way as a row says it: its glyph and a value short enough for one line — `1/2`, `2`, `lint` —,
/// else its words; the whole sentence is the Merge button's tooltip.
fn reason_look(what: &str, detail: &str) -> (&'static str, String) {
    let first = |text: &str| {
        text.split_whitespace()
            .next()
            .unwrap_or_default()
            .to_owned()
    };
    match what {
        "Approvals" => {
            let nums: Vec<&str> = detail
                .split(|c: char| !c.is_ascii_digit())
                .filter(|n| !n.is_empty())
                .collect();
            match nums.as_slice() {
                [have, required, ..] => ("circle-check", format!("{have}/{required}")),
                _ => ("circle-check", detail.to_owned()),
            }
        }
        "Discussions" => ("message", first(detail)),
        "Pipeline" => ("circle-x", first(detail)),
        _ => ("circle-slash", detail.to_owned()),
    }
}
