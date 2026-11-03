//! The Inbox drawn (#49, Design L4): its filters, the rows loading before the first read or one message when nothing
//! is listed, a notice's row and the notice selected previewed — its request, the code its discussion is on, the
//! whole discussion, the reply — and the keys along the preview's foot. What the tab keeps comes in a look; a click
//! asks it an `InboxEvent`.

use crate::inbox::{InboxFilter, InboxView};
use crate::notices::{Kind, Notice};
use crate::waiting::{FADED, Waiting};
use artoo_core::markdown::plain;
use artoo_hosts_api::{Mr, Thread};
use artoo_ui_kit::controls::{self, bar_button, chip, skeleton};
use artoo_ui_kit::discussion::{self, Kind as Button, Source};
use artoo_ui_kit::excerpt::excerpt_view;
use artoo_ui_kit::hover::{self, CutInTip, Hover};
use artoo_ui_kit::keys::{self, On};
use artoo_ui_kit::markdown::{self, Selecting};
use artoo_ui_kit::{ROW_RADIUS, format, gap, icons, island, px, theme, words};
use gpui::{
    AnyElement, App, ClickEvent, Div, FontWeight, MouseButton, MouseDownEvent,
    Stateful, div, prelude::*,
};

pub use artoo_requests_api::{InboxEvent, OnInbox, RowLook};

/// The Inbox's filters, in its page bar (#71).
pub fn filters(shown: InboxFilter, on: &OnInbox) -> Div {
    let filter = |f: InboxFilter, label: &'static str| {
        let id = match f {
            InboxFilter::All => "inbox-all",
            InboxFilter::Mentions => "inbox-mentions",
            InboxFilter::Agents => "inbox-agents",
            InboxFilter::Reviews => "inbox-reviews",
        };
        let on = on.clone();
        chip(id, label, shown == f)
            .on_click(move |_: &ClickEvent, _, cx| on(InboxEvent::Filter(f), cx))
    };
    controls::group()
        .child(filter(InboxFilter::All, "All"))
        .child(filter(InboxFilter::Mentions, "Mentions"))
        .child(filter(InboxFilter::Agents, "Agents"))
        .child(filter(InboxFilter::Reviews, "Reviews"))
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

/// Nothing listed under `filter`; `others`: something is under another, and Show all goes there.
pub fn empty(filter: InboxFilter, others: bool, on: &OnInbox) -> AnyElement {
    let (title, text) = match filter {
        InboxFilter::All => (
            "Nothing needs you",
            "Review requests, mentions and replies in your discussions land here.",
        ),
        InboxFilter::Mentions => (
            "No mentions",
            "When someone mentions you in this project, it lands here.",
        ),
        InboxFilter::Agents => (
            "Nothing from your agents",
            "When an agent asks about one of your notes, or says it is done, it lands here.",
        ),
        InboxFilter::Reviews => (
            "No review requests",
            "When someone asks for your review, it lands here.",
        ),
    };
    let all = (filter != InboxFilter::All && others).then(|| {
        let on = on.clone();
        div()
            .id("inbox-show-all")
            .child(controls::outline_button("Show all"))
            .on_click(move |_: &ClickEvent, _, cx| on(InboxEvent::Filter(InboxFilter::All), cx))
            .into_any_element()
    });
    island()
        .flex_1()
        .bg(theme::editor())
        // Its button's room kept when there is none, so switching the filter moves nothing.
        .child(controls::empty_state(
            "inbox",
            title.to_owned(),
            text.to_owned(),
            Some(all.unwrap_or_else(|| div().h(px(28.)).into_any_element())),
        ))
        .into_any_element()
}

/// A group's heading in the list: Today, Earlier.
pub fn heading(text: &'static str) -> AnyElement {
    div()
        .px(px(16.))
        .pt(px(10.))
        .pb(px(4.))
        .text_size(px(11.))
        .text_color(theme::faint())
        .child(text)
        .into_any_element()
}

/// Notice `ix`: unread dot, who did what, the request always; when, and the start of what was written, while the
/// list is `wide`; all of it in its tooltip.
pub fn notice_row(n: &Notice, ix: usize, look: RowLook, on: &OnInbox) -> Stateful<Div> {
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
        format!("{} {}", n.who, n.kind_verb()),
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
                                .child(format!("{} {}", n.who, n.kind_verb())),
                        )
                        .children(wide.then(|| {
                            div()
                                .flex_none()
                                .text_size(px(11.))
                                .text_color(theme::faint())
                                .child(format::ago(n.created))
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
        .tip(tip.join("\n"))
        .on_click(move |_: &ClickEvent, _, cx| pick(InboxEvent::Pick(ix), cx))
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
                .items_center()
                .gap(px(6.))
                .child(
                    icons::icon("pull-request")
                        .size(px(14.))
                        .text_color(theme::muted()),
                )
                .child(format!("{} · {} → {}", m.author, m.source, m.target))
        }))
        .child(div().flex_1())
        .child(format!(
            "{} {} · {}",
            n.who,
            n.kind_verb(),
            format::ago(n.created)
        ));
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
    let answerable = !matches!(n.kind, Kind::Review | Kind::Todo);
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

/// The keys of the Inbox, along the preview's foot; `agent`: an agent's answer is selected, and whether it says it is
/// done.
pub fn foot_keys(agent: Option<bool>) -> Div {
    let key = |caps: &[&str], what: &'static str| {
        div()
            .flex()
            .items_center()
            .gap(px(4.))
            .children(caps.iter().map(|c| keys::caps(c, On::Plain)))
            .child(div().pl(px(2.)).child(what))
    };
    div()
        .flex_none()
        .flex()
        .gap_4()
        .px(px(16.))
        .py(px(8.))
        .border_t_1()
        .border_color(theme::island_border())
        .text_size(px(12.))
        .text_color(theme::muted())
        .child(key(&["J", "K"], "move"))
        .child(key(
            &["↵"],
            if agent.is_some() {
                "open at the line"
            } else {
                "open request"
            },
        ))
        .child(key(
            &["E"],
            match agent {
                Some(true) => "resolve",
                Some(false) => "later",
                None => "done",
            },
        ))
        .child(key(
            &["R"],
            if agent.is_some() { "answer" } else { "reply" },
        ))
}
