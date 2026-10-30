//! Submit review (`artoo_requests::submit` keeps and draws its card): the tab opens it, gives it
//! the keyboard, sends what it submits and puts it in the tool window.

use super::Workspace;
use super::tools::ToolView;
use super::typing::{self, Input};
use artoo_requests::submit::{Submission, SubmitLook};
use gpui::{App, Context, KeyDownEvent, prelude::*};

impl Workspace {
    /// ⇧⌘↵ or "Submit review": the card, on Comment.
    pub(super) fn open_submit(&mut self, cx: &mut Context<Self>) {
        if !self.showing_request(cx) || !self.requests.update(cx, |rq, _| rq.open_submit_card()) {
            return;
        }
        self.take_keyboard(Input::Submit, cx);
        cx.notify();
    }

    pub(super) fn close_submit(&mut self, cx: &mut Context<Self>) {
        self.requests.update(cx, |rq, cx| rq.close_submit(cx));
    }

    /// Submit review's card is open.
    pub(super) fn submitting(&self, cx: &App) -> bool {
        self.requests.read(cx).submitting.is_some()
    }

    /// The card's keys (`Requests::submit_key`); ⌘↵ submits.
    pub(super) fn submit_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        if self.requests.update(cx, |rq, cx| rq.submit_key(event, cx)) {
            self.run_submit(cx);
        }
    }

    /// Sends what the card submits (`Requests::submission`). Refused, the card comes back saying
    /// why.
    pub(super) fn run_submit(&mut self, cx: &mut Context<Self>) {
        let Some(Submission {
            iid,
            ops,
            work,
            card,
        }) = self.requests.update(cx, |rq, cx| rq.submission(cx))
        else {
            return;
        };
        let refused = move |this: &mut Self, why: String, cx: &mut Context<Self>| {
            let back = this
                .requests
                .update(cx, |rq, _| rq.submit_refused(iid, card, why.clone()));
            if !back {
                this.error = Some(why.into());
            }
            cx.notify();
        };
        self.send_change(iid, ops, work, refused, cx);
    }

    /// Submit review, in the tool window while it is open (`Requests::submit_card`).
    pub(super) fn submit_tool(&self, cx: &mut Context<Self>) -> Option<ToolView> {
        let look = SubmitLook {
            active: self.has_keyboard(Input::Submit, cx),
            rich: self.settings.rich_comments,
            github: self.on_github(),
            agents: self.draft_agents(cx),
        };
        let acts = self.request_acts(cx);
        let (body, foot) = self
            .requests
            .update(cx, |rq, cx| rq.submit_card(look, acts, cx))?;
        Some(ToolView {
            title: "Submit review".into(),
            count: None,
            actions: Vec::new(),
            // A press anywhere in the window gives it the keyboard (#168).
            body: typing::input_box(body, Input::Submit, cx).into_any_element(),
            foot: Some(typing::input_box(foot, Input::Submit, cx).into_any_element()),
        })
    }
}
