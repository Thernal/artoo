//! The menu on the left drawn (#70): the four spaces of the shell — Inbox, Code, Branches and Merge requests — with
//! what waits in each, and what is open under each; a space the project cannot have is greyed, with the line that says
//! what it needs. In Menu mode the spaces nest under the active project's row (X13); folded, the menu is icons (D2).
//! What is open, what waits and what a click does are the tab's.

use crate::rail;
use artoo_shell_api::tabs::{TabInfo, TabsHost};
use artoo_ui_kit::controls::DragLabel;
use artoo_ui_kit::hover::{self, CutInTip, Hover, Tip, TipOwned};
use artoo_ui_kit::motion::{self, Folds};
use artoo_ui_kit::room::Heights;
use artoo_ui_kit::{ROW_RADIUS, icons, px, theme, words};
use gpui::{
    AnyElement, App, ClickEvent, FontWeight, MouseButton, MouseDownEvent, Pixels, Point,
    SharedString, Window, canvas, div, point, prelude::*,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// The menu's width, open and folded to icons.
const OPEN: f32 = 216.;
const ICONS: f32 = 52.;
/// The menu's side padding, folded or open.
const EDGE: f32 = 6.;
/// A space's icon (17 px) from its row's start: centred in the folded menu, as far in open, and
/// further under a project.
const CENTRED: f32 = (ICONS - 2. * EDGE - 17.) / 2.;
const INSET: f32 = 10.;
const NESTED: f32 = 28.;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Space {
    Inbox,
    Code,
    Branches,
    Requests,
}

impl Space {
    pub const ALL: [Space; 4] = [Space::Inbox, Space::Code, Space::Branches, Space::Requests];

    pub fn label(self) -> &'static str {
        match self {
            Space::Inbox => "Inbox",
            Space::Code => "Code",
            Space::Branches => "Branches",
            Space::Requests => "Merge requests",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Space::Inbox => "inbox",
            Space::Code => "folder",
            Space::Branches => "branch",
            Space::Requests => "pull-request",
        }
    }

    /// Its key as the app writes keys (Ctrl+Shift+1…4 on Windows and Linux).
    pub fn key(self) -> &'static str {
        match self {
            Space::Inbox => "⌃⌘1",
            Space::Code => "⌃⌘2",
            Space::Branches => "⌃⌘3",
            Space::Requests => "⌃⌘4",
        }
    }

    /// Its element's id in the menu.
    pub fn id(self) -> &'static str {
        match self {
            Space::Inbox => "space-inbox",
            Space::Code => "space-code",
            Space::Branches => "space-branches",
            Space::Requests => "space-requests",
        }
    }

    /// As `layout.txt` keeps the menu's order, and features name it (`Pages::space`).
    pub fn name(self) -> &'static str {
        self.id().trim_start_matches("space-")
    }

    /// Its place in `Space::ALL`.
    pub fn ix(self) -> usize {
        match self {
            Space::Inbox => 0,
            Space::Code => 1,
            Space::Branches => 2,
            Space::Requests => 3,
        }
    }

    /// The space named `name`.
    pub fn named(name: &str) -> Option<Space> {
        Space::ALL.into_iter().find(|s| s.name() == name)
    }
}

/// The menu's spaces in the order `kept` names them (dragged there), any it leaves out after
/// them in their first order.
pub fn ordered(kept: &str) -> Vec<Space> {
    let mut out: Vec<Space> = kept
        .split(',')
        .filter_map(|n| Space::named(n.trim()))
        .collect();
    out.dedup();
    for s in Space::ALL {
        if !out.contains(&s) {
            out.push(s);
        }
    }
    out
}

/// A space being dragged in the menu.
#[derive(Clone, Copy)]
pub struct DragSpace(pub Space);

/// The menu's scroll (#146): its rows scroll once they are taller than the window — many
/// requests open under Merge requests —, and the row of the page on screen comes into view when
/// that page changes.
#[derive(Default)]
pub struct Places {
    scroll: gpui::ScrollHandle,
    /// The row last brought into view, by what it shows.
    revealed: RefCell<String>,
    /// The next frame brings the row on screen into view.
    reveal: Rc<Cell<bool>>,
}

impl Places {
    /// The row on screen, `key`, in the menu: when it is not the one last brought into view, the
    /// next frame scrolls the menu the least that shows it whole (#146). Drawn inside the row.
    pub fn reveal(&self, key: String) -> impl IntoElement + use<> {
        if *self.revealed.borrow() != key {
            self.revealed.replace(key);
            self.reveal.set(true);
        }
        let (scroll, reveal) = (self.scroll.clone(), self.reveal.clone());
        canvas(
            move |row, window, _| {
                if !reveal.replace(false) {
                    return;
                }
                let view = scroll.bounds();
                let by = if row.top() < view.top() {
                    view.top() - row.top()
                } else if row.bottom() > view.bottom() {
                    view.bottom() - row.bottom()
                } else {
                    return;
                };
                let (y, max) = (scroll.offset().y + by, scroll.max_offset().y);
                scroll.set_offset(point(scroll.offset().x, y.clamp(-max, gpui::px(0.))));
                window.request_animation_frame();
            },
            |_, _, _, _| {},
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full()
    }
}

/// What is open under a space, as its row in the menu: a request, a branch, Code's file.
pub struct Open {
    /// Its icon: a request's or a branch's, Code's file by its own (#95).
    pub icon: AnyElement,
    pub label: String,
    pub mono: bool,
    /// Its long hover, the whole of it.
    pub tip: String,
    /// It is the page on screen.
    pub on: bool,
    /// Picked with shift, for ⌘W.
    pub picked: bool,
    /// Agent comments not sent yet on its files.
    pub drafts: bool,
    /// Notifications of it not read yet.
    pub news: bool,
    /// The agent that gathered it (#148): its badge, and how many of its pending comments wait.
    pub agent: Option<(AnyElement, usize)>,
    /// What an agent put there for the reviewer, not opened since (#160).
    pub call: Option<String>,
}

/// A space's row, with what is open under it.
pub struct SpaceRow {
    pub space: Space,
    /// What the project lacks for it, as its line: greyed.
    pub missing: Option<&'static str>,
    /// It cannot be opened at all (no remote): no hover.
    pub inert: bool,
    /// What waits in it: its count, a badge folded.
    pub waiting: usize,
    /// What is open under it is folded away.
    pub shut: bool,
    pub opens: Vec<Open>,
    /// What is open under it is folding away or back.
    pub opens_moving: bool,
}

/// The menu as the tab sees it.
pub struct SpacesMenu<'a> {
    /// Folded to icons.
    pub folded: bool,
    /// Folding or unfolding.
    pub moving: bool,
    /// The space on screen.
    pub here: Option<Space>,
    /// The spaces, in the menu's order.
    pub rows: Vec<SpaceRow>,
    /// Menu mode: the window's projects, the spaces under the active one's row (X13).
    pub projects: Vec<(usize, TabInfo)>,
    pub tabs: Rc<dyn TabsHost>,
    /// A narrow window's side panel, folded into this column (#80).
    pub panel_icons: Option<AnyElement>,
    /// Zen: it slides away (#56, 16), `zen_moving` while it does.
    pub zen: bool,
    pub zen_moving: bool,
    pub places: &'a Places,
    pub folds: &'a Folds,
    pub heights: &'a Heights,
}

/// What a click on the menu does.
pub trait SpacesHost {
    /// A space's row: its page.
    fn go(&self, space: Space, window: &mut Window, cx: &mut App);
    /// What is open under `space` shown or hidden.
    fn toggle(&self, space: Space, window: &mut Window, cx: &mut App);
    /// `from` dragged onto `to`: it takes that place.
    fn moved(&self, from: Space, to: Space, window: &mut Window, cx: &mut App);
    /// Row `n` under `space` clicked; with shift it is picked, for ⌘W.
    fn open(&self, space: Space, n: usize, shift: bool, window: &mut Window, cx: &mut App);
    /// Row `n`'s ×.
    fn close(&self, space: Space, n: usize, window: &mut Window, cx: &mut App);
    /// Row `n`'s right click, at `at`: its link, and a request's number (#162).
    fn row_menu(
        &self,
        space: Space,
        n: usize,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut App,
    );
}

/// The menu.
pub fn menu(menu: SpacesMenu, host: Rc<dyn SpacesHost>) -> AnyElement {
    let folded = menu.folded;
    // Folding or unfolding: the names fade while the menu narrows or widens, and the icons
    // stay where they are (#56, 2).
    let moving = menu.moving;
    let named = !folded || moving;
    let here = menu.here;
    let nested = !menu.projects.is_empty();
    let draw = |row: SpaceRow| {
        let space = row.space;
        let id = space.id();
        let label = space.label();
        let (missing, opens, shut) = (row.missing, row.opens, row.shut);
        let any = !opens.is_empty();
        let (toggle, go, moved) = (host.clone(), host.clone(), host.clone());
        let call = opens
            .iter()
            .filter(|_| shut)
            .find_map(|o| o.call.as_ref().map(|what| format!("{}: {what}", o.label)));
        // The space's row is marked when none of what is open under it is the page — at its
        // root, or with them folded away.
        let within = here == Some(space);
        let on = within && (folded || shut || !opens.iter().any(|o| o.on));
        let n = Some(row.waiting).filter(|&n| n > 0 && missing.is_none());
        let tip: gpui::SharedString = match missing {
            Some(line) => format!("{label} — {line}"),
            None => format!("{label}  {}", space.key()),
        }
        .into();
        let disclose = (!folded && !opens.is_empty()).then(|| {
            div()
                .id((id, 5usize))
                .flex_none()
                .size(px(18.))
                .mr(px(-4.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(4.))
                .hover(|s| s.bg(theme::hover()))
                // Folded it points right, open down (#56).
                .child(motion::toward(
                    icons::icon("chevron")
                        .size(px(13.))
                        .text_color(theme::faint()),
                    (id, 6usize),
                    !shut,
                    false,
                    motion::FAST,
                    |chevron, v| {
                        chevron.with_transformation(gpui::Transformation::rotate(gpui::radians(
                            -std::f32::consts::FRAC_PI_2 * (1. - v),
                        )))
                    },
                ))
                .tooltip_show_delay(hover::LONG)
                .tooltip(|_, cx| cx.new(|_| Tip("Show or hide what is open here")).into())
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(move |e: &ClickEvent, window, cx| {
                    if e.click_count() < 2 {
                        toggle.toggle(space, window, cx)
                    }
                })
        });
        let line = div()
            .id(id)
            .relative()
            .h(px(32.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(10.))
            .pr(px(8.))
            .rounded(px(ROW_RADIUS))
            .text_color(if within {
                theme::text()
            } else {
                theme::muted()
            })
            .when(on, |s| {
                s.bg(theme::selected())
                    .child(menu.places.reveal(id.to_owned()))
            })
            .when(missing.is_some(), |s| s.opacity(0.5))
            .when(!row.inert, |s| {
                s.cursor_pointer()
                    .hover(|s| s.bg(theme::hover()).text_color(theme::text()))
            })
            .group(id)
            .child(
                icons::icon(space.icon())
                    .size(px(17.))
                    .text_color(if within {
                        theme::text()
                    } else {
                        theme::muted()
                    })
                    .when(!row.inert, |s| {
                        s.group_hover(id, |s| s.text_color(theme::text()))
                    }),
            )
            .when(named, |s| {
                // The names fade in as the menu widens, and out as it narrows (#56, 2).
                s.child(motion::toward(
                    div()
                        .flex_1()
                        .min_w_0()
                        .cut_in_tip()
                        .child(words::say(label).into_owned()),
                    (id, 1usize),
                    !folded,
                    moving,
                    motion::FAST,
                    |label, v| label.opacity(v),
                ))
                // What an agent put under it, folded away: the dot on the space (#160).
                .children(call.filter(|_| !folded).map(|what| {
                    div()
                        .id((id, 7usize))
                        .flex_none()
                        .size(px(7.))
                        .rounded_full()
                        .bg(theme::warning())
                        .tip(what)
                }))
                // The count is the badge's once the menu folds.
                .children(n.filter(|_| !folded).map(|n| {
                    div()
                        .flex_none()
                        .text_size(px(11.))
                        .text_color(theme::faint())
                        .child(n.to_string())
                }))
                .children(disclose)
            })
            // Folded, a badge on the icon's corner says something waits for you; it fades in
            // as the menu narrows.
            .when(folded, |s| {
                s.children(n.map(|n| {
                    let badge = div()
                        .absolute()
                        .top(px(3.))
                        .left(px(CENTRED + 11.5))
                        .min_w(px(14.))
                        .h(px(14.))
                        .px(px(3.))
                        .rounded_full()
                        .bg(theme::accent())
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(9.))
                        .font_weight(FontWeight::BOLD)
                        .text_color(theme::base())
                        .child(n.min(99).to_string());
                    motion::toward(
                        badge,
                        (id, 8usize),
                        true,
                        moving,
                        motion::FAST,
                        |badge, v| badge.opacity(v),
                    )
                }))
            })
            .tooltip_show_delay(hover::LONG)
            .tooltip(move |_, cx| cx.new(|_| TipOwned(tip.clone())).into())
            // Dragged onto another space, it takes that place in the menu (kept).
            .drag_over::<DragSpace>(|s, _, _, _| s.bg(theme::selected()))
            .on_drag(DragSpace(space), move |_, _, _, cx| {
                cx.new(|_| DragLabel(words::say(label).into_owned()))
            })
            .on_drop(move |dragged: &DragSpace, window, cx| {
                moved.moved(dragged.0, space, window, cx)
            })
            // A double click folds or opens what is open under it, as its chevron does.
            .on_click(move |e: &ClickEvent, window, cx| {
                if e.click_count() == 2 && any {
                    go.toggle(space, window, cx)
                } else {
                    go.go(space, window, cx)
                }
            });
        // The icon's place: centred in the folded menu, after the indent open; it moves only
        // with the menu's own motion.
        let line = motion::toward(
            line,
            (id, 7usize),
            !folded,
            moving,
            motion::BASE,
            move |line, v| {
                let open = if nested { NESTED } else { INSET };
                line.pl(px(motion::mix(CENTRED, open, v)))
            },
        );
        let why = missing.filter(|_| !folded).map(|m| {
            div()
                .flex_none()
                .pt(px(2.))
                .pl(px(if nested { 55. } else { 37. }))
                .pr(px(8.))
                .pb(px(6.))
                .text_size(px(11.))
                .text_color(theme::faint())
                .child(words::say(m).into_owned())
        });
        let shows = !folded && !shut && !opens.is_empty();
        let opens = opens
            .into_iter()
            .enumerate()
            .map(|(n, open)| {
                let group: gpui::SharedString = format!("{id}-open-{n}").into();
                let (on, picked) = (open.on, open.picked);
                let (pick, close, menu_row) = (host.clone(), host.clone(), host.clone());
                let dot = |color: gpui::Rgba, tip: &'static str| {
                    div()
                        .id(SharedString::from(format!("{id}-dot-{n}-{tip}")))
                        .flex_none()
                        .size(px(7.))
                        .rounded_full()
                        .bg(color)
                        .tooltip_show_delay(hover::LONG)
                        .tooltip(move |_, cx| cx.new(|_| Tip(tip)).into())
                };
                div()
                    .id(SharedString::from(format!("{id}-open-{n}")))
                    .group(group.clone())
                    .flex_none()
                    .min_h(px(28.))
                    .ml(px(if nested { 35. } else { 17. }))
                    .pl(px(12.))
                    .pr(px(6.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .text_size(px(12.))
                    .cursor_pointer()
                    .when(on, |s| {
                        s.relative()
                            .border_l_2()
                            .border_color(theme::accent())
                            .rounded_r(px(ROW_RADIUS))
                            .bg(theme::selected())
                            .text_color(theme::text())
                            .child(menu.places.reveal(format!("{id}:{}", open.tip)))
                    })
                    .when(!on, |s| {
                        s.border_l_1()
                            .border_color(theme::island_border())
                            .text_color(theme::muted())
                            .hover(|s| s.text_color(theme::text()))
                    })
                    // Picked with shift, for ⌘W.
                    .when(picked && !on, |s| {
                        s.rounded_r(px(ROW_RADIUS))
                            .bg(theme::selected())
                            .text_color(theme::text())
                    })
                    .child(open.icon)
                    // The row's long hover holds the whole of it.
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .cut_in_tip()
                            .when(open.mono, |s| s.font_family(theme::code_font()))
                            .child(open.label),
                    )
                    .children(open.agent.map(|(badge, waiting)| {
                        div()
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap(px(4.))
                            .child(badge)
                            .when(waiting > 0, |s| {
                                s.child(
                                    div()
                                        .text_size(px(11.))
                                        .text_color(theme::faint())
                                        .child(waiting.to_string()),
                                )
                            })
                    }))
                    .children(open.call.map(|what| {
                        div()
                            .id(SharedString::from(format!("{id}-call-{n}")))
                            .flex_none()
                            .size(px(7.))
                            .rounded_full()
                            .bg(theme::warning())
                            .tip(what)
                    }))
                    .children(open.drafts.then(|| dot(theme::agent(), "Unsent drafts")))
                    .children(open.news.then(|| dot(theme::accent(), "New activity")))
                    .child(
                        div()
                            .id(SharedString::from(format!("{id}-close-{n}")))
                            .flex_none()
                            .size(px(18.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(4.))
                            .when(!on, |s| s.invisible().group_hover(group, |s| s.visible()))
                            .hover(|s| s.bg(theme::hover()))
                            .child(icons::icon("x").size(px(12.)).text_color(theme::faint()))
                            .tooltip_show_delay(hover::LONG)
                            .tooltip(|_, cx| cx.new(|_| Tip("Close  ⌘W")).into())
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(move |_: &ClickEvent, window, cx| {
                                close.close(space, n, window, cx)
                            }),
                    )
                    .tooltip_show_delay(hover::LONG)
                    .tooltip({
                        let tip: SharedString = open.tip.into();
                        move |_, cx| cx.new(|_| TipOwned(tip.clone())).into()
                    })
                    // Its link, and a request's number (#162).
                    .on_mouse_down(MouseButton::Right, move |e: &MouseDownEvent, window, cx| {
                        menu_row.row_menu(space, n, e.position, window, cx)
                    })
                    // Shift picks a run of them to close together with ⌘W.
                    .on_click(move |e: &ClickEvent, window, cx| {
                        pick.open(space, n, e.modifiers().shift, window, cx)
                    })
                    .into_any_element()
            })
            .collect::<Vec<_>>();
        // What is open under it folds away and back in size and alpha (#110), the space
        // above it inside, so none is left at the end.
        let opens = motion::folding(
            menu.folds,
            menu.heights,
            row.opens_moving,
            format!("{id}-opens"),
            shows,
            div()
                .pt(px(2.))
                .flex()
                .flex_col()
                .gap(px(2.))
                .children(opens),
        );
        div()
            .flex_none()
            .flex()
            .flex_col()
            .child(line)
            .children(why)
            .children(opens)
            .into_any_element()
    };
    let mut drawn = Some(menu.rows.into_iter().map(draw).collect::<Vec<_>>());
    let under = if nested { None } else { drawn.take() };
    let tabs = menu.tabs;
    let list = div()
        .id("places")
        .flex_none()
        .flex()
        .flex_col()
        .gap(px(2.))
        .px(px(EDGE))
        .pt(px(4.))
        .pb(px(4.))
        // Taller than the window, it scrolls (#146).
        .overflow_y_scroll()
        .track_scroll(&menu.places.scroll)
        .when(nested, |s| {
            s.child(if folded {
                div()
                    .flex_none()
                    .h(px(1.))
                    .mx(px(8.))
                    .my(px(10.))
                    .bg(theme::island_border())
            } else {
                div()
                    .flex_none()
                    .px(px(8.))
                    .pt(px(10.))
                    .pb(px(4.))
                    .text_size(px(11.))
                    .text_color(theme::faint())
                    .child("Projects")
            })
        })
        .children(menu.projects.into_iter().flat_map(move |(ix, tab)| {
            let mark = rail::initials(&tab.title);
            let tint = rail::project_hue(tab.root.as_deref(), &tab.title);
            let active = tab.active;
            let (select, project_menu) = (tabs.clone(), tabs.clone());
            let project = div()
                .id(("menu-project", ix))
                // The rail's right click: name and colour, move, show, a window, close.
                .on_mouse_down(MouseButton::Right, move |e: &MouseDownEvent, window, cx| {
                    project_menu.menu(ix, e.position, window, cx)
                })
                .h(px(32.))
                .flex()
                .flex_none()
                .items_center()
                .gap(px(10.))
                .pr(px(6.))
                .rounded(px(ROW_RADIUS))
                .cursor_pointer()
                .when(tab.active, |s| s.bg(theme::hover()))
                .hover(|s| s.bg(theme::hover()))
                .child(rail::project_mark(&mark, tint, 22.))
                .when(named, |s| {
                    s.child(motion::toward(
                        hover::cut(("menu-project-title", ix), tab.title.clone())
                            .flex_1()
                            .font_weight(FontWeight::MEDIUM),
                        ("menu-project-name", ix),
                        !folded,
                        moving,
                        motion::FAST,
                        |name, v| name.opacity(v),
                    ))
                    .children((!tab.active && tab.unread > 0).then(|| {
                        div()
                            .flex_none()
                            .text_size(px(11.))
                            .text_color(theme::accent())
                            .child(tab.unread.to_string())
                    }))
                    // Open (its spaces under it) or another one to switch to.
                    .child(
                        icons::icon(if tab.active {
                            "chevron"
                        } else {
                            "chevron-right"
                        })
                        .size(px(14.))
                        .text_color(theme::faint()),
                    )
                })
                .on_click(move |_: &ClickEvent, window, cx| select.select(ix, window, cx));
            // Its mark (22 px) centred folded, near the edge open; it moves with the menu.
            let project = motion::toward(
                project,
                ("menu-project-place", ix),
                !folded,
                moving,
                motion::BASE,
                |project, v| {
                    let centred = (ICONS - 2. * EDGE - 22.) / 2.;
                    project.pl(px(motion::mix(centred, 6., v)))
                },
            )
            .into_any_element();
            let mut rows = vec![project];
            if active {
                rows.extend(drawn.take().into_iter().flatten());
            }
            rows
        }))
        .children(under.into_iter().flatten())
        // A narrow window's side panel, folded into this column (#80).
        .children(menu.panel_icons);
    // Folding to icons narrows it (#56, 2); zen slides it away (16).
    let list = motion::toward(
        list,
        "places-fold",
        !folded,
        false,
        motion::BASE,
        |menu, v| menu.w(px(motion::mix(ICONS, OPEN, v))),
    );
    motion::toward(
        div().flex_none().flex().overflow_hidden().child(list),
        "places-zen",
        !menu.zen,
        menu.zen_moving,
        motion::BASE,
        |menu, v| menu.max_w(px(OPEN * v)).opacity(v),
    )
    .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_menus_order_is_kept_by_name_and_fills_in() {
        assert_eq!(ordered(""), Space::ALL.to_vec());
        assert_eq!(
            ordered("requests,inbox"),
            [Space::Requests, Space::Inbox, Space::Code, Space::Branches]
        );
        assert_eq!(ordered("nonsense,code,code").first(), Some(&Space::Code));
    }
}
