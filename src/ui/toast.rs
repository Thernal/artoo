//! Errors as desktop notifications, the tab's side: the toasts are the kit's (`artoo_ui_kit::toast`). Code still
//! reports an error by setting `Workspace::error`; each frame moves it there. A deleted draft's toast says so and
//! offers Undo instead (#76); what an agent left offers to go and see it (`Act`, #150).

use super::Workspace;
use artoo_ui_kit::toast::Toasts;
use gpui::{App, Context, Entity, SharedString, prelude::*};
use std::cell::Cell;
use std::rc::Rc;

/// What a toast's button does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Act {
    /// Opens request `iid` at the first pending comment `agent` left on it (#150).
    Review { iid: u64, agent: String },
    /// Opens request `iid`, the first of those `agent` gathered under Merge requests (#148).
    Gathered { iid: u64, agent: String },
    /// Shows the comparison or the working tree `agent` opened under Branches (#149).
    Show {
        item: super::nav::Item,
        unstaged: Option<bool>,
        agent: String,
    },
    /// Opens the New merge request card for branch `source` with the request `agent` drafted
    /// (#152).
    Drafted { source: String, agent: String },
    /// Opens under Branches the changes `agent` left notes on, and shows them (#151).
    Notes {
        page: super::nav::Page,
        unstaged: Option<bool>,
        agent: String,
    },
    /// Opens Settings → Merge requests, where the way in to a host changes (#156).
    Connected,
    /// Goes to the place `agent` pointed at (#154).
    Point {
        place: crate::bus::Place,
        agent: String,
    },
}

impl Act {
    pub(super) fn label(&self) -> &'static str {
        match self {
            Act::Review { .. } | Act::Gathered { .. } => "Review",
            Act::Show { .. } | Act::Notes { .. } | Act::Point { .. } => "Show",
            Act::Drafted { .. } => "Open",
            Act::Connected => "Settings",
        }
    }
}

/// A tab's toasts, their buttons run by the tab.
pub(super) fn toasts(cx: &mut Context<Workspace>) -> Entity<Toasts<Act>> {
    let tab = cx.weak_entity();
    cx.new(|_| {
        Toasts::new(
            Act::label,
            Rc::new(move |act, cx| {
                tab.update(cx, |this, cx| this.do_toast_act(act, cx)).ok();
            }),
        )
    })
}

impl Workspace {
    /// Moves a reported error into a toast.
    pub(super) fn take_error(&mut self, cx: &mut Context<Self>) {
        if let Some(text) = self.error.take() {
            self.toasts.update(cx, |t, cx| t.show(text, false, cx));
        }
    }

    /// News that is not an error: a chat began or stopped listening (#77).
    pub(super) fn toast_info(&mut self, text: SharedString, cx: &mut Context<Self>) {
        self.toasts.update(cx, |t, cx| t.show(text, true, cx));
    }

    /// News with a button doing `act`, about `count` things: a toast of the same act still showing
    /// goes, its count added to this one's, and `say` words the sum (#150).
    pub(super) fn toast_act(
        &mut self,
        act: Act,
        count: usize,
        say: impl Fn(usize) -> String,
        cx: &mut Context<Self>,
    ) {
        let count = count + self.toasts.update(cx, |t, _| t.take_count(&act));
        let text = say(count);
        // An agent's news goes to the system's notifications too, the newer replacing the older
        // (#26).
        if act != Act::Connected {
            let tag = format!("{act:?}");
            let open = super::system_notes::Open::Act(act.clone());
            let project = self.tab_title();
            self.notify_system(tag, text.clone(), project, open, cx);
        }
        self.toasts
            .update(cx, |t, cx| t.show_act(text.into(), act, count, cx));
        cx.notify();
    }

    /// The toasts whose button does what `over` picks go now: news another has taken in.
    pub(super) fn toast_drop(&mut self, over: impl Fn(&Act) -> bool, cx: &mut Context<Self>) {
        self.toasts.update(cx, |t, cx| t.drop_acts(over, cx));
    }

    /// The news of `text` is over: its toast goes now.
    pub(super) fn toast_over(&mut self, text: &str, cx: &mut Context<Self>) {
        self.toasts.update(cx, |t, cx| t.drop_text(text, cx));
    }

    /// "Draft deleted · Money.kt:23" with Undo, which puts `comment` back.
    pub(super) fn toast_deleted(
        &mut self,
        comment: crate::comment::Comment,
        cx: &mut Context<Self>,
    ) {
        let name = comment.path.rsplit('/').next().unwrap_or(&comment.path);
        let text = format!("Draft deleted · {name}:{}", comment.line).into();
        let (tab, comment) = (cx.weak_entity(), Cell::new(Some(comment)));
        let undo = Rc::new(move |cx: &mut App| {
            if let Some(comment) = comment.take() {
                tab.update(cx, |this, cx| this.undo(comment, cx)).ok();
            }
        });
        self.toasts
            .update(cx, |t, cx| t.show_with(text, "Undo", undo, cx));
        cx.notify();
    }

    /// Undo: the deleted draft is back where it was.
    fn undo(&mut self, comment: crate::comment::Comment, cx: &mut Context<Self>) {
        self.change_comments(cx, |list| {
            list.push(comment);
            list.sort_by_key(|c| c.id);
        });
        self.save_comments(cx);
        self.refresh_rows(cx);
        cx.notify();
    }

    /// What a toast's button says to do, done: pressed, or its system notification clicked
    /// (#26).
    pub(super) fn do_toast_act(&mut self, act: Act, cx: &mut Context<Self>) {
        match act {
            Act::Review { iid, agent } => self.review_agent(iid, &agent, cx),
            Act::Gathered { iid, .. } => self.review_gathered(iid, cx),
            Act::Show { item, unstaged, .. } => self.show_changes(&item, unstaged, cx),
            Act::Notes { page, unstaged, .. } => self.show_agent_notes(page, unstaged, cx),
            Act::Drafted { source, .. } => self.open_agent_draft(source, cx),
            Act::Connected => self.open_requests_settings(cx),
            Act::Point { place, .. } => self.show_place(place, cx),
        }
    }

    /// The newest toast's button, pressed (the snapshot checks).
    #[cfg(feature = "snapshot")]
    pub(super) fn press_toast(&mut self, cx: &mut Context<Self>) {
        self.toasts.update(cx, |t, cx| t.press_newest(cx));
    }

    /// The stack, over everything else, under the title bar's right end.
    pub(super) fn render_toasts(&self, cx: &App) -> Option<impl IntoElement + use<>> {
        (!self.toasts.read(cx).is_empty()).then(|| self.toasts.clone())
    }
}
