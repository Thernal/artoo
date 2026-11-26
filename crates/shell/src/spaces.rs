//! The menu on the left drawn (#70): the four spaces of the shell — Inbox, Code, Branches and Merge requests — with
//! what waits in each, and what is open under each; a space the project cannot have is greyed, with the line that says
//! what it needs. In Menu mode the spaces nest under the active project's row (X13); folded, the menu is icons (D2).
//! What is open, what waits and what a click does are the tab's (the contract `artoo_shell_api::spaces`).

use crate::rail::project_hue;
use artoo_shell_api::spaces::{
    DropAt, Drops, Head, Open, Places, Space, SpaceRow, SpacesHost, SpacesMenu, SpacesRenderer,
};
use artoo_ui_kit::controls::DragLabel;
use artoo_ui_kit::hover::{self, CutInTip, Hover, Tip, TipOwned};
use artoo_ui_kit::monogram::{initials, monogram};
use artoo_ui_kit::motion::{self};
use artoo_ui_kit::{ROW_RADIUS, icons, px, theme, words};
use gpui::{
    AnyElement, ClickEvent, Context, FontWeight, MouseButton, MouseDownEvent, Render, SharedString,
    Window, canvas, div, point, prelude::*,
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

/// What follows the pointer while a row is dragged: its words, under the pointer, so the row it is over shows what
/// a drop would do.
struct Ghost(SharedString);

impl Render for Ghost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().pt(px(30.)).pl(px(24.)).child(
            div()
                .w(px(196.))
                .px(px(10.))
                .py(px(4.))
                .rounded(px(ROW_RADIUS))
                .bg(theme::selected())
                .border_1()
                .border_color(theme::island_border())
                .text_size(px(12.))
                .text_color(theme::text())
                .child(hover::cut("drag-ghost", self.0.clone())),
        )
    }
}

/// A page's or a group's row being dragged in the menu (#246): what the drop names, and the words that follow the
/// pointer. Where it may go the tab says (`Drops`), as it draws the menu during the drag.
#[derive(Clone)]
struct DragRow {
    key: SharedString,
    label: SharedString,
}

/// The row a drop would take in: the accent, faint, over its words.
fn tint() -> gpui::Rgba {
    gpui::Rgba {
        a: 0.14,
        ..theme::accent()
    }
}

/// A step in for each group a row stands in, three at most (Design round 48: the name stays readable).
fn indent(depth: usize) -> f32 {
    16. * depth.min(3) as f32
}

/// The words over a place a drop would change more than an order: `New group`, `Move to Groups`.
fn hint_chip(group: SharedString, hint: Option<&'static str>) -> Option<gpui::Div> {
    hint.map(|hint| {
        div()
            // A cursor gives it the hitbox its style under a drag is read by.
            .cursor_pointer()
            .invisible()
            .group_drag_over::<DragRow>(group, |s| s.visible())
            .flex_none()
            .px(px(8.))
            .rounded(px(4.))
            .bg(theme::accent())
            .text_color(theme::base())
            .text_size(px(11.))
            .font_weight(FontWeight::SEMIBOLD)
            .child(hint)
    })
}

/// Row `n`'s places to drop on, over it, as the tab says (`drops`): before it and after it at its edges, onto it
/// between them, where its hint shows.
fn zones(
    host: &Rc<dyn SpacesHost>,
    space: Space,
    id: &SharedString,
    n: usize,
    key: SharedString,
    drops: Drops,
) -> Vec<AnyElement> {
    let zone = |at: DropAt, takes: bool| {
        let (host, to) = (host.clone(), key.clone());
        let name: SharedString = format!("{id}-drop-{n}-{}", at as u8).into();
        div()
            .absolute()
            .left_0()
            .right_0()
            .group(name)
            .can_drop(move |dragged, _, _| {
                takes
                    && dragged
                        .downcast_ref::<DragRow>()
                        .is_some_and(|d| d.key != to)
            })
            .on_drop({
                let to = key.clone();
                move |dragged: &DragRow, window, cx| {
                    host.dropped(space, dragged.key.clone(), to.clone(), at, window, cx)
                }
            })
    };
    let line = |s: gpui::StyleRefinement| s.border_color(theme::accent());
    let mut out = Vec::new();
    if drops.before {
        out.push(
            zone(DropAt::Before, true)
                .top_0()
                .h(px(7.))
                .drag_over::<DragRow>(move |s, _, _, _| line(s).border_t_2())
                .into_any_element(),
        );
    }
    if drops.after {
        out.push(
            zone(DropAt::After, true)
                .bottom_0()
                .h(px(7.))
                .drag_over::<DragRow>(move |s, _, _, _| line(s).border_b_2())
                .into_any_element(),
        );
    }
    if drops.onto {
        let name: SharedString = format!("{id}-drop-{n}-{}", DropAt::Onto as u8).into();
        out.push(
            zone(DropAt::Onto, true)
                .top(px(if drops.before { 7. } else { 0. }))
                .bottom(px(if drops.after { 7. } else { 0. }))
                .flex()
                .items_center()
                .justify_end()
                .pr(px(6.))
                .rounded(px(ROW_RADIUS))
                .drag_over::<DragRow>(move |s, _, _, _| line(s).border_1().bg(tint()))
                .children(hint_chip(name, drops.hint))
                .into_any_element(),
        );
    }
    out
}

/// The row's drag: its words follow the pointer, and the tab learns which row it is (`spaces::set_dragging`).
fn drag_of(key: SharedString, label: SharedString) -> DragRow {
    DragRow { key, label }
}

fn ghost(drag: &DragRow, cx: &mut gpui::App) -> gpui::Entity<Ghost> {
    artoo_shell_api::spaces::set_dragging(cx, drag.key.clone());
    cx.new(|_| Ghost(drag.label.clone()))
}

/// A group's head (#246, Design round 48): its chevron, its name — or the field it is typed in — and how many pages
/// it holds; a click folds it, a double click renames it, a right click is its menu, a row dropped on it goes in.
fn group_head(
    host: &Rc<dyn SpacesHost>,
    space: Space,
    id: &SharedString,
    n: usize,
    nested: bool,
    head: Head,
    open: Open,
) -> AnyElement {
    let (click, menu_row) = (host.clone(), host.clone());
    let key = open.key.clone();
    let drag = drag_of(key.clone(), open.label.clone().into());
    let drops = zones(host, space, id, n, key, open.drops);
    div()
        .id(SharedString::from(format!("{id}-open-{n}")))
        .relative()
        .flex_none()
        .h(px(28.))
        .ml(px(if nested { 35. } else { 17. }))
        // Its chevron nearer the rows in it, as the design's tree stands.
        .pl(px(14. + indent(open.depth)))
        .pr(px(8.))
        .flex()
        .items_center()
        .gap(px(6.))
        .rounded(px(ROW_RADIUS))
        .text_size(px(12.))
        .text_color(theme::muted())
        .cursor_pointer()
        .hover(|s| s.bg(theme::hover()).text_color(theme::text()))
        .when(open.on, |s| {
            s.bg(theme::selected()).text_color(theme::text())
        })
        // Renamed in place: the whole row is the field, ringed.
        .when(open.editing.is_some(), |s| {
            s.border_1()
                .border_color(theme::accent())
                .bg(gpui::transparent_black())
                .text_color(theme::text())
        })
        .child(
            icons::icon(if head.shut {
                "chevron-right"
            } else {
                "chevron-down"
            })
            .size(px(13.))
            .flex_none()
            .text_color(theme::faint()),
        )
        .child(match open.editing {
            Some(field) => field,
            None => div()
                .flex_1()
                .min_w_0()
                .cut_in_tip()
                .child(open.label)
                .into_any_element(),
        })
        .child(
            div()
                .flex_none()
                .text_size(px(11.))
                .text_color(theme::faint())
                .child(head.count.to_string()),
        )
        .tooltip_show_delay(hover::LONG)
        .tooltip({
            let tip: SharedString = open.tip.into();
            move |_, cx| cx.new(|_| TipOwned(tip.clone())).into()
        })
        .on_mouse_down(MouseButton::Right, move |e: &MouseDownEvent, window, cx| {
            menu_row.row_menu(space, n, e.position, window, cx)
        })
        .on_click(move |e: &ClickEvent, window, cx| match e.click_count() {
            1 => click.open(space, n, false, window, cx),
            2 => click.rename(space, n, window, cx),
            _ => {}
        })
        .on_drag(drag, |drag: &DragRow, _, _, cx| ghost(drag, cx))
        .children(drops)
        .into_any_element()
}

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
    // A foot's row (Plugins) is drawn as a space's, but is never dragged nor dropped on, and its scroll is not
    // the list's.
    let draw = |row: SpaceRow, foot: bool| {
        let space = row.space;
        let id = space.id();
        let label = row.label;
        let key = row.key.clone();
        let (missing, opens, shut) = (row.missing, row.opens, row.shut);
        let any = !opens.is_empty();
        let (toggle, go, moved) = (host.clone(), host.clone(), host.clone());
        let (row_drops, dropped_on) = (row.drops, host.clone());
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
                    .when(!foot, |s| s.child(reveal(menu.places, id.to_string())))
            })
            .when(missing.is_some(), |s| s.opacity(0.5))
            .when(!row.inert, |s| {
                s.cursor_pointer()
                    .hover(|s| s.bg(theme::hover()).text_color(theme::text()))
            })
            // ⌃⌘ held: its key over it (#193).
            .children(
                (!row.key.is_empty())
                    .then(|| {
                        artoo_ui_kit::keys::held_cap(&row.key, artoo_ui_kit::keys::Held::CtrlCmd)
                    })
                    .flatten()
                    .map(|cap| cap.right(px(30.)).bottom(px(8.))),
            )
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
            // Dragged onto another space, it takes that place in the menu (kept). A page or a group dropped on it
            // goes to the bottom of its list, where the tab says it may (Design round 48).
            .when(!foot, |s| {
                s.drag_over::<DragSpace>(|s, _, _, _| s.bg(theme::selected()))
                    .on_drag(DragSpace(space), move |_, _, _, cx| {
                        cx.new(|_| DragLabel(words::say(label).into_owned()))
                    })
                    .on_drop(move |dragged: &DragSpace, window, cx| {
                        moved.moved(dragged.0, space, window, cx)
                    })
                    .can_drop(move |dragged, _, _| {
                        dragged.is::<DragSpace>() || (row_drops.onto && dragged.is::<DragRow>())
                    })
                    .drag_over::<DragRow>(|s, _, _, _| {
                        s.bg(tint()).border_1().border_color(theme::accent())
                    })
                    .on_drop(move |dragged: &DragRow, window, cx| {
                        dropped_on.dropped_on(dragged.key.clone(), space, window, cx)
                    })
                    .children(hint_chip(id.clone(), row_drops.hint).map(|chip| {
                        div()
                            .absolute()
                            .right(px(30.))
                            .top_0()
                            .bottom_0()
                            .flex()
                            .items_center()
                            .child(chip)
                    }))
            })
            // A double click folds or opens what is open under it, as its chevron does.
            .on_click(move |e: &ClickEvent, window, cx| {
                if e.click_count() == 2 && any {
                    go.toggle(space, window, cx)
                } else {
                    go.go(space, window, cx);
                    // Its key, once, after the mouse went there (#193).
                    if !key.is_empty() {
                        artoo_ui_kit::keys::teach(&words::say(label), &key, window, cx)
                    }
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
                if let Some(head) = open.head {
                    return group_head(&host, space, &id, n, nested, head, open);
                }
                let group: gpui::SharedString = format!("{id}-open-{n}").into();
                let key = open.key.clone();
                let depth = open.depth;
                let drag = drag_of(key.clone(), open.label.clone().into());
                let drops = (!key.is_empty()).then(|| zones(&host, space, &id, n, key, open.drops));
                let (on, picked, editing) = (open.on, open.picked, open.editing.is_some());
                let (pick, close, menu_row, rename) =
                    (host.clone(), host.clone(), host.clone(), host.clone());
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
                    .pl(px(12. + indent(depth)))
                    .pr(px(6.))
                    .relative()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .text_size(px(12.))
                    .cursor_pointer()
                    .when(on, |s| {
                        s.border_l_2()
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
                    // Renamed in place: the whole row is the field, ringed, its close and dots away.
                    .when(editing, |s| {
                        s.border_1()
                            .border_color(theme::accent())
                            .rounded(px(ROW_RADIUS))
                            .bg(gpui::transparent_black())
                            .text_color(theme::text())
                    })
                    .child(open.icon)
                    // The row's long hover holds the whole of it; renamed, the field takes its place.
                    .child(match open.editing {
                        Some(field) => field,
                        None => div()
                            .flex_1()
                            .min_w_0()
                            .cut_in_tip()
                            .when(open.mono, |s| s.font_family(theme::code_font()))
                            .when(open.preview, |s| s.italic())
                            .child(open.label)
                            .into_any_element(),
                    })
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
                    .children(
                        (open.drafts && !editing).then(|| dot(theme::agent(), "Unsent drafts")),
                    )
                    .children((open.news && !editing).then(|| dot(theme::accent(), "New activity")))
                    .when(!editing, |s| {
                        s.child(
                            div()
                                .id(SharedString::from(format!("{id}-close-{n}")))
                                .flex_none()
                                .size(px(18.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(4.))
                                // Its room only while it shows: a row not on reads to the menu's edge (the design).
                                .when(!on, |s| s.hidden().group_hover(group, |s| s.flex()))
                                .hover(|s| s.bg(theme::hover()))
                                .child(icons::icon("x").size(px(12.)).text_color(theme::faint()))
                                .tooltip_show_delay(hover::LONG)
                                .tooltip(|_, cx| cx.new(|_| Tip("Close  ⌘W")).into())
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .on_click(move |_: &ClickEvent, window, cx| {
                                    close.close(space, n, window, cx)
                                }),
                        )
                    })
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
                        if e.click_count() >= 2 {
                            return rename.rename(space, n, window, cx);
                        }
                        pick.open(space, n, e.modifiers().shift, window, cx)
                    })
                    // Dragged among the pages and groups of its space (#246).
                    .when(drops.is_some(), |s| {
                        s.on_drag(drag, |drag: &DragRow, _, _, cx| ghost(drag, cx))
                    })
                    .children(drops.into_iter().flatten())
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
    let mut drawn = Some(
        (menu.rows.into_iter())
            .map(|row| draw(row, false))
            .collect::<Vec<_>>(),
    );
    let foot = (menu.foot.into_iter())
        .map(|row| draw(row, true))
        .collect::<Vec<_>>();
    let under = if nested { None } else { drawn.take() };
    // After the last space, while a group dragged there would go to the Groups tab (Design round 48, M3).
    let end = menu.end.onto.then(|| {
        let host = host.clone();
        div()
            .id("menu-end-drop")
            .group("menu-end-drop")
            .flex_none()
            .h(px(28.))
            .mx(px(8.))
            .flex()
            .items_center()
            .justify_end()
            .border_t_2()
            .border_color(gpui::transparent_black())
            .can_drop(|dragged, _, _| dragged.is::<DragRow>())
            .drag_over::<DragRow>(|s, _, _, _| s.border_color(theme::accent()))
            .on_drop(move |dragged: &DragRow, window, cx| {
                host.dropped_on(
                    dragged.key.clone(),
                    artoo_shell_api::spaces::GROUPS,
                    window,
                    cx,
                )
            })
            .children(hint_chip("menu-end-drop".into(), menu.end.hint))
    });
    let tabs = menu.tabs;
    let list = div()
        .id("places")
        .flex_1()
        .min_h_0()
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
        .children(end)
        // A narrow window's side panel, folded into this column (#80).
        .children(menu.panel_icons);
    // The foot's rows, under the list and apart from it: they stay when the list scrolls (#176).
    let foot = (!foot.is_empty()).then(|| {
        div()
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(2.))
            .px(px(EDGE))
            .py(px(4.))
            .border_t_1()
            .border_color(theme::island_border())
            .children(foot)
    });
    let list = div()
        .flex_none()
        .flex()
        .flex_col()
        .child(list)
        .children(foot);
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
