//! The page bar (#71) drawn: one 44 px line over every page — the path ending in the item's id, each part above it a
//! way up; "‹ Inbox" while a page reached from another space is on screen; the page's view switch; its actions. A
//! request's bar carries the public orange along its top. What the path is and what a click does are the tab's
//! (`artoo_shell_api::bar`, its contract).

use artoo_shell_api::MenuItem;
use artoo_shell_api::bar::{BarAction, BarHost, BarRenderer, BarRoom, PageBar};
use artoo_ui_kit::hover::{self, Tip};
use artoo_ui_kit::{ISLAND_RADIUS, ROW_RADIUS, flat, icons, island, px, theme, words};
use gpui::{
    AnyElement, ClickEvent, MouseButton, MouseDownEvent, SharedString, canvas, div, prelude::*,
};
use std::rc::Rc;

/// The room kept for the bar's left — the path cut to it — before an action folds; less when the left is shorter
/// whole, `‹ !398` (#186).
const LEFT: f32 = 140.;
/// `⋯`'s width, and the gap between actions.
const MORE: f32 = 28.;
const ACTIONS_GAP: f32 = 8.;

/// Whether `actions` are drawn short, and which fold into `⋯`, by the widths the bar was last drawn at: labels
/// shorten first (#187), then they fold by their `order`, the last first among the same (#186, #188), until what
/// stays fits beside the left and the view. The first `lead` are drawn after the title. Nothing shortens or folds
/// before the bar was measured.
fn folded(actions: &[BarAction], lead: usize, room: &BarRoom) -> (bool, Vec<bool>) {
    let mut folded = vec![false; actions.len()];
    let width = room.width.get();
    if width <= 0. {
        return (false, folded);
    }
    let (widths, shorts) = (room.actions.borrow(), room.shorts.borrow());
    let unknown = f32::from(px(100.));
    let left = f32::from(px(LEFT));
    let left = room.left_whole.get().map_or(left, |whole| whole.min(left));
    // The island's padding (14 + 8) and its gaps around the left, the view, the spacer and the actions.
    let room_left = width - f32::from(px(22. + 12. * 3.)) - left - room.view.get();
    // A short form not measured yet is taken as wide as the whole.
    let wide = |a: &BarAction, short: bool| {
        let whole = widths.get(a.id).copied().unwrap_or(unknown);
        match (short, &a.short) {
            (true, Some(_)) => shorts.get(a.id).copied().unwrap_or(whole),
            _ => whole,
        }
    };
    let need = |short: bool, folded: &[bool]| {
        let kept = |range: std::ops::Range<usize>| -> Vec<f32> {
            range
                .filter(|&ix| !folded[ix])
                .map(|ix| wide(&actions[ix], short))
                .collect()
        };
        let sum = |kept: &[f32]| {
            kept.iter().sum::<f32>()
                + f32::from(px(ACTIONS_GAP)) * kept.len().saturating_sub(1) as f32
        };
        let (on_left, on_right) = (kept(0..lead), kept(lead..actions.len()));
        let more = if folded.contains(&true) {
            f32::from(px(MORE + ACTIONS_GAP))
        } else {
            0.
        };
        // The lead's group stands apart from the left by the island's gap.
        let gap = if on_left.is_empty() { 0. } else { 12. };
        sum(&on_left) + gap + sum(&on_right) + more
    };
    if need(false, &folded) <= room_left {
        return (false, folded);
    }
    for at in fold_sequence(actions) {
        if need(true, &folded) <= room_left {
            break;
        }
        folded[at] = true;
    }
    (true, folded)
}

/// The actions that can fold, in the order they do: by their `order`, the last first among the same.
fn fold_sequence(actions: &[BarAction]) -> Vec<usize> {
    let mut sequence: Vec<usize> = (0..actions.len())
        .filter(|&ix| actions[ix].fold.is_some())
        .collect();
    sequence.sort_by_key(|&ix| (actions[ix].order, std::cmp::Reverse(ix)));
    sequence
}

/// A canvas over its parent that keeps the parent's width in `keep`, which says whether it changed: the bar is drawn
/// again on the next frame then (a refresh while drawing is dropped), as what folds was decided by the width before.
/// It draws nothing and takes no pointer.
fn measure(keep: impl Fn(f32) -> bool + 'static) -> impl IntoElement {
    canvas(
        move |bounds, window, _| {
            if keep(f32::from(bounds.size.width)) {
                window.request_animation_frame();
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

/// A crumb's words, to measure.
struct Sized {
    text: String,
    mono: bool,
    /// A note's dot follows its words.
    dot: bool,
}

/// A canvas over the left that sets how many of the `foldable` crumbs fold into `…`: the fewest with which the path,
/// the title and the count fit the room the left may take (`BarRoom::avail`, as last drawn). The bar is drawn again
/// on the next frame when it changed. It draws nothing and takes no pointer.
fn fit(
    room: Rc<BarRoom>,
    crumbs: Vec<Sized>,
    foldable: Vec<usize>,
    title: String,
    count: Option<String>,
    up: bool,
) -> impl IntoElement {
    canvas(
        move |_, window, _| {
            let avail = room.avail.get();
            if foldable.is_empty() || avail <= 0. {
                return;
            }
            let style = window.text_style();
            let size = style.font_size.to_pixels(window.rem_size());
            let width = |text: &str, mono: bool, bold: bool, size: gpui::Pixels| -> f32 {
                let line = SharedString::from(text.to_owned());
                let mut run = style.to_run(line.len());
                if mono {
                    run.font.family = theme::code_font();
                }
                if bold {
                    run.font.weight = gpui::FontWeight::SEMIBOLD;
                }
                f32::from(
                    window
                        .text_system()
                        .shape_line(line, size, &[run], None)
                        .width,
                )
            };
            // Each crumb: its words, a note's dot, the gap, `›`, and the left's gap after it.
            let sep = width("›", false, false, size) + 12.;
            let own = |c: &Sized| {
                width(&c.text, c.mono, false, size) + if c.dot { 10. } else { 0. } + sep
            };
            let mut fixed = width(&title, false, true, size);
            if up {
                fixed += 8. + width("‹", false, false, size) + 6.;
            }
            if let Some(count) = &count {
                fixed += 10. + width(count, false, false, gpui::px(12.));
            }
            let dots = width("…", false, false, size) + sep;
            let all: f32 = crumbs.iter().map(own).sum();
            let mut need = fixed + all;
            let mut k = 0;
            while need > avail - 4. && k < foldable.len() {
                need -= own(&crumbs[foldable[k]]);
                if k == 0 {
                    need += dots;
                }
                k += 1;
            }
            if room.folded.replace(k) != k {
                window.request_animation_frame();
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

/// The bar's renderer, installed by `install`.
pub(crate) struct Renderer;

impl BarRenderer for Renderer {
    fn render(&self, bar: PageBar, host: Rc<dyn BarHost>) -> AnyElement {
        page_bar(bar, host)
    }
}

/// The bar.
fn page_bar(bar: PageBar, host: Rc<dyn BarHost>) -> AnyElement {
    // The folders between the repository and the last folder fold into one `…` as the bar narrows, before any name is
    // cut (#16); `fit` decides how many from the widths.
    let last_folder = bar.crumbs.iter().rposition(|c| c.folder);
    let foldable: Vec<usize> = (0..bar.crumbs.len())
        .filter(|&n| bar.crumbs[n].folder && Some(n) != last_folder)
        .collect();
    let gone = bar.room.folded.get().min(foldable.len());
    let gone: Vec<usize> = foldable[..gone].to_vec();
    let sized: Vec<Sized> = bar
        .crumbs
        .iter()
        .map(|c| Sized {
            text: words::say(&c.label).into_owned(),
            mono: c.mono,
            dot: c.noted,
        })
        .collect();
    let fit_room = bar.room.clone();
    let fit = fit(
        fit_room,
        sized,
        foldable,
        bar.title_text.clone(),
        bar.count.clone(),
        bar.up,
    );
    // A click on `…` lists the folded folders, each opening as its crumb does (#225).
    let fold_items: Vec<MenuItem> = gone
        .iter()
        .map(|&n| {
            let host = host.clone();
            MenuItem::new(
                words::say(&bar.crumbs[n].label).into_owned(),
                move |w, cx| host.folder(n, w, cx),
            )
        })
        .collect();
    let dots = (!gone.is_empty()).then(|| {
        let whole = gone
            .iter()
            .map(|&n| words::say(&bar.crumbs[n].label).into_owned())
            .collect::<Vec<_>>()
            .join("/");
        let at = gone[0];
        (at, whole)
    });
    let crumbs = bar.crumbs.into_iter().enumerate().filter_map(|(n, crumb)| {
        if let Some((_, whole)) = dots.as_ref().filter(|(at, _)| *at == n) {
            return Some(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(
                        div()
                            .id("crumb-fold")
                            .child("…")
                            .cursor_pointer()
                            .hover(|s| s.text_color(theme::accent()))
                            .on_click({
                                let (host, items) = (host.clone(), fold_items.clone());
                                move |e: &ClickEvent, window, cx| {
                                    host.menu(e.position(), vec![items.clone()], window, cx)
                                }
                            })
                            .tooltip_show_delay(hover::LONG)
                            .tooltip({
                                let whole = SharedString::from(whole.clone());
                                move |_, cx| cx.new(|_| hover::TipOwned(whole.clone())).into()
                            }),
                    )
                    .child(div().flex_none().text_color(theme::faint()).child("›")),
            );
        }
        if gone.contains(&n) {
            return None;
        }
        let (link, folder, menu) = (host.clone(), host.clone(), host.clone());
        // A crumb too long for the bar is cut, a branch or a path in its middle, never clipped (#184).
        let label = words::say(&crumb.label).into_owned();
        let label = hover::name(SharedString::from(format!("crumb-label-{n}")), label);
        Some(
            div()
                .min_w_0()
                .flex()
                .items_center()
                .gap(px(6.))
                .child(
                    div()
                        .id(SharedString::from(format!("crumb-{n}")))
                        .relative()
                        .min_w_0()
                        .flex()
                        .items_center()
                        .gap(px(4.))
                        .when(crumb.mono, |s| s.font_family(theme::code_font()))
                        .when(crumb.open, |s| s.text_color(theme::accent()))
                        .child(label)
                        .when(crumb.noted, |s| {
                            s.child(div().size(px(6.)).rounded_full().bg(theme::agent()))
                        })
                        .when(crumb.link && !crumb.folder, |s| {
                            s.cursor_pointer()
                                .hover(|s| s.text_color(theme::accent()))
                                .on_click(move |_: &ClickEvent, window, cx| {
                                    link.up_to(n, window, cx)
                                })
                        })
                        .when(crumb.folder, |s| {
                            s.cursor_pointer()
                                .hover(|s| s.text_color(theme::accent()))
                                .on_click(move |_: &ClickEvent, window, cx| {
                                    folder.folder(n, window, cx)
                                })
                                .on_mouse_down(
                                    MouseButton::Right,
                                    move |e: &MouseDownEvent, window, cx| {
                                        menu.folder_menu(n, e.position, window, cx)
                                    },
                                )
                        })
                        .children(crumb.list),
                )
                .child(div().flex_none().text_color(theme::faint()).child("›")),
        )
    });
    let (back, up, more) = (host.clone(), host.clone(), host.clone());
    // What the bar has no room for folds into ⋯, the last first, never clipped (#186).
    let room = bar.room.clone();
    if bar.view.is_none() {
        room.view.set(0.);
    }
    let lead = bar.lead.len();
    let actions: Vec<BarAction> = bar.lead.into_iter().chain(bar.actions).collect();
    let (short, folds) = folded(&actions, lead, &room);
    // `⋯` lists what folded in the order it folds.
    let sequence = fold_sequence(&actions);
    let (mut items, mut shown, mut leading, mut folded) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for (ix, (action, fold)) in actions.into_iter().zip(folds).enumerate() {
        let id = action.id;
        let shown = if ix < lead { &mut leading } else { &mut shown };
        let drawn = |element: AnyElement, short: bool| {
            let keep = room.clone();
            div()
                .relative()
                .flex_none()
                .child(element)
                .child(measure(move |w| {
                    let widths = if short { &keep.shorts } else { &keep.actions };
                    widths.borrow_mut().insert(id, w) != Some(w)
                }))
        };
        let whole = drawn(action.element, false);
        let narrow = action.short.map(|element| drawn(element, true));
        match (fold, action.fold) {
            // Still measured, out of sight, so it comes back when its width fits — narrow, an icon.
            (true, Some(item)) => {
                items.push((sequence.iter().position(|&at| at == ix), item));
                folded.push(whole);
                folded.extend(narrow);
            }
            // The form not shown measured out of sight too: the whole comes back when it fits.
            (_, _) => match narrow {
                Some(narrow) if short => {
                    shown.push(narrow);
                    folded.push(whole);
                }
                Some(narrow) => {
                    shown.push(whole);
                    folded.push(narrow);
                }
                None => shown.push(whole),
            },
        }
    }
    items.sort_by_key(|(at, _)| *at);
    let items: Vec<_> = items.into_iter().map(|(_, item)| item).collect();
    let more_button = (!items.is_empty() || bar.more_shown).then(|| {
        let groups: Vec<_> = [items, bar.more]
            .into_iter()
            .filter(|g| !g.is_empty())
            .collect();
        div()
            .id("bar-more")
            .flex_none()
            .size(px(MORE))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(ROW_RADIUS))
            .border_1()
            .border_color(theme::island_border())
            .cursor_pointer()
            .hover(|s| s.bg(theme::hover()))
            // As the bar's other icon buttons (`bar_icon`): an svg takes no colour from its parent.
            .child(
                icons::icon("ellipsis")
                    .size(px(16.))
                    .text_color(theme::muted()),
            )
            .tooltip_show_delay(hover::LONG)
            .tooltip(|_, cx| cx.new(|_| Tip("More actions")).into())
            .on_click(move |e: &ClickEvent, window, cx| {
                more.menu(e.position(), groups.clone(), window, cx)
            })
    });
    let (whole, view, left, spare) = (room.clone(), room.clone(), room.clone(), room.clone());
    // clip: the bar's edge; its crumbs cut with an ellipsis and its actions fold into ⋯ (#186), so nothing reaches it.
    let island = island()
        .relative()
        .overflow_hidden()
        .flex_none()
        .flex_row()
        .h(px(44.))
        .items_center()
        .gap(px(12.))
        .pl(px(14.))
        .pr(px(8.))
        .whitespace_nowrap()
        .children(bar.from.map(|from| {
            div()
                .id("bar-from")
                .flex_none()
                .px(px(8.))
                .rounded_full()
                .bg(theme::hover())
                .text_size(px(11.))
                .text_color(theme::muted())
                .cursor_pointer()
                .hover(|s| s.text_color(theme::text()))
                .child(format!("‹ {}", words::say(from)))
                .tooltip_show_delay(hover::LONG)
                .tooltip(|_, cx| cx.new(|_| Tip("Back where you came from  ⌘[")).into())
                .on_click(move |_: &ClickEvent, window, cx| back.back(window, cx))
        }))
        .child(
            div()
                .relative()
                .min_w_0()
                .flex()
                .items_center()
                .gap(px(6.))
                .overflow_hidden()
                .text_color(theme::muted())
                .child(measure(move |w| {
                    left.left.set(w);
                    false
                }))
                .child(fit)
                .children(bar.up.then(|| {
                    div()
                        .id("crumb-up")
                        .flex_none()
                        .px(px(4.))
                        .rounded(px(ROW_RADIUS))
                        .cursor_pointer()
                        .hover(|s| s.bg(theme::hover()).text_color(theme::text()))
                        .child("‹")
                        .tooltip_show_delay(hover::LONG)
                        .tooltip(|_, cx| cx.new(|_| Tip("Up a level  esc")).into())
                        .on_click(move |_: &ClickEvent, window, cx| up.up(window, cx))
                }))
                .children(crumbs)
                .child(bar.title)
                .children(bar.count.map(|n| {
                    div()
                        .flex_none()
                        .ml(px(4.))
                        .text_size(px(12.))
                        .text_color(theme::faint())
                        .child(n)
                })),
        )
        .when(!leading.is_empty(), |s| {
            s.child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(ACTIONS_GAP))
                    .children(leading),
            )
        })
        .children(bar.view.map(|v| {
            div()
                .relative()
                .flex_none()
                .child(v)
                .child(measure(move |w| view.view.replace(w) != w))
        }))
        // Room to spare: the left is whole, as wide as it was just drawn (its measure comes first).
        .child(div().relative().flex_1().min_w_0().child(measure(move |w| {
            spare.avail.set(spare.left.get() + w);
            let whole = Some(spare.left.get());
            w > 0. && spare.left_whole.replace(whole) != whole
        })))
        .child(
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap(px(ACTIONS_GAP))
                .children(shown)
                .children(more_button),
        )
        .child(measure(move |w| whole.width.replace(w) != w))
        // The folded actions and the forms not shown, measured: nothing shows of them and they take no pointer,
        // clipped to nothing; apart by their own id, the same action's two forms never meet.
        .child(
            div()
                .id("bar-measured")
                .absolute()
                .top_0()
                .left_0()
                .size_0()
                .overflow_hidden()
                .flex()
                .children(folded),
        )
        // Public: what is done here is seen on GitLab (#71).
        // Over the island's own border, its top edge turning down with its round corners (a
        // strip would cross them square).
        .when(bar.public, |s| {
            s.child(
                div()
                    .absolute()
                    .top(px(-1.))
                    .left(px(-1.))
                    .right(px(-1.))
                    .bottom(px(-1.))
                    .rounded(px(if flat() { 0. } else { ISLAND_RADIUS }))
                    .border_t_2()
                    .border_color(theme::warning()),
            )
        });
    island.into_any_element()
}
