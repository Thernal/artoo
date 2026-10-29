//! What hover shows (#53, Design `docs/specs/hover.md`): tooltips on a long hover (`LONG`, never on a
//! pointer passing over) — the whole of cut text, the date behind a relative time, what a short form
//! stands for. A card (a commit, a person, a branch) is its feature's, shown with `Hover::card`.

use super::{ROW_RADIUS, format, keys, motion, px, theme};
use gpui::{
    AnyView, App, Context, ElementId, IntoElement, Render, SharedString,
    StatefulInteractiveElement, Window, canvas, div, prelude::*,
};
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

/// How long the pointer rests before a tooltip or a card shows.
pub const LONG: Duration = Duration::from_millis(800);

/// A tooltip or a card on a long hover.
pub trait Hover: StatefulInteractiveElement + Sized {
    /// One line of text.
    fn tip(self, text: impl Into<SharedString>) -> Self {
        let text = text.into();
        self.tooltip_show_delay(LONG)
            .tooltip(move |_, cx| cx.new(|_| TipOwned(text.clone())).into())
    }

    /// A card that stays while the pointer is on it.
    fn card(self, build: impl Fn(&mut Window, &mut App) -> AnyView + 'static) -> Self {
        self.tooltip_show_delay(LONG).hoverable_tooltip(build)
    }
}

impl<E: StatefulInteractiveElement> Hover for E {}

/// A tooltip with nothing to say draws nothing.
pub struct NoTip;

impl Render for NoTip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

/// Text cut to its room with an ellipsis, whose long hover gives the whole of it — only when it is
/// cut this time (X60): a canvas over it measures the text against the room it has.
pub fn cut(id: impl Into<ElementId>, text: impl Into<SharedString>) -> gpui::Stateful<gpui::Div> {
    cut_to(id, text, 1)
}

/// The same for text clamped to `lines` lines.
pub fn cut_to(
    id: impl Into<ElementId>,
    text: impl Into<SharedString>,
    lines: usize,
) -> gpui::Stateful<gpui::Div> {
    let text: SharedString = text.into();
    cut_shown(id, text.clone(), text.clone(), text, lines)
}

/// `shown` — text drawn its own way, such as selectable — cut to its room, measured as `text`.
pub fn cut_with(
    id: impl Into<ElementId>,
    text: impl Into<SharedString>,
    shown: impl IntoElement,
) -> gpui::Stateful<gpui::Div> {
    let text: SharedString = text.into();
    cut_shown(id, shown, text.clone(), text, 1)
}

/// `shown` cut to its room, measured as `text`; `whole` in its long hover when the room cut it or
/// `text` is already shorter than `whole`.
fn cut_shown(
    id: impl Into<ElementId>,
    shown: impl IntoElement,
    text: SharedString,
    whole: SharedString,
    lines: usize,
) -> gpui::Stateful<gpui::Div> {
    let shortened = text != whole;
    let is_cut = Rc::new(Cell::new(shortened));
    let measure = (is_cut.clone(), text);
    let shown = div().child(shown);
    div()
        .id(id)
        .relative()
        .min_w_0()
        .overflow_hidden()
        .child(if lines > 1 {
            shown.line_clamp(lines)
        } else {
            shown.truncate()
        })
        .child(
            canvas(
                move |bounds, window, _| {
                    let (is_cut, text) = measure;
                    let style = window.text_style();
                    let size = style.font_size.to_pixels(window.rem_size());
                    let room = bounds.size.width + gpui::px(0.5);
                    let cut = if lines > 1 {
                        let run = style.to_run(text.len());
                        window
                            .text_system()
                            .shape_text(text, size, &[run], Some(room), None)
                            .map(|l| l.iter().map(|l| l.wrap_boundaries.len() + 1).sum::<usize>())
                            .is_ok_and(|n| n > lines)
                    } else {
                        let line = SharedString::from(text.replace('\n', " "));
                        let run = style.to_run(line.len());
                        window
                            .text_system()
                            .shape_line(line, size, &[run], None)
                            .width
                            > room
                    };
                    is_cut.set(shortened || cut);
                },
                |_, _, _, _| {},
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full(),
        )
        .tooltip_show_delay(LONG)
        .tooltip(move |_, cx| {
            if is_cut.get() {
                cx.new(|_| TipOwned(whole.clone())).into()
            } else {
                cx.new(|_| NoTip).into()
            }
        })
}

/// Text cut in the middle to `n` characters (`format::mid`), so a long branch name keeps both
/// ends, then to its room: its long hover gives the whole when either cut it.
pub fn mid(id: impl Into<ElementId>, text: &str, n: usize) -> gpui::Stateful<gpui::Div> {
    shortened(id, format::mid(text, n), text.to_owned())
}

/// `shown`, a shortened `whole` (a name cut in the middle within a line), cut to its room: its
/// long hover gives `whole` when they differ or the room cut it.
pub fn shortened(
    id: impl Into<ElementId>,
    shown: impl Into<SharedString>,
    whole: impl Into<SharedString>,
) -> gpui::Stateful<gpui::Div> {
    let shown: SharedString = shown.into();
    cut_shown(id, shown.clone(), shown, whole.into(), 1)
}

/// Text cut to its room inside an element whose own long hover already gives the whole of it (a
/// row's tooltip): a tooltip of its own, even an empty one, would hide that one. Everything else
/// is cut with `cut`, `cut_to`, `mid` or `shortened` (#161).
pub trait CutInTip: Styled + Sized {
    fn cut_in_tip(self) -> Self {
        self.truncate()
    }

    /// The same for text clamped to `lines` lines.
    fn clamp_in_tip(self, lines: usize) -> Self {
        self.overflow_hidden().line_clamp(lines)
    }
}

impl<E: Styled> CutInTip for E {}

/// "Thu 16 Oct 2026, 11:24": the date behind a relative time, in local time (X61).
pub fn full_date(time: i64) -> String {
    full_date_at(time + format::local_offset(time))
}

/// The date of `time` as it is, written out.
fn full_date_at(time: i64) -> String {
    const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let days = time.div_euclid(86400);
    let secs = time.rem_euclid(86400);
    let date = format::date_at(time);
    let (y, m, d) = (&date[..4], &date[5..7], &date[8..10]);
    let month = m.parse::<usize>().unwrap_or(1).clamp(1, 12);
    format!(
        "{} {} {} {y}, {:02}:{:02}",
        DAYS[(days + 4).rem_euclid(7) as usize],
        d.trim_start_matches('0'),
        MONTHS[month - 1],
        secs / 3600,
        secs % 3600 / 60
    )
}

/// A tooltip: a small panel with a line of text.
pub struct Tip(pub &'static str);

/// A tooltip whose text is made when it is shown (blame's: the commit's title, hash and age).
pub struct TipOwned(pub SharedString);

impl Render for TipOwned {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let tip = div()
            .mt(px(6.))
            // Long text wraps rather than leaving the window.
            .max_w(px(440.))
            .px_2()
            .py_1()
            .rounded(px(ROW_RADIUS))
            .border_1()
            .border_color(theme::island_border())
            .bg(theme::hover())
            .text_size(px(11.))
            .text_color(theme::text())
            .child(SharedString::from(keys::text(&self.0).into_owned()));
        // It fades in after the long rest (#56, 7).
        motion::appear(tip, "tip-in", motion::FAST, |tip, v| tip.opacity(v))
    }
}

impl Render for Tip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let tip = div()
            .mt(px(6.))
            .px_2()
            .py_1()
            .rounded(px(ROW_RADIUS))
            .border_1()
            .border_color(theme::island_border())
            .bg(theme::hover())
            .text_size(px(11.))
            .text_color(theme::text())
            .child(keys::text(self.0).into_owned());
        motion::appear(tip, "tip-in", motion::FAST, |tip, v| tip.opacity(v))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_time_reads_as_a_date() {
        // 2026-10-16 11:24 UTC was a Friday; four hours on, 15:24.
        assert_eq!(full_date_at(1_792_149_840), "Fri 16 Oct 2026, 11:24");
        assert_eq!(
            full_date_at(1_792_149_840 + 4 * 3600),
            "Fri 16 Oct 2026, 15:24"
        );
    }
}
