//! The Inbox drawn (#49, Design L4): its filters — what happened, and Source ▾ (#197) —, the rows loading before the
//! first read or one message when nothing is listed, a notice's row and the notice selected previewed — its request,
//! the code its discussion is on, the whole discussion, the reply. What the tab keeps comes in a look; a click asks
//! it an `InboxEvent`.

use crate::inbox::{Counts, InboxFilter, InboxView, SourcePick, SourceRow, picked_sources};
use crate::notices::Notice;
use crate::waiting::{FADED, Waiting};
use artoo_core::markdown::plain;
use artoo_hosts_api::{Mr, Thread};
use artoo_shell_api::notices::Why;
use artoo_ui_kit::controls::{self, bar_button, chip, chip_lead};
use artoo_ui_kit::discussion::{self, Kind as Button, Source};
use artoo_ui_kit::excerpt::excerpt_view;
use artoo_ui_kit::hover::{self, CutInTip, Hover, RowTip, Tip, WasCut};
use artoo_ui_kit::keys::{self, On};
use artoo_ui_kit::markdown::{self, Selecting};
use artoo_ui_kit::marked::{self, Marked};
use artoo_ui_kit::motion;
use artoo_ui_kit::room::Heights;
use artoo_ui_kit::{ROW_RADIUS, format, fresh, gap, icons, island, monogram, px, theme, words};
use gpui::{
    Animation, AnimationExt, AnyElement, App, Bounds, ClickEvent, Div, Entity, FontWeight,
    MouseButton, MouseDownEvent, Pixels, SharedString, Stateful, canvas, div, prelude::*,
};
use std::cell::Cell;
use std::rc::Rc;

pub use artoo_requests_api::{InboxEvent, OnInbox, RowLook};

/// A filter's name with what is new under it: `Failures 3`, `Mentions` when nothing is.
fn counted(name: &str, new: usize) -> String {
    if new > 0 {
        format!("{name} {new}")
    } else {
        name.to_owned()
    }
}

/// The Inbox's filters in its page bar (#71, #197): what happened, each with what is new under it — in a narrow
/// window one menu —, then `Source: All ▾`. `at` keeps where Source ▾ was drawn, for its list to open under it.
pub fn filters(
    view: &Entity<InboxView>,
    at: Rc<Cell<Option<Bounds<Pixels>>>>,
    shown: InboxFilter,
    counts: &Counts,
    sources: &[SourceRow],
    // `reading`: the first read, when each count is a tint (round 41).
    (compact, all, reading): (bool, bool, bool),
    on: &OnInbox,
) -> Div {
    let whys: Vec<(Option<Why>, &'static str, usize)> = std::iter::once((None, "All", counts.all))
        .chain(Why::ALL.iter().map(|w| (Some(*w), w.name(), counts.of(*w))))
        .collect();
    let what = if compact {
        let (name, new) = (whys.iter())
            .find(|(why, ..)| *why == shown.why)
            .map_or(("All", 0), |(_, name, new)| (*name, *new));
        let (view, on) = (view.clone(), on.clone());
        chip("inbox-why", counted(name, new), shown.why.is_some())
            .flex()
            .items_center()
            .gap(px(4.))
            .child(
                icons::icon("chevron")
                    .size(px(12.))
                    .text_color(theme::muted()),
            )
            .on_click(move |e: &ClickEvent, _, cx| {
                let items = (whys.iter())
                    .map(|&(why, name, new)| {
                        let on = on.clone();
                        artoo_shell_api::MenuItem::new(counted(name, new), move |_, cx| {
                            on(InboxEvent::Filter(InboxFilter { why, ..shown }), cx)
                        })
                        .checked(why == shown.why)
                    })
                    .collect();
                if let Some(frame) = view.read(cx).frame.as_ref().and_then(|f| f.upgrade()) {
                    frame.menu(e.position(), vec![items], cx);
                }
            })
            .into_any_element()
    } else {
        controls::group()
            .children(whys.into_iter().map(|(why, name, new)| {
                let on = on.clone();
                let id = match why {
                    None => "inbox-all",
                    Some(Why::Mentions) => "inbox-mentions",
                    Some(Why::Reviews) => "inbox-reviews",
                    Some(Why::Asks) => "inbox-asks",
                    Some(Why::Failures) => "inbox-failures",
                };
                chip(id, counted(name, new), shown.why == why)
                    .when(reading, |s| {
                        s.flex()
                            .items_center()
                            .gap(px(5.))
                            .child(fresh::tint(px(10.).into(), 10.))
                    })
                    .tooltip_show_delay(artoo_ui_kit::hover::LONG)
                    .tooltip(|_, cx| cx.new(|_| Tip("Next tab ⌃⇥ · back ⌃⇧⇥")).into())
                    .on_click(move |_: &ClickEvent, _, cx| {
                        on(InboxEvent::Filter(InboxFilter { why, ..shown }), cx)
                    })
            }))
            .into_any_element()
    };
    let (name, glyph) = (sources.iter())
        .find(|s| Some(s.id) == shown.source)
        .map_or(("All", None), |s| (s.name.as_str(), s.icon));
    let (view, layers) = (view.clone(), view.clone());
    // Its chevron an icon, not a glyph (the owner, 2026-11-26).
    // A source picked wears its icon before its name, a plugin's tab's (the owner, 2026-11-27).
    let source = chip_lead(
        "inbox-source",
        glyph,
        format!("Source: {name}"),
        shown.source.is_some(),
    )
    .flex()
    .items_center()
    .gap(px(4.))
    .child(
        icons::icon("chevron")
            .size(px(12.))
            .text_color(theme::muted()),
    )
    .relative()
    .child(
        canvas(move |bounds, _, _| at.set(Some(bounds)), |_, _, _, _| {})
            .absolute()
            .top_0()
            .left_0()
            .size_full(),
    )
    .on_click(move |_: &ClickEvent, _, cx| open_sources(&view, cx));
    div()
        .flex()
        .items_center()
        .gap_2()
        .child(what)
        .child(source)
        .child(all_projects(&layers, all))
}

/// The switch of the list's head: every project's rows, each naming its project, or this project's alone.
fn all_projects(view: &Entity<InboxView>, on: bool) -> Stateful<Div> {
    let view = view.clone();
    div()
        .id("inbox-all-projects")
        .flex_none()
        .size(px(28.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(ROW_RADIUS))
        .cursor_pointer()
        .hover(|s| s.bg(theme::hover()))
        .when(on, |s| s.bg(theme::selected()))
        .child(icons::icon("layers").size(px(16.)).text_color(if on {
            theme::text()
        } else {
            theme::muted()
        }))
        .tooltip_show_delay(artoo_ui_kit::hover::LONG)
        .tooltip(move |_, cx| {
            cx.new(|_| {
                Tip(if on {
                    "All projects: on"
                } else {
                    "All projects: off"
                })
            })
            .into()
        })
        .on_click(move |_: &ClickEvent, _, cx| {
            view.update(cx, |v, cx| {
                v.all = !v.all;
                cx.notify();
            })
        })
}

/// Source ▾ opened, its field with the keyboard; again, it closes.
fn open_sources(view: &Entity<InboxView>, cx: &mut App) {
    let open = view.read(cx).sources.is_none();
    let frame = view.update(cx, |v, cx| {
        v.sources = open.then(SourcePick::default);
        cx.notify();
        v.frame.as_ref().and_then(|f| f.upgrade())
    });
    if let Some(frame) = frame {
        if open {
            frame.take_keyboard(crate::inbox::SOURCES, cx);
        } else {
            frame.leave_keyboard(crate::inbox::SOURCES, cx);
        }
    }
}

/// Source ▾ closed: a press outside it, a source picked.
pub fn close_sources(view: &Entity<InboxView>, cx: &mut App) {
    let frame = view.update(cx, |v, cx| {
        v.sources = None;
        cx.notify();
        v.frame.as_ref().and_then(|f| f.upgrade())
    });
    if let Some(frame) = frame {
        frame.leave_keyboard(crate::inbox::SOURCES, cx);
    }
}

/// Source ▾ open (#197): its field, then `All sources` with what is new in all, the sources with something new, the
/// rest dimmed under `Nothing new`; the source chosen checked, the row the keys point at lit. A click picks one as
/// ↵ does.
pub fn source_picker(
    view: &Entity<InboxView>,
    pick: &SourcePick,
    sources: &[SourceRow],
    shown: InboxFilter,
    everywhere: usize,
    on: &OnInbox,
) -> AnyElement {
    let rows = picked_sources(sources, &pick.text);
    pick.rows.replace(rows.clone());
    let row = |n: usize, id: Option<&'static str>| {
        let (name, new, mark, icon) = match id {
            None => ("All sources".to_owned(), everywhere, String::new(), None),
            Some(id) => (sources.iter())
                .find(|s| s.id == id)
                .map_or((String::new(), 0, String::new(), None), |s| {
                    (s.name.clone(), s.new, s.mark.clone(), s.icon)
                }),
        };
        let (view, on) = (view.clone(), on.clone());
        div()
            .id(("inbox-source-row", n))
            .h(px(28.))
            .px(px(8.))
            .flex()
            .items_center()
            .gap_2()
            .rounded(px(ROW_RADIUS))
            .cursor_pointer()
            .when(n == pick.at, |s| s.bg(theme::hover()))
            .hover(|s| s.bg(theme::hover()))
            // The source chosen is lit, not checked: no column of marks before the rows (the design).
            .when(shown.source == id, |s| s.bg(theme::selected()))
            .when(id.is_some() && new == 0, |s| s.text_color(theme::muted()))
            .child(match id {
                Some(_) => lead(Face::Mark(mark, name.clone(), icon), None, 16.).into_any_element(),
                None => div()
                    .flex_none()
                    .size(px(16.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        icons::icon("inbox")
                            .size(px(14.))
                            .text_color(theme::muted()),
                    )
                    .into_any_element(),
            })
            .child(div().flex_1().min_w_0().cut_in_tip().child(name))
            .children((new > 0).then(|| {
                div()
                    .flex_none()
                    .text_size(px(12.))
                    .text_color(theme::muted())
                    .child(new.to_string())
            }))
            .on_click(move |_: &ClickEvent, _, cx| {
                close_sources(&view, cx);
                on(
                    InboxEvent::Filter(InboxFilter {
                        source: id,
                        ..shown
                    }),
                    cx,
                )
            })
            .into_any_element()
    };
    let heading = |text: &'static str| {
        div()
            .px(px(8.))
            .pt(px(6.))
            .pb(px(2.))
            .text_size(px(11.))
            .text_color(theme::faint())
            .child(text)
            .into_any_element()
    };
    let mut list: Vec<AnyElement> = Vec::new();
    let quiet = |id: &Option<&'static str>| {
        id.is_some_and(|id| sources.iter().any(|s| s.id == id && s.new == 0))
    };
    let mut said_quiet = false;
    for (n, id) in rows.iter().enumerate() {
        if quiet(id) && !said_quiet {
            said_quiet = true;
            list.push(heading("Nothing new"));
        }
        list.push(row(n, *id));
        if id.is_none() {
            list.push(
                div()
                    .mx(px(8.))
                    .my(px(4.))
                    .h(px(1.))
                    .bg(theme::island_border())
                    .into_any_element(),
            );
        }
    }
    if rows.is_empty() {
        list.push(heading("No source matches"));
    }
    let field = pick.area.field_of(
        &pick.text,
        true,
        "Filter sources",
        false,
        |v: &mut InboxView| v.sources.as_mut().map(|p| &mut p.area),
        view.downgrade(),
    );
    div()
        .w(px(260.))
        .p(px(6.))
        .rounded(px(10.))
        .border_1()
        .border_color(theme::island_border())
        .bg(theme::panel())
        .shadow_lg()
        .flex()
        .flex_col()
        .gap(px(2.))
        .text_size(px(13.))
        .child(
            div()
                .h(px(30.))
                .mb(px(4.))
                .px(px(8.))
                .flex()
                .items_center()
                .gap_2()
                .rounded(px(ROW_RADIUS))
                .border_2()
                .border_color(theme::focus())
                .bg(theme::base())
                .cursor_text()
                .child(
                    icons::icon("search")
                        .size(px(14.))
                        .text_color(theme::faint()),
                )
                .child(field),
        )
        .child(
            div()
                .id("inbox-sources")
                .max_h(px(360.))
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap(px(2.))
                .children(list),
        )
        .into_any_element()
}

/// The first read's tints (round 41), in a list `width` wide: the real rows' and preview's boxes, each text a still
/// tint of about its width.
pub fn loading(width: f32) -> AnyElement {
    let rows: [(f32, f32); 7] = [
        (214., 160.),
        (176., 196.),
        (232., 128.),
        (150., 182.),
        (196., 144.),
        (168., 172.),
        (204., 120.),
    ];
    div()
        .size_full()
        .flex()
        .gap(px(gap()))
        .child(
            island()
                .w(px(width))
                .flex_none()
                .child(head_box().child(tint(34., 8., 16.)))
                .children(rows.map(|(title, gist)| skeleton_row(title, gist))),
        )
        .child(
            island()
                .flex_1()
                .min_w_0()
                .bg(theme::editor())
                .child(skeleton_preview()),
        )
        .into_any_element()
}

/// A tint `width` by `height`, centred in a line `line` tall.
fn tint(width: f32, height: f32, line: f32) -> Div {
    div()
        .h(px(line))
        .flex_none()
        .flex()
        .items_center()
        .child(fresh::tint(px(width).into(), height))
}

/// A row as tints, in `draw_row`'s boxes: the dot's room, the face, the title and time, what was written.
fn skeleton_row(title: f32, gist: f32) -> Div {
    div()
        .mx(px(6.))
        .px(px(10.))
        .py(px(10.))
        .flex()
        .items_start()
        .gap(px(10.))
        .child(div().flex_none().size(px(7.)))
        .child(fresh::tint(px(20.).into(), 20.).flex_none().rounded_full())
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(
                    div()
                        .flex()
                        .justify_between()
                        .gap_2()
                        .child(tint(title, 8., 20.).flex_1())
                        .child(fresh::tint(px(14.).into(), 14.).mt(px(3.)).flex_none())
                        .child(tint(20., 8., 20.)),
                )
                .child(tint(gist, 8., 16.)),
        )
}

/// A notice's preview as tints: its head, title, the discussion and the answers.
fn skeleton_preview() -> Div {
    let button = |w: f32| fresh::tint(px(w).into(), 28.).rounded(px(ROW_RADIUS));
    div()
        .flex()
        .flex_col()
        .gap(px(14.))
        .px(px(28.))
        .py(px(22.))
        .max_w(px(820.))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(tint(28., 8., 18.))
                .child(fresh::tint(px(16.).into(), 16.).rounded_full())
                .child(tint(72., 8., 18.))
                .child(tint(20., 8., 18.)),
        )
        .child(tint(300., 12., 26.))
        .child(
            div()
                .flex()
                .flex_col()
                .child(tint(390., 8., 20.))
                .child(tint(340., 8., 20.))
                .child(tint(210., 8., 20.)),
        )
        .child(
            div()
                .flex()
                .gap(px(8.))
                .child(button(104.))
                .child(button(64.)),
        )
}

/// Nothing listed because `what` could not be read (#185).
pub fn failed(what: &str, why: String) -> AnyElement {
    island()
        .flex_1()
        .bg(theme::editor())
        .child(controls::empty_state(
            "inbox",
            words::say(what).into_owned(),
            why,
            None,
        ))
        .into_any_element()
}

/// Nothing listed under `filter`, a state word (#185); `others`: something is under another, and Show all goes there.
pub fn empty(
    filter: InboxFilter,
    source: Option<&str>,
    query: &str,
    others: bool,
    on: &OnInbox,
) -> AnyElement {
    let query = query.trim();
    let filtered = filter != InboxFilter::default() || !query.is_empty();
    let title = if !query.is_empty() {
        format!("Nothing matches “{query}”")
    } else {
        let what = match filter.why {
            None if filter.source.is_none() => "Nothing needs you",
            None => "Nothing new",
            Some(Why::Mentions) => "No mentions",
            Some(Why::Reviews) => "No review requests",
            Some(Why::Asks) => "Nothing asks you",
            Some(Why::Failures) => "No failures",
        };
        match source {
            Some(source) => format!("{what} from {source}"),
            None => what.to_owned(),
        }
    };
    // Where Filter looks, the one line that changes what you do next (#185).
    let reach = if query.is_empty() {
        String::new()
    } else {
        "Looks in what happened, what it is about and what was written.".to_owned()
    };
    let all = (filtered && others).then(|| {
        let on = on.clone();
        div()
            .id("inbox-show-all")
            .child(controls::outline_button(if query.is_empty() {
                "Show all"
            } else {
                "Clear the filter"
            }))
            .on_click(move |_: &ClickEvent, _, cx| on(InboxEvent::ShowAll, cx))
            .into_any_element()
    });
    island()
        .flex_1()
        .bg(theme::editor())
        // Its button's room kept when there is none, so switching the filter moves nothing.
        .child(controls::empty_state(
            "inbox",
            title,
            reach,
            Some(all.unwrap_or_else(|| div().h(px(28.)).into_any_element())),
        ))
        .into_any_element()
}

/// A row's face (Design round 12, I1): a person's, a glyph of what happened, or its source's mark, with a badge at
/// its corner while the kind has one.
pub enum Face {
    /// A person by name, as the discussions draw them.
    Person(String),
    /// What happened, a glyph in a colour: `circle-x` for a pipeline that failed.
    Glyph(&'static str, gpui::Rgba),
    /// The source's mark and its name, which tints it; its icon in the letters' place when it has one (a plugin's
    /// tab's).
    Mark(String, String, Option<&'static str>),
}

/// Source marks for the requests' host: `GitLab` is `GL`, `GitHub` `GH`.
pub fn source_mark(name: &str, mark: Option<&str>) -> String {
    match (mark, name) {
        (Some(mark), _) => mark.to_owned(),
        (None, "GitLab") => "GL".to_owned(),
        (None, "GitHub") => "GH".to_owned(),
        (None, name) => {
            // One letter for one word (`Jira` is `J`), the first of each of two (`Merge requests`).
            let letters = monogram::initials(name);
            letters
                .chars()
                .take(if name.contains([' ', '-', '_']) { 2 } else { 1 })
                .collect()
        }
    }
}

/// The face and its badge at the start of a row.
pub fn lead(face: Face, badge: Option<&'static str>, size: f32) -> Div {
    let size_px = size;
    let body = match face {
        Face::Person(name) => discussion::avatar(&name, false, size_px),
        Face::Glyph(name, color) => div()
            .size(px(size_px))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .bg(monogram::tint(color))
            .child(icons::icon(name).size(px(12.)).text_color(color)),
        Face::Mark(_, name, Some(icon)) => {
            let hue = monogram::hue(&name);
            div()
                .size(px(size_px))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .bg(monogram::tint(hue))
                .child(icons::icon(icon).size(px(size_px * 0.6)).text_color(hue))
        }
        Face::Mark(mark, name, None) => {
            let hue = monogram::hue(&name);
            div()
                .size(px(size_px))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .bg(monogram::tint(hue))
                .text_color(hue)
                .text_size(px(if mark.chars().count() > 1 { 8. } else { 10. }))
                .font_weight(FontWeight::BOLD)
                .child(mark)
        }
    };
    div()
        .relative()
        .flex_none()
        .size(px(size_px))
        .child(body)
        .children(badge.map(|icon| {
            div()
                .absolute()
                .right(px(-4.))
                .bottom(px(-4.))
                .size(px(12.))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .bg(theme::base())
                .child(icons::icon(icon).size(px(9.)).text_color(theme::muted()))
        }))
}

/// A group's head in the list: `New 7`, the rows not read, and `Read` (round 16). A row read stays in its group
/// until the Inbox is left.
pub fn group_head(new: bool, unread: usize) -> AnyElement {
    let name = match new {
        true => counted("New", unread),
        false => "Read".to_owned(),
    };
    head_box().child(name).into_any_element()
}

fn head_box() -> Div {
    div()
        .flex_none()
        .px(px(16.))
        .pt(px(12.))
        .pb(px(6.))
        .text_size(px(11.))
        .line_height(px(16.))
        .text_color(theme::faint())
}

/// What a row says besides its title: the tooltip (#202) names what happened, its source and exactly when, and,
/// while the list is narrow, what was written, which the row then leaves out.
fn tip_words(says: &str, source: &str, at: i64, text: &str, wide: bool) -> String {
    let mut tip = vec![
        says.to_owned(),
        format!("{source} · {}", hover::full_date(at)),
    ];
    if !wide && !text.is_empty() {
        tip.push(text.to_owned());
    }
    tip.join("\n")
}

/// One row of the Inbox, two lines: its face at the start, the title and the short time at the end, what was written
/// under it; unread is a dot and a heavier title, read the same row with neither, in its place (Design round 10, U4).
struct RowDraw<'a> {
    id: gpui::ElementId,
    /// The notice's key: what its motion to read is known by.
    key: &'a str,
    lead: Div,
    /// Its source's mark, name and icon (`GL`, `GitLab`): the small mark before the time, the icon in its letters'
    /// place when there is one.
    from: (String, &'a str, Option<&'static str>),
    /// What it is about; what happened when it is about nothing.
    title: String,
    at: i64,
    gist: &'a str,
    /// What the tooltip says: what happened, the source, the text.
    tip: String,
    /// The project it is of, when the Inbox lists every project's.
    project: &'a str,
    look: RowLook,
}

fn draw_row(row: RowDraw) -> Stateful<Div> {
    let RowDraw {
        id,
        key,
        lead,
        from: (mark, source, glyph),
        title,
        at,
        gist,
        tip,
        project,
        look,
    } = row;
    let from = monogram::source_badge(
        SharedString::from(format!("inbox-from-{key}")),
        &mark,
        glyph,
        monogram::source_hue(source),
        source,
    );
    let RowLook { fresh, on, wide } = look;
    let [title_cut, gist_cut, project_cut] = [(); 3].map(|_| WasCut::default());
    let mut cut: Vec<(Option<WasCut>, SharedString)> =
        vec![(Some(title_cut.clone()), title.clone().into())];
    // Every row of a wide list two lines tall, the second empty when nothing was written: one height for all (the
    // owner, 2026-11-26).
    let gist = wide.then(|| gist.to_owned());
    if let Some(gist) = gist.as_ref().filter(|g| !g.is_empty()) {
        cut.push((Some(gist_cut.clone()), gist.clone().into()));
    }
    cut.push((None, tip.into()));
    // The list's whole width, lit edge to edge as the design's rows are; its words where they stood.
    div()
        .id(id)
        .px(px(16.))
        .py(px(10.))
        .flex()
        .items_start()
        .gap(px(10.))
        .cursor_pointer()
        .when(on, |s| s.bg(theme::selected()))
        .hover(|s| s.bg(theme::hover()))
        // Read where it is: the dot fades and the title's weight goes regular (round 41).
        .child(motion::toward(
            div()
                .flex_none()
                .mt(px(6.5))
                .size(px(7.))
                .rounded_full()
                .bg(theme::accent()),
            SharedString::from(format!("inbox-dot-{key}")),
            fresh,
            false,
            motion::FAST,
            |dot, v| dot.opacity(v),
        ))
        .child(lead)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .line_height(px(20.))
                        .child(motion::toward(
                            hover::cut_in_row(title.clone(), title, &title_cut).flex_1(),
                            SharedString::from(format!("inbox-title-{key}")),
                            fresh,
                            false,
                            motion::FAST,
                            discussion::unread_title,
                        ))
                        .children((!project.is_empty()).then(|| {
                            hover::cut_in_row(project.to_owned(), project.to_owned(), &project_cut)
                                .flex_none()
                                .max_w(px(110.))
                                .text_size(px(11.))
                                .text_color(theme::muted())
                        }))
                        .child(from.mr(px(2.)))
                        .child(
                            format::time_cell(at)
                                .text_size(px(11.))
                                .text_color(theme::faint()),
                        ),
                )
                .children(gist.map(|gist| {
                    hover::cut_in_row(gist.clone(), gist, &gist_cut)
                        .h(px(16.))
                        .text_size(px(12.))
                        .line_height(px(16.))
                        .text_color(theme::muted())
                })),
        )
        .row_tip_cut(on, cut)
}

/// The face of notice `n`: who it came from, or what happened when nobody is named.
fn notice_face(n: &Notice) -> (Face, Option<&'static str>) {
    use crate::notices::Kind;
    let glyph = |icon, color| (Face::Glyph(icon, color), None);
    match n.kind {
        Kind::Failed => glyph("circle-x", theme::removed()),
        Kind::Unmergeable => glyph("merge", theme::warning()),
        Kind::Review => glyph("eye", theme::code(8).into()),
        kind => {
            let badge = match kind {
                Kind::Mention => "at-sign",
                Kind::Reply | Kind::OnYours => "message",
                Kind::Resolved | Kind::Approval => "circle-check",
                _ => "user",
            };
            if n.who.trim().is_empty() {
                glyph("user", theme::muted())
            } else {
                (Face::Person(n.who.clone()), Some(badge))
            }
        }
    }
}

/// Notice `ix`: its face, the request's number and title with the short time, the start of what was written under
/// it while the list is `wide`; the tooltip says what happened, from `source` (`GitLab`) and exactly when (#202).
pub fn notice_row(
    n: &Notice,
    ix: usize,
    source: &str,
    look: RowLook,
    on: &OnInbox,
) -> Stateful<Div> {
    let text = plain(&n.text);
    let gist = text
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();
    let (face, badge) = notice_face(n);
    let tip = tip_words(&n.says(), source, n.created, text.trim(), look.wide);
    let iid = n.iid;
    let (menu, pick) = (on.clone(), on.clone());
    draw_row(RowDraw {
        id: ("inbox-row", ix).into(),
        key: &n.key,
        lead: lead(face, badge, 20.),
        from: (source_mark(source, None), source, None),
        title: format!("{} {}", words::number(n.iid), n.title),
        at: n.created,
        gist,
        tip,
        project: "",
        look,
    })
    .link_row(format!("inbox-row-{ix}"), marked::request(iid))
    .on_mouse_down(MouseButton::Right, move |e: &MouseDownEvent, _, cx| {
        menu(
            InboxEvent::Menu {
                iid,
                at: e.position,
            },
            cx,
        )
    })
    .on_click(move |e: &ClickEvent, _, cx| {
        // ⌘ (Ctrl elsewhere): the request in the browser.
        if e.modifiers().secondary() {
            pick(InboxEvent::OnWeb(iid), cx)
        } else {
            pick(InboxEvent::Pick(ix), cx)
        }
    })
}

/// Notice `n` just done, where it was: its content fades, then the gap closes (round 41). It answers nothing.
pub fn gone_row(n: &Notice, source: &str, wide: bool, heights: &Heights) -> AnyElement {
    let look = RowLook {
        fresh: false,
        on: false,
        wide,
    };
    let still: OnInbox = Rc::new(|_, _| {});
    let row = notice_row(n, 0, source, look, &still);
    let height = format!("inbox-gone-{}", n.key);
    let heights = heights.clone();
    let total = motion::DONE_FADE + motion::DONE_CLOSE;
    div()
        // Its own ids: the row now at its place has the same ones.
        .id(SharedString::from(height.clone()))
        .flex_none()
        .overflow_hidden()
        .child(
            div()
                .relative()
                .flex_none()
                .child(row)
                .child(heights.measure(height.clone())),
        )
        .with_animation(
            SharedString::from(format!("{height}-going")),
            Animation::new(total),
            move |gone, t| {
                let at = total.mul_f32(t);
                let fade = at.div_duration_f32(motion::DONE_FADE).min(1.);
                let close = (at.saturating_sub(motion::DONE_FADE))
                    .div_duration_f32(motion::DONE_CLOSE)
                    .min(1.);
                let tall = heights.get(&height);
                gone.opacity(1. - motion::ease(fade)).when(close > 0., |g| {
                    g.max_h(gpui::px(tall * (1. - motion::ease(close))))
                })
            },
        )
        .into_any_element()
}

/// A part's row (a plugin's, #196), drawn as a notice's is: a person's face, or its source's mark, with the badge its
/// kind or it names; what it is about, or what happened when it is about nothing.
pub fn part_row(
    r: &artoo_shell_api::notices::Listed,
    source: &str,
    (mark, icon): (&str, Option<&'static str>),
    badge: Option<&'static str>,
    look: RowLook,
    on: &OnInbox,
) -> Stateful<Div> {
    let id = artoo_shell_api::notices::row_id(&r.key);
    let gist = r
        .text
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();
    let tip = tip_words(&r.says, source, r.at, r.text.trim(), look.wide);
    let face = if r.who.trim().is_empty() {
        Face::Mark(mark.to_owned(), source.to_owned(), icon)
    } else {
        Face::Person(r.who.clone())
    };
    let pick = on.clone();
    draw_row(RowDraw {
        id: ("inbox-part", id).into(),
        key: &r.key,
        lead: lead(face, r.badge.or(badge), 20.),
        from: (mark.to_owned(), source, icon),
        title: if r.about.is_empty() {
            r.says.clone()
        } else {
            r.about.clone()
        },
        at: r.at,
        gist,
        tip,
        project: &r.project,
        look,
    })
    .on_click(move |_: &ClickEvent, _, cx| pick(InboxEvent::PickPart(id), cx))
}

/// A part's row selected: its source and when, what happened, what it is about, what was written, and `Open ↵`.
pub fn part_preview(
    r: &artoo_shell_api::notices::Listed,
    source: &str,
    (mark, icon): (&str, Option<&'static str>),
    on: &OnInbox,
) -> AnyElement {
    let id = artoo_shell_api::notices::row_id(&r.key);
    let (open, done) = (on.clone(), on.clone());
    // The about line without the id the header says already (`PAY-412 Refund…` → `Refund…`).
    let title = (r.about.strip_prefix(r.tag.as_str()))
        .filter(|_| !r.tag.is_empty())
        .map_or(r.about.as_str(), str::trim_start)
        .to_owned();
    // The face says what it is in its tooltip: its source, where, and what happened.
    let whence = [
        source,
        r.project.as_str(),
        r.place.as_str(),
        r.says.as_str(),
    ]
    .iter()
    .filter(|w| !w.is_empty())
    .map(|w| (*w).to_owned())
    .collect::<Vec<_>>()
    .join(" · ");
    let face = if r.who.trim().is_empty() {
        Face::Mark(mark.to_owned(), source.to_owned(), icon)
    } else {
        Face::Person(r.who.clone())
    };
    let facts = (r.facts.iter()).fold(
        div().flex().flex_wrap().gap(px(24.)),
        |row, (label, value)| {
            row.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .text_size(px(12.))
                    .child(div().text_color(theme::muted()).child(label.clone()))
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(theme::text())
                            .child(value.clone()),
                    ),
            )
        },
    );
    let open_words = if r.open.is_empty() {
        "Open"
    } else {
        r.open.as_str()
    };
    div()
        .p(px(20.))
        .flex()
        .flex_col()
        .gap(px(10.))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(12.))
                .text_size(px(12.))
                .text_color(theme::muted())
                .when(!r.tag.is_empty(), |d| {
                    d.child(div().text_color(theme::accent()).child(r.tag.clone()))
                })
                .child(
                    div()
                        .id(("inbox-part-source", id))
                        .child(lead(face, None, 16.))
                        .tip(whence),
                )
                .child(div().flex_1())
                .child(
                    div()
                        .id(("inbox-part-when", id))
                        .child(format::short_ago(r.at))
                        .tip(hover::full_date(r.at)),
                ),
        )
        .child(
            div()
                .text_size(px(15.))
                .font_weight(FontWeight::SEMIBOLD)
                .child(title),
        )
        .when(r.facts.is_empty(), |d| {
            d.child(div().text_color(theme::text()).child(r.says.clone()))
        })
        .children((!r.text.trim().is_empty() && r.facts.is_empty()).then(|| {
            div()
                .text_size(px(13.))
                .text_color(theme::muted())
                .child(r.text.trim().to_owned())
        }))
        .when(!r.facts.is_empty(), |d| {
            d.child(facts.pb(px(14.)).border_b_1().border_color(theme::border()))
        })
        .child(
            div()
                .flex()
                .gap_2()
                .pt(px(4.))
                .child(
                    artoo_ui_kit::controls::primary_button("inbox-part-open", open_words, "↵")
                        .on_click(move |_: &ClickEvent, _, cx| open(InboxEvent::OpenPart(id), cx)),
                )
                .child(
                    bar_button("inbox-part-done", "Done", "E", false)
                        .on_click(move |_: &ClickEvent, _, cx| done(InboxEvent::DonePart(id), cx)),
                ),
        )
        .into_any_element()
}

/// What the notice selected shows besides itself.
pub struct NoticeLook<'a> {
    /// Its place in the list.
    pub ix: usize,
    pub request: Option<&'a Mr>,
    /// Its discussion, when it has one and it was read.
    pub thread: Option<&'a Thread>,
    /// The code read for it, or being read.
    pub view: &'a InboxView,
    /// The changes sent and not yet read back (#139).
    pub waiting: &'a Waiting,
    /// Who you are, by login.
    pub me: &'a str,
    /// Comments drawn as markdown.
    pub rich: bool,
    pub selecting: Selecting,
    /// A reply being written to it: the composer, in Reply…'s place.
    pub writing: Option<AnyElement>,
}

/// The notice selected: its request, the code, the discussion, the reply.
pub fn notice_preview(n: &Notice, look: NoticeLook, on: &OnInbox, cx: &App) -> Div {
    let NoticeLook {
        ix,
        request,
        thread,
        view,
        waiting,
        me,
        rich,
        selecting,
        writing,
    } = look;
    let props = div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap_2()
        .text_size(px(12.))
        .text_color(theme::muted())
        .child({
            let (iid, web, menu) = (n.iid, on.clone(), on.clone());
            div()
                .id("inbox-number")
                .font_family(theme::code_font())
                .text_color(theme::accent())
                .child(words::number(n.iid))
                .link_mark("inbox-number", marked::request(n.iid))
                // ⌘/Ctrl-click: the browser; right-click: the request's menu.
                .on_click(move |e: &ClickEvent, _, cx| {
                    if e.modifiers().secondary() {
                        web(InboxEvent::OnWeb(iid), cx)
                    }
                })
                .on_mouse_down(MouseButton::Right, move |e: &MouseDownEvent, _, cx| {
                    menu(
                        InboxEvent::Menu {
                            iid,
                            at: e.position,
                        },
                        cx,
                    )
                })
        })
        .children(request.map(|m| {
            div()
                .flex()
                .min_w_0()
                .items_center()
                .gap(px(6.))
                .child(discussion::avatar(&m.author, false, 16.))
                .child(div().flex_none().child(m.author.clone()))
        }))
        // When, short, its full date in a tooltip; what happened is the row's (#202).
        .child(
            div()
                .id("inbox-when")
                .child(format::short_ago(n.created))
                .tip(hover::full_date(n.created)),
        );
    let title = div()
        .text_size(px(19.))
        .line_height(px(26.))
        .font_weight(FontWeight::SEMIBOLD)
        .child(n.title.clone());
    let code = thread.and_then(|t| {
        let path = t.path.clone()?;
        let line = t.new_line.or(t.old_line)?;
        match view.excerpts.get(&t.id) {
            Some(ex) => Some(excerpt_view(&path, line, Some(ex.as_ref()))),
            None if view.reading.as_ref() == Some(&t.id) => Some(excerpt_view(&path, line, None)),
            None => None,
        }
    });
    // The discussion, or what the notice says when there is none (a mention in a description).
    let talk = match thread {
        Some(t) => {
            let last = t.notes.len().saturating_sub(1);
            let notes = t.notes.iter().enumerate().map(|(i, note)| {
                // Sent from here or the diff: faded until the host answers; the read after it is the Inbox's
                // next look (#139).
                let waits = waiting.note(n.iid, u64::MAX, note.id);
                discussion::note(
                    &note.author,
                    note.login == me,
                    None,
                    waits.map_or_else(
                        // Changed after it was written (#199).
                        || match note.edited {
                            true => format!("{} · edited", format::ago(note.created)),
                            false => format::ago(note.created),
                        },
                        str::to_owned,
                    ),
                    markdown::body(
                        &note.body,
                        rich,
                        format!("md-inbox-{}", note.id),
                        None,
                        Some(selecting.clone()),
                        cx,
                    ),
                    i == last,
                    None,
                    None,
                )
                .when(waits.is_some(), |s| s.opacity(FADED))
            });
            let (iid, id, resolved) = (n.iid, t.id.clone(), t.resolved);
            let resolving = waiting.thread(iid, u64::MAX, &id).filter(|_| t.resolvable);
            let on = on.clone();
            discussion::frame(Source::GitLab, n.note.is_some())
                .children(notes)
                .children(t.resolvable.then(|| {
                    div().flex().justify_end().pt(px(6.)).child(
                        discussion::button(
                            "inbox-resolve",
                            match resolving {
                                Some(word) => word,
                                None if resolved => "Reopen",
                                None => "Resolve",
                            },
                            None,
                            Button::Plain,
                        )
                        .when(resolving.is_some(), |s| s.opacity(FADED))
                        .on_click(move |_: &ClickEvent, _, cx| {
                            on(
                                InboxEvent::Resolve {
                                    iid,
                                    id: id.clone(),
                                    resolved: !resolved,
                                },
                                cx,
                            )
                        }),
                    )
                }))
                .into_any_element()
        }
        None if !n.text.trim().is_empty() => discussion::frame(Source::GitLab, false)
            .child(discussion::note(
                &n.who,
                false,
                None,
                format::ago(n.created),
                markdown::body(
                    &n.text,
                    rich,
                    format!("md-inbox-{}", n.key),
                    None,
                    Some(selecting),
                    cx,
                ),
                true,
                None,
                None,
            ))
            .into_any_element(),
        None => div().into_any_element(),
    };
    let answerable = n.kind.answerable();
    let composer = answerable.then(|| match writing {
        Some(editor) => editor,
        None => {
            let placeholder = if thread.is_some() {
                format!("Reply to {}…", n.who)
            } else {
                format!("Comment on {}…", words::number(n.iid))
            };
            let on = on.clone();
            div()
                .id("inbox-reply")
                .max_w(px(720.))
                .h(px(36.))
                .px(px(12.))
                .flex()
                .items_center()
                .gap_2()
                .rounded(px(8.))
                .border_1()
                .border_color(theme::island_border())
                .bg(theme::panel())
                .cursor_text()
                .text_color(theme::faint())
                .child(div().flex_1().child(placeholder))
                .child(keys::caps("R", On::Plain))
                .on_click(move |_: &ClickEvent, _, cx| on(InboxEvent::Reply, cx))
                .into_any_element()
        }
    });
    let (open, done) = (on.clone(), on.clone());
    let actions = div()
        .flex()
        .gap_2()
        .child(
            // Buttons that look like buttons, the way on filled (the developer, 2026-10-23).
            bar_button("inbox-open", "Open request", "↵", true)
                .on_click(move |_: &ClickEvent, _, cx| open(InboxEvent::Open(ix), cx)),
        )
        .child(
            bar_button("inbox-done", "Done", "E", false)
                .on_click(move |_: &ClickEvent, _, cx| done(InboxEvent::Done(ix), cx)),
        );
    div()
        .flex()
        .flex_col()
        .gap(px(14.))
        .px(px(28.))
        .py(px(22.))
        .max_w(px(820.))
        .child(props)
        .child(title)
        .children(code)
        .child(talk)
        .children(composer)
        .child(actions)
}
