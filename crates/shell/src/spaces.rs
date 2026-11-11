//! The menu on the left drawn (#70): the four spaces of the shell — Inbox, Code, Branches and Merge requests — with
//! what waits in each, and what is open under each; a space the project cannot have is greyed, with the line that says
//! what it needs. In Menu mode the spaces nest under the active project's row (X13); folded, the menu is icons (D2).
//! What is open, what waits and what a click does are the tab's (the contract `artoo_shell_api::spaces`).

use crate::rail::project_hue;
use artoo_shell_api::spaces::{Places, Space, SpaceRow, SpacesHost, SpacesMenu, SpacesRenderer};
use artoo_ui_kit::controls::DragLabel;
use artoo_ui_kit::hover::{self, CutInTip, Hover, Tip, TipOwned};
use artoo_ui_kit::monogram::{initials, monogram};
use artoo_ui_kit::motion::{self};
use artoo_ui_kit::{ROW_RADIUS, icons, px, theme, words};
use gpui::{
    AnyElement, ClickEvent, FontWeight, MouseButton, MouseDownEvent, SharedString, canvas, div,
    point, prelude::*,
};
use std::rc::Rc;

/// The menu's width folded to icons; open, it is the reader's, its edge dragged (#186).
const ICONS: f32 = 52.;
/// The menu's side padding, folded or open.
const EDGE: f32 = 6.;
/// A space's icon (17 px) from its row's start: centred in the folded menu, as far in open, and
/// further under a project.
const CENTRED: f32 = (ICONS - 2. * EDGE - 17.) / 2.;
const INSET: f32 = 10.;
const NESTED: f32 = 28.;

/// A space being dragged in the menu.
#[derive(Clone, Copy)]
pub struct DragSpace(pub Space);

/// The row on screen, `key`, in the menu: when it is not the one last brought into view, the
/// next frame scrolls the menu the least that shows it whole (#146). Drawn inside the row.
fn reveal(places: &Places, key: String) -> impl IntoElement + use<> {
    if *places.revealed.borrow() != key {
        places.revealed.replace(key);
        places.reveal.set(true);
    }
    let (scroll, reveal) = (places.scroll.clone(), places.reveal.clone());
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

/// The menu's renderer, installed by `install`.
pub(crate) struct Renderer;

impl SpacesRenderer for Renderer {
    fn menu(&self, menu: SpacesMenu<'_>, host: Rc<dyn SpacesHost>) -> AnyElement {
        self::menu(menu, host)
    }
}

/// The menu.
fn menu(menu: SpacesMenu, host: Rc<dyn SpacesHost>) -> AnyElement {
    let folded = menu.folded;
    let (open, edge) = (menu.width, menu.edge);
    // Folding or unfolding: the names fade while the menu narrows or widens, and the icons
    // stay where they are (#56, 2).
    let moving = menu.moving;
    let named = !folded || moving;
    let here = menu.here;
    let nested = !menu.projects.is_empty();
    let draw = |row: SpaceRow| {
        let space = row.space;
        let id = space.id();
        let label = row.label;
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
            None if row.key.is_empty() => label.to_owned(),
            None => format!("{label}  {}", row.key),
        }
        .into();
        let disclose = (!folded && !opens.is_empty()).then(|| {
            div()
                .id((id.clone(), 5usize))
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
                    (id.clone(), 6usize),
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
            .id(id.clone())
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
                    .child(reveal(menu.places, id.to_string()))
            })
            .when(missing.is_some(), |s| s.opacity(0.5))
            .when(!row.inert, |s| {
                s.cursor_pointer()
                    .hover(|s| s.bg(theme::hover()).text_color(theme::text()))
            })
            .group(id.clone())
            .child(
                icons::icon(row.icon)
                    .size(px(17.))
                    .text_color(if within {
                        theme::text()
                    } else {
                        theme::muted()
                    })
                    .when(!row.inert, |s| {
                        s.group_hover(id.clone(), |s| s.text_color(theme::text()))
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
                    (id.clone(), 1usize),
                    !folded,
                    moving,
                    motion::FAST,
                    |label, v| label.opacity(v),
                ))
                // What an agent put under it, folded away: the dot on the space (#160).
                .children(call.filter(|_| !folded).map(|what| {
                    div()
                        .id((id.clone(), 7usize))
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
                        (id.clone(), 8usize),
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
            (id.clone(), 7usize),
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
                            .child(reveal(menu.places, format!("{id}:{}", open.tip)))
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
            let mark = initials(&tab.title);
            let tint = project_hue(tab.root.as_deref(), &tab.title);
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
                .child(monogram(&mark, tint, 22.))
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
        move |menu, v| menu.w(px(motion::mix(ICONS, open, v))),
    );
    motion::toward(
        div()
            .relative()
            .flex_none()
            .flex()
            .overflow_hidden()
            .child(list)
            // Its edge drags it, from 216 to 360 px, over its own right side (#186).
            .children(edge.map(|edge| {
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .right_0()
                    .flex()
                    .child(edge)
            })),
        "places-zen",
        !menu.zen,
        menu.zen_moving,
        motion::BASE,
        move |menu, v| menu.max_w(px(open * v)).opacity(v),
    )
    .into_any_element()
}
