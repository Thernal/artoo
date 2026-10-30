//! Reactions on a request's notes (Design #9 B, `docs/specs/reactions.md`): the map of them by note id; pills
//! under a note once it has some (yours outlined, hover names who, a click toggles yours); hovering a note
//! floats a bar on its corner — your five most used, ☺+ for the rest, 🤖 a note for your agent (#50 G), Reply
//! with the author already mentioned. ☺+ opens a popup under the bar (#91): a search over every emoji GitLab
//! has, your five first, then GitLab's groups; arrows and ↵, `:name:` at its foot; `:name:` typed whole reacts
//! with a name the list lacks (a custom one). On GitHub only its eight, no search. The five are counted on this
//! Mac (`reactions-used.txt`). What a click asks of the tab goes through `Acts`.

use crate::requests::{Act, Acts, Event, Requests};
use crate::waiting::FADED;
use artoo_core::textedit::TextEdit;
use artoo_core::{emoji, storage};
use artoo_hosts_api::{Award, Host};
use artoo_ui_kit::hover::{self, Tip, TipOwned};
use artoo_ui_kit::input::{self, Look};
use artoo_ui_kit::motion::{self, Marks};
use artoo_ui_kit::{ROW_RADIUS, px, theme};
use gpui::{AnyElement, App, Context, Rgba, UniformListScrollHandle, WeakEntity, div, prelude::*};
use std::borrow::Cow;
use std::cell::Cell;
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

/// Keeps the map type named once.
pub type Awards = HashMap<u64, Vec<Award>>;

const USED: &str = "reactions-used.txt";
/// ☺+'s cells to a row, and a row's height.
const COLS: usize = 12;
const ROW: f32 = 30.;
/// How many rows show before it scrolls.
const ROWS_SHOWN: usize = 8;
/// Until there is a history: the five most people reach for.
const DEFAULT_FIVE: [&str; 5] = ["thumbsup", "heart", "tada", "eyes", "rocket"];

/// ☺+ open on a note: its search with its caret and sideways offset, the emoji picked, the list's
/// scroll.
pub struct Grid {
    pub note: u64,
    query: String,
    at: usize,
    scroll: UniformListScrollHandle,
    edit: TextEdit,
    offset: Rc<Cell<f32>>,
}

/// What a key in ☺+ asks of the tab.
pub enum GridKey {
    /// ↵: react with the one picked (none when nothing matches), and close.
    React(Option<String>),
    /// Esc.
    Close,
}

/// The note a hover bar is drawn on, and what it knows of the tab.
pub struct Hovered {
    /// The discussion's index, and the note's id and author's login.
    pub thread: usize,
    pub note: u64,
    pub author: String,
    /// The pointer is on the note.
    pub hovered: bool,
    /// The host is GitHub: its eight, no search.
    pub github: bool,
}

impl Requests {
    /// Adds your reaction `name` to note `note`, or takes it back: shown at once, ☺+ closed; a
    /// failure puts it back and says why (`Event::Failed`, then `Event::Awards`).
    pub fn toggle_reaction(
        &mut self,
        note: u64,
        name: &str,
        host: Arc<dyn Host>,
        cx: &mut Context<Self>,
    ) {
        let Some(iid) = self.current else {
            return;
        };
        let me = self.login.clone().unwrap_or_default();
        let issue = self
            .threads
            .iter()
            .find(|t| t.notes.iter().any(|n| n.id == note))
            .is_some_and(|t| t.id.starts_with("issue|"));
        let name = name.to_owned();
        let list = self.awards.entry(note).or_default();
        let mine = list.iter().position(|a| a.name == name && a.user == me);
        // Shown at once; a failure puts it back and says why.
        let undo = list.clone();
        let removed = match mine {
            Some(at) => Some(list.remove(at)),
            None => {
                list.push(Award {
                    id: 0,
                    name: name.clone(),
                    user: me.clone(),
                });
                None
            }
        };
        match removed {
            Some(award) => {
                self.spawn_reaction(note, undo, cx, move || {
                    host.unaward(iid, note, issue, &award.name, award.id)
                        .map(|_| None)
                });
            }
            None => {
                count_use(&name);
                self.spawn_reaction(note, undo, cx, move || {
                    host.award(iid, note, issue, &name)
                        .map(|id| Some((id, name)))
                });
            }
        }
        self.reaction_grid = None;
    }

    fn spawn_reaction(
        &mut self,
        note: u64,
        undo: Vec<Award>,
        cx: &mut Context<Self>,
        call: impl FnOnce() -> anyhow::Result<Option<(u64, String)>> + Send + 'static,
    ) {
        cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move { call() }).await;
            this.update(cx, |rq, cx| {
                match result {
                    // The new reaction learns its id, to be taken back later.
                    Ok(Some((id, name))) => {
                        if let Some(a) = rq
                            .awards
                            .get_mut(&note)
                            .and_then(|l| l.iter_mut().find(|a| a.name == name && a.id == 0))
                        {
                            a.id = id;
                        }
                    }
                    Ok(None) => {}
                    Err(e) => {
                        rq.awards.insert(note, undo);
                        cx.emit(Event::Failed(format!("{e:#}")));
                        cx.emit(Event::Awards);
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// ☺+ opens on note `note`, its search empty and taking the keyboard.
    pub fn open_reaction_grid(&mut self, note: u64) {
        self.reaction_grid = Some(Grid {
            note,
            query: String::new(),
            at: 0,
            scroll: UniformListScrollHandle::new(),
            edit: TextEdit::default(),
            offset: Rc::new(Cell::new(0.)),
        });
    }

    /// ☺+ closes.
    pub fn close_reaction_grid(&mut self) {
        self.reaction_grid = None;
    }

    /// The note whose ☺+ is open.
    pub fn grid_note(&self) -> Option<u64> {
        self.reaction_grid.as_ref().map(|g| g.note)
    }

    /// Keys while ☺+ is open: typing searches, arrows move (↑ ↓ by row, across the groups); ↵
    /// asks to react with the one picked, esc to close (`GridKey`).
    pub fn reaction_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        github: bool,
        cx: &mut App,
    ) -> Option<GridKey> {
        let g = self.reaction_grid.as_mut()?;
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
            "escape" => return Some(GridKey::Close),
            "enter" => return Some(GridKey::React(names.get(at).map(|n| n.to_string()))),
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

    /// Under a note: its reactions as pills, one per emoji, yours outlined.
    pub fn reaction_pills(&self, note: u64, motion: &Marks, acts: Acts) -> Option<AnyElement> {
        let list = self.awards.get(&note).filter(|l| !l.is_empty())?;
        let me = self.login.clone().unwrap_or_default();
        let mut groups: Vec<(String, Vec<String>)> = Vec::new();
        for a in list {
            match groups.iter_mut().find(|(n, _)| *n == a.name) {
                Some((_, who)) => who.push(a.user.clone()),
                None => groups.push((a.name.clone(), vec![a.user.clone()])),
            }
        }
        Some(
            div()
                .flex()
                .flex_wrap()
                .gap(px(6.))
                .mt(px(6.))
                .children(groups.into_iter().enumerate().map(|(i, (name, who))| {
                    let mine = who.contains(&me);
                    // Yours, on its way to the host: faded until it answers (#139).
                    let waits = list
                        .iter()
                        .any(|a| a.id == 0 && a.name == name && a.user == me);
                    let tip: gpui::SharedString = format!(":{name}: {}", who.join(", ")).into();
                    let acts = acts.clone();
                    let toggle = name.clone();
                    let pops = motion.recent(&format!("award-{note}-{name}"));
                    let pill = div()
                        .id(("reaction", note as usize * 64 + i))
                        .h(px(24.))
                        .px(px(8.))
                        .flex()
                        .items_center()
                        .gap(px(4.))
                        .rounded_full()
                        .border_1()
                        .border_color(if mine {
                            Rgba {
                                a: 0.55,
                                ..theme::accent()
                            }
                        } else {
                            theme::island_border()
                        })
                        .bg(if mine {
                            theme::info_bg()
                        } else {
                            theme::hover()
                        })
                        .cursor_pointer()
                        .when(waits, |s| s.opacity(FADED))
                        // One the list lacks (a custom one) by its name.
                        .child(match emoji::get(&name) {
                            Some(glyph) => glyph.to_owned(),
                            None => format!(":{name}:"),
                        })
                        .child(div().text_size(px(12.)).child(who.len().to_string()))
                        .tooltip_show_delay(hover::LONG)
                        .tooltip(move |_, cx| cx.new(|_| TipOwned(tip.clone())).into())
                        .on_click(move |_, _, cx| {
                            acts(
                                Act::React {
                                    note,
                                    name: toggle.clone(),
                                },
                                cx,
                            )
                        });
                    if pops {
                        motion::appear(
                            pill,
                            ("reaction-pop", note as usize * 64 + i),
                            motion::FAST,
                            |pill, v| pill.px(px(motion::mix(4., 8., v))).opacity(v),
                        )
                        .into_any_element()
                    } else {
                        pill.into_any_element()
                    }
                }))
                .into_any_element(),
        )
    }

    /// The bar on a hovered note: the five, ☺+ and Reply.
    pub fn hover_bar(&self, at: Hovered, this: WeakEntity<Self>, acts: Acts) -> Option<AnyElement> {
        let Hovered {
            thread,
            note,
            author,
            hovered,
            github,
        } = at;
        let open = self.grid_note() == Some(note);
        if !hovered && !open {
            return None;
        }
        let cell = |id: (&'static str, usize), child: AnyElement| {
            div()
                .id(id)
                .size(px(26.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(ROW_RADIUS))
                .cursor_pointer()
                .hover(|s| s.bg(theme::hover()))
                .child(child)
        };
        let five = five(github);
        let popup = open.then(|| self.reaction_popup(note, github, this, acts.clone()));
        let (grid_acts, reply_acts, agent_acts) = (acts.clone(), acts.clone(), acts.clone());
        Some(
            div()
                .relative()
                .flex()
                .items_center()
                .gap(px(2.))
                .p(px(2.))
                .rounded(px(ROW_RADIUS))
                .border_1()
                .border_color(theme::island_border())
                .bg(theme::panel())
                .children(five.into_iter().enumerate().map(|(i, name)| {
                    let acts = acts.clone();
                    cell(
                        ("bar-react", note as usize * 8 + i),
                        div()
                            .child(emoji::get(name).unwrap_or("·"))
                            .into_any_element(),
                    )
                    .on_click(move |_, _, cx| {
                        acts(
                            Act::React {
                                note,
                                name: name.to_owned(),
                            },
                            cx,
                        )
                    })
                }))
                .child(
                    cell(
                        ("bar-more", note as usize),
                        div()
                            .text_color(if open { theme::text() } else { theme::muted() })
                            .child("☺+")
                            .into_any_element(),
                    )
                    .on_click(move |_, _, cx| {
                        let act = if open {
                            Act::CloseGrid(note)
                        } else {
                            Act::OpenGrid(note)
                        };
                        grid_acts(act, cx)
                    }),
                )
                .child(
                    div()
                        .w(px(1.))
                        .h(px(18.))
                        .mx(px(2.))
                        .bg(theme::island_border()),
                )
                // A note on this discussion for your agent, kept on this Mac (#50 G).
                .child(
                    cell(
                        ("bar-agent", note as usize),
                        div().child("🤖").into_any_element(),
                    )
                    .tooltip_show_delay(hover::LONG)
                    .tooltip(|_, cx| {
                        cx.new(|_| Tip("A note for your agent — only on this Mac"))
                            .into()
                    })
                    .on_click(move |_, _, cx| agent_acts(Act::ThreadNote(thread), cx)),
                )
                .child(
                    div()
                        .id(("bar-reply", note as usize))
                        .h(px(26.))
                        .px(px(8.))
                        .flex()
                        .items_center()
                        .rounded(px(ROW_RADIUS))
                        .text_size(px(12.))
                        .text_color(theme::muted())
                        .cursor_pointer()
                        .hover(|s| s.bg(theme::hover()).text_color(theme::text()))
                        .child("Reply")
                        .on_click(move |_, _, cx| {
                            reply_acts(
                                Act::ReplyMentioning {
                                    thread,
                                    login: author.clone(),
                                },
                                cx,
                            )
                        }),
                )
                // ☺+'s popup hangs from the bar's lower right corner, over what is below
                // (above it near the window's foot), the discussion staying as it is.
                .children(popup.map(|popup| {
                    div().absolute().right_0().bottom_0().size_0().child(
                        gpui::deferred(
                            gpui::anchored()
                                .anchor(gpui::Anchor::TopRight)
                                .offset(gpui::point(px(0.), px(4.)))
                                .child(popup),
                        )
                        .with_priority(1),
                    )
                }))
                .into_any_element(),
        )
        // It fades in on the hovered note (#56, 11).
        .map(|bar| {
            motion::appear(
                div().child(bar),
                ("bar-coming", note as usize),
                motion::FAST,
                |bar, v| bar.opacity(v),
            )
            .into_any_element()
        })
    }

    /// ☺+'s popup: the search, the emoji in rows (only those on screen are built), the name of
    /// the one picked at its foot.
    fn reaction_popup(
        &self,
        note: u64,
        github: bool,
        this: WeakEntity<Self>,
        acts: Acts,
    ) -> AnyElement {
        let Some(g) = self.reaction_grid.as_ref() else {
            return div().into_any_element();
        };
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
                .child(search_field(g, this))
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
            .child("←↑↓→ · ↵ react · esc");
        let shown = rows.len().clamp(1, ROWS_SHOWN);
        let (names, rows) = (Rc::new(names), Rc::new(rows));
        let close = acts.clone();
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
                                    .map(|i| grid_cell(note, i, &names[i], i == at, acts.clone())),
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
            .on_mouse_down_out(move |_, _, cx| close(Act::CloseGrid(note), cx));
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
fn five(github: bool) -> Vec<&'static str> {
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

/// Counts reaction `name` used once more, on this Mac.
fn count_use(name: &str) {
    let text = storage::read(USED);
    let mut lines: Vec<(String, usize)> = text
        .lines()
        .filter_map(|l| {
            let (n, c) = l.split_once(' ')?;
            Some((n.to_owned(), c.parse().ok()?))
        })
        .collect();
    match lines.iter_mut().find(|(n, _)| n == name) {
        Some((_, c)) => *c += 1,
        None => lines.push((name.to_owned(), 1)),
    }
    storage::save(
        USED,
        lines.iter().map(|(n, c)| format!("{n} {c}\n")).collect(),
    );
}

/// What ☺+ offers, and its rows: your five then GitLab's groups, each under its name; searching,
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
    group("Frequently used", &mut five(github).into_iter());
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

/// ☺+'s search: its text with the caret, a click placing it, ⇧ or a drag selecting, a double
/// click taking the word.
fn search_field(g: &Grid, this: WeakEntity<Requests>) -> gpui::Div {
    let look = Look::default();
    input::field(
        &g.query,
        true,
        "Search emoji, or type :name:",
        &look,
        || {
            let shown = g.query.clone();
            let place: input::Place = Rc::new(move |at, shift, clicks, drag, lower, cx| {
                this.update(cx, |rq, cx| {
                    if let Some(g) = rq.reaction_grid.as_mut() {
                        input::sync(&shown, &mut g.edit);
                        g.edit.click(at, clicks, shift || drag);
                        if clicks == 1 && !shift && !drag {
                            g.edit.set_downstream(lower);
                        }
                        cx.notify();
                    }
                })
                .ok();
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

/// One of ☺+'s rows: a group's name, or up to `COLS` emoji by their place in the list.
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

/// An emoji of ☺+: a click reacts with it. One the list lacks (typed as `:name:`) shows its name.
fn grid_cell(note: u64, i: usize, name: &str, picked: bool, acts: Acts) -> AnyElement {
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
        .on_click(move |_, _, cx| {
            acts(
                Act::React {
                    note,
                    name: react.clone(),
                },
                cx,
            )
        })
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
