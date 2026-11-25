//! Reactions on a request's notes (Design #9 B, `docs/specs/reactions.md`): the map of them by note id; pills
//! under a note once it has some (yours outlined, hover names who, a click toggles yours); hovering a note
//! floats a bar on its corner — your five most used, ☺+ for the rest, 🤖 a note for your agent (#50 G), Reply
//! with the author already mentioned. ☺+ opens the emoji picker under the bar (#91,
//! `artoo_ui_kit::emoji_picker`). The five are counted on this Mac (`reactions-used.txt`). What a click asks of
//! the tab goes through `Acts`.

use crate::requests::{Act, Acts, Event, Requests};
use crate::waiting::FADED;
use artoo_core::{emoji, storage};
use artoo_hosts_api::{Award, Host};
use artoo_ui_kit::emoji_picker::{Draw, Picker, PickerKey, USED, frequent};
use artoo_ui_kit::hover::{self, Tip, TipOwned};
use artoo_ui_kit::motion::{self, Marks};
use artoo_ui_kit::{ROW_RADIUS, px, theme};
use gpui::{AnyElement, App, Context, Rgba, WeakEntity, div, prelude::*};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

/// Keeps the map type named once.
pub type Awards = HashMap<u64, Vec<Award>>;

/// ☺+ open on a note.
pub struct Grid {
    pub note: u64,
    picker: Picker,
}

pub use artoo_requests_api::GridKey;

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
            picker: Picker::default(),
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

    /// Keys while ☺+ is open (`Picker::key`): ↵ asks to react with the one picked, esc to close (`GridKey`).
    pub fn reaction_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        github: bool,
        cx: &mut App,
    ) -> Option<GridKey> {
        let g = self.reaction_grid.as_mut()?;
        Some(match g.picker.key(event, github, cx)? {
            PickerKey::Pick(name) => GridKey::React(name),
            PickerKey::Close => GridKey::Close,
        })
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
        let five = frequent(github);
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

    /// ☺+'s popup, the kit's picker (`artoo_ui_kit::emoji_picker`).
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
        let (react, close) = (acts.clone(), acts);
        g.picker.render(Draw {
            id: note,
            github,
            keys: "←↑↓→ · ↵ react · esc",
            pick: Rc::new(move |name, cx| react(Act::React { note, name }, cx)),
            close: Rc::new(move |cx| close(Act::CloseGrid(note), cx)),
            change: Rc::new(move |change, cx| {
                this.update(cx, |rq, cx| {
                    if let Some(g) = rq.reaction_grid.as_mut() {
                        change(&mut g.picker);
                        cx.notify();
                    }
                })
                .ok();
            }),
        })
    }
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
