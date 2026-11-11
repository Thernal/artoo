//! The Discussions tool window (#76): the request's discussions and pending comments — public ones only, the
//! agent's are the Agents window's —, the outdated ones apart by the version they were written on (#46); a click
//! shows one in the diff. What a click asks goes to the tab (`Act`); what it is drawn with that the tab knows
//! comes from the tab's `DiscussionsHost`, the timeline's last version from the branches. It is the requests' tool
//! window, in a request's page.

use crate::requests::{Act, Acts, Requests};
use crate::waiting::{FADED, is_temp};
use artoo_branches_api::Branches;
use artoo_hosts_api::{Draft, Thread};
use artoo_requests_api::{DISCUSSIONS, DiscussionsHost};
use artoo_shell_api::{Badge, ListKey, Services, Tool};
use artoo_ui_kit::controls::{comment_item, group_heading, skeleton, tool_foot};
use artoo_ui_kit::format::{one_line, plural};
use artoo_ui_kit::hover::{self, Hover, Tip};
use artoo_ui_kit::{ROW_RADIUS, px, theme};
use gpui::{AnyElement, App, ClickEvent, Context, Entity, FontWeight, Window, div, prelude::*};
use std::cell::Cell;
use std::rc::{Rc, Weak};

pub use artoo_requests_api::Listing;

/// The window's body (which scrolls itself) and its foot, Submit review.
pub struct DiscussionsWindow {
    pub body: AnyElement,
    pub foot: Option<AnyElement>,
}

/// The Discussions tool window in a tab: in Merge requests while a page is on screen, its icon counting the
/// request's discussions and pending comments; ↑ ↓ walk them in the diff.
pub(crate) fn tool(
    requests: Entity<Requests>,
    host: Rc<dyn DiscussionsHost>,
    services: Weak<Services>,
    acts: Acts,
    cx: &mut App,
) -> Tool {
    let view = cx.new(|_| DiscussionsView {
        requests: requests.clone(),
        host: host.clone(),
        services,
        acts: acts.clone(),
    });
    // Asked while the tab draws its stripe and the window's title: the tab is not read.
    let total = Rc::new(move |cx: &App| {
        let requests = requests.read(cx);
        if host.showing(cx) {
            requests.threads.len() + requests.drafts.len()
        } else {
            0
        }
    });
    let counted = total.clone();
    Tool {
        id: DISCUSSIONS,
        icon: "message",
        tip: "Discussions  ⌘⇧D",
        title: "Discussions",
        spaces: &[artoo_requests_api::SPACE],
        on_pages: true,
        // About the page: before the windows that are in every space.
        order: 10,
        badge: Some(Rc::new(move |cx| {
            let n = total(cx);
            Badge {
                count: n,
                color: theme::accent(),
                what: plural(n, "comment"),
                under: None,
            }
        })),
        count: Some(Rc::new(move |cx| {
            let n = counted(cx);
            (n > 0).then(|| n.to_string())
        })),
        view: view.into(),
        opened: Rc::new(|_| {}),
        typing: Rc::new(Cell::new(false)),
        key: Rc::new(|_, _, _| {}),
        // The tab walks them once its update is over: it shows each in the diff.
        list_key: Some(Rc::new(move |key, cx| {
            let down = match key {
                ListKey::Down => true,
                ListKey::Up => false,
                ListKey::Enter | ListKey::Delete => return false,
            };
            let acts = acts.clone();
            cx.defer(move |cx| acts(Act::StepThread { down }, cx));
            true
        })),
    }
}

/// What the window draws: the list, and Submit review under it while comments are pending.
struct DiscussionsView {
    requests: Entity<Requests>,
    host: Rc<dyn DiscussionsHost>,
    /// The tab's, where the branches are.
    services: Weak<Services>,
    acts: Acts,
}

impl Render for DiscussionsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let look = self.host.listing(cx);
        let latest = (self.services.upgrade())
            .and_then(|s| s.get::<Rc<dyn Branches>>())
            .and_then(|b| b.latest_version(cx))
            .map(|v| v.number);
        let window = (self.requests.read(cx)).discussions_window(look, latest, self.acts.clone());
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(window.body)
            .children(window.foot.map(|foot| tool_foot().child(foot)))
    }
}

impl Requests {
    /// The Discussions tool window: the request's discussions and pending notes — public ones only,
    /// the agent's are the Agents window's (#76); a click shows one in the diff.
    pub fn discussions_window(
        &self,
        look: Listing,
        latest: Option<usize>,
        acts: Acts,
    ) -> DiscussionsWindow {
        let (threads, drafts): (&[Thread], &[Draft]) = if look.showing {
            (&self.threads, &self.drafts)
        } else {
            (&[], &[])
        };
        let theirs: Vec<(usize, &Thread)> = threads.iter().enumerate().collect();
        let pending = drafts.len();
        // Public only: the agent's comments are the Agents window's (#76).
        let discussions = theirs.len();
        let total = discussions + pending;
        // Group headings once there is more than one kind. Pending ones always have theirs: it is what
        // says they are pending, and how to walk them.
        let grouped = [discussions, pending].iter().filter(|&&n| n > 0).count() > 1;
        // Written on an earlier version: listed apart, grouped by that version (#46).
        let outdated = if look.showing {
            self.outdated_by_version()
        } else {
            Vec::new()
        };
        let old: Vec<usize> = outdated.iter().flat_map(|(_, l)| l.clone()).collect();
        let theirs: Vec<(usize, &Thread)> = theirs
            .into_iter()
            .filter(|(ix, _)| !old.contains(ix))
            .collect();
        let discussions = theirs.len();
        let mut outdated_items = Vec::new();
        for (version, list) in outdated {
            let items: Vec<gpui::AnyElement> = list
                .iter()
                .map(|&ix| self.outdated_item(ix, &look, latest, &acts))
                .collect();
            outdated_items.push(
                div()
                    .flex()
                    .flex_col()
                    .child(group_heading(
                        match version {
                            Some(v) => format!("Written on v{v} · {}", list.len()),
                            None => format!("An earlier version · {}", list.len()),
                        },
                        None,
                    ))
                    .children(items),
            );
        }
        let request_items = theirs.into_iter().map(|(ix, t)| {
            let target = t.path.clone().zip(t.new_line.or(t.old_line));
            let old = t.new_line.is_none();
            let where_ = match &target {
                Some((p, n)) => format!("{}:{n}", p.rsplit('/').next().unwrap_or(p)),
                None => "General".to_owned(),
            };
            let first = &t.notes[0];
            // Faded while a change to it waits on the host (#139).
            let waits = self.thread_waits(&t.id).is_some()
                || t.notes.iter().any(|n| self.note_time(n.id, n.created).1);
            comment_item(
                ("comment-mr", ix),
                theme::warning(),
                where_,
                format!("{}: {}", first.author, one_line(&first.body, look.rich)),
            )
            .when(waits, |s| s.opacity(FADED))
            .on_click({
                let acts = acts.clone();
                move |_: &ClickEvent, _, cx| {
                    if let Some((path, line)) = target.clone() {
                        acts(Act::JumpToLine { path, line, old }, cx)
                    }
                }
            })
        });
        let draft_items = drafts.iter().enumerate().map(|(ix, d)| {
            let (where_, target, old) = self.draft_where(d);
            comment_item(
                ("comment-draft", ix),
                theme::warning(),
                where_,
                draft_line(d, look.rich, look.agents.get(&d.id).map(String::as_str)),
            )
            .relative()
            .group("pending-row")
            .when(self.draft_waits(d.id).is_some(), |s| s.opacity(FADED))
            .children(
                send_now_chip(d.id, "pending-row", look.github, acts.clone())
                    .filter(|_| self.draft_waits(d.id).is_none() && !is_temp(d.id))
                    .map(|chip| chip.absolute().top(px(3.)).right(px(6.))),
            )
            .on_click({
                let acts = acts.clone();
                move |_: &ClickEvent, _, cx| {
                    if let Some((path, line)) = target.clone() {
                        acts(Act::JumpToLine { path, line, old }, cx)
                    }
                }
            })
        });
        let submit = (pending > 0).then(|| {
            div()
                .id("submit-review")
                .px_2()
                .h(px(24.))
                .flex()
                .items_center()
                .rounded(px(ROW_RADIUS))
                .bg(theme::warning())
                .text_color(theme::base())
                .text_size(px(12.))
                .font_weight(FontWeight::SEMIBOLD)
                .cursor_pointer()
                .child(format!("Submit review · {pending}"))
                .on_click({
                    let acts = acts.clone();
                    move |_: &ClickEvent, _, cx| acts(Act::OpenSubmit { approve: false }, cx)
                })
        });
        DiscussionsWindow {
            body: div()
                .id("comments-list")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .py(px(6.))
                .children(
                    (pending > 0)
                        .then(|| group_heading(format!("Pending · {pending}"), Some("⌥N"))),
                )
                .children(draft_items)
                .children(
                    (grouped && discussions > 0)
                        .then(|| group_heading(format!("Discussions · {discussions}"), None)),
                )
                .children(request_items)
                .children((!old.is_empty()).then(|| {
                    div()
                        .px(px(14.))
                        .pt(px(14.))
                        .pb(px(2.))
                        .text_size(px(12.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::text())
                        .child("Outdated")
                        .child(
                            div()
                                .text_size(px(11.))
                                .font_weight(FontWeight::NORMAL)
                                .text_color(theme::faint())
                                .child("by the version they were written on"),
                        )
                }))
                .children(outdated_items)
                .children((total == 0 && self.reading).then(|| {
                    div()
                        .px(px(14.))
                        .py(px(12.))
                        .child(skeleton("discussions-loading", 3))
                }))
                .children((total == 0 && !self.reading).then(|| {
                    div()
                        .px(px(14.))
                        .py(px(10.))
                        .text_size(px(12.))
                        .text_color(theme::muted())
                        .child("No comments yet")
                }))
                .into_any_element(),
            foot: submit.map(IntoElement::into_any_element),
        }
    }

    /// An outdated discussion in the Discussions window: where, its first words, whether its line
    /// changed. A click shows it — its line where it is now, else the versions compared since —,
    /// and it is resolved or reopened there, where it is read (the developer, 2026-10-23: no Show
    /// or Reopen on the row).
    fn outdated_item(
        &self,
        ix: usize,
        look: &Listing,
        latest: Option<usize>,
        acts: &Acts,
    ) -> AnyElement {
        let t = &self.threads[ix];
        let line_now = self.histories.get(&t.id).and_then(|h| h.line_now);
        let path = t.path.clone().unwrap_or_default();
        let place = format!(
            "{}:{}",
            path.rsplit('/').next().unwrap_or(&path),
            t.new_line.or(t.old_line).unwrap_or(0)
        );
        let first = &t.notes[0];
        let written = self.histories.get(&t.id).map(|h| h.written);
        // What a click does: the line where it is now, or (gone) the versions compared.
        let shows = match (line_now, self.compare_label(ix, latest)) {
            (Some(_), _) => "Show its line now".to_owned(),
            (None, Some(compare)) => compare,
            (None, None) => String::new(),
        };
        div()
            .id(("comment-old-item", ix))
            .flex()
            .flex_col()
            .cursor_pointer()
            .child(comment_item(
                ("comment-old", ix),
                theme::faint(),
                place,
                format!("{}: {}", first.author, one_line(&first.body, look.rich)),
            ))
            .child(
                div()
                    .pl(px(28.))
                    .pr(px(14.))
                    .pb(px(6.))
                    .text_size(px(11.))
                    .text_color(theme::warning())
                    .child(
                        self.since_comment(ix)
                            .unwrap_or_else(|| "Written on an earlier version".to_owned()),
                    ),
            )
            .when(!shows.is_empty(), |s| s.tip(shows))
            .on_click({
                let acts = acts.clone();
                move |_: &ClickEvent, _, cx| match line_now {
                    Some(line) => acts(
                        Act::JumpToLine {
                            path: path.clone(),
                            line,
                            old: false,
                        },
                        cx,
                    ),
                    None if written.is_some() => acts(Act::CompareSince(ix), cx),
                    None => {}
                }
            })
            .into_any_element()
    }
}

/// A pending comment in one line, an agent's after its name (#150).
pub fn draft_line(d: &Draft, rich: bool, agent: Option<&str>) -> String {
    match agent {
        Some(agent) => format!("{agent}: {}", one_line(&d.body, rich)),
        None => one_line(&d.body, rich),
    }
}

/// Send now on a pending comment's row in a list (the Discussions window, Submit review),
/// shown while the row is under the pointer; none on GitHub (#138).
pub fn send_now_chip(
    id: u64,
    group: &'static str,
    github: bool,
    acts: Acts,
) -> Option<gpui::Stateful<gpui::Div>> {
    (!github).then(|| {
        div()
            .id(("send-now", id as usize))
            .flex_none()
            .px(px(6.))
            .h(px(20.))
            .flex()
            .items_center()
            .rounded(px(ROW_RADIUS))
            .border_1()
            .border_color(theme::island_border())
            .bg(theme::panel())
            .text_size(px(11.))
            .text_color(theme::text())
            .cursor_pointer()
            .hover(|s| s.bg(theme::selected()))
            .invisible()
            .group_hover(group, |s| s.visible())
            .child("Send now")
            .tooltip_show_delay(hover::LONG)
            .tooltip(|_, cx| {
                cx.new(|_| Tip("Publishes this comment alone; the others stay pending"))
                    .into()
            })
            .on_click(move |_: &ClickEvent, _, cx| {
                cx.stop_propagation();
                acts(Act::SendNow(id), cx);
            })
    })
}
