//! The composer (#9, #51, #129, #168): a comment being written — under a line of the diff, on a page, a file or a
//! folder in Code, for the agent, a merge request, a reply or an edit — and its own editing: completion, markdown
//! helpers, preview —, the one input component it is drawn in and a new comment's foot (who sees it, the ways to
//! send it). Who it goes to, where it is drawn, its keys routed and what it writes sent are the tab's.

use crate::complete::Choice;
use crate::discussion::{self, Kind, Source};
use crate::input::{self, Edit};
use crate::{hover, icons, keys, markdown, px, theme};
use artoo_core::markdown::Helper;
use artoo_core::textedit::TextEdit;
use gpui::{AnyElement, App, Div, Keystroke, Pixels, Point, Rgba, SharedString, div, prelude::*};
use std::rc::Rc;

/// Who a comment is for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Target {
    /// Kept on this Mac and copied for a coding agent.
    Agent,
    /// Posted to a merge request on GitLab, where everyone on it sees it.
    Request(u64),
    /// A reply to discussion number `0` of the open merge request.
    Reply(usize),
    /// A new text for one's own comment.
    Edit(Change),
    /// A purple note on discussion `0` of the request, kept on this Mac for the agent (#50 G).
    ThreadNote(usize),
    /// The reviewer's note answering the agent's question on comment `0` (#10).
    Answer(u64),
    /// Text's own value on the agent's form of comment `0` (#10 C).
    AnswerValue(u64),
    /// Written in the Inbox: a reply to the selected notice's discussion, or a comment on its
    /// request when it has none (`Workspace::inbox_view.reply`).
    InboxReply,
}

/// The comment an edit changes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Change {
    /// A comment for the agent, by its id.
    Agent(u64),
    /// Note `note` of discussion number `thread` of the open merge request.
    Note { thread: usize, note: u64 },
    /// One's pending comment `0` on the open merge request, before the review is sent (#200).
    Draft(u64),
}

/// A comment being written.
pub struct Compose {
    pub path: String,
    pub old: bool,
    /// The first line of a comment on several lines; it sits under its last, `line`.
    pub first: Option<u32>,
    pub line: u32,
    pub code: String,
    /// The text, its caret and selection.
    pub edit: TextEdit,
    pub at: String,
    pub target: Target,
    /// Why the last attempt to post failed.
    pub error: Option<String>,
    /// The text is shown formatted instead of as written (⌘⇧P); typing goes on and the preview
    /// follows it.
    pub preview: bool,
    /// The completion list's selected row, and where a list was closed with esc (#51, #9).
    pub pick: usize,
    pub closed: Option<usize>,
    /// Written on a document read as a page: its file in the diff, the composer under the block
    /// (#11).
    pub page: Option<usize>,
    /// Written on a file read in Code, at its commit (None: the working tree) — a note on a plain
    /// line, under it in the file (#75).
    pub file: Option<Option<git2::Oid>>,
    /// The agent's note it answers, carried into the comment (#151).
    pub answers: Option<String>,
}

impl Compose {
    /// The discussion of the open request a reply, an edit or a note for the agent is in.
    pub fn thread(&self) -> Option<usize> {
        match self.target {
            Target::Reply(ix) | Target::ThreadNote(ix) => Some(ix),
            Target::Edit(Change::Note { thread, .. }) => Some(thread),
            _ => None,
        }
    }

    /// That discussion moved to `ix` in the list.
    pub fn set_thread(&mut self, ix: usize) {
        match &mut self.target {
            Target::Reply(i) | Target::ThreadNote(i) => *i = ix,
            Target::Edit(Change::Note { thread, .. }) => *thread = ix,
            _ => {}
        }
    }

    /// ↑ ↓ ↵ ⇥ esc while the list of choices for the token at `at` is open: None when it did not take the key,
    /// else whether the text changed.
    pub fn completion_key(
        &mut self,
        key: &Keystroke,
        at: usize,
        choices: &[Choice],
    ) -> Option<bool> {
        if key.modifiers.number_of_modifiers() > 0 {
            return None;
        }
        match key.key.as_str() {
            "down" => self.pick = (self.pick + 1).min(choices.len() - 1),
            "up" => self.pick = self.pick.saturating_sub(1),
            "enter" | "tab" => {
                let choice = &choices[self.pick.min(choices.len() - 1)];
                let caret = self.edit.caret();
                let end = at + choice.insert.len();
                self.edit.replace(at..caret, &choice.insert, end..end);
                self.pick = 0;
                return Some(true);
            }
            "escape" => self.closed = Some(at),
            _ => return None,
        }
        Some(false)
    }

    /// ⌘B ⌘I ⇧⌘X ⇧⌘E ⇧⌘K and ⇧⌘P (Ctrl on Windows and Linux), no other modifier.
    pub fn shortcut(key: &Keystroke) -> Option<Short> {
        let m = key.modifiers;
        if !m.secondary() || m.number_of_modifiers() != 1 + u8::from(m.shift) {
            return None;
        }
        match (key.key.as_str(), m.shift) {
            ("b", false) => Some(Short::Helper(Helper::Bold)),
            ("i", false) => Some(Short::Helper(Helper::Italic)),
            ("x", true) => Some(Short::Helper(Helper::Strike)),
            ("e", true) => Some(Short::Helper(Helper::Code)),
            ("k", true) => Some(Short::Helper(Helper::Link)),
            ("p", true) => Some(Short::Preview),
            _ => None,
        }
    }

    /// A markdown helper of the toolbar, on the selection or at the caret.
    pub fn apply(&mut self, helper: Helper) {
        artoo_core::markdown::apply(&mut self.edit, helper);
    }

    /// The text shown formatted, or as written.
    pub fn toggle_preview(&mut self) {
        self.preview = !self.preview;
    }

    /// `text` put at the caret (a reply that mentions the author).
    pub fn insert(&mut self, text: &str) {
        self.edit.insert(text);
    }

    /// A click in the text: the caret goes there; ⇧ extends the selection, a double click takes the word; a drag
    /// (`drag`) selects.
    pub fn place(&mut self, at: usize, shift: bool, clicks: usize, drag: bool, lower: bool) {
        self.edit.click(at, clicks, shift || drag);
        if clicks == 1 && !shift && !drag {
            self.edit.set_downstream(lower);
        }
    }

    /// Key `key` on the text: ↵ a new line that keeps a list going, ⇧↵ a plain one.
    pub fn edit_key(&mut self, key: &Keystroke, cx: &mut App) -> Typed {
        match input::edit_text(&mut self.edit, key, cx) {
            // Written in, it keeps what was written and lets the keyboard go (#168).
            Edit::Escape if !self.edit.text().trim().is_empty() => Typed::Leave,
            Edit::Escape => Typed::Cancel,
            Edit::Ignored => Typed::Ignored,
            Edit::Enter { shift: true } => {
                self.edit.insert("\n");
                Typed::Changed
            }
            Edit::Enter { shift: false } => {
                artoo_core::markdown::newline(&mut self.edit);
                Typed::Changed
            }
            Edit::Changed | Edit::Selected => Typed::Changed,
        }
    }
}

/// A key of the composer's own beside its text (`Compose::shortcut`).
pub enum Short {
    Helper(Helper),
    /// ⇧⌘P: the text formatted or as written.
    Preview,
}

/// What a key did to the text (`Compose::edit_key`).
pub enum Typed {
    /// Esc on text written: it is kept, the keyboard let go.
    Leave,
    /// Esc on nothing written: the comment goes.
    Cancel,
    /// Not a key of the text.
    Ignored,
    /// The text, its caret or selection changed: the box may grow.
    Changed,
}

/// What a press in the composer asks of the tab (`Compose::render_input`).
pub enum ComposeAct {
    /// A markdown helper of its toolbar.
    Helper(Helper),
    /// The text formatted, or as written.
    Preview,
    /// The folded helpers' menu, at a window position: `tools`, then Preview.
    Format {
        at: Point<Pixels>,
        tools: Vec<Helper>,
    },
    /// A press or a drag in the text (`Compose::place`).
    Place {
        at: usize,
        shift: bool,
        clicks: usize,
        drag: bool,
        lower: bool,
    },
}

/// What answers the composer's presses.
pub type ComposeActs = Rc<dyn Fn(ComposeAct, &mut App)>;

/// How the tab has the composer drawn: who sees it, what is along its edge, what it owns of the window.
pub struct Look {
    /// The outline, in the audience's colour.
    pub tone: Rgba,
    pub placeholder: SharedString,
    /// Who reads it, a row over the text (#200: the `To:` chip).
    pub head: Option<AnyElement>,
    /// Who sees it, first along the bottom edge.
    pub lead: Option<AnyElement>,
    /// The markdown helpers, after `lead`.
    pub tools: Vec<Helper>,
    /// The buttons, at the right end.
    pub tail: Vec<AnyElement>,
    /// The helpers fold into one menu (`Room::S`).
    pub folded: bool,
    /// It has the keyboard: the caret shows.
    pub focused: bool,
    /// Where the text's layout is kept for ↑ ↓ by the lines as drawn, unless a card owns the arrows.
    pub lines: Option<input::Lines>,
    /// The completion list, when one is open for it.
    pub completion: Option<Div>,
    /// What measures its width, for its buttons' room.
    pub measure: AnyElement,
    /// Where a relative link in the preview points.
    pub link_base: Option<String>,
}

impl Compose {
    /// The one input component, open: one outline in the audience's colour, the text (or its
    /// preview), and along its bottom edge `lead`, the markdown `tools`, Preview, then `tail`. The tab wraps it in
    /// the box that gives it the keyboard.
    pub fn render_input(&self, look: Look, act: ComposeActs, cx: &App) -> Div {
        let Look {
            tone,
            placeholder,
            head,
            lead,
            tools,
            tail,
            folded,
            focused,
            lines,
            completion,
            measure,
            link_base,
        } = look;
        let compose = self;
        let text = if compose.preview {
            div()
                .min_h(px(66.))
                .px(px(10.))
                .py(px(7.))
                .child(if compose.edit.text().trim().is_empty() {
                    div().text_color(theme::muted()).child("Nothing to preview")
                } else {
                    div().child(markdown::body(
                        compose.edit.text(),
                        true,
                        "md-compose",
                        link_base.as_deref(),
                        None,
                        cx,
                    ))
                })
                .into_any_element()
        } else {
            self.render_text(placeholder, true, focused, lines, act.clone())
        };
        let tool = |h: Helper, act: ComposeActs| {
            let (icon, tip) = markdown::helper_look(h);
            div()
                .id(("md-tool", h as usize))
                .size(px(26.))
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .cursor_pointer()
                .hover(|s| s.bg(theme::hover()))
                .child(icons::icon(icon).size(px(15.)).text_color(theme::muted()))
                .tooltip_show_delay(hover::LONG)
                .tooltip(move |_, cx| cx.new(|_| hover::Tip(tip)).into())
                .on_click(move |_, _, cx| act(ComposeAct::Helper(h), cx))
        };
        let preview = compose.preview;
        let toggle = act.clone();
        // One row at every width (Design shell round 6, #129): who sees it and the writing helpers
        // on the left, the buttons on the right; at S the helpers fold into T, a menu of them.
        let (open_format, format) = (act.clone(), tools.clone());
        let helpers = div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(2.))
            .children(lead.map(|lead| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .mr(px(4.))
                    .child(lead)
                    .children((!folded).then(|| div().w(px(1.)).h(px(14.)).bg(theme::selected())))
            }))
            .children(folded.then(|| {
                div()
                    .id("md-format")
                    .size(px(26.))
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.))
                    .cursor_pointer()
                    .when(preview, |s| s.bg(theme::selected()))
                    .hover(|s| s.bg(theme::hover()))
                    .child(icons::icon("type").size(px(15.)).text_color(theme::muted()))
                    .tooltip_show_delay(hover::LONG)
                    .tooltip(|_, cx| {
                        cx.new(|_| hover::Tip("Format: bold, code, link, list, preview"))
                            .into()
                    })
                    .on_click(move |event, _, cx| {
                        let (at, tools) = (event.position(), format.clone());
                        open_format(ComposeAct::Format { at, tools }, cx);
                    })
            }))
            .children(
                tools
                    .iter()
                    .filter(|_| !folded)
                    .map(|&h| tool(h, act.clone())),
            )
            .when(!folded, |s| {
                s.child(
                    div()
                        .id("md-preview")
                        .size(px(26.))
                        .flex()
                        .flex_none()
                        .items_center()
                        .justify_center()
                        .rounded(px(6.))
                        .cursor_pointer()
                        .when(preview, |s| s.bg(theme::selected()))
                        .hover(|s| s.bg(theme::hover()))
                        .child(icons::icon("eye").size(px(15.)).text_color(if preview {
                            theme::text()
                        } else {
                            theme::muted()
                        }))
                        .tooltip_show_delay(hover::LONG)
                        .tooltip(|_, cx| cx.new(|_| hover::Tip("Preview  ⌘⇧P")).into())
                        .on_click(move |_, _, cx| toggle(ComposeAct::Preview, cx)),
                )
            });
        let edge = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .px(px(6.))
            .pt(px(4.))
            .pb(px(6.))
            .child(helpers)
            .child(div().flex_1().min_w_0())
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(6.))
                    .children(tail),
            );
        div()
            .relative()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .rounded(px(8.))
            .border_1()
            .border_color(tone)
            .bg(theme::base())
            .overflow_hidden()
            .font_family(theme::ui_font())
            .text_size(px(13.))
            .line_height(px(20.))
            .text_color(theme::text())
            // Its own width decides its buttons' (the tab's `compose_narrow`).
            .child(measure)
            .children(head.map(|head| {
                div()
                    .flex()
                    .items_center()
                    .min_w_0()
                    .px(px(6.))
                    .pt(px(6.))
                    .child(head)
            }))
            .child(text)
            .children(completion)
            .children(compose.error.clone().map(|e| {
                div()
                    .px(px(10.))
                    .pb(px(4.))
                    .text_size(px(12.))
                    .text_color(theme::removed())
                    .child(e)
            }))
            .child(edge)
    }

    /// The composer's text as a field: the text wrapping in its box, the selection tinted, the
    /// caret drawn where the text's layout puts it; a click places the caret, a drag selects.
    fn render_text(
        &self,
        placeholder: SharedString,
        compact: bool,
        focused: bool,
        lines: Option<input::Lines>,
        act: ComposeActs,
    ) -> AnyElement {
        let compose = self;
        // A reply grows out of its one-line field, so its text starts where the field's did.
        let (pad_x, pad_y, min_h) = if compact {
            (10., 6., 64.)
        } else {
            (12., 10., 88.)
        };
        use gpui::{Bounds, HighlightStyle, MouseButton, StyledText, canvas, fill, point, size};
        let text = compose.edit.text();
        // A trailing space gives the caret a place on a last, empty line.
        let styled = StyledText::new(format!("{text} ")).with_highlights(
            compose.edit.selection().map(|sel| {
                (
                    sel,
                    HighlightStyle {
                        background_color: Some(theme::selection().into()),
                        ..Default::default()
                    },
                )
            }),
        );
        let layout = styled.layout().clone();
        if let Some(lines) = lines {
            *lines.borrow_mut() = Some(layout.clone());
        }
        let (for_caret, for_down, for_move) = (layout.clone(), layout.clone(), layout);
        let (caret, downstream) = (compose.edit.caret(), compose.edit.downstream());
        let (down, drag) = (act.clone(), act);
        div()
            .relative()
            .min_h(px(min_h))
            .px(px(pad_x))
            .py(px(pad_y))
            .bg(theme::base())
            .cursor_text()
            .child(div().w_full().min_w_0().child(styled))
            .children(text.is_empty().then(|| {
                div()
                    .absolute()
                    .top(px(pad_y))
                    .left(px(pad_x + 2.))
                    // Bounded on the right, so a long hint wraps rather than being cut (#129).
                    .right(px(pad_x))
                    .text_color(theme::muted())
                    .child(placeholder)
            }))
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        if !focused {
                            return;
                        }
                        let at = input::caret_point(&for_caret, caret, downstream)
                            .unwrap_or(bounds.origin + point(px(pad_x), px(pad_y)));
                        window.paint_quad(fill(
                            Bounds::new(at, size(px(2.), px(20.))),
                            theme::accent(),
                        ));
                    },
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .on_mouse_down(MouseButton::Left, move |event, _, cx| {
                let at = input::nearest_index(&for_down, event.position);
                let (shift, clicks) = (event.modifiers.shift, event.click_count);
                let lower = input::lower_line(&for_down, at, event.position.y);
                let drag = false;
                down(
                    ComposeAct::Place {
                        at,
                        shift,
                        clicks,
                        drag,
                        lower,
                    },
                    cx,
                );
            })
            .on_mouse_move(move |event, _, cx| {
                if event.pressed_button == Some(MouseButton::Left) {
                    let at = input::nearest_index(&for_move, event.position);
                    let place = ComposeAct::Place {
                        at,
                        shift: true,
                        clicks: 1,
                        drag: true,
                        lower: false,
                    };
                    drag(place, cx);
                }
            })
            .into_any_element()
    }
}

/// Who reads a new comment, as its `To:` chip says it (#200).
pub struct Audience {
    /// In words: `Everyone on !398`, a chat's name, `Your agent`.
    pub words: String,
    /// A chat's letter on its hue; `None`, the request's icon (everyone on it) or the agent's purple.
    pub mark: Option<(char, Rgba)>,
    /// A request's page: ▾ lists who can read it, ⇥ switches to the branch's chat and back.
    pub list: bool,
}

/// What a comment's foot is drawn with that the tab knows.
pub struct FootLook {
    /// Who reads it: the `To:` chip over the text (#200).
    pub audience: Audience,
    /// The diff narrow: the secondary buttons are their icons (`compose_narrow`).
    pub narrow: bool,
    /// The box's room (#129).
    pub room: Room,
    /// The comments for the agent waiting, which Send all sends with this one.
    pub drafts: usize,
}

/// A click on a comment's foot.
#[derive(Clone, Copy)]
pub enum FootEvent {
    Cancel,
    /// The `To:` chip's ▾ at a point: who can read it (#200).
    Audience(Point<Pixels>),
    /// Added to the review, pending until it is submitted: a comment on a line, a reply (#200).
    AddToReview,
    /// Added for the agent, and every draft sent.
    SendAll,
    /// The agent's ▾ at a point: Send this only, the chat, Copy (#50 F).
    More(Point<Pixels>),
    /// The primary: added for the agent, or posted.
    Add,
    /// The primary's own ▾ at S, at a point: what folded into it.
    Folded(Point<Pixels>),
}

/// What answers a foot's clicks.
pub type OnFoot = Rc<dyn Fn(FootEvent, &mut App)>;

impl Compose {
    /// Who sees the comment: your agent or everyone on the request.
    pub fn source(&self) -> Source {
        match self.target {
            Target::Agent
            | Target::Edit(Change::Agent(_))
            | Target::ThreadNote(_)
            | Target::Answer(_)
            | Target::AnswerValue(_) => Source::Agent,
            Target::Request(_)
            | Target::Reply(_)
            | Target::Edit(Change::Note { .. } | Change::Draft(_))
            | Target::InboxReply => Source::GitLab,
        }
    }

    /// The `To:` chip over a new comment's text (#200): who reads it in words at every width, a chat's letter on its
    /// hue or the request's icon before them, ▾ for the list of who can.
    pub fn audience_chip(&self, audience: &Audience, on: OnFoot) -> AnyElement {
        let tone = self.source().color();
        let hue = audience.mark.map_or(tone, |(_, hue)| hue);
        let said: SharedString = if audience.list {
            format!("Who reads it: {}  ·  ⇥ switches", audience.words).into()
        } else {
            format!("Who reads it: {}", audience.words).into()
        };
        let list = audience.list;
        div()
            .id("compose-audience")
            .h(px(24.))
            .max_w_full()
            .min_w_0()
            .px(px(8.))
            .flex()
            .items_center()
            .gap(px(6.))
            .rounded(px(6.))
            .border_1()
            .border_color(theme::selected())
            .text_size(px(12.))
            .whitespace_nowrap()
            .child(div().flex_none().text_color(theme::muted()).child("To:"))
            .child(match audience.mark {
                Some((letter, hue)) => div()
                    .flex_none()
                    .size(px(16.))
                    .rounded(px(4.))
                    .bg(hue)
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(10.))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(theme::base())
                    .child(letter.to_string())
                    .into_any_element(),
                None => icons::icon(match self.source() {
                    Source::Agent => "bot",
                    _ => "pull-request",
                })
                .size(px(13.))
                .flex_none()
                .text_color(hue)
                .into_any_element(),
            })
            .child(
                hover::cut("compose-audience-words", audience.words.clone())
                    .min_w_0()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(theme::text()),
            )
            .tooltip_show_delay(hover::LONG)
            .tooltip(move |_, cx| cx.new(|_| hover::TipOwned(said.clone())).into())
            .when(list, |s| {
                s.cursor_pointer()
                    .hover(|s| s.bg(theme::hover()))
                    .child(
                        icons::icon("chevron")
                            .size(px(12.))
                            .flex_none()
                            .text_color(theme::muted()),
                    )
                    .on_click(move |event, _, cx| on(FootEvent::Audience(event.position()), cx))
            })
            .into_any_element()
    }

    /// A new comment's foot by its room (Design shell round 6, #129): Cancel, the other ways to send and the
    /// primary; who reads it is the `To:` chip over the text (#200, `audience_chip`). At S "Add to drafts" is "Add"
    /// (named in full in its tooltip), and the other ways to send fold into its own ▾.
    pub fn foot(&self, look: FootLook, on: OnFoot) -> Vec<AnyElement> {
        let source = self.source();
        let tone = source.color();
        let narrow = look.narrow;
        let folded = look.room == Room::S;
        let primary = match &self.target {
            Target::Agent if folded => "Add",
            Target::Agent => "Add to drafts",
            _ => "Comment",
        };
        // An agent comment's bar (#50 F): Send all beside Add to drafts, Send this only in ▾.
        let agent = self.target == Target::Agent;
        let send_label = if look.drafts > 0 {
            format!("Send all {}", look.drafts + 1)
        } else {
            "Send".to_owned()
        };
        let request = match self.target {
            Target::Request(iid) => Some(iid),
            _ => None,
        };
        let pad = px(if narrow { 6. } else { 10. });
        let cancel = on.clone();
        let mut tail = vec![
            // Narrow, Cancel is its × icon, named in the tooltip.
            if narrow {
                discussion::icon_button(
                    "compose-cancel",
                    "x",
                    "Cancel",
                    Some("esc"),
                    None,
                    Kind::Quiet,
                    true,
                )
            } else {
                discussion::button("compose-cancel", "Cancel", None, Kind::Quiet)
                    .tooltip_show_delay(hover::LONG)
                    .tooltip(|_, cx| cx.new(|_| hover::Tip("Esc")).into())
            }
            .h(px(28.))
            .px(pad)
            .on_click(move |_, _, cx| cancel(FootEvent::Cancel, cx))
            .into_any_element(),
        ];
        // The secondary buttons are one weight, bare as Cancel and ▾ are; only the primary is
        // filled.
        if request.is_some() && !folded {
            let review = on.clone();
            tail.push(
                // Beside Comment in words wherever it is not folded (#200).
                discussion::icon_button(
                    "compose-review",
                    "layers",
                    "Add to review",
                    (!narrow).then_some("⇧⌘↵"),
                    Some("Pending until you submit the review"),
                    Kind::Quiet,
                    false,
                )
                .h(px(28.))
                .px(pad)
                .on_click(move |_, _, cx| review(FootEvent::AddToReview, cx))
                .into_any_element(),
            );
        }
        if agent && !folded {
            let (send_all, more) = (on.clone(), on.clone());
            tail.push(
                discussion::icon_button(
                    "compose-send-all",
                    "send",
                    send_label,
                    Some("⇧⌘↵"),
                    Some("Adds it and sends every draft to your agent's chat"),
                    Kind::Quiet,
                    narrow,
                )
                .h(px(28.))
                .px(pad)
                .on_click(move |_, _, cx| send_all(FootEvent::SendAll, cx))
                .into_any_element(),
            );
            // ▾: Send this only, the chat it goes to, Copy as markdown (#50 F).
            tail.push(
                discussion::icon_button(
                    "compose-more",
                    "chevron",
                    "Send this only, choose the chat, copy",
                    None,
                    None,
                    Kind::Quiet,
                    true,
                )
                .h(px(28.))
                .px(px(6.))
                .on_click(move |event, _, cx| more(FootEvent::More(event.position()), cx))
                .into_any_element(),
            );
        }
        let add = on.clone();
        // ⌘↵ is the composer's: its primary says so wherever there is room for it, the smallest box its tooltip (#183).
        let small = look.room == Room::S;
        let add_button = discussion::button(
            "compose-add",
            primary,
            (!small).then_some("⌘↵"),
            Kind::Primary(tone),
        )
        .h(px(28.))
        .px(px(10.))
        .when(small, |s| {
            s.tooltip_show_delay(hover::LONG).tooltip(move |_, cx| {
                cx.new(|_| {
                    hover::Tip(if agent {
                        "Add to drafts  ⌘↵"
                    } else {
                        "Comment  ⌘↵"
                    })
                })
                .into()
            })
        })
        .on_click(move |_, _, cx| add(FootEvent::Add, cx));
        // At S, what folded: into the primary's own ▾.
        let more = folded && (agent || request.is_some());
        tail.push(if !more {
            add_button.into_any_element()
        } else {
            div()
                .flex()
                .flex_none()
                .gap(px(1.))
                .child(add_button.rounded_r(px(0.)))
                .child(
                    discussion::icon_button(
                        "compose-add-more",
                        "chevron",
                        if agent {
                            "Send all, send this only, choose the chat, copy"
                        } else {
                            "Add to review"
                        },
                        None,
                        None,
                        Kind::Primary(tone),
                        true,
                    )
                    .h(px(28.))
                    .px(px(6.))
                    .rounded_l(px(0.))
                    .on_click(move |event, _, cx| on(FootEvent::Folded(event.position()), cx)),
                )
                .into_any_element()
        });
        tail
    }

    /// A reply's foot, the same as a new comment's (#129) at `room`: which of the two it writes on a discussion,
    /// a reply or a note for the agent, and ⇥ to switch (#50 G); Cancel and the primary (Send answer in the Inbox,
    /// `inbox`). With its placeholder.
    pub fn reply_foot(
        &self,
        room: Room,
        inbox: bool,
        on: OnFoot,
    ) -> (Option<AnyElement>, Vec<AnyElement>, &'static str) {
        // ⇥ turned it into a note for the agent: purple, and it stays on this Mac (#50 G).
        let agent = self.source() == Source::Agent;
        let tone = self.source().color();
        let (cancel, review) = (on.clone(), on.clone());
        // Narrow, Cancel is its ×.
        let narrow = room != Room::L;
        let mut tail = vec![
            if narrow {
                discussion::icon_button(
                    "reply-cancel",
                    "x",
                    "Cancel",
                    Some("esc"),
                    None,
                    Kind::Quiet,
                    true,
                )
            } else {
                discussion::button("reply-cancel", "Cancel", None, Kind::Quiet)
                    .tooltip_show_delay(hover::LONG)
                    .tooltip(|_, cx| cx.new(|_| hover::Tip("Esc")).into())
            }
            .h(px(28.))
            .px(px(if narrow { 6. } else { 10. }))
            .on_click(move |_, _, cx| cancel(FootEvent::Cancel, cx))
            .into_any_element(),
        ];
        // A reply on the request can wait for the review too (#200): ⇧⌘↵.
        if matches!(self.target, Target::Reply(_)) && !inbox {
            tail.push(
                discussion::icon_button(
                    "reply-review",
                    "layers",
                    "Add to review",
                    Some("⇧⌘↵"),
                    Some("Pending until you submit the review"),
                    Kind::Quiet,
                    narrow,
                )
                .h(px(28.))
                .px(px(if narrow { 6. } else { 10. }))
                .on_click(move |_, _, cx| review(FootEvent::AddToReview, cx))
                .into_any_element(),
            );
        }
        let (send, more) = (on.clone(), on.clone());
        // A reply on a discussion has its own ▾: Reply and resolve ⌥⌘↵, or Reply and reopen (#199); under L the
        // room it takes is its key's, said in the tooltip.
        let with_more = matches!(self.target, Target::Reply(_)) && !agent;
        let cap = room == Room::L || (room == Room::M && !with_more);
        let reply = discussion::button(
            "reply-send",
            if agent && inbox && matches!(self.target, Target::Answer(_)) {
                "Send answer"
            } else if agent {
                "Add to drafts"
            } else {
                "Reply"
            },
            cap.then_some("⌘↵"),
            Kind::Primary(tone),
        )
        .h(px(28.))
        .px(px(10.))
        .when(!cap, |s| {
            s.tooltip_show_delay(hover::LONG)
                .tooltip(|_, cx| cx.new(|_| hover::Tip("Reply  ⌘↵")).into())
        })
        .on_click(move |_, _, cx| send(FootEvent::Add, cx));
        tail.push(if with_more {
            div()
                .flex()
                .flex_none()
                .gap(px(1.))
                .child(reply.rounded_r(px(0.)))
                .child(
                    discussion::icon_button(
                        "reply-more",
                        "chevron",
                        "Reply and resolve, or reopen",
                        None,
                        None,
                        Kind::Primary(tone),
                        true,
                    )
                    .h(px(28.))
                    .px(px(6.))
                    .rounded_l(px(0.))
                    .on_click(move |event, _, cx| more(FootEvent::Folded(event.position()), cx)),
                )
                .into_any_element()
        } else {
            reply.into_any_element()
        });
        // Who reads it, in words at every width (#200): shorter when narrow.
        let lead = matches!(self.target, Target::Reply(_) | Target::ThreadNote(_)).then(|| {
            let said = match (agent, narrow) {
                (true, false) => "🔒 For your agent",
                (true, true) => "🔒 Agent",
                (false, false) => "Reply on the merge request",
                (false, true) => "Everyone",
            };
            div()
                .id("reply-audience")
                .h(px(26.))
                .px(px(8.))
                .flex()
                .flex_none()
                .items_center()
                .gap(px(6.))
                .rounded(px(6.))
                .bg(if agent {
                    theme::agent_bg()
                } else {
                    theme::warning_bg()
                })
                .text_size(px(12.))
                .text_color(tone)
                .whitespace_nowrap()
                .child(said)
                .map(|s| {
                    if narrow {
                        s.tooltip_show_delay(hover::LONG).tooltip(move |_, cx| {
                            cx.new(|_| hover::TipOwned(format!("{said} — ⇥ switches").into()))
                                .into()
                        })
                    } else {
                        s.child(keys::caps("⇥", keys::On::Plain))
                    }
                })
                .into_any_element()
        });
        let placeholder = match (self.target, room) {
            (Target::ThreadNote(_), _) => "Note for your agent",
            (Target::Answer(_), Room::S) => "Note with your answer",
            (Target::Answer(_), _) => "A note for the agent with your answer",
            (Target::AnswerValue(_), _) => "Your own value",
            _ => "Reply…",
        };
        (lead, tail, placeholder)
    }

    /// An edit's foot: Cancel and Save, its keys unless the diff is `narrow`.
    pub fn edit_foot(&self, narrow: bool, on: OnFoot) -> Vec<AnyElement> {
        let cancel = on.clone();
        vec![
            discussion::button("edit-cancel", "Cancel", None, Kind::Quiet)
                .h(px(28.))
                .px(px(10.))
                .tooltip_show_delay(hover::LONG)
                .tooltip(|_, cx| cx.new(|_| hover::Tip("Esc")).into())
                .on_click(move |_, _, cx| cancel(FootEvent::Cancel, cx))
                .into_any_element(),
            discussion::button(
                "edit-save",
                "Save",
                (!narrow).then_some("⌘↵"),
                Kind::Primary(self.source().color()),
            )
            .h(px(28.))
            .px(px(10.))
            .on_click(move |_, _, cx| on(FootEvent::Add, cx))
            .into_any_element(),
        ]
    }

    /// The box `input` of a comment on several lines: which, over it.
    pub fn with_lines(&self, input: Div) -> Div {
        match self.first {
            Some(first) => div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(4.))
                .child(
                    div()
                        .font_family(theme::ui_font())
                        .text_size(px(12.))
                        .text_color(theme::muted())
                        .child(format!("Lines {first}–{}", self.line)),
                )
                .child(input),
            None => input,
        }
    }
}

/// What a composer's foot shows, by its box's width (Design shell round 6, 2026-10-24, #129). The
/// foot is one row at each: what does not fit folds, nothing wraps and nothing is cut.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Room {
    /// 720 px and up: who sees it in words, the helpers, every button with its label and keys.
    L,
    /// The secondary buttons are their icons, who sees it its dot, the keys go.
    M,
    /// Under 440 px: the helpers fold into T, Send and ▾ into the primary's own ▾; the hint is
    /// short. (Drawn at 420; a request's audience chip with its ▾ needs 423 for M's row.)
    S,
}

/// The room of a box `width` wide; 0 is not measured yet. A tight diff is never L.
pub fn room_for(width: f32, tight: bool) -> Room {
    match width {
        w if w > 0. && w < 440. => Room::S,
        w if tight || (w > 0. && w < 720.) => Room::M,
        _ => Room::L,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_composer_folds_its_foot_by_its_own_width() {
        assert_eq!(room_for(0., false), Room::L);
        assert_eq!(room_for(760., false), Room::L);
        assert_eq!(room_for(630., false), Room::M);
        assert_eq!(room_for(440., false), Room::M);
        assert_eq!(room_for(439., false), Room::S);
        assert_eq!(room_for(136., false), Room::S);
        assert_eq!(room_for(900., true), Room::M);
        assert_eq!(room_for(300., true), Room::S);
    }
}
