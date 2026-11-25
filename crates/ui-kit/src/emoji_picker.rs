//! The emoji picker (#91, #240): a search over every emoji GitLab has, your five most used first, then GitLab's
//! groups; arrows and ↵, the picked one's `:name:` at its foot; `:name:` typed whole picks a name the list lacks
//! (a custom one). On GitHub's eight reactions, no search. What it picks is its owner's: a reaction on a note, or
//! an emoji put into a comment being written (the composer's ☺).

use crate::hover::{self, TipOwned};
use crate::input::{self, Look};
use crate::{ROW_RADIUS, motion, px, theme};
use artoo_core::textedit::TextEdit;
use artoo_core::{emoji, storage};
use gpui::{AnyElement, App, KeyDownEvent, UniformListScrollHandle, div, prelude::*};
use std::borrow::Cow;
use std::cell::Cell;
use std::ops::Range;
use std::rc::Rc;

/// The file the reactions used are counted in, on this Mac.
pub const USED: &str = "reactions-used.txt";
/// The picker's cells to a row, and a row's height.
const COLS: usize = 12;
const ROW: f32 = 30.;
/// How many rows show before it scrolls.
const ROWS_SHOWN: usize = 8;
/// Until there is a history: the five most people reach for.
const DEFAULT_FIVE: [&str; 5] = ["thumbsup", "heart", "tada", "eyes", "rocket"];

/// The picker open: its search with its caret and sideways offset, the emoji picked, the list's scroll.
#[derive(Default)]
pub struct Picker {
    query: String,
    at: usize,
    scroll: UniformListScrollHandle,
    edit: TextEdit,
    offset: Rc<Cell<f32>>,
}

/// What a key in the picker asks.
pub enum PickerKey {
    /// ↵: the one picked, none when nothing matches.
    Pick(Option<String>),
    /// esc.
    Close,
}

/// A click picked the emoji of this name.
pub type Pick = Rc<dyn Fn(String, &mut App)>;
/// A press outside it.
pub type Close = Rc<dyn Fn(&mut App)>;
/// A change to the picker where its owner keeps it (a click in its search), the owner drawn again.
pub type Change = Rc<dyn Fn(Edit, &mut App)>;
/// One such change.
pub type Edit = Rc<dyn Fn(&mut Picker)>;

/// How its owner has it drawn.
pub struct Draw {
    /// The owner's own number, which its element ids are told apart by.
    pub id: u64,
    /// GitHub's eight reactions only.
    pub github: bool,
    /// The keys, at its foot.
    pub keys: &'static str,
    pub pick: Pick,
    pub close: Close,
    pub change: Change,
}

impl Picker {
    /// Keys while it is open: typing searches, arrows move (↑ ↓ by row, across the groups); ↵ picks, esc
    /// closes.
    pub fn key(&mut self, event: &KeyDownEvent, github: bool, cx: &mut App) -> Option<PickerKey> {
        let g = self;
        let key = &event.keystroke;
        let (names, rows) = grid(&g.query, github);
        let at = g.at.min(names.len().saturating_sub(1));
        let moved = match key.key.as_str() {
            "left" => Some(at.saturating_sub(1)),
            "right" => Some((at + 1).min(names.len().saturating_sub(1))),
            "up" => Some(row_step(&rows, at, false)),
            "down" => Some(row_step(&rows, at, true)),
            _ => None,
        };
        if let Some(to) = moved.filter(|_| key.modifiers.number_of_modifiers() == 0) {
            g.at = to;
            if let Some(row) = row_of(&rows, to) {
                g.scroll.scroll_to_item(row, gpui::ScrollStrategy::Nearest);
            }
            return None;
        }
        match key.key.as_str() {
            "escape" => return Some(PickerKey::Close),
            "enter" => return Some(PickerKey::Pick(names.get(at).map(|n| n.to_string()))),
            _ => {}
        }
        if github {
            return None;
        }
        if matches!(
            input::edit(&mut g.query, &mut g.edit, key, cx),
            input::Edit::Changed
        ) {
            g.at = 0;
            g.scroll.scroll_to_item(0, gpui::ScrollStrategy::Top);
        }
        None
    }

    /// The picker drawn: the search, the emoji in rows (only those on screen are built), the name of
    /// the one picked at its foot beside the keys.
    pub fn render(&self, draw: Draw) -> AnyElement {
        let Draw {
            id: note,
            github,
            keys,
            pick,
            close,
            change,
        } = draw;
        let g = self;
        let (names, rows) = grid(&g.query, github);
        let at = g.at.min(names.len().saturating_sub(1));
        let search = (!github).then(|| {
            div()
                .mb(px(4.))
                .px(px(8.))
                .py(px(4.))
                .rounded(px(ROW_RADIUS))
                .border_1()
                .border_color(theme::focus())
                .bg(theme::base())
                .child(search_field(g, change))
        });
        let foot = match names.get(at) {
            Some(name) => hover::cut(
                "reaction-name",
                match emoji::get(name) {
                    Some(_) => format!(":{name}:"),
                    None => format!(":{name}: — by name, if GitLab has it"),
                },
            )
            .flex_1()
            .text_color(theme::muted())
            .into_any_element(),
            None => div()
                .flex_1()
                .child("No emoji by that name")
                .into_any_element(),
        };
        let foot = div()
            .mt(px(4.))
            .flex()
            .gap_3()
            .text_size(px(11.))
            .text_color(theme::faint())
            .child(foot)
            .child(keys);
        let shown = rows.len().clamp(1, ROWS_SHOWN);
        let (names, rows) = (Rc::new(names), Rc::new(rows));
        let list = gpui::uniform_list(
            ("reaction-rows", note as usize),
            rows.len(),
            move |range: Range<usize>, _, _| {
                range
                    .map(|r| match &rows[r] {
                        GridRow::Group(title) => div()
                            .h(px(ROW))
                            .px(px(4.))
                            .flex()
                            .items_end()
                            .pb(px(4.))
                            .text_size(px(11.))
                            .text_color(theme::faint())
                            .child(*title)
                            .into_any_element(),
                        GridRow::Cells(cells) => div()
                            .h(px(ROW))
                            .flex()
                            .gap(px(2.))
                            .children(
                                cells
                                    .clone()
                                    .map(|i| grid_cell(i, &names[i], i == at, pick.clone())),
                            )
                            .into_any_element(),
                    })
                    .collect()
            },
        )
        .track_scroll(&g.scroll)
        .h(px(shown as f32 * ROW));
        let popup = div()
            .id(("reaction-popup", note as usize))
            .occlude()
            .p(px(6.))
            .w(px(COLS as f32 * ROW + 14.))
            .rounded(px(8.))
            .border_1()
            .border_color(theme::island_border())
            .bg(theme::panel())
            .shadow_lg()
            .children(search)
            .child(list)
            .child(foot)
            // A press anywhere else closes it.
            .on_mouse_down_out(move |_, _, cx| close(cx));
        // It drops into place as it fades in, as a menu does.
        motion::appear(
            popup,
            ("reaction-popup-coming", note as usize),
            motion::FAST,
            |popup, v| popup.opacity(v).mt(px(motion::mix(-4., 0., v))),
        )
        .into_any_element()
    }
}

/// Your five most used reactions (GitHub: from its eight).
pub fn frequent(github: bool) -> Vec<&'static str> {
    let mut counts: Vec<(String, usize)> = storage::read(USED)
        .lines()
        .filter_map(|l| {
            let (name, n) = l.split_once(' ')?;
            Some((name.to_owned(), n.parse().ok()?))
        })
        .collect();
    counts.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    let mut out: Vec<&'static str> = counts
        .iter()
        .filter_map(|(name, _)| emoji::all().iter().find(|e| e.name == name).map(|e| e.name))
        .filter(|n| !github || emoji::github_name(n).is_some())
        .collect();
    for d in DEFAULT_FIVE {
        if !out.contains(&d) {
            out.push(d);
        }
    }
    out.truncate(5);
    out
}

/// What the picker offers, and its rows: your five then GitLab's groups, each under its name; searching,
/// the matches, and `:name:` typed whole that the list lacks as a cell of its own. GitHub: its
/// eight.
fn grid(query: &str, github: bool) -> (Vec<Cow<'static, str>>, Vec<GridRow>) {
    if github {
        let names: Vec<_> = emoji::GITHUB
            .iter()
            .map(|(_, n)| Cow::Borrowed(*n))
            .collect();
        let rows = cell_rows(0..names.len());
        return (names, rows);
    }
    let typed = query.trim();
    if !typed.is_empty() {
        let mut names: Vec<Cow<'static, str>> = emoji::find(typed, usize::MAX)
            .into_iter()
            .map(|(n, _)| Cow::Borrowed(n))
            .collect();
        if let Some(name) = typed
            .strip_prefix(':')
            .and_then(|t| t.strip_suffix(':'))
            .filter(|n| emoji::could_be_name(n) && emoji::get(n).is_none())
        {
            names.push(Cow::Owned(name.to_owned()));
        }
        let rows = cell_rows(0..names.len());
        return (names, rows);
    }
    let mut names: Vec<Cow<'static, str>> = Vec::new();
    let mut rows = Vec::new();
    let mut group = |title: &'static str, of: &mut dyn Iterator<Item = &'static str>| {
        let from = names.len();
        names.extend(of.map(Cow::Borrowed));
        rows.push(GridRow::Group(title));
        rows.extend(cell_rows(from..names.len()));
    };
    group("Frequently used", &mut frequent(github).into_iter());
    let all = emoji::all();
    let mut at = 0;
    while let Some(first) = all[at..].iter().position(|e| !e.group.is_empty()) {
        let title = all[at + first].group;
        let end = all[at + first..]
            .iter()
            .position(|e| e.group != title && !e.group.is_empty())
            .map_or(all.len(), |n| at + first + n);
        group(
            title,
            &mut all[at + first..end]
                .iter()
                .filter(|e| !e.group.is_empty())
                .map(|e| e.name),
        );
        at = end;
    }
    (names, rows)
}

/// The picker's search: its text with the caret, a click placing it, ⇧ or a drag selecting, a double
/// click taking the word.
fn search_field(g: &Picker, change: Change) -> gpui::Div {
    let look = Look::default();
    input::field(
        &g.query,
        true,
        "Search emoji, or type :name:",
        &look,
        || {
            let shown = g.query.clone();
            let place: input::Place = Rc::new(move |at, shift, clicks, drag, lower, cx| {
                let shown = shown.clone();
                change(
                    Rc::new(move |picker: &mut Picker| {
                        input::sync(&shown, &mut picker.edit);
                        picker.edit.click(at, clicks, shift || drag);
                        if clicks == 1 && !shift && !drag {
                            picker.edit.set_downstream(lower);
                        }
                    }),
                    cx,
                );
            });
            input::text_body(
                &g.query,
                &g.edit,
                look.clone(),
                g.offset.clone(),
                Some(place),
            )
        },
    )
}

/// One of the picker's rows: a group's name, or up to `COLS` emoji by their place in the list.
#[derive(Clone)]
enum GridRow {
    Group(&'static str),
    Cells(Range<usize>),
}

fn cell_rows(of: Range<usize>) -> Vec<GridRow> {
    of.clone()
        .step_by(COLS)
        .map(|from| GridRow::Cells(from..(from + COLS).min(of.end)))
        .collect()
}

/// The row that holds emoji `at`.
fn row_of(rows: &[GridRow], at: usize) -> Option<usize> {
    rows.iter()
        .position(|r| matches!(r, GridRow::Cells(c) if c.contains(&at)))
}

/// ↑ or ↓ from emoji `at`: the same column of the row of emoji before or after (over a group's
/// name), the nearest when that row is shorter; at the first or last row it stays.
fn row_step(rows: &[GridRow], at: usize, down: bool) -> usize {
    let Some(row) = row_of(rows, at) else {
        return at;
    };
    let GridRow::Cells(here) = &rows[row] else {
        return at;
    };
    let col = at - here.start;
    let mut others: Box<dyn Iterator<Item = &GridRow>> = if down {
        Box::new(rows[row + 1..].iter())
    } else {
        Box::new(rows[..row].iter().rev())
    };
    match others.find_map(|r| match r {
        GridRow::Cells(c) => Some(c.clone()),
        GridRow::Group(_) => None,
    }) {
        Some(there) => (there.start + col).min(there.end - 1),
        None => at,
    }
}

/// An emoji of the picker: a click picks it. One the list lacks (typed as `:name:`) shows its name.
fn grid_cell(i: usize, name: &str, picked: bool, pick: Pick) -> AnyElement {
    let tip: gpui::SharedString = format!(":{name}:").into();
    let react = name.to_owned();
    let glyph = emoji::get(name);
    div()
        .id(("grid-react", i))
        .h(px(28.))
        .min_w(px(28.))
        .when(glyph.is_none(), |s| s.px(px(6.)).text_size(px(12.)))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(ROW_RADIUS))
        .when(picked, |s| s.bg(theme::selected()))
        .cursor_pointer()
        .hover(|s| s.bg(theme::hover()))
        .child(match glyph {
            Some(glyph) => glyph.to_owned(),
            None => format!(":{name}:"),
        })
        .tooltip_show_delay(hover::LONG)
        .tooltip(move |_, cx| cx.new(|_| TipOwned(tip.clone())).into())
        .on_click(move |_, _, cx| pick(react.clone(), cx))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn up_and_down_keep_the_column_across_groups() {
        // A group of 5, then one of 14: rows 0..5 · 5..17 · 17..19, names between.
        let mut rows = vec![GridRow::Group("a")];
        rows.extend(cell_rows(0..5));
        rows.push(GridRow::Group("b"));
        rows.extend(cell_rows(5..19));
        assert_eq!(row_of(&rows, 16), Some(3));
        // Down from the short row's fourth: the next group's fourth.
        assert_eq!(row_step(&rows, 3, true), 8);
        // Down into a shorter row: its last.
        assert_eq!(row_step(&rows, 15, true), 18);
        // Up over a group's name: the short row, as far as it goes.
        assert_eq!(row_step(&rows, 11, false), 4);
        // The first and the last row stay.
        assert_eq!(row_step(&rows, 2, false), 2);
        assert_eq!(row_step(&rows, 18, true), 18);
    }
}
