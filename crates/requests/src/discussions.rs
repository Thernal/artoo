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
use artoo_ui_kit::format::one_line;
use artoo_ui_kit::hover::{self, Hover, Tip};
use artoo_ui_kit::{ROW_RADIUS, px, theme};
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, FontWeight, MouseButton, MouseDownEvent, Window,
    div, prelude::*,
};
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
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
        folds: Rc::new(RefCell::new(Folds::default())),
    });
    // Asked while the tab draws its stripe and the window's title: the tab is not read. The work left: the open
    // discussions and the pending comments (#199).
    let total = Rc::new(move |cx: &App| {
        let requests = requests.read(cx);
        if host.showing(cx) {
            (requests.open_threads(), requests.drafts.len())
        } else {
            (0, 0)
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
        stripe: true,
        // About the page: before the windows that are in every space.
        order: 10,
        badge: Some(Rc::new(move |cx| {
            let (open, pending) = total(cx);
            Badge {
                count: open + pending,
                color: theme::accent(),
                what: work_left(open, pending),
                under: None,
            }
        })),
        count: Some(Rc::new(move |cx| {
            let (open, _) = counted(cx);
            (open > 0).then(|| format!("{open} open"))
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
    /// What is unfolded in it: the resolved ones, the general discussions read whole (#199).
    folds: Rc<RefCell<Folds>>,
}

/// What the window has unfolded (#199).
#[derive(Default)]
pub struct Folds {
    /// The resolved discussions, folded under `Resolved · N` until opened.
    pub resolved: bool,
    /// The general discussions read whole, by their ids.
    pub general: HashSet<String>,
}

/// One fold of the window opened or closed.
pub enum Fold {
    Resolved,
    General(String),
}

/// Opens or closes a fold, the window drawn again.
pub type OnFold = Rc<dyn Fn(Fold, &mut App)>;

impl Render for DiscussionsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let look = self.host.listing(cx);
        let latest = (self.services.upgrade())
            .and_then(|s| s.get::<Rc<dyn Branches>>())
            .and_then(|b| b.latest_version(cx))
            .map(|v| v.number);
        let composer = self.host.composer(cx);
        let (folds, view) = (self.folds.clone(), cx.entity().downgrade());
        let on_fold: OnFold = Rc::new(move |fold, cx| {
            {
                let mut folds = folds.borrow_mut();
                match fold {
                    Fold::Resolved => folds.resolved = !folds.resolved,
                    Fold::General(id) => {
                        if !folds.general.remove(&id) {
                            folds.general.insert(id);
                        }
                    }
                }
            }
            view.update(cx, |_, cx| cx.notify()).ok();
        });
        let folds = self.folds.borrow();
        let window = (self.requests.read(cx)).discussions_window(
            look,
            latest,
            self.acts.clone(),
            Unfolded {
                folds: &folds,
                on_fold,
                composer,
            },
        );
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(window.body)
            .children(window.foot.map(|foot| tool_foot().child(foot)))
    }
}

/// What the window is drawn with besides the request's comments: what is unfolded, and the composer writing in it.
pub struct Unfolded<'a> {
    pub folds: &'a Folds,
    pub on_fold: OnFold,
    pub composer: Option<(Option<usize>, AnyElement)>,
}

/// The stripe's tip: `3 open · 2 pending`.
fn work_left(open: usize, pending: usize) -> String {
    match (open, pending) {
        (0, 0) => "Nothing open".to_owned(),
        (open, 0) => format!("{open} open"),
        (0, pending) => format!("{pending} pending"),
        (open, pending) => format!("{open} open · {pending} pending"),
    }
}

/// Who moves next in discussion `t`: `waiting on you` when the last word is someone else's, else `waiting on` the
/// one you answered; none once resolved, or without knowing who you are (#199).
pub fn waiting_on(t: &Thread, me: Option<&str>) -> Option<String> {
    let me = me?;
    // A plain comment is no work: nothing waits on it.
    if t.resolved || !t.resolvable {
        return None;
    }
    let last = t.notes.last()?;
    if last.login != me {
        return Some("waiting on you".to_owned());
    }
    let other = (t.notes.iter().rev()).find(|n| n.login != me)?;
    let first = other
        .author
        .split_whitespace()
        .next()
        .unwrap_or(&other.author);
    Some(format!("waiting on {first}"))
}

impl Requests {
    /// The discussions still open: resolvable and not resolved — the work left (#199).
    pub fn open_threads(&self) -> usize {
        (self.threads.iter())
            .filter(|t| t.resolvable && !t.resolved)
            .count()
    }

    /// The Discussions tool window: the request's pending notes, then its discussions — `3 open`, each saying who
    /// moves —, the resolved ones folded under `Resolved · N` (#199), the outdated ones apart; public ones only, the
    /// agent's are the Agents window's (#76). A click shows one in the diff; a general one opens whole, with Reply…
    /// and Resolve under it.
    pub fn discussions_window(
        &self,
        look: Listing,
        latest: Option<usize>,
        acts: Acts,
        unfolded: Unfolded,
    ) -> DiscussionsWindow {
        let Unfolded {
            folds,
            on_fold,
            composer,
        } = unfolded;
        let (threads, drafts): (&[Thread], &[Draft]) = if look.showing {
            (&self.threads, &self.drafts)
        } else {
            (&[], &[])
        };
        let pending = drafts.len();
        // Written on an earlier version: listed apart, grouped by that version (#46).
        let outdated = if look.showing {
            self.outdated_by_version()
        } else {
            Vec::new()
        };
        let old: Vec<usize> = outdated.iter().flat_map(|(_, l)| l.clone()).collect();
        let current: Vec<(usize, &Thread)> = (threads.iter().enumerate())
            .filter(|(ix, _)| !old.contains(ix))
            .collect();
        let (resolved, open): (Vec<_>, Vec<_>) = current.into_iter().partition(|(_, t)| t.resolved);
        let open_count = open.iter().filter(|(_, t)| t.resolvable).count();
        let total = threads.len() + pending;
        // A request comment's composer goes in the foot; a general discussion's under it.
        let (mut reply_box, mut comment_box) = (None, None);
        match composer {
            Some((Some(ix), element)) => reply_box = Some((ix, element)),
            Some((None, element)) => comment_box = Some(element),
            None => {}
        }
        let me = self.login.as_deref();
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
        let mut row = |ix: usize, t: &Thread| -> AnyElement {
            let target = t.path.clone().zip(t.new_line.or(t.old_line));
            let old = t.new_line.is_none();
            let general = target.is_none();
            let whole = general && folds.general.contains(&t.id);
            let where_ = match &target {
                Some((p, n)) => format!("{}:{n}", p.rsplit('/').next().unwrap_or(p)),
                None if whole => "General ▾".to_owned(),
                None => "General ▸".to_owned(),
            };
            let first = &t.notes[0];
            // Faded while a change to it waits on the host (#139).
            let waits = self.thread_waits(&t.id).is_some()
                || t.notes.iter().any(|n| self.note_time(n.id, n.created).1);
            let tone = if t.resolved {
                theme::faint()
            } else {
                theme::warning()
            };
            let waiting = waiting_on(t, me);
            let id = t.id.clone();
            // Read whole, the row is its heading: who began it; the notes follow in full.
            let line = if whole {
                first.author.clone()
            } else {
                format!("{}: {}", first.author, one_line(&first.body, look.rich))
            };
            let item = comment_item(("comment-mr", ix), tone, where_, line)
                .when(whole, |s| s.bg(theme::selected()))
                .children(waiting.map(|w| {
                    div()
                        .pl(px(14.))
                        .text_size(px(11.))
                        .text_color(if w == "waiting on you" {
                            theme::warning()
                        } else {
                            theme::faint()
                        })
                        .child(w)
                }))
                .when(waits, |s| s.opacity(FADED))
                .on_click({
                    let (acts, on_fold) = (acts.clone(), on_fold.clone());
                    move |_: &ClickEvent, _, cx| match target.clone() {
                        Some((path, line)) => acts(Act::JumpToLine { path, line, old }, cx),
                        None => on_fold(Fold::General(id.clone()), cx),
                    }
                });
            if !whole {
                return item.into_any_element();
            }
            // A general discussion read whole: every note, then Reply… and Resolve, or its composer (#199).
            let notes = t.notes.iter().skip(1).map(|n| {
                div()
                    .pl(px(28.))
                    .pr(px(14.))
                    .py(px(3.))
                    .text_size(px(12.))
                    .text_color(theme::muted())
                    .child(format!("{}: {}", n.author, n.body.trim()))
            });
            let replying = reply_box.as_ref().is_some_and(|(r, _)| *r == ix);
            let composer = replying.then(|| reply_box.take().map(|(_, e)| e)).flatten();
            let buttons = (!replying).then(|| {
                let (reply, resolve) = (acts.clone(), acts.clone());
                div()
                    .pl(px(28.))
                    .pr(px(14.))
                    .py(px(4.))
                    .flex()
                    .gap(px(6.))
                    .child(
                        small_button(("general-reply", ix), "Reply…")
                            .on_click(move |_: &ClickEvent, _, cx| reply(Act::Reply(ix), cx)),
                    )
                    .children(t.resolvable.then(|| {
                        small_button(
                            ("general-resolve", ix),
                            if t.resolved { "Reopen" } else { "Resolve" },
                        )
                        .on_click(move |_: &ClickEvent, _, cx| resolve(Act::ToggleResolved(ix), cx))
                    }))
            });
            div()
                .flex()
                .flex_col()
                .pb(px(4.))
                .child(item)
                .child(
                    div()
                        .pl(px(28.))
                        .pr(px(14.))
                        .pb(px(3.))
                        .text_size(px(12.))
                        .text_color(theme::text())
                        .child(first.body.trim().to_owned()),
                )
                .children(notes)
                .children(buttons)
                .children(composer.map(|c| div().px(px(8.)).py(px(4.)).child(c)))
                .into_any_element()
        };
        let open_items: Vec<AnyElement> = open.iter().map(|(ix, t)| row(*ix, t)).collect();
        let resolved_items: Vec<AnyElement> = if folds.resolved {
            resolved.iter().map(|(ix, t)| row(*ix, t)).collect()
        } else {
            Vec::new()
        };
        let draft_items = drafts.iter().enumerate().map(|(ix, d)| {
            let (where_, target, old) = self.draft_where(d);
            let id = d.id;
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
            // Its right click: Send now, Delete (#199).
            .on_mouse_down(MouseButton::Right, {
                let acts = acts.clone();
                move |e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    acts(Act::PendingMenu { id, at: e.position }, cx)
                }
            })
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
        // A comment on the request as a whole, from the foot (#199).
        let comment = (look.showing && comment_box.is_none()).then(|| {
            let acts = acts.clone();
            small_button("comment-on-request", "Comment on the request…")
                .on_click(move |_: &ClickEvent, _, cx| acts(Act::CommentOnRequest, cx))
        });
        let foot = match (comment_box, submit, comment) {
            (Some(composer), submit, _) => Some(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .w_full()
                    .child(composer)
                    .children(submit)
                    .into_any_element(),
            ),
            (None, None, None) => None,
            (None, submit, comment) => Some(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .children(comment)
                    .children(submit)
                    .into_any_element(),
            ),
        };
        let resolved_count = resolved.len();
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
                .children((!open_items.is_empty()).then(|| {
                    group_heading(
                        match open_count {
                            0 => format!("Discussions · {}", open_items.len()),
                            n => format!("Discussions · {n} open"),
                        },
                        Some("⇧N"),
                    )
                }))
                .children(open_items)
                .children((resolved_count > 0).then(|| {
                    let on_fold = on_fold.clone();
                    div()
                        .id("resolved-fold")
                        .cursor_pointer()
                        .child(group_heading(
                            format!(
                                "{} Resolved · {resolved_count}",
                                if folds.resolved { "▾" } else { "▸" }
                            ),
                            None,
                        ))
                        .on_click(move |_: &ClickEvent, _, cx| on_fold(Fold::Resolved, cx))
                }))
                .children(resolved_items)
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
            foot,
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

/// A small bordered button in the window: Reply…, Resolve, Comment on the request.
fn small_button(id: impl Into<gpui::ElementId>, label: &'static str) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .flex_none()
        .px(px(8.))
        .h(px(22.))
        .flex()
        .items_center()
        .rounded(px(ROW_RADIUS))
        .border_1()
        .border_color(theme::island_border())
        .bg(theme::panel())
        .text_size(px(12.))
        .text_color(theme::text())
        .cursor_pointer()
        .hover(|s| s.bg(theme::hover()))
        .child(label)
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

#[cfg(test)]
mod tests {
    use super::*;
    use artoo_core::mr::Note;

    fn note(login: &str, author: &str) -> Note {
        Note {
            id: 1,
            author: author.to_owned(),
            login: login.to_owned(),
            body: String::new(),
            created: 0,
        }
    }

    fn thread(notes: Vec<Note>, resolvable: bool, resolved: bool) -> Thread {
        Thread {
            id: "d".to_owned(),
            notes,
            path: None,
            new_line: None,
            old_line: None,
            resolved,
            resolvable,
            resolved_by: None,
            resolved_at: 0,
            commit: None,
            outdated: false,
        }
    }

    #[test]
    fn a_discussion_says_who_moves_next() {
        let theirs = thread(vec![note("ember", "Ember Lind")], true, false);
        assert_eq!(
            waiting_on(&theirs, Some("you")).as_deref(),
            Some("waiting on you")
        );
        let answered = thread(
            vec![note("ember", "Ember Lind"), note("you", "You")],
            true,
            false,
        );
        assert_eq!(
            waiting_on(&answered, Some("you")).as_deref(),
            Some("waiting on Ember")
        );
        // Resolved, a plain comment, or not knowing who you are: no word.
        assert_eq!(
            waiting_on(&thread(vec![note("ember", "E")], true, true), Some("you")),
            None
        );
        assert_eq!(
            waiting_on(&thread(vec![note("ember", "E")], false, false), Some("you")),
            None
        );
        assert_eq!(waiting_on(&theirs, None), None);
        assert_eq!(work_left(3, 2), "3 open · 2 pending");
    }
}
