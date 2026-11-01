//! How the agents are drawn where the review shows them (#151): a chat's badge, an agent's note under its line in
//! the agent's colour, apart from the reviewer's comments, and the summary of its notes over the diff. What a click
//! asks goes to the tab (`NoteEvent`).

use crate::notes::{Note, Notes};
use artoo_ui_kit::discussion::{self, Kind};
use artoo_ui_kit::format::plural;
use artoo_ui_kit::markdown::{self, Selecting};
use artoo_ui_kit::{hover, px, theme};
use gpui::{AnyElement, App, FontWeight, Rgba, div, prelude::*};

/// What a note is drawn with and a click on it asks, the contract's.
pub use artoo_agents_api::view::{NoteEvent, NoteLook, OnNote};

/// A chat's mark: its letter on its hue, hollow when it is gone.
pub fn chat_badge(letter: char, hue: Rgba, gone: bool) -> gpui::Div {
    div()
        .flex_none()
        .size(px(18.))
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(10.))
        .font_weight(FontWeight::BOLD)
        .border_1()
        .border_color(hue)
        .when(!gone, |s| s.bg(hue).text_color(theme::base()))
        .when(gone, |s| s.text_color(hue))
        .child(letter.to_string())
}

/// "limited" beside a Codex chat (#18; the developer, 2026-10-23: not "beta"), what it cannot do yet in its tooltip.
pub fn limited(id: impl Into<gpui::ElementId>) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .flex_none()
        .px(px(5.))
        .rounded(px(4.))
        .border_1()
        .border_color(theme::island_border())
        .font_family(theme::ui_font())
        .text_size(px(10.))
        .text_color(theme::faint())
        .child("limited")
        .tooltip_show_delay(hover::LONG)
        .tooltip(|_, cx| cx.new(|_| hover::Tip(crate::codex::LIMITS)).into())
}

/// Note `note` of `notes` under its line, `indent` from the left.
pub fn note_card(
    notes: &Notes,
    note: &Note,
    indent: f32,
    look: NoteLook,
    on: OnNote,
    cx: &App,
) -> AnyElement {
    let id = note.id;
    let (letter, hue) = look.mark;
    let (answer, dismiss) = (on.clone(), on);
    let bar = div()
        .flex()
        .items_center()
        .gap(px(8.))
        .child(chat_badge(letter, hue, false))
        .child(
            div()
                .flex_none()
                .font_weight(FontWeight::SEMIBOLD)
                .child(notes.agent.clone()),
        )
        .child(
            hover::cut("note-hint", "· a note on its change")
                .flex_1()
                .text_color(theme::muted()),
        )
        .child(
            discussion::icon_button(
                ("note-answer", id as usize),
                "message",
                "Answer",
                None,
                Some("A comment for the agent on this line"),
                Kind::Plain,
                look.tight,
            )
            .h(px(24.))
            .px(px(8.))
            .on_click(move |_, _, cx| {
                cx.stop_propagation();
                answer(NoteEvent::Answer(id), cx);
            }),
        )
        .child(
            discussion::icon_button(
                ("note-dismiss", id as usize),
                "x",
                "Dismiss",
                None,
                None,
                Kind::Quiet,
                true,
            )
            .h(px(24.))
            .px(px(6.))
            .on_click(move |_, _, cx| {
                cx.stop_propagation();
                dismiss(NoteEvent::Dismiss(id), cx);
            }),
        );
    div()
        .w_full()
        .pl(px(indent))
        .pr(px(12.))
        .py(px(6.))
        .child(note_frame(hue).child(bar).child(markdown::body(
            &note.body,
            look.rich,
            format!("md-agent-note-{id}"),
            None,
            Some(look.selecting),
            cx,
        )))
        .into_any_element()
}

/// Over the diff, the summary of each agent's notes on these changes (`shown`, each with its agent's mark): the
/// mark, how many notes on how many files, the summary, Dismiss all; `gap` apart and under.
pub fn notes_summary(
    shown: &[(&Notes, (char, Rgba))],
    rich: bool,
    selecting: Selecting,
    gap: f32,
    on: OnNote,
    cx: &App,
) -> Option<AnyElement> {
    if shown.is_empty() {
        return None;
    }
    let cards = shown.iter().map(|&(n, (letter, hue))| {
        let mut files: Vec<&str> = n.notes.iter().map(|x| x.path.as_str()).collect();
        files.sort_unstable();
        files.dedup();
        let count = match n.notes.len() {
            0 => String::new(),
            notes => format!(
                "· {} on {}",
                plural(notes, "note"),
                plural(files.len(), "file")
            ),
        };
        let (dismiss, uid) = (on.clone(), n.uid.clone());
        note_frame(hue)
            .max_w_full()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(chat_badge(letter, hue, false))
                    .child(
                        div()
                            .flex_none()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(format!("{}'s notes", n.agent)),
                    )
                    .child(
                        hover::cut("notes-count", count)
                            .flex_1()
                            .text_color(theme::muted()),
                    )
                    .child(
                        discussion::button(
                            gpui::SharedString::from(format!("notes-dismiss-{}", n.uid)),
                            "Dismiss all",
                            None,
                            Kind::Quiet,
                        )
                        .h(px(24.))
                        .px(px(8.))
                        .on_click(move |_, _, cx| dismiss(NoteEvent::DismissAll(uid.clone()), cx)),
                    ),
            )
            // A long one scrolls in its card; the diff keeps the room.
            .children((!n.summary.is_empty()).then(|| {
                div()
                    .id(gpui::SharedString::from(format!("notes-summary-{}", n.uid)))
                    .max_h(px(160.))
                    .overflow_y_scroll()
                    .child(markdown::body(
                        &n.summary,
                        rich,
                        format!("md-notes-summary-{}", n.uid),
                        None,
                        Some(selecting.clone()),
                        cx,
                    ))
            }))
    });
    Some(
        div()
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(gap))
            .pb(px(gap))
            .children(cards)
            .into_any_element(),
    )
}

/// A note's frame: a discussion's, its rail in the agent's colour.
fn note_frame(hue: Rgba) -> gpui::Div {
    const RAIL: f32 = 3.;
    div()
        .w_full()
        .max_w(px(720.))
        .flex()
        .flex_col()
        .gap(px(6.))
        .rounded(px(10.))
        .bg(theme::panel())
        .border_l(px(RAIL))
        .border_color(hue)
        .pl(px(14. - RAIL))
        .pr(px(14.))
        .pt(px(8.))
        .pb(px(10.))
        .font_family(theme::ui_font())
        .text_size(px(13.))
        .line_height(px(20.))
        .text_color(theme::text())
}
