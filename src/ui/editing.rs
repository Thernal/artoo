//! Editing a request that is there (Design round 46): its title where it stands (↵ saves, esc leaves it), its
//! description in place — kept on this Mac until saved or cancelled, ⌘↵ saves —, its state (`Draft ▾`), and an
//! agent's proposal (`edit_request`) shown over the description with Compare, Discard and Save: nothing is sent
//! to the host without the reviewer's press. Only where the host lets you edit.

use super::Workspace;
use super::composer::{Change, Compose, Target};
use super::input;
use super::lists::Field;
use super::menu::{Act, Entry};
use super::typing::Input;
use super::{discussion, px};
use crate::markdown::Helper;
use crate::textedit::TextEdit;
use artoo_core::mr::Edits;
use artoo_core::request_edits::{self, Mark, Proposal};
use artoo_requests_api::{Editable, OverviewComposer};
use artoo_ui_kit::{ROW_RADIUS, hover::Hover, icons, theme};
use gpui::{
    AnyElement, App, ClickEvent, Context, FontWeight, HighlightStyle, Pixels, Point, Rgba,
    StrikethroughStyle, StyledText, WeakEntity, div, prelude::*,
};

impl Workspace {
    /// The open request when you may edit it.
    fn editable(&self, cx: &App) -> Option<(u64, Editable)> {
        let iid = self.current_request(cx)?;
        let now = self.requests_api()?.editable(iid, cx)?;
        now.can.then_some((iid, now))
    }

    /// The host as a button names it.
    fn host_name(&self) -> &'static str {
        match self.on_github() {
            true => "GitHub",
            false => "GitLab",
        }
    }

    /// Sends `edits` of request `iid`; `after` runs once the host took them. Refused, the error says why and what
    /// was typed stays.
    fn send_edits(
        &mut self,
        iid: u64,
        edits: Edits,
        after: impl FnOnce(&mut Self, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        let Some(requests) = self.requests_api() else {
            return;
        };
        if self.saving_edit.is_some() {
            return;
        }
        let this = cx.weak_entity();
        let done = Box::new(move |result: Result<(), Option<String>>, cx: &mut App| {
            this.update(cx, |this, cx| {
                this.saving_edit = None;
                match result {
                    Ok(()) => {
                        // Touched: its open row stays (#194).
                        this.nav.touched(iid);
                        after(this, cx);
                    }
                    Err(Some(why)) => {
                        this.error = Some(format!("Not saved — {why}").into());
                    }
                    Err(None) => {}
                }
                cx.notify();
            })
            .ok();
        });
        if requests.edit(iid, edits, done, cx) {
            self.saving_edit = Some(iid);
            cx.notify();
        }
    }

    // ---- the title

    /// The pencil or a double click: the title becomes a field.
    pub(super) fn start_title_edit(&mut self, cx: &mut Context<Self>) {
        let Some((iid, now)) = self.editable(cx) else {
            return;
        };
        self.renaming = None;
        self.renaming_group = None;
        self.label_name = now.title;
        self.title_editing = Some(iid);
        self.start_field(Field::Title, cx);
    }

    /// ↵: the title saved; unchanged or empty, the field goes with nothing sent.
    pub(super) fn save_title(&mut self, cx: &mut Context<Self>) {
        let title = self.label_name.trim().to_owned();
        let Some((iid, now)) = self.editable(cx) else {
            return self.cancel_title(cx);
        };
        if self.title_editing != Some(iid) || title.is_empty() || title == now.title {
            return self.cancel_title(cx);
        }
        // The host says a draft in the title on GitLab: what is typed says it.
        let edits = Edits {
            title: Some(title),
            ..Default::default()
        };
        self.send_edits(iid, edits, |this, cx| this.cancel_title(cx), cx);
    }

    /// Esc or a press elsewhere: the title as it was.
    pub(super) fn cancel_title(&mut self, cx: &mut Context<Self>) {
        self.title_editing = None;
        if self.field == Some(Field::Title) {
            self.field = None;
            self.label_name.clear();
        }
        cx.notify();
    }

    // ---- the state

    /// `Draft ▾`: Mark as ready, or back; and editing the title and the description.
    pub(super) fn state_menu(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) {
        let Some((iid, now)) = self.editable(cx) else {
            return;
        };
        let draft = now.draft;
        let (mark, is) = match draft {
            true => ("Mark as ready", "Draft  \u{b7}  now"),
            false => ("Mark as draft", "Open  \u{b7}  now"),
        };
        let turn = Entry::new(
            mark,
            Act::new(move |this, _, cx| {
                let edits = Edits {
                    draft: Some(!draft),
                    ..Default::default()
                };
                this.send_edits(iid, edits, |_, _| {}, cx);
            }),
        );
        let edit = Entry::new(
            "Edit title and description\u{2026}",
            Act::new(|this, _, cx| this.start_description_edit(cx)),
        )
        .key("E");
        self.open_menu(at, vec![vec![turn, Entry::off(is)], vec![edit]], cx);
    }

    // ---- the description

    /// A double click, E, or the state's menu: the description in the composer's editor, with what was left in it
    /// before.
    pub(super) fn start_description_edit(&mut self, cx: &mut Context<Self>) {
        let (Some((iid, now)), Some(root)) = (self.editable(cx), self.root.clone()) else {
            return;
        };
        let target = Target::Edit(Change::Description(iid));
        if self.compose.as_ref().is_some_and(|c| c.target == target) {
            self.take_keyboard(Input::Compose, cx);
            return cx.notify();
        }
        let text = (request_edits::load(&root, iid).editing).unwrap_or(now.description);
        let mut edit = TextEdit::new(text);
        edit.move_to(crate::textedit::Move::End, false);
        self.compose = Some(Compose {
            path: String::new(),
            old: false,
            first: None,
            line: 0,
            code: String::new(),
            edit,
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
        });
        self.take_keyboard(Input::Compose, cx);
        self.reset_field_edit();
        cx.notify();
    }

    /// The description being written, kept on this Mac as it changes.
    pub(super) fn keep_description_edit(&mut self) {
        if let Some(c) = &self.compose
            && let Target::Edit(Change::Description(iid)) = c.target
        {
            let text = c.edit.text().to_owned();
            self.keep_description(iid, Some(text));
        }
    }

    pub(super) fn keep_description(&mut self, iid: u64, text: Option<String>) {
        if let Some(root) = &self.root {
            let mut kept = request_edits::load(root, iid);
            kept.editing = text;
            request_edits::keep(kept);
        }
    }

    /// ⌘↵: the description saved to the host; the editor goes once it is taken.
    pub(super) fn save_description(&mut self, iid: u64, cx: &mut Context<Self>) {
        let Some(compose) = self.compose.as_ref() else {
            return;
        };
        let edits = Edits {
            description: Some(compose.edit.text().trim_end().to_owned()),
            ..Default::default()
        };
        let done = move |this: &mut Self, cx: &mut Context<Self>| {
            this.keep_description(iid, None);
            let target = Target::Edit(Change::Description(iid));
            if this.compose.as_ref().is_some_and(|c| c.target == target) {
                this.compose = None;
            }
            cx.notify();
        };
        self.send_edits(iid, edits, done, cx);
    }

    // ---- the proposal kept

    /// The proposal waiting on request `iid` of this project, read from disk once.
    pub(super) fn proposal(&self, iid: u64) -> Option<Proposal> {
        let root = self.root.clone()?;
        (self.proposals.borrow_mut())
            .entry((root, iid))
            .or_insert_with_key(|(root, iid)| request_edits::load(root, *iid).proposal)
            .clone()
    }

    /// Keeps `proposal` for request `iid`, in place of an earlier one; none drops it.
    pub(super) fn set_proposal(&mut self, iid: u64, proposal: Option<Proposal>) {
        let Some(root) = self.root.clone() else {
            return;
        };
        let mut kept = request_edits::load(&root, iid);
        kept.proposal = proposal.clone();
        request_edits::keep(kept);
        self.proposals.get_mut().insert((root, iid), proposal);
    }

    // ---- what Overview shows of it

    /// The title's field, the description's editor or the proposal in its place, and the proposal's bar.
    pub(super) fn overview_edits(
        &self,
        iid: u64,
        this: WeakEntity<Self>,
        cx: &App,
    ) -> OverviewComposer {
        let title = (self.title_editing == Some(iid) && self.field == Some(Field::Title))
            .then(|| self.title_field(this.clone()));
        let editing = Target::Edit(Change::Description(iid));
        let editor = (self.compose.as_ref())
            .filter(|c| c.target == editing)
            .map(|c| self.description_editor(c, this.clone(), cx));
        let now = self.requests_api().and_then(|r| r.editable(iid, cx));
        let proposal = self.proposal(iid).zip(now);
        let (bar, proposed) = match &proposal {
            Some((p, now)) => (
                Some(self.proposal_bar(iid, p, now, this)),
                self.proposed_text(p, now),
            ),
            None => (None, None),
        };
        OverviewComposer {
            title,
            description: editor.or(proposed),
            proposal: bar,
            ..Default::default()
        }
    }

    fn title_field(&self, this: WeakEntity<Self>) -> AnyElement {
        let out = this.clone();
        div()
            .id("title-field")
            .w_full()
            .min_w_0()
            .flex()
            .items_center()
            .h(px(30.))
            .px(px(8.))
            .rounded(px(ROW_RADIUS))
            .border_2()
            .border_color(theme::focus())
            .bg(theme::base())
            .cursor_text()
            .child(self.field_look(
                &self.label_name,
                true,
                "Title",
                input::Look::default(),
                this,
            ))
            .on_mouse_down_out(move |_, _, cx| {
                out.update(cx, |this, cx| this.cancel_title(cx)).ok();
            })
            .into_any_element()
    }

    /// The description's editor: the composer's, with `Kept on this Mac`, Cancel and Save.
    fn description_editor(
        &self,
        compose: &Compose,
        this: WeakEntity<Self>,
        cx: &App,
    ) -> AnyElement {
        let on = this.clone();
        let tail = compose.edit_foot(
            false,
            std::rc::Rc::new(move |event, cx| {
                on.update(cx, |this, cx| this.foot_event(event, cx)).ok();
            }),
        );
        let kept = div()
            .text_size(px(11.))
            .text_color(theme::faint())
            .child(match self.saving_edit {
                Some(_) => "Saving\u{2026}",
                None => "Kept on this Mac",
            })
            .into_any_element();
        let tail = std::iter::once(kept).chain(tail).collect();
        self.open_input(
            compose,
            theme::focus(),
            "What this request does".into(),
            None,
            None,
            &[Helper::Bold, Helper::Code, Helper::Link, Helper::Bullet],
            tail,
            this,
            cx,
        )
        .into_any_element()
    }

    /// `Claude proposes a new description and title · from edit_request · 2m`, with Compare, Discard and Save.
    fn proposal_bar(
        &self,
        iid: u64,
        p: &Proposal,
        now: &Editable,
        this: WeakEntity<Self>,
    ) -> AnyElement {
        let what = match (&p.description, &p.title) {
            (Some(_), Some(_)) => "a new description and title",
            (Some(_), None) => "a new description",
            _ => "a new title",
        };
        let button = |id: &'static str, label: String| {
            discussion::button(id, label, None, discussion::Kind::Quiet)
                .h(px(26.))
                .px(px(10.))
        };
        let (compare, discard, save) = (this.clone(), this.clone(), this);
        let plain = self.proposal_plain;
        let title = p.title.as_ref().map(|title| {
            div()
                .flex()
                .flex_col()
                .gap(px(2.))
                .text_size(px(13.))
                .when(!plain, |s| {
                    s.child(
                        div()
                            .line_through()
                            .text_color(theme::muted())
                            .child(now.title.clone()),
                    )
                })
                .child(div().font_weight(FontWeight::SEMIBOLD).child(title.clone()))
        });
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .px(px(12.))
            .py(px(8.))
            .rounded(px(ROW_RADIUS))
            .border_1()
            .border_color(Rgba {
                a: 0.4,
                ..theme::agent()
            })
            .bg(theme::agent_bg())
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(8.))
                    .text_size(px(12.))
                    .child(icons::icon("bot").size(px(14.)).text_color(theme::agent()))
                    .child(format!("{} proposes {what}", p.agent))
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(theme::faint())
                            .child(format!(
                                "from edit_request \u{b7} {}",
                                artoo_ui_kit::format::short_ago(p.at)
                            )),
                    )
                    .child(div().flex_1())
                    .child(
                        button("proposal-compare", "Compare".to_owned())
                            .when(!plain, |s| s.bg(theme::selected()))
                            .tip(match plain {
                                true => "Mark what it changes",
                                false => "Show it as it would read",
                            })
                            .on_click(move |_: &ClickEvent, _, cx| {
                                compare
                                    .update(cx, |this, cx| {
                                        this.proposal_plain = !this.proposal_plain;
                                        cx.notify();
                                    })
                                    .ok();
                            }),
                    )
                    .child(button("proposal-discard", "Discard".to_owned()).on_click(
                        move |_: &ClickEvent, _, cx| {
                            discard
                                .update(cx, |this, cx| this.drop_proposal(iid, cx))
                                .ok();
                        },
                    ))
                    // Only where the host lets you edit.
                    .children(now.can.then(|| {
                        let label = match self.saving_edit {
                            Some(_) => "Saving\u{2026}".to_owned(),
                            None => format!("Save to {}", self.host_name()),
                        };
                        discussion::button(
                            "proposal-save",
                            label,
                            Some("\u{2318}\u{21b5}"),
                            discussion::Kind::Primary(theme::accent()),
                        )
                        .h(px(26.))
                        .px(px(10.))
                        .on_click(move |_: &ClickEvent, _, cx| {
                            save.update(cx, |this, cx| this.save_proposal(iid, cx)).ok();
                        })
                    })),
            )
            .children(title)
            .into_any_element()
    }

    /// The proposed description in the place of the one it would replace: with Compare on, as written, what goes
    /// struck out and what comes marked; off, formatted as it would read.
    fn proposed_text(&self, p: &Proposal, now: &Editable) -> Option<AnyElement> {
        let new = p.description.as_ref()?;
        if self.proposal_plain {
            return None;
        }
        let mut text = String::new();
        let mut marks = Vec::new();
        for (mark, run) in request_edits::compared(&now.description, new) {
            let from = text.len();
            text.push_str(&run);
            let style = match mark {
                Mark::Same => continue,
                Mark::Out => HighlightStyle {
                    background_color: Some(theme::removed_word().into()),
                    strikethrough: Some(StrikethroughStyle {
                        thickness: gpui::px(1.),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                Mark::In => HighlightStyle {
                    background_color: Some(theme::added_word().into()),
                    ..Default::default()
                },
            };
            marks.push((from..text.len(), style));
        }
        Some(
            div()
                .text_size(px(13.))
                .child(StyledText::new(text).with_highlights(marks))
                .into_any_element(),
        )
    }

    /// The agent's proposal on request `iid` goes.
    pub(super) fn drop_proposal(&mut self, iid: u64, cx: &mut Context<Self>) {
        self.set_proposal(iid, None);
        self.proposal_plain = false;
        cx.notify();
    }

    /// Save to GitLab: the proposal sent as the reviewer's own edit.
    pub(super) fn save_proposal(&mut self, iid: u64, cx: &mut Context<Self>) {
        if self.editable(cx).map(|(n, _)| n) != Some(iid) {
            return;
        }
        let Some(p) = self.proposal(iid) else {
            return;
        };
        let edits = Edits {
            title: p.title,
            description: p.description,
            draft: None,
        };
        self.send_edits(iid, edits, move |this, cx| this.drop_proposal(iid, cx), cx);
    }

    /// ⌘↵ with nothing being typed, on the Overview of a request an agent proposed an edit to: saved. Whether it
    /// was one.
    pub(super) fn save_shown_proposal(&mut self, cx: &mut Context<Self>) -> bool {
        let on_overview = self.page_section(cx) == Some(super::nav::Section::Overview);
        let Some(iid) = self.current_request(cx).filter(|_| on_overview) else {
            return false;
        };
        if self.proposal(iid).is_none() || self.compose.is_some() || self.editable(cx).is_none() {
            return false;
        }
        self.save_proposal(iid, cx);
        true
    }
}
