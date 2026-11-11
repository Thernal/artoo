//! What hover shows (#53, Design `docs/specs/hover.md`): tooltips on a long hover (`LONG`, never on a
//! pointer passing over) — the whole of cut text, the date behind a relative time, what a short form
//! stands for. A card (a commit, a person, a branch) is its feature's, shown with `Hover::card`.

use super::{ROW_RADIUS, format, keys, motion, px, theme};
use gpui::{
    AnyView, App, Bounds, Context, ElementId, Global, IntoElement, Pixels, Render, SharedString,
    StatefulInteractiveElement, Window, WindowId, canvas, div, prelude::*,
};
use std::cell::Cell;
use std::collections::HashMap;
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

/// A row's tooltip that F1 also opens, at the row, while the row is picked (#184): "Show the whole text".
pub trait RowTip: Hover + ParentElement {
    /// The tooltip `text`; while `picked`, the row says where it is drawn, so F1 finds it.
    fn row_tip(self, picked: bool, text: impl Into<SharedString>) -> Self {
        let text: SharedString = text.into();
        self.tip(text.clone()).row_whole(picked, text)
    }

    /// What F1 opens while the row is `picked`, for a row whose long hover is a card (a commit's).
    fn row_whole(self, picked: bool, text: impl Into<SharedString>) -> Self {
        if !picked {
            return self;
        }
        let text: SharedString = text.into();
        // Over the row and the row's size; it draws nothing and takes no pointer.
        self.child(
            canvas(
                move |bounds, window, cx| picked_drawn(text, bounds, window, cx),
                |_, _, _, _| {},
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full(),
        )
    }
}

impl<E: Hover + ParentElement> RowTip for E {}

/// The rows drawn picked, by window (#184): each list has its own, and the one picked last is the one
/// the keyboard or a click is on.
#[derive(Default)]
struct Picked(HashMap<WindowId, Drawn>);

impl Global for Picked {}

#[derive(Default)]
struct Drawn {
    /// The window's drawings, counted (`drawing`).
    draw: u64,
    rows: Vec<PickedRow>,
}

struct PickedRow {
    text: SharedString,
    bounds: Bounds<Pixels>,
    /// The drawing it was first picked in, and the last it was drawn in.
    since: u64,
    seen: u64,
}

/// A new drawing of `window` begins: its root calls this first, so the rows picked are counted per
/// drawing.
pub fn drawing(window: &Window, cx: &mut App) {
    let drawn = cx
        .default_global::<Picked>()
        .0
        .entry(window.window_handle().window_id())
        .or_default();
    drawn.draw += 1;
    let draw = drawn.draw;
    drawn.rows.retain(|r| r.seen + 1 >= draw);
}

fn picked_drawn(text: SharedString, bounds: Bounds<Pixels>, window: &Window, cx: &mut App) {
    let drawn = cx
        .default_global::<Picked>()
        .0
        .entry(window.window_handle().window_id())
        .or_default();
    let draw = drawn.draw;
    match drawn.rows.iter_mut().find(|r| r.text == text) {
        Some(row) => {
            // Not drawn picked last time: picked again now.
            if row.seen + 1 < draw {
                row.since = draw;
            }
            row.seen = draw;
            row.bounds = bounds;
        }
        None => drawn.rows.push(PickedRow {
            text,
            bounds,
            since: draw,
            seen: draw,
        }),
    }
}

/// The whole text of the row picked last of those on screen, and where it is: what F1 opens.
pub fn picked_row(window: &Window, cx: &App) -> Option<(SharedString, Bounds<Pixels>)> {
    let drawn = cx
        .try_global::<Picked>()?
        .0
        .get(&window.window_handle().window_id())?;
    drawn
        .rows
        .iter()
        .filter(|r| r.seen == drawn.draw)
        .max_by_key(|r| r.since)
        .map(|r| (r.text.clone(), r.bounds))
}

/// The whole text F1 opens at a row (#184): wrapped, every line of it, `⌘C` copies it and `esc` closes.
pub fn whole_card(text: &str) -> gpui::Div {
    card_frame()
        .mt(px(2.))
        .w(px(440.))
        .gap(px(4.))
        .children(
            text.lines()
                .map(|line| div().child(SharedString::from(line.to_owned()))),
        )
        .child(
            div()
                .mt(px(4.))
                .flex()
                .items_center()
                .gap(px(6.))
                .text_size(px(11.))
                .text_color(theme::faint())
                .child(keys::caps("⌘C", keys::On::Plain))
                .child("copy")
                .child(div().flex_1())
                .child(keys::caps("esc", keys::On::Plain)),
        )
}

/// A tooltip with nothing to say draws nothing.
pub struct NoTip;

impl Render for NoTip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

/// Where a text one line long loses its characters when the room is short (#184): prose at its end, a
/// name (a branch, a file alone) in its middle, a path at its front so its file name stays.
#[derive(Clone, Copy, PartialEq, Eq)]
enum CutAt {
    End,
    Middle,
    /// The text cuts itself: `shown` is laid out in parts, each with its own cut (`path`).
    Parts,
}

/// Text cut to its room with an ellipsis, whose long hover gives the whole of it — only when it is
/// cut this time (X60): a canvas over it measures the text against the room it has.
pub fn cut(id: impl Into<ElementId>, text: impl Into<SharedString>) -> gpui::Stateful<gpui::Div> {
    cut_to(id, text, 1)
}

/// The same for text clamped to `lines` lines, its last line ending in an ellipsis.
pub fn cut_to(
    id: impl Into<ElementId>,
    text: impl Into<SharedString>,
    lines: usize,
) -> gpui::Stateful<gpui::Div> {
    let text: SharedString = text.into();
    cut_shown(id, text.clone(), text.clone(), text, lines, CutAt::End)
}

/// `shown` — text drawn its own way, such as selectable — cut to its room, measured as `text`.
pub fn cut_with(
    id: impl Into<ElementId>,
    text: impl Into<SharedString>,
    shown: impl IntoElement,
) -> gpui::Stateful<gpui::Div> {
    let text: SharedString = text.into();
    cut_shown(id, shown, text.clone(), text, 1, CutAt::End)
}

/// A name — a branch, a file without its folders — cut in its middle to its room, so names that start
/// alike keep the endings they differ by: `feature/AND-2041-log…ogin-state` (#184).
pub fn name(id: impl Into<ElementId>, text: impl Into<SharedString>) -> gpui::Stateful<gpui::Div> {
    let text: SharedString = text.into();
    cut_shown(id, text.clone(), text.clone(), text, 1, CutAt::Middle)
}

/// A request's branches, `source → target`: the source cut in its middle (`name`), so two that start alike
/// keep their endings, and the target whole (#184).
pub fn branches(id: impl Into<ElementId>, source: &str, target: &str) -> gpui::Div {
    div()
        .flex()
        .min_w_0()
        .child(name(id, source.to_owned()))
        .child(div().flex_none().child(format!(" → {target}")))
}

/// A file's path cut to its room at its front, so its file name stays:
/// `…/presentation/viewmodel/impl/LoginViewModelImpl.kt`; a file name longer than the room alone is cut
/// in its middle, its extension kept: `LoginStateViewMo…grationTest.kt` (#184).
pub fn path(id: impl Into<ElementId>, path: impl Into<SharedString>) -> gpui::Stateful<gpui::Div> {
    let whole: SharedString = path.into();
    // A folder (`core/network/`) is all front; a file's name follows its last slash.
    let (folder, file) = match whole.rfind('/') {
        Some(at) => (&whole[..=at], &whole[at + 1..]),
        None => ("", &whole[..]),
    };
    let parts = div()
        .flex()
        .min_w_0()
        .overflow_hidden()
        .whitespace_nowrap()
        .when(!folder.is_empty(), |d| {
            d.child(
                div()
                    .flex_shrink(1.)
                    .min_w_0()
                    .overflow_hidden()
                    // Cut at its front, what is left of the room goes before the ellipsis, never
                    // between the folder and the name.
                    .text_right()
                    .text_ellipsis_start()
                    .child(SharedString::from(folder.to_owned())),
            )
        })
        .when(!file.is_empty(), |d| {
            d.child(
                div()
                    .flex_none()
                    .max_w_full()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis_middle()
                    .child(SharedString::from(file.to_owned())),
            )
        });
    cut_shown(id, parts, whole.clone(), whole, 1, CutAt::Parts)
}

/// `shown` cut to its room, measured as `text`; `whole` in its long hover when the room cut it or
/// `text` is already shorter than `whole`.
fn cut_shown(
    id: impl Into<ElementId>,
    shown: impl IntoElement,
    text: SharedString,
    whole: SharedString,
    lines: usize,
    from: CutAt,
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
            shown.line_clamp(lines).text_ellipsis()
        } else {
            match from {
                CutAt::End => shown.truncate(),
                CutAt::Middle => shown
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis_middle(),
                CutAt::Parts => shown.min_w_0().overflow_hidden(),
            }
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

/// `shown`, a shortened `whole` (a name cut by hand within a line), cut to its room: its
/// long hover gives `whole` when they differ or the room cut it.
pub fn shortened(
    id: impl Into<ElementId>,
    shown: impl Into<SharedString>,
    whole: impl Into<SharedString>,
) -> gpui::Stateful<gpui::Div> {
    let shown: SharedString = shown.into();
    cut_shown(id, shown.clone(), shown, whole.into(), 1, CutAt::End)
}

/// Text cut to its room inside an element whose own long hover already gives the whole of it (a
/// row's tooltip): a tooltip of its own, even an empty one, would hide that one. Everything else
/// is cut with `cut`, `cut_to`, `name`, `path`, `cut_with` or `shortened` (#161, #184).
pub trait CutInTip: Styled + Sized {
    fn cut_in_tip(self) -> Self {
        self.truncate()
    }

    /// A name (a branch, a file alone) cut in its middle (`name`).
    fn name_in_tip(self) -> Self {
        self.overflow_hidden()
            .whitespace_nowrap()
            .text_ellipsis_middle()
    }

    /// A folder or a path cut at its front, its end kept (`path`).
    fn path_in_tip(self) -> Self {
        self.overflow_hidden()
            .whitespace_nowrap()
            .text_ellipsis_start()
    }

    /// The same for text clamped to `lines` lines, its last line ending in an ellipsis.
    fn clamp_in_tip(self, lines: usize) -> Self {
        self.overflow_hidden().line_clamp(lines).text_ellipsis()
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

/// The frame every card is drawn in.
pub fn card_frame() -> gpui::Div {
    div()
        .mt(px(6.))
        .w(px(360.))
        .p(px(12.))
        .rounded(px(10.))
        .border_1()
        .border_color(theme::island_border())
        .bg(theme::panel())
        .shadow_lg()
        .flex()
        .flex_col()
        .gap(px(8.))
        .text_size(px(12.))
        .text_color(theme::text())
}

/// One of a card's actions: Copy, Open ↗, Mention.
pub fn card_action(id: &'static str, label: &'static str) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .px(px(8.))
        .h(px(24.))
        .flex()
        .items_center()
        .rounded(px(ROW_RADIUS))
        .border_1()
        .border_color(theme::island_border())
        .cursor_pointer()
        .hover(|s| s.bg(theme::hover()))
        .child(label)
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
