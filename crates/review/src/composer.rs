//! The composer (#9, #51, #129, #168): a comment being written — under a line of the diff, on a page, a file or a
//! folder in Code, for the agent, a merge request, a reply or an edit — and its own editing: completion, markdown
//! helpers, preview —, and the one input component it is drawn in. Who sees it, the buttons along its edge, where
//! it is drawn, its keys routed and what it writes sent are the tab's.

use crate::complete::Choice;
use artoo_core::markdown::Helper;
use artoo_core::textedit::TextEdit;
use artoo_ui_kit::input::{self, Edit};
use artoo_ui_kit::{hover, icons, markdown, px, theme};
use gpui::{AnyElement, App, Div, Keystroke, Pixels, Point, Rgba, div, prelude::*};
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
    pub placeholder: &'static str,
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
        placeholder: &'static str,
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
