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
use artoo_ui_kit::controls::{self, bar_button, chip, skeleton};
use artoo_ui_kit::discussion::{self, Kind as Button, Source};
use artoo_ui_kit::excerpt::excerpt_view;
use artoo_ui_kit::hover::{self, CutInTip, RowTip};
use artoo_ui_kit::keys::{self, On};
use artoo_ui_kit::markdown::{self, Selecting};
use artoo_ui_kit::{ROW_RADIUS, format, gap, icons, island, px, theme, words};
use gpui::{
    AnyElement, App, Bounds, ClickEvent, Div, Entity, FontWeight, MouseButton, MouseDownEvent,
    Pixels, Stateful, canvas, div, prelude::*,
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
    compact: bool,
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
        chip(
            "inbox-why",
            format!("{} ▾", counted(name, new)),
            shown.why.is_some(),
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
                chip(id, counted(name, new), shown.why == why).on_click(
                    move |_: &ClickEvent, _, cx| {
                        on(InboxEvent::Filter(InboxFilter { why, ..shown }), cx)
                    },
                )
            }))
            .into_any_element()
    };
    let name = (sources.iter())
        .find(|s| Some(s.id) == shown.source)
        .map_or("All", |s| s.name.as_str());
    let view = view.clone();
    let source = chip(
        "inbox-source",
        format!("Source: {name} ▾"),
        shown.source.is_some(),
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
        let (name, new) = match id {
            None => ("All sources".to_owned(), everywhere),
            Some(id) => (sources.iter())
                .find(|s| s.id == id)
                .map_or((String::new(), 0), |s| (s.name.clone(), s.new)),
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
            .when(id.is_some() && new == 0, |s| s.text_color(theme::muted()))
            .child(
                div()
                    .flex_none()
                    .w(px(14.))
                    .children((shown.source == id).then(|| {
                        icons::icon("check")
                            .size(px(13.))
                            .text_color(theme::accent())
                    })),
            )
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

/// Rows loading before the first read, in a list `width` wide, and the preview's.
pub fn loading(width: f32) -> AnyElement {
    div()
        .flex_1()
        .min_h_0()
        .flex()
        .gap(px(gap()))
        .child(
            island()
                .w(px(width))
                .flex_none()
                .p(px(16.))
                .child(skeleton("inbox-loading", 6)),
        )
        .child(
            island()
                .flex_1()
                .bg(theme::editor())
                .px(px(28.))
                .py(px(26.))
                .child(skeleton("inbox-preview-loading", 3)),
        )
        .into_any_element()
}

/// Nothing listed because `what` could not be read (#185).
pub fn failed(what: &str) -> AnyElement {
    island()
        .flex_1()
        .bg(theme::editor())
        .child(controls::empty_state(
            "inbox",
            words::say(what).into_owned(),
            String::new(),
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

/// A group's heading in the list: `New 7`, `Read` (#197).
pub fn heading(text: impl Into<gpui::SharedString>) -> AnyElement {
    div()
        .px(px(16.))
        .pt(px(10.))
        .pb(px(4.))
        .text_size(px(11.))
        .text_color(theme::faint())
        .child(text.into())
        .into_any_element()
}

/// Notice `ix`: unread dot, what happened, the request always; its `source` (`GitLab`) and when, and the start of
/// what was written, while the list is `wide` (#197); all of it in its tooltip.
pub fn notice_row(
    n: &Notice,
    ix: usize,
    source: &str,
    look: RowLook,
    on: &OnInbox,
) -> Stateful<Div> {
    let RowLook {
        fresh,
        on: picked,
        wide,
    } = look;
    let text = plain(&n.text);
    let gist = text
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();
    let mut tip = vec![
        format!("{source} · {}", n.says()),
        format!("{} {}", words::number(n.iid), n.title),
        hover::full_date(n.created),
    ];
    if !text.trim().is_empty() {
        tip.push(text.trim().to_owned());
    }
    let iid = n.iid;
    let (menu, pick) = (on.clone(), on.clone());
    div()
        .id(("inbox-row", ix))
        .on_mouse_down(MouseButton::Right, move |e: &MouseDownEvent, _, cx| {
            menu(
                InboxEvent::Menu {
                    iid,
                    at: e.position,
                },
                cx,
            )
        })
        .mx(px(6.))
        .px(px(10.))
        .py(px(8.))
        .flex()
        .items_start()
        .gap(px(10.))
        .rounded(px(ROW_RADIUS))
        .cursor_pointer()
        .when(picked, |s| s.bg(theme::selected()))
        .hover(|s| s.bg(theme::hover()))
        .child(
            div()
                .flex_none()
                .mt(px(6.))
                .size(px(7.))
                .rounded_full()
                .when(fresh, |s| s.bg(theme::accent())),
        )
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
                        .gap_2()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .cut_in_tip()
                                .when(fresh, |s| s.font_weight(FontWeight::SEMIBOLD))
                                .child(n.says()),
                        )
                        .children(wide.then(|| {
                            div()
                                .flex_none()
                                .text_size(px(11.))
                                .text_color(theme::faint())
                                .child(format!("{source} · {}", format::ago(n.created)))
                        })),
                )
                .child(
                    div()
                        .cut_in_tip()
                        .text_size(px(12.))
                        .text_color(theme::accent())
                        .child(format!("{} {}", words::number(n.iid), n.title)),
                )
                .children((wide && !gist.is_empty()).then(|| {
                    div()
                        .cut_in_tip()
                        .text_size(px(12.))
                        .text_color(theme::muted())
                        .child(gist.to_owned())
                })),
        )
        .row_tip(picked, tip.join("\n"))
        .on_click(move |_: &ClickEvent, _, cx| pick(InboxEvent::Pick(ix), cx))
}

/// A part's row (a plugin's, #196): its source and what happened, what it is about, the start of what was written;
/// drawn as a notice's is, its source's name before the time.
pub fn part_row(
    r: &artoo_shell_api::notices::Listed,
    source: &str,
    look: RowLook,
    on: &OnInbox,
) -> Stateful<Div> {
    let RowLook {
        fresh,
        on: picked,
        wide,
    } = look;
    let id = artoo_shell_api::notices::row_id(&r.key);
    let gist = r
        .text
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim()
        .to_owned();
    let mut tip = vec![
        format!("{source} · {}", r.says),
        r.about.clone(),
        hover::full_date(r.at),
    ];
    if !gist.is_empty() {
        tip.push(r.text.trim().to_owned());
    }
    let pick = on.clone();
    div()
        .id(("inbox-part", id))
        .mx(px(6.))
        .px(px(10.))
        .py(px(8.))
        .flex()
        .items_start()
        .gap(px(10.))
        .rounded(px(ROW_RADIUS))
        .cursor_pointer()
        .when(picked, |s| s.bg(theme::selected()))
        .hover(|s| s.bg(theme::hover()))
        .child(
            div()
                .flex_none()
                .mt(px(6.))
                .size(px(7.))
                .rounded_full()
                .when(fresh, |s| s.bg(theme::accent())),
        )
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
                        .gap_2()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .cut_in_tip()
                                .when(fresh, |s| s.font_weight(FontWeight::SEMIBOLD))
                                .child(r.says.clone()),
                        )
                        .children(wide.then(|| {
                            div()
                                .flex_none()
                                .text_size(px(11.))
                                .text_color(theme::faint())
                                .child(format!("{source} · {}", format::ago(r.at)))
                        })),
                )
                .children((!r.about.is_empty()).then(|| {
                    div()
                        .cut_in_tip()
                        .text_size(px(12.))
                        .text_color(theme::accent())
                        .child(r.about.clone())
                }))
                .children((wide && !gist.is_empty()).then(|| {
                    div()
                        .cut_in_tip()
                        .text_size(px(12.))
                        .text_color(theme::muted())
                        .child(gist)
                })),
        )
        .row_tip(picked, tip.join("\n"))
        .on_click(move |_: &ClickEvent, _, cx| pick(InboxEvent::PickPart(id), cx))
}

/// A part's row selected: its source and when, what happened, what it is about, what was written, and `Open ↵`.
pub fn part_preview(
    r: &artoo_shell_api::notices::Listed,
    source: &str,
    on: &OnInbox,
) -> AnyElement {
    let id = artoo_shell_api::notices::row_id(&r.key);
    let open = on.clone();
    div()
        .p(px(20.))
        .flex()
        .flex_col()
        .gap(px(10.))
        .child(
            div()
                .flex()
                .gap_2()
                .text_size(px(12.))
                .text_color(theme::muted())
                .child(source.to_owned())
                .child(div().flex_1())
                .child(format::ago(r.at)),
        )
        .child(
            div()
                .text_size(px(15.))
                .font_weight(FontWeight::SEMIBOLD)
                .child(r.about.clone()),
        )
        .child(div().text_color(theme::text()).child(r.says.clone()))
        .children((!r.text.trim().is_empty()).then(|| {
            div()
                .text_size(px(13.))
                .text_color(theme::muted())
                .child(r.text.trim().to_owned())
        }))
        .child(
            div().flex().pt(px(4.)).child(
                artoo_ui_kit::controls::primary_button("inbox-part-open", "Open", "↵")
                    .on_click(move |_: &ClickEvent, _, cx| open(InboxEvent::OpenPart(id), cx)),
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
        .child(
            div()
                .font_family(theme::code_font())
                .text_color(theme::accent())
                .child(words::number(n.iid)),
        )
        .children(request.map(|m| {
            div()
                .flex()
                .min_w_0()
                .items_center()
                .gap(px(6.))
                .child(
                    icons::icon("pull-request")
                        .size(px(14.))
                        .text_color(theme::muted()),
                )
                .child(div().flex_none().child(format!("{} ·", m.author)))
                .child(hover::branches("inbox-branches", &m.source, &m.target))
        }))
        .child(div().flex_1())
        .child(format!("{} · {}", n.says(), format::ago(n.created)));
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
                    waits.map_or_else(|| format::ago(note.created), str::to_owned),
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
