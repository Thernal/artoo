//! A context menu at the pointer (right click on a commit, a file or a line; the diff's View menu): entries in
//! groups, each doing an act of the tab's kind `A`, handed to the runner the tab gave once the menu has let go.

use crate::hover::Hover;
use crate::{ISLAND_RADIUS, hover, keys, motion, px, row, theme, ui_zoom};
use gpui::{
    App, Context, MouseButton, Pixels, Point, Render, Rgba, SharedString, Window, div, prelude::*,
};
use std::rc::Rc;

/// One entry; `A` is what it does.
#[derive(Clone)]
pub struct Entry<A> {
    pub label: String,
    pub key: &'static str,
    /// `Some` for a choice: whether it is on.
    pub checked: Option<bool>,
    /// Its label drawn in this face (a face's menu, #24).
    pub face: Option<SharedString>,
    /// Said faint after the label: why a request cannot merge (#164).
    pub detail: Option<String>,
    /// `None`: shown dimmed, taking no click; its label says why (a branch only on this Mac, #162).
    pub act: Option<A>,
    /// Worn before its label: who it is (#200).
    pub lead: Option<Lead>,
    /// A second line under the label, faint, `true` in the code face (a chat's branch, #200). Its `detail` then
    /// stands at the row's right end instead of after the label.
    pub under: Option<(String, bool)>,
    /// A glyph at the row's right end with its sentence as the tooltip, in place of `key` (Open in's
    /// `This project's editor before`).
    pub note: Option<(&'static str, String)>,
    /// A switch at the row's end: whether it is on (a plugin source). Clicking the row flips it.
    pub switch: Option<bool>,
    /// Said quiet before the end's glyphs (`@you`).
    pub quiet: Option<String>,
    /// A fact at the end: a glyph and a short word, the sentence its tooltip (Merge's list: `✓ 1/2`).
    pub value: Option<Value>,
    /// A chevron of its own at the very end with its own tooltip; `Some(None)` keeps its room, drawn empty.
    pub go: Option<Option<String>>,
    /// The whole row's tooltip: a title and, under it, a line.
    pub tip: Option<(String, Option<String>)>,
}

/// A fact at a row's end.
#[derive(Clone)]
pub struct Value {
    pub icon: &'static str,
    pub text: String,
    /// What it says, on a long hover.
    pub tip: String,
    /// `Some`: drawn as a pill in this colour (what is wrong); `None`: quiet.
    pub tone: Option<Rgba>,
}

/// What an entry wears before its label (#200, the `To:` list).
#[derive(Clone, Copy)]
pub enum Lead {
    /// A chat's letter on its hue.
    Letter(char, Rgba),
    /// An icon in a colour: the request's, everyone on it.
    Icon(&'static str, Rgba),
}

impl<A> Entry<A> {
    pub fn new(label: impl Into<String>, act: A) -> Self {
        Self::blank(label.into(), Some(act))
    }

    /// What cannot be done here, and why: `Copy link — only on this Mac`.
    pub fn off(label: impl Into<String>) -> Self {
        Self::blank(label.into(), None)
    }

    fn blank(label: String, act: Option<A>) -> Self {
        Self {
            label,
            key: "",
            checked: None,
            face: None,
            detail: None,
            act,
            lead: None,
            under: None,
            note: None,
            switch: None,
            quiet: None,
            value: None,
            go: None,
            tip: None,
        }
    }

    /// A switch at the end, `on` or off.
    pub fn switch(mut self, on: bool) -> Self {
        self.switch = Some(on);
        self
    }

    /// Quiet words before the end's glyphs.
    pub fn quiet(mut self, quiet: impl Into<String>) -> Self {
        self.quiet = Some(quiet.into());
        self
    }

    /// A fact at the end: `icon` and `text`, `tip` its sentence; `tone` makes it a pill.
    pub fn value(
        mut self,
        icon: &'static str,
        text: impl Into<String>,
        tip: impl Into<String>,
        tone: Option<Rgba>,
    ) -> Self {
        self.value = Some(Value {
            icon,
            text: text.into(),
            tip: tip.into(),
            tone,
        });
        self
    }

    /// A chevron at the very end, `tip` its own sentence.
    pub fn go(mut self, tip: impl Into<String>) -> Self {
        self.go = Some(Some(tip.into()));
        self
    }

    /// The chevron's room without a chevron, so the rows' ends line up.
    pub fn go_blank(mut self) -> Self {
        self.go = Some(None);
        self
    }

    /// The row's tooltip: one line.
    pub fn tip(mut self, title: impl Into<String>) -> Self {
        self.tip = Some((title.into(), None));
        self
    }

    /// The row's tooltip: a title and a quieter line.
    pub fn tip_two(mut self, title: impl Into<String>, line: impl Into<String>) -> Self {
        self.tip = Some((title.into(), Some(line.into())));
        self
    }

    pub fn key(mut self, key: &'static str) -> Self {
        self.key = key;
        self
    }

    pub fn checked(mut self, on: bool) -> Self {
        self.checked = Some(on);
        self
    }

    pub fn face(mut self, face: SharedString) -> Self {
        self.face = Some(face);
        self
    }

    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// A glyph, `icon`, at the right end, saying `tip` on a long hover.
    pub fn note(mut self, icon: &'static str, tip: impl Into<String>) -> Self {
        self.note = Some((icon, tip.into()));
        self
    }

    pub fn lead(mut self, lead: Lead) -> Self {
        self.lead = Some(lead);
        self
    }

    pub fn under(mut self, under: impl Into<String>) -> Self {
        self.under = Some((under.into(), false));
        self
    }

    /// `under`, in the code face: a branch.
    pub fn under_code(mut self, under: impl Into<String>) -> Self {
        self.under = Some((under.into(), true));
        self
    }
}

/// A row's end: quiet words, a fact, a switch, a note or its keys, a chevron.
fn end<A>(entry: &Entry<A>) -> impl IntoElement {
    div()
        .flex_none()
        .flex()
        .items_center()
        .gap(px(8.))
        .children(
            entry
                .quiet
                .clone()
                .map(|q| div().text_size(px(11.)).text_color(theme::faint()).child(q)),
        )
        .children(entry.value.clone().map(|v| {
            let hue = v.tone.unwrap_or_else(theme::muted);
            div()
                .id("value")
                .flex()
                .items_center()
                .gap(px(3.))
                .text_size(px(12.))
                .text_color(hue)
                .when(v.tone.is_some(), |s| {
                    s.h(px(20.))
                        .px(px(6.))
                        .rounded_full()
                        .border_1()
                        .border_color(hue)
                })
                .child(crate::icons::icon(v.icon).size(px(14.)).text_color(hue))
                .child(v.text)
                .tip(v.tip)
        }))
        .children(
            entry
                .switch
                .map(|on| crate::controls::switch("menu-switch", on)),
        )
        .child(match entry.note.clone() {
            Some((icon, tip)) => div()
                .id("note")
                .flex_none()
                .child(
                    crate::icons::icon(icon)
                        .size(px(14.))
                        .text_color(theme::faint()),
                )
                .tip(tip)
                .into_any_element(),
            None if entry.key.is_empty() => div().into_any_element(),
            None => div()
                .text_size(px(11.))
                .text_color(theme::faint())
                .child(keys::text(entry.key).into_owned())
                .into_any_element(),
        })
        .children(entry.go.clone().map(|tip| {
            let chevron = div().id("go").flex_none().size(px(14.));
            match tip {
                Some(tip) => chevron
                    .child(
                        crate::icons::icon("chevron-right")
                            .size(px(14.))
                            .text_color(theme::faint()),
                    )
                    .tip(tip),
                None => chevron,
            }
        }))
}

/// Entries in groups; a rule is drawn between groups, a group may have a heading.
struct Panel<A> {
    at: Point<Pixels>,
    groups: Vec<Vec<Entry<A>>>,
    titles: Vec<Option<&'static str>>,
    /// `Some`: headings in sentence case, the first with an info glyph whose tooltip is this (Merge's list).
    plain: Option<String>,
    /// `WIDTH`, or wider for entries with a detail (#164).
    width: f32,
}

const ROW: f32 = 28.;
/// An entry with a line under its label.
const TALL: f32 = 42.;
/// A group's heading row.
const HEAD: f32 = 26.;
/// The room a menu keeps from the window's edges (#48: menus stay inside the window).
const EDGE: f32 = 8.;
pub const WIDTH: f32 = 260.;

/// What the tab does when an entry with act `A` is clicked.
pub type OnEntry<A> = Rc<dyn Fn(A, &mut Window, &mut App)>;

/// One tab's context menu.
pub struct Menu<A> {
    open: Option<Panel<A>>,
    /// Closed a moment ago: it fades out, taking no clicks (#56, 5).
    leaving: Option<Panel<A>>,
    marks: motion::Marks,
    /// An entry clicked.
    run: OnEntry<A>,
}

impl<A: Clone + 'static> Menu<A> {
    pub fn new(run: OnEntry<A>) -> Self {
        Self {
            open: None,
            leaving: None,
            marks: Default::default(),
            run,
        }
    }

    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }

    /// Whether it draws: open, or on its way out.
    pub fn shows(&self) -> bool {
        self.open.is_some() || self.leaving.is_some() && self.marks.recent("menu")
    }

    /// A menu whose groups have headings (the View menu: Changes, Show, Files, Panels); `width` wider than
    /// `WIDTH` for entries with a detail.
    pub fn open(
        &mut self,
        at: Point<Pixels>,
        groups: Vec<(Option<&'static str>, Vec<Entry<A>>)>,
        width: f32,
        cx: &mut Context<Self>,
    ) {
        let (titles, groups): (Vec<_>, Vec<_>) =
            groups.into_iter().filter(|(_, g)| !g.is_empty()).unzip();
        if !groups.is_empty() {
            self.open = Some(Panel {
                at,
                groups,
                titles,
                plain: None,
                width,
            });
            self.leaving = None;
            cx.notify();
        }
    }

    /// `open` with headings in sentence case, the first one ending in an info glyph saying `info`.
    pub fn open_plain(
        &mut self,
        at: Point<Pixels>,
        groups: Vec<(Option<&'static str>, Vec<Entry<A>>)>,
        width: f32,
        info: impl Into<String>,
        cx: &mut Context<Self>,
    ) {
        self.open(at, groups, width, cx);
        if let Some(panel) = self.open.as_mut() {
            panel.plain = Some(info.into());
        }
    }

    /// It closes; it fades on its way out (#56, 5).
    pub fn close(&mut self, cx: &mut Context<Self>) {
        if let Some(menu) = self.open.take() {
            self.leaving = Some(menu);
            self.marks.mark("menu");
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(motion::BASE + std::time::Duration::from_millis(120))
                    .await;
                this.update(cx, |_, cx| cx.notify()).ok();
            })
            .detach();
            cx.notify();
        }
    }

    /// It is gone at once, with no fade.
    pub fn shut(&mut self, cx: &mut Context<Self>) {
        if self.open.take().is_some() {
            cx.notify();
        }
    }

    /// Entry `act` clicked: the menu lets go, then the tab runs it.
    fn press(&mut self, act: A, window: &mut Window, cx: &mut Context<Self>) {
        self.close(cx);
        let run = self.run.clone();
        window.defer(cx, move |window, cx| run(act, window, cx));
    }

    /// The menu's panel at its place, and its top; `live`, its entries take clicks.
    fn panel(
        menu: &Panel<A>,
        viewport: gpui::Size<Pixels>,
        live: bool,
        cx: &mut Context<Self>,
    ) -> (gpui::Div, f32) {
        let rows: f32 = (menu.groups.iter().flatten())
            .map(|e| if e.under.is_some() { TALL } else { ROW })
            .sum();
        let heads = menu.titles.iter().flatten().count();
        let height = rows + heads as f32 * HEAD + menu.groups.len() as f32 * 9. + 12.;
        // Inside the window: moved back from an edge it would cross, and scrolling when taller
        // than the window has room for.
        // The pointer and the window in the interface's own units: `px` scales by the interface
        // size, so at 110 % a menu placed in window pixels landed a tenth too far (#87).
        let zoom = ui_zoom();
        let viewport = gpui::size(viewport.width / zoom, viewport.height / zoom);
        let at = gpui::point(menu.at.x / zoom, menu.at.y / zoom);
        let room = f32::from(viewport.height) - 2. * EDGE;
        let scrolls = height > room;
        let height = height.min(room);
        let width = menu.width;
        let x = f32::from(at.x)
            .min(f32::from(viewport.width) - width - EDGE)
            .max(EDGE);
        let y = f32::from(at.y)
            .min(f32::from(viewport.height) - height - EDGE)
            .max(EDGE);
        let groups = menu.groups.iter().enumerate().map(|(g, entries)| {
            let title = menu.titles.get(g).copied().flatten();
            div()
                .flex()
                .flex_col()
                .when(g > 0, |s| {
                    s.child(div().h(px(1.)).my(px(4.)).bg(theme::island_border()))
                })
                .children(title.map(|t| {
                    div()
                        .h(px(HEAD))
                        .px(px(10.))
                        .flex()
                        .items_center()
                        .text_size(px(11.))
                        .when(menu.plain.is_none(), |s| {
                            s.font_weight(gpui::FontWeight::SEMIBOLD)
                        })
                        .text_color(theme::faint())
                        .child(div().flex_1().child(if menu.plain.is_some() {
                            t.to_owned()
                        } else {
                            t.to_uppercase()
                        }))
                        .children((g == 0).then_some(()).and(menu.plain.clone()).map(|info| {
                            div()
                                .id("head-info")
                                .flex_none()
                                .child(crate::icons::icon("info").size(px(14.)))
                                .tip(info)
                        }))
                }))
                .children(entries.iter().enumerate().map(|(i, entry)| {
                    let act = entry.act.clone();
                    let off = act.is_none();
                    // One that cannot be done takes no hover: `row` gives it none as "selected",
                    // and its fill is taken away.
                    row(("ctx-entry", (g * 100 + i) as u64), off)
                        .h(px(if entry.under.is_some() { TALL } else { ROW }))
                        .gap_2()
                        .when(off, |s| {
                            s.bg(gpui::transparent_black())
                                .text_color(theme::faint())
                                .cursor_default()
                        })
                        // The mark's column, unless the row begins with a glyph of its own or has two lines.
                        .when(
                            entry.checked.is_some()
                                || (entry.lead.is_none() && entry.under.is_none()),
                            |s| {
                                s.child(div().w(px(14.)).text_color(theme::accent()).child(
                                    if entry.checked == Some(true) {
                                        "✓"
                                    } else {
                                        ""
                                    },
                                ))
                            },
                        )
                        // A gone one's lead is dimmed with its words.
                        .children(entry.lead.map(|lead| {
                            div()
                                .flex_none()
                                .size(px(16.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .when(off, |s| s.opacity(0.55))
                                .child(match lead {
                                    Lead::Letter(letter, hue) => {
                                        crate::monogram::letter(letter, hue, 16.).into_any_element()
                                    }
                                    Lead::Icon(name, hue) => crate::icons::icon(name)
                                        .size(px(14.))
                                        .text_color(hue)
                                        .into_any_element(),
                                })
                        }))
                        .child(match (entry.detail.clone(), entry.under.clone()) {
                            // Two lines, the design's `To:` list: the name over its branch, its state at the right.
                            (detail, Some((under, code))) => div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .flex()
                                        .flex_col()
                                        .child(
                                            hover::cut("label", entry.label.clone())
                                                .font_weight(gpui::FontWeight::SEMIBOLD),
                                        )
                                        .child(
                                            hover::cut("under", under)
                                                .text_size(px(11.))
                                                .when(code, |s| s.font_family(theme::code_font()))
                                                .text_color(theme::faint()),
                                        ),
                                )
                                .children(detail.map(|d| {
                                    div()
                                        .flex_none()
                                        .text_size(px(11.))
                                        .text_color(theme::faint())
                                        .child(d)
                                }))
                                .into_any_element(),
                            (None, None) => {
                                hover::cut("label", keys::text(&entry.label).into_owned())
                                    .when_some(entry.face.clone(), |s, face| s.font_family(face))
                                    .flex_1()
                                    .into_any_element()
                            }
                            (Some(detail), None) => div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .gap(px(6.))
                                .child(
                                    div()
                                        .flex_none()
                                        .font_weight(gpui::FontWeight::MEDIUM)
                                        .child(entry.label.clone()),
                                )
                                .child(
                                    hover::cut("detail", detail)
                                        .flex_1()
                                        .text_color(theme::faint()),
                                )
                                .into_any_element(),
                        })
                        .child(end(entry))
                        .when_some(entry.tip.clone(), |s, (title, line)| match line {
                            Some(line) => s.tip_two(title, line),
                            None => s.tip(title),
                        })
                        .when_some(act.filter(|_| live), |s, act| {
                            s.on_click(cx.listener(move |this, _, window, cx| {
                                this.press(act.clone(), window, cx)
                            }))
                        })
                }))
        });
        let panel = div()
            .absolute()
            // The panel takes the mouse: otherwise the backdrop behind it sees the press
            // first, closes the menu, and the click never reaches an entry.
            .occlude()
            .left(px(x))
            .top(px(y))
            .w(px(width))
            .h(px(height))
            .flex()
            .flex_col()
            .rounded(px(ISLAND_RADIUS))
            .border_1()
            .border_color(theme::island_border())
            .bg(theme::panel())
            .shadow_lg()
            .text_size(px(13.))
            .overflow_hidden()
            .child(
                div()
                    .id("ctx-scroll")
                    .flex_1()
                    .min_h_0()
                    .p(px(6.))
                    .flex()
                    .flex_col()
                    .when(scrolls, |s| s.overflow_y_scroll())
                    .children(groups),
            );
        (panel, y)
    }
}

impl<A: Clone + 'static> Render for Menu<A> {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let viewport = window.viewport_size();
        let Some(menu) = self.open.as_ref() else {
            // Closed a moment ago: it fades out, taking no clicks.
            let Some(menu) = self.leaving.as_ref().filter(|_| self.marks.recent("menu")) else {
                return div().into_any_element();
            };
            let (panel, _) = Self::panel(menu, viewport, false, cx);
            return motion::toward(
                panel,
                "menu-leaving",
                false,
                true,
                motion::FAST,
                |panel, v| panel.opacity(v),
            )
            .into_any_element();
        };
        let (panel, y) = Self::panel(menu, viewport, true, cx);
        div()
            .absolute()
            .size_full()
            .child(
                div()
                    .id("ctx-backdrop")
                    .absolute()
                    .size_full()
                    // Modal: a press on the View button must only close the menu, not reach
                    // the button and open it again.
                    .occlude()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| this.close(cx)),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(|this, _, _, cx| this.close(cx)),
                    ),
            )
            // It drops 4 px into place as it fades in (#56, 5).
            .child(motion::appear(
                panel,
                "menu-coming",
                motion::FAST,
                move |panel, v| panel.top(px(y - 4. * (1. - v))).opacity(v),
            ))
            .into_any_element()
    }
}
