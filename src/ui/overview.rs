//! What Overview's composers are the tab's to do (#241): the one at the end of its discussions — drawn empty
//! until it is written in —, + on a paragraph of the description for a note to the agent, and that note kept as a
//! draft on its place (`requests.description`).

use super::Workspace;
use super::composer::{Compose, ComposeAct, FootEvent, Target};
use super::typing::Input;
use super::{discussion, px};
use crate::textedit::TextEdit;
use artoo_agents_api::places;
use artoo_requests_api::{OverviewComposer, OverviewComposers};
use artoo_ui_kit::{icons, theme, words};
use gpui::{AnyElement, App, Context, MouseButton, SharedString, WeakEntity, div, prelude::*};
use std::rc::Rc;

/// A composer of Overview, empty: at its discussions' end, or on a paragraph.
fn blank(target: Target) -> Compose {
    Compose {
        path: String::new(),
        old: false,
        first: None,
        line: 0,
        code: String::new(),
        edit: TextEdit::default(),
        at: String::new(),
        target,
        error: None,
        preview: false,
        pick: 0,
        closed: None,
        page: None,
        file: None,
        answers: None,
        attachments: Vec::new(),
        emoji: None,
    }
}

/// The tab `this`'s composers on a request's Overview.
pub(super) fn overview_composers(this: WeakEntity<Workspace>) -> OverviewComposers {
    OverviewComposers(Rc::new(move |iid, cx| {
        let Some(tab) = this.upgrade() else {
            return OverviewComposer::default();
        };
        tab.read(cx).overview_composer(iid, this.clone(), cx)
    }))
}

impl Workspace {
    /// What a comment at Overview's end says while empty.
    pub(super) fn first_words(&self, cx: &App) -> SharedString {
        match self.request_threads(cx).is_empty() && self.request_drafts(cx).is_empty() {
            true => "Start the first discussion".into(),
            false => "Start a discussion".into(),
        }
    }

    fn overview_composer(&self, iid: u64, this: WeakEntity<Self>, cx: &App) -> OverviewComposer {
        // Nothing to post to: no host, or its token refused.
        if !self.host.as_ref().is_some_and(|h| h.available()) {
            return OverviewComposer::default();
        }
        let compose = self.compose.as_ref();
        let paragraph = match compose.map(|c| c.target) {
            Some(Target::Overview(i, k)) if i == iid && k > 0 => self
                .compose_box(this.clone(), cx)
                .map(|boxed| (k, boxed.into_any_element())),
            _ => None,
        };
        let end = match compose.and_then(Compose::overview) {
            Some(i) if i == iid => self
                .compose_box(this, cx)
                .map(|boxed| self.beside_me(boxed.into_any_element())),
            _ => Some(self.beside_me(self.idle_composer(iid, this, cx))),
        };
        OverviewComposer { end, paragraph }
    }

    /// The composer at the section's end beside my face.
    fn beside_me(&self, boxed: AnyElement) -> AnyElement {
        div()
            .flex()
            .items_start()
            .gap(px(12.))
            .child(discussion::avatar(&self.me, true, 28.).mt(px(3.)))
            .child(div().flex_1().min_w_0().flex().child(boxed))
            .into_any_element()
    }

    /// The composer at the section's end before anything is written in it: the same box, without the keyboard;
    /// a press anywhere on it starts the comment, and what was pressed is done.
    fn idle_composer(&self, iid: u64, this: WeakEntity<Self>, cx: &App) -> AnyElement {
        let idle = blank(Target::Request(iid));
        let on = this.clone();
        let foot: artoo_ui_kit::composer::OnFoot = Rc::new(move |event, cx| {
            on.update(cx, |this, cx| {
                this.start_overview_comment(iid, cx);
                if matches!(event, FootEvent::Agent) {
                    this.foot_event(event, cx);
                }
            })
            .ok();
        });
        let on = this.clone();
        let act: super::composer::ComposeActs = Rc::new(move |act, cx| {
            on.update(cx, |this, cx| {
                this.start_overview_comment(iid, cx);
                match act {
                    ComposeAct::Helper(h) => this.apply_helper(h, cx),
                    ComposeAct::Emoji => this.open_compose_emoji(cx),
                    ComposeAct::Preview => this.toggle_preview(cx),
                    ComposeAct::Clip => this.pick_attachment(cx),
                    ComposeAct::Dropped(paths) => this.attach_files(paths, cx),
                    _ => {}
                }
            })
            .ok();
        });
        let boxed = self.new_comment_box(&idle, foot, act, cx);
        div()
            .id("overview-composer-idle")
            .flex_1()
            .min_w_0()
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                this.update(cx, |this, cx| this.start_overview_comment(iid, cx))
                    .ok();
            })
            .child(boxed)
            .into_any_element()
    }

    /// A comment at the end of request `iid`'s Overview, for everyone on it until ⇥; the one already open stays.
    pub(super) fn start_overview_comment(&mut self, iid: u64, cx: &mut Context<Self>) {
        if self.compose.as_ref().and_then(Compose::overview) != Some(iid) {
            self.open_overview(blank(Target::Request(iid)), cx);
        }
    }

    /// + on paragraph `k` of the description on screen.
    pub(super) fn start_paragraph_note(&mut self, k: u32, cx: &mut Context<Self>) {
        let Some(iid) = self.current_request(cx) else {
            return;
        };
        let target = Target::Overview(iid, k);
        if self.compose.as_ref().map(|c| c.target) != Some(target) {
            self.open_overview(blank(target), cx);
        }
    }

    fn open_overview(&mut self, compose: Compose, cx: &mut Context<Self>) {
        self.compose = Some(compose);
        self.take_keyboard(Input::Compose, cx);
        self.reset_field_edit();
        self.set_inbox_reply(None, cx);
        self.refresh_rows(cx);
        self.rv().remeasure_pages(cx);
        cx.notify();
    }

    /// The place a note on paragraph `k` is on, as it reads: `!398 · description · ¶1`.
    pub(super) fn paragraph_chip(&self, iid: u64, k: u32) -> AnyElement {
        div()
            .h(px(22.))
            .px(px(8.))
            .flex()
            .items_center()
            .gap(px(6.))
            .rounded(px(6.))
            .bg(theme::selected())
            .text_size(px(11.))
            .text_color(theme::muted())
            .child(icons::icon("pin").size(px(12.)).text_color(theme::muted()))
            .child(words::say(&format!("!{iid} · description · ¶{k}")).into_owned())
            .into_any_element()
    }

    /// ⌘↵ on a note on request `iid`'s Overview: kept as a draft on its place, paragraph `k` of the description
    /// or with 0 the request. Whether it was.
    pub(super) fn add_overview_note(&mut self, iid: u64, k: u32, cx: &mut Context<Self>) -> bool {
        let Some(compose) = self.compose.as_ref() else {
            return false;
        };
        let body = compose.edit.text().trim().to_owned();
        if body.is_empty() || compose.waiting() {
            return false;
        }
        let data = (self.requests_api()).and_then(|r| r.description_place(iid, k, cx));
        let Some(on) =
            data.and_then(|data| places::on(cx, artoo_requests_api::DESCRIPTION_PLACE, &data))
        else {
            return false;
        };
        let mut note = places::note(on, body);
        note.branch = self.page_branch(cx);
        self.agents.add_comment(note, cx);
        self.compose = None;
        self.copied = false;
        self.refresh_rows(cx);
        cx.notify();
        true
    }
}
