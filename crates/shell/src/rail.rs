//! The window's tabs drawn (Design flow 02, #49): the project rail on its left edge — a mark per open repository,
//! ⌘1…9 in its order, the unread count on the ones you are not in, + to open another, a drag reorders —, or the
//! tabs in the title bar (Design `shell.js`, D12, X10). Settings → Appearance picks the rail (default), tabs or the
//! menu. What a click does is the tab's (`TabsHost`, the contract `artoo_shell_api::tabs`): a right click opens its menu (move, show, a window, close).

use artoo_shell_api::tabs::{TabInfo, TabsHost, TabsRenderer};
use artoo_ui_kit::controls::DragLabel;
use artoo_ui_kit::hover::{self, Tip, TipOwned};
use artoo_ui_kit::monogram::{hue, hue_at, initials, monogram};
use artoo_ui_kit::{ROW_RADIUS, format, icons, keys, motion, px, theme};
use gpui::{
    AnimationExt as _, ClickEvent, Context, FontWeight, MouseButton, MouseDownEvent, Rgba, Window,
    div, prelude::*,
};
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

/// The window's tabs' renderer, installed by `install`.
pub(crate) struct Renderer;

impl TabsRenderer for Renderer {
    fn rail(&self, tabs: Vec<TabInfo>, host: Rc<dyn TabsHost>) -> gpui::AnyElement {
        rail(tabs, host).into_any_element()
    }

    fn title_tabs(&self, tabs: Vec<TabInfo>, host: Rc<dyn TabsHost>) -> gpui::AnyElement {
        title_tabs(tabs, host)
    }
}

/// The project the rail last marked: a rail drawn after a switch slides its marker from there.
static MARKED: AtomicUsize = AtomicUsize::new(usize::MAX);

/// Where a project's marker sits in the rail: its top, from the rail's.
fn marker_top(ix: usize) -> f32 {
    4. + 48. * ix as f32 + 9.
}

/// A project being dragged along the rail: its index.
#[derive(Clone, Copy)]
struct DragProject(usize);

/// What follows the pointer while a project is dragged: its mark.
struct DragMark {
    mark: String,
    hue: Rgba,
}

impl Render for DragMark {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        monogram(&self.mark, self.hue, 34.)
    }
}

/// The rail of `tabs`.
fn rail(tabs: Vec<TabInfo>, host: Rc<dyn TabsHost>) -> impl IntoElement {
    let n = tabs.len();
    let active = tabs.iter().position(|t| t.active);
    let items = tabs.into_iter().enumerate().map(|(ix, tab)| {
        let mark = initials(&tab.title);
        let tint = project_hue(tab.root.as_deref(), &tab.title);
        let (drag_mark, drag_hue) = (mark.clone(), tint);
        // A long hover: the project's name, where it is, its unread count and key (#53).
        let place = tab
            .root
            .as_ref()
            .map(|r| format!(" · {}", format::home_relative(r)))
            .unwrap_or_default();
        let unread = match tab.unread {
            0 => String::new(),
            n => format!(" · {n} new"),
        };
        let tip = if ix < 9 {
            format!("{}{place}{unread}  ⌘{}", tab.title, ix + 1)
        } else {
            format!("{}{place}{unread}", tab.title)
        };
        let (select, menu, moved) = (host.clone(), host.clone(), host.clone());
        div()
            .id(("project", ix))
            .relative()
            .size(px(38.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(12.))
            .cursor_pointer()
            .when(tab.active, |s| s.border_2().border_color(theme::accent()))
            .when(!tab.active, |s| s.hover(|s| s.bg(theme::hover())))
            .drag_over::<DragProject>(|s, _, _, _| s.bg(theme::selected()))
            .child(monogram(&mark, tint, 30.))
            // A new count pops in (#56, 13).
            .children((!tab.active && tab.unread > 0).then(|| {
                let badge = div()
                    .absolute()
                    .top(px(-4.))
                    .right(px(-4.))
                    .min_w(px(16.))
                    .h(px(16.))
                    .px(px(4.))
                    .rounded_full()
                    .bg(theme::removed())
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(10.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(theme::base())
                    .child(tab.unread.min(99).to_string());
                motion::appear(
                    badge,
                    ("rail-badge", tab.unread.min(99)),
                    motion::FAST,
                    |badge, v| badge.opacity(v),
                )
            }))
            .children((!tab.active && tab.unread == 0 && tab.news).then(|| {
                div()
                    .absolute()
                    .top(px(-1.))
                    .right(px(-1.))
                    .size(px(8.))
                    .rounded_full()
                    .bg(theme::accent())
            }))
            // ⌘ held: its key over it (#193).
            .children(
                (ix < 9)
                    .then(|| {
                        artoo_ui_kit::keys::held_cap(
                            &format!("⌘{}", ix + 1),
                            artoo_ui_kit::keys::Held::Cmd,
                        )
                    })
                    .flatten(),
            )
            .tooltip_show_delay(hover::LONG)
            .tooltip(move |_, cx| cx.new(|_| TipOwned(tip.clone().into())).into())
            .on_click(move |_: &ClickEvent, window, cx| select.select(ix, window, cx))
            .on_mouse_down(
                MouseButton::Right,
                move |event: &MouseDownEvent, window, cx| menu.menu(ix, event.position, window, cx),
            )
            .on_drag(DragProject(ix), move |_, _, _, cx| {
                cx.new(|_| DragMark {
                    mark: drag_mark.clone(),
                    hue: drag_hue,
                })
            })
            .on_drop(move |dragged: &DragProject, window, cx| {
                moved.moved(dragged.0, ix, window, cx)
            })
    });
    // The project you are in has a bar at the rail's edge; it slides to the one switched to
    // (#56, 13).
    let marker = active.map(|ix| {
        let from = MARKED.swap(ix, Relaxed);
        let from = if from < n { from } else { ix };
        div()
            .absolute()
            .left(px(-2.))
            .w(px(3.))
            .h(px(18.))
            .rounded_full()
            .bg(theme::text())
            .with_spring(
                "rail-marker",
                gpui::SpringAnimation::new(motion::spring(motion::BASE))
                    .to(marker_top(ix))
                    .from(marker_top(from)),
                |marker, top| marker.top(px(top)),
            )
    });
    div()
        .relative()
        .w(px(48.))
        .flex_none()
        .flex()
        .flex_col()
        .items_center()
        .gap(px(10.))
        .pt(px(4.))
        .children(marker)
        .children(items)
        .child(
            div()
                .id("project-add")
                .size(px(34.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(10.))
                .border_1()
                .border_dashed()
                .border_color(theme::selected())
                .cursor_pointer()
                .hover(|s| s.bg(theme::hover()))
                .child(icons::icon("open").size(px(16.)).text_color(theme::muted()))
                .tooltip_show_delay(hover::LONG)
                .tooltip(|_, cx| {
                    cx.new(|_| TipOwned(keys::text("Open a repository  ⌘T").into_owned().into()))
                        .into()
                })
                .on_click(move |_: &ClickEvent, window, cx| host.add(window, cx)),
        )
}

/// The tabs in the title bar: each project's mark, name and unread count, a dot when it has news, ✕ on the active
/// one (on the others under the pointer), + to open another.
fn title_tabs(tabs: Vec<TabInfo>, host: Rc<dyn TabsHost>) -> gpui::AnyElement {
    div()
        .flex_none()
        .min_w_0()
        .max_w(px(760.))
        .overflow_hidden()
        .flex()
        .items_center()
        .gap(px(4.))
        .children(tabs.into_iter().enumerate().map(|(ix, tab)| {
            let mark = initials(&tab.title);
            let hue = project_hue(tab.root.as_deref(), &tab.title);
            let group: gpui::SharedString = format!("tab-{ix}").into();
            let (menu, close, moved, select) =
                (host.clone(), host.clone(), host.clone(), host.clone());
            div()
                .id(("tab", ix))
                .group(group.clone())
                // The rail's right click: name and colour, move, show, a window, close.
                .on_mouse_down(MouseButton::Right, move |e: &MouseDownEvent, window, cx| {
                    menu.menu(ix, e.position, window, cx)
                })
                .flex_shrink(1.)
                .min_w(px(60.))
                .max_w(px(220.))
                .h(px(28.))
                .flex()
                .items_center()
                .gap(px(8.))
                .pl(px(6.))
                .pr(px(4.))
                .rounded(px(ROW_RADIUS))
                .cursor_pointer()
                .when(tab.active, |s| {
                    s.bg(theme::panel())
                        .border_1()
                        .border_color(theme::island_border())
                })
                .when(!tab.active, |s| {
                    s.text_color(theme::muted())
                        .hover(|s| s.bg(theme::hover()).text_color(theme::text()))
                })
                .child(monogram(&mark, hue, 18.))
                .child(hover::cut(("tab-title", ix), tab.title.clone()).flex_1())
                .children((!tab.active && tab.unread > 0).then(|| {
                    div()
                        .flex_none()
                        .min_w(px(16.))
                        .h(px(16.))
                        .px(px(4.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .bg(theme::accent())
                        .text_size(px(10.))
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(theme::on_accent())
                        .child(tab.unread.to_string())
                }))
                .children(tab.news.then(|| {
                    div()
                        .flex_none()
                        .size(px(6.))
                        .rounded_full()
                        .bg(theme::accent())
                }))
                .child(
                    div()
                        .id(("tab-close", ix))
                        .flex_none()
                        .w(px(18.))
                        .h(px(18.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(4.))
                        .text_size(px(12.))
                        .text_color(theme::faint())
                        .when(!tab.active, |s| {
                            s.invisible().group_hover(group, |s| s.visible())
                        })
                        .hover(|s| s.bg(theme::hover()).text_color(theme::text()))
                        .child("×")
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(move |_: &ClickEvent, window, cx| close.close(ix, window, cx)),
                )
                // Dragged onto another tab, it takes that place, as on the rail.
                .drag_over::<DragProject>(|s, _, _, _| s.bg(theme::selected()))
                .on_drag(DragProject(ix), {
                    let title = tab.title.clone();
                    move |_, _, _, cx| cx.new(|_| DragLabel(title.clone()))
                })
                .on_drop(move |dragged: &DragProject, window, cx| {
                    moved.moved(dragged.0, ix, window, cx)
                })
                .on_click(move |_: &ClickEvent, window, cx| select.select(ix, window, cx))
        }))
        .child(
            div()
                .id("tab-new")
                .flex_none()
                .w(px(26.))
                .h(px(26.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(ROW_RADIUS))
                .text_color(theme::muted())
                .cursor_pointer()
                .hover(|s| s.bg(theme::hover()).text_color(theme::text()))
                .tooltip_show_delay(hover::LONG)
                .tooltip(|_, cx| cx.new(|_| Tip("Open a repository  ⌘T")).into())
                .child("+")
                .on_click(move |_: &ClickEvent, window, cx| host.add(window, cx)),
        )
        .into_any_element()
}

/// Project `root`'s colour: the hue it was given (the developer, 2026-10-23), else one of its own
/// by its folder's name, so naming it does not change it.
pub(crate) fn project_hue(root: Option<&std::path::Path>, name: &str) -> Rgba {
    let folder = root
        .and_then(|r| r.file_name())
        .map(|n| n.to_string_lossy().into_owned());
    match root.and_then(|r| artoo_core::labels::project(r).hue) {
        Some(degrees) => hue_at(degrees),
        None => hue(folder.as_deref().unwrap_or(name)),
    }
}
