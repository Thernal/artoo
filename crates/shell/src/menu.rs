//! A context menu at the pointer (right click on a commit, a file or a line; the diff's View menu): entries in
//! groups, each doing an act of the tab's kind `A`, handed to the runner the tab gave once the menu has let go.

use artoo_ui_kit::{ISLAND_RADIUS, hover, keys, motion, px, row, theme, ui_zoom};
use gpui::{
    App, Context, MouseButton, Pixels, Point, Render, SharedString, Window, div, prelude::*,
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
}

impl<A> Entry<A> {
    pub fn new(label: impl Into<String>, act: A) -> Self {
        Self {
            label: label.into(),
            key: "",
            checked: None,
            face: None,
            detail: None,
            act: Some(act),
        }
    }

    /// What cannot be done here, and why: `Copy link — only on this Mac`.
    pub fn off(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            key: "",
            checked: None,
            face: None,
            detail: None,
            act: None,
        }
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
}

/// Entries in groups; a rule is drawn between groups, a group may have a heading.
struct Panel<A> {
    at: Point<Pixels>,
    groups: Vec<Vec<Entry<A>>>,
    titles: Vec<Option<&'static str>>,
    /// `WIDTH`, or wider for entries with a detail (#164).
    width: f32,
}

const ROW: f32 = 28.;
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
                width,
            });
            self.leaving = None;
            cx.notify();
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
        let rows: usize = menu.groups.iter().map(Vec::len).sum();
        let heads = menu.titles.iter().flatten().count();
        let height = rows as f32 * ROW + heads as f32 * HEAD + menu.groups.len() as f32 * 9. + 12.;
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
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(theme::faint())
                        .child(t.to_uppercase())
                }))
                .children(entries.iter().enumerate().map(|(i, entry)| {
                    let act = entry.act.clone();
                    let off = act.is_none();
                    // One that cannot be done takes no hover: `row` gives it none as "selected",
                    // and its fill is taken away.
                    row(("ctx-entry", (g * 100 + i) as u64), off)
                        .h(px(ROW))
                        .gap_2()
                        .when(off, |s| {
                            s.bg(gpui::transparent_black())
                                .text_color(theme::faint())
                                .cursor_default()
                        })
                        .child(div().w(px(14.)).text_color(theme::accent()).child(
                            if entry.checked == Some(true) {
                                "✓"
                            } else {
                                ""
                            },
                        ))
                        .child(match entry.detail.clone() {
                            None => hover::cut("label", keys::text(&entry.label).into_owned())
                                .when_some(entry.face.clone(), |s, face| s.font_family(face))
                                .flex_1()
                                .into_any_element(),
                            Some(detail) => div()
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
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(theme::faint())
                                .child(keys::text(entry.key).into_owned()),
                        )
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
