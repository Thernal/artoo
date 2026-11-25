//! A request's page in every state of round 41 (Design `docs/specs/loading-stability.md`, `overview.html?s=…`):
//! what its head's slot says, the new comments that came in below, the page kept where it is while it changes, and a
//! merge seen without the page being read again.

use crate::freshness::{Freshness, frames};
use crate::requests::{Act, Acts, Requests};
use artoo_hosts_api::{Host, MrState, Note, Thread};
use artoo_ui_kit::fresh::{self, Held, Slot, SlotAction, SlotFade};
use artoo_ui_kit::{format, px};
use gpui::{
    AnyElement, App, Context, IntoElement, Pixels, Point, ScrollHandle, canvas, div, prelude::*,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

/// Who merged it, when the host does not say.
const SOMEONE: &str = "someone else";

/// What the open request's page keeps beside what it shows.
#[derive(Default)]
pub struct PageFresh {
    /// The request these are of.
    pub(crate) of: Option<u64>,
    /// When what the page shows was last read; none before its first answer.
    pub(crate) read_at: Option<i64>,
    /// The details, pipeline and merge state have answered.
    pub(crate) side_in: bool,
    /// A read, or the merge, is on its way.
    pub(crate) reading: bool,
    /// What shows is the page as an earlier visit left it, not read again yet.
    pub(crate) kept: bool,
    /// The last read failed; whether the host refused the token.
    pub(crate) failed: Option<bool>,
    /// The notes by others that came in under the page shown and were not gone to yet.
    pub(crate) new_notes: Vec<u64>,
    /// Who merged it elsewhere while its page was open.
    pub(crate) merged_by: Option<String>,
    /// Where the page is scrolled to, kept with it.
    pub(crate) scroll: ScrollHandle,
    pub(crate) anchor: Rc<RefCell<Anchor>>,
    /// The slot's fade, the hairline's clock and the first read's skeleton.
    pub(crate) fresh: Freshness,
}

/// Where the threads of the Discussions card are, and the one that must stay where it is while the page changes.
#[derive(Default)]
pub struct Anchor {
    /// Each thread's top in the page's content, as last drawn.
    tops: HashMap<String, f32>,
    /// The thread to hold, with its top before the change.
    hold: Option<(String, f32)>,
}

impl Anchor {
    /// The page is about to change: the first thread on screen (scrolled by `offset`, 0 or less) is to be held.
    pub(crate) fn hold_first(&mut self, offset: f32) {
        self.hold = first_visible(&self.tops, offset);
    }

    /// Thread `id` is drawn at `top`: the offset that keeps it where it was, when it is the one held and moved.
    fn drawn(&mut self, id: &str, top: f32, offset: f32) -> Option<f32> {
        self.tops.insert(id.to_owned(), top);
        let (_, was) = self.hold.take_if(|(held, _)| held == id)?;
        ((top - was).abs() > 0.5).then(|| anchored(offset, was, top))
    }

    pub(crate) fn top(&self, id: &str) -> Option<f32> {
        self.tops.get(id).copied()
    }

    pub(crate) fn clear(&mut self) {
        self.tops.clear();
        self.hold = None;
    }
}

/// The first thread whose top is on screen or under it.
fn first_visible(tops: &HashMap<String, f32>, offset: f32) -> Option<(String, f32)> {
    tops.iter()
        .filter(|(_, top)| **top + offset >= 0.)
        .min_by(|a, b| a.1.total_cmp(b.1))
        .map(|(id, top)| (id.clone(), *top))
}

/// The scroll offset that shows a thread moved from `was` to `now` where it was on screen.
fn anchored(offset: f32, was: f32, now: f32) -> f32 {
    (offset - (now - was)).min(0.)
}

/// An unseen mark on a thread's row: it reports where the row is, and holds the page on it.
pub(crate) fn mark(id: String, page: &PageFresh) -> impl IntoElement + use<> {
    let (anchor, scroll) = (page.anchor.clone(), page.scroll.clone());
    canvas(
        move |bounds, window, _| {
            let offset = scroll.offset();
            let top = f32::from(bounds.origin.y - scroll.bounds().origin.y - offset.y);
            let held = anchor.borrow_mut().drawn(&id, top, f32::from(offset.y));
            if let Some(y) = held {
                scroll.set_offset(Point::new(offset.x, Pixels::from(y)));
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

/// The notes in `now` that `before` does not hold, written by someone other than `me`.
fn notes_new(before: &[Thread], now: &[Thread], me: Option<&str>) -> Vec<u64> {
    let known: std::collections::HashSet<u64> = (before.iter())
        .flat_map(|t| t.notes.iter().map(|n: &Note| n.id))
        .collect();
    (now.iter())
        .flat_map(|t| &t.notes)
        .filter(|n| !known.contains(&n.id) && me != Some(n.login.as_str()))
        .map(|n| n.id)
        .collect()
}

impl Requests {
    /// A page is opened: nothing of the one before it stays.
    pub(crate) fn page_opened(&mut self, iid: u64) {
        let scroll = self.page.scroll.clone();
        scroll.set_offset(Point::default());
        self.page.anchor.borrow_mut().clear();
        self.page = PageFresh {
            of: Some(iid),
            scroll,
            anchor: self.page.anchor.clone(),
            ..Default::default()
        };
    }

    /// Whether request `iid`'s page is on screen with what a visit read: a read of it changes it in place.
    pub(crate) fn page_shown(&self, iid: u64) -> bool {
        self.page.of == Some(iid) && self.page.read_at.is_some()
    }

    /// A read of the open page begins.
    pub(crate) fn page_reads(&mut self) {
        self.page.reading = true;
        let nothing = self.page.read_at.is_none();
        self.page.fresh.begin(nothing, Instant::now());
    }

    /// The discussions answered: the new notes under a page already shown are counted, and the page held on the
    /// first thread on screen.
    pub(crate) fn page_read(&mut self, before: &[Thread]) {
        if self.page.read_at.is_some() {
            let new = notes_new(before, &self.base.threads, self.login.as_deref());
            self.page.new_notes.extend(new);
            let offset = f32::from(self.page.scroll.offset().y);
            self.page.anchor.borrow_mut().hold_first(offset);
        }
        self.page.read_at = Some(format::now());
        self.page.reading = false;
        self.page.kept = false;
        self.page.failed = None;
        self.page.fresh.end(Instant::now());
    }

    /// A read of the page failed: what shows stays, the slot says it.
    pub(crate) fn page_failed(&mut self, error: &str) {
        self.page.reading = false;
        self.page.failed = Some(error.contains("token"));
        self.page.fresh.end(Instant::now());
    }

    /// What the page's slot says.
    pub fn page_slot(&self) -> Slot {
        let page = &self.page;
        if let Some(who) = &page.merged_by {
            return Slot::MergedBy(who.clone());
        }
        let at = page.read_at.map(format::clock);
        if page.reading {
            return match at {
                Some(at) if page.kept => Slot::Kept(at),
                Some(_) => Slot::Refreshing,
                None => Slot::Reading,
            };
        }
        if let Some(refused) = page.failed {
            return match (refused, at) {
                (true, _) => Slot::TokenExpired,
                (false, Some(at)) => Slot::CantReach(at),
                (false, None) => Slot::CantRead,
            };
        }
        let Some(at) = at else {
            return Slot::Reading;
        };
        match page.new_notes.len() {
            0 if page.kept => Slot::Kept(at),
            0 => Slot::Updated(at),
            n => Slot::Waiting(at, n, Held::NewBelow),
        }
    }

    /// The slot's `1 new ↓`: the page goes to the first new comment's thread.
    pub(crate) fn show_new_notes(&mut self) {
        let first = (self.threads.iter())
            .find(|t| t.notes.iter().any(|n| self.page.new_notes.contains(&n.id)));
        if let Some(top) = first.and_then(|t| self.page.anchor.borrow().top(&t.id)) {
            let scroll = &self.page.scroll;
            let most = f32::from(scroll.max_offset().y);
            let y = (top - 24.).clamp(0., most.max(0.));
            scroll.set_offset(Point::new(scroll.offset().x, px(-y)));
        }
        self.page.new_notes.clear();
    }

    /// Merged from here: the page stays as it is, only the request's state changes; its row stays in the list until
    /// the next apply.
    pub(crate) fn merged_here_now(&mut self, number: u64) {
        self.set_state(number, MrState::Merged);
        if !self.merged_here.contains(&number) {
            self.merged_here.push(number);
        }
        if self.current == Some(number) {
            self.merge = None;
            self.page.reading = false;
            self.page.failed = None;
            self.page.read_at = Some(format::now());
            self.page.fresh.end(Instant::now());
        }
    }

    fn set_state(&mut self, number: u64, state: MrState) {
        for m in self.list.iter_mut().chain(&mut self.left) {
            if m.iid == number {
                m.state = state;
            }
        }
    }

    /// Whether the open request was merged while its page was open, here or elsewhere.
    pub(crate) fn merged_on_page(&self) -> bool {
        self.current.is_some_and(|iid| {
            self.page.merged_by.is_some()
                || (self.merged_here.contains(&iid)
                    && (self.find(iid)).is_some_and(|m| m.state == MrState::Merged))
        })
    }

    /// The request whose page is open left the list a read brought: it is read once, its state changes in place,
    /// and when someone merged it, the page says who.
    pub(crate) fn look_for_merge(&mut self, iid: u64, host: Arc<dyn Host>, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move { host.one(iid).ok() })
                .await;
            this.update(cx, |rq, cx| {
                let Some(m) = read.filter(|m| m.state != MrState::Opened) else {
                    return;
                };
                if rq.current != Some(iid) || rq.merged_here.contains(&iid) {
                    return;
                }
                rq.set_state(iid, m.state);
                rq.merge = None;
                if m.state == MrState::Merged {
                    rq.page.merged_by = Some(m.merged_by.unwrap_or_else(|| SOMEONE.to_owned()));
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// A snapshot's state of the open page (`ARTOO_SNAPSHOT_FRESH`, as `overview.html?s=<state>`), no host asked.
    pub(crate) fn force_page_fresh(&mut self, state: &str) {
        let Some(iid) = self.current else {
            return;
        };
        let long = Instant::now() - std::time::Duration::from_secs(5);
        self.page.read_at.get_or_insert_with(format::now);
        match state {
            "loading" => {
                self.page.read_at = None;
                self.page.side_in = false;
                self.page.reading = true;
                self.page.fresh = Freshness::default();
                self.page.fresh.begin(true, long);
            }
            "refreshing" | "switch" => {
                self.page.kept = state == "switch";
                self.page.reading = true;
                self.page.fresh.begin(false, long);
            }
            "comment" => {
                let Some(mut thread) = self.threads.last().cloned() else {
                    return;
                };
                thread.id = "snapshot-new".into();
                thread.resolved = false;
                thread.path = None;
                thread.notes.truncate(1);
                if let Some(note) = thread.notes.first_mut() {
                    note.id = u64::MAX;
                    note.author = "Kestrel Okafor".into();
                    note.login = "kestrel".into();
                    note.body = "Can the error text keep the last value while a retry runs?".into();
                }
                self.page.new_notes = vec![u64::MAX];
                self.base.threads.push(thread.clone());
                self.threads.push(thread);
            }
            "merging" => {
                self.merge_busy = Some(crate::bar::MERGING);
                self.page.reading = true;
                self.page.fresh.begin(false, long);
            }
            "merged" => self.merged_here_now(iid),
            "elsewhere" => {
                self.set_state(iid, MrState::Merged);
                self.merge = None;
                self.page.merged_by = Some("Ember Lind".into());
            }
            "error" => self.page.failed = Some(false),
            // `loaded`, and anything else.
            _ => {}
        }
    }
}

/// The slot at the page head's right.
pub(crate) fn head_slot(
    rq: &Requests,
    acts: Acts,
    show_new: impl Fn(&mut App) + 'static,
) -> AnyElement {
    let slot = rq.page_slot();
    let mut kept = rq.page.fresh.fade.borrow_mut();
    let fade = kept.get_or_insert_with(|| SlotFade::new(slot.clone()));
    fade.set(slot, Instant::now());
    let element = fresh::slot("request-fresh", fade, move |action, _, cx| match action {
        SlotAction::ShowHeld => show_new(cx),
        SlotAction::Retry => acts(Act::Relist, cx),
        SlotAction::Reconnect => acts(Act::RenewToken, cx),
    });
    div().mr(px(8.)).child(element).into_any_element()
}

/// The hairline under the page's head, while a read or the merge takes long.
pub(crate) fn head_hairline(rq: &Requests, cx: &App) -> AnyElement {
    let at = Instant::now();
    let clock = rq.page.fresh.hair.get();
    let opacity = clock.opacity(at);
    div()
        .absolute()
        .left_0()
        .right_0()
        .bottom_0()
        .h(px(fresh::HAIRLINE_HEIGHT))
        .child(fresh::hairline("request-hairline", opacity, cx))
        .when(clock.active(at) && opacity < 1., |s| s.child(frames()))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::held::tests::mr;
    use crate::page::card_order;

    fn thread(id: &str, note: u64, login: &str) -> Thread {
        Thread {
            id: id.into(),
            notes: vec![Note {
                id: note,
                author: login.into(),
                login: login.into(),
                body: "?".into(),
                created: 0,
                edited: false,
            }],
            path: None,
            new_line: None,
            old_line: None,
            resolved: false,
            resolvable: true,
            resolved_by: None,
            resolved_at: 0,
            commit: None,
            outdated: false,
        }
    }

    /// A page of request 1, read once, with one thread.
    fn page() -> Requests {
        let mut rq = Requests {
            list: vec![mr(1, 1, 10)],
            current: Some(1),
            login: Some("me".into()),
            ..Default::default()
        };
        rq.page_opened(1);
        rq.page_reads();
        rq.base.threads = vec![thread("a", 1, "ann")];
        rq.threads = rq.base.threads.clone();
        rq.page_read(&[]);
        rq
    }

    #[test]
    fn the_pages_slot_says_what_exists() {
        let mut rq = page();
        assert!(matches!(rq.page_slot(), Slot::Updated(_)));
        rq.page_reads();
        assert_eq!(rq.page_slot(), Slot::Refreshing);
        rq.page.kept = true;
        assert!(matches!(rq.page_slot(), Slot::Kept(_)), "a switch back");
        rq.page_failed("Can't reach gitlab.example");
        assert!(matches!(rq.page_slot(), Slot::CantReach(_)));
        assert_eq!(rq.base.threads.len(), 1, "the page stays");
        rq.page.merged_by = Some("Ember Lind".into());
        assert_eq!(rq.page_slot(), Slot::MergedBy("Ember Lind".into()));
        rq.page_opened(2);
        rq.page_reads();
        assert_eq!(rq.page_slot(), Slot::Reading, "nothing read of it yet");
    }

    #[test]
    fn a_new_comment_is_appended_and_the_page_held_on_the_first_thread_on_screen() {
        let mut rq = page();
        let before = rq.base.threads.clone();
        {
            let mut anchor = rq.page.anchor.borrow_mut();
            anchor.drawn("a", 300., 0.);
        }
        rq.page.scroll.set_offset(Point::new(px(0.), px(-100.)));
        rq.page_reads();
        rq.base.threads = vec![
            thread("new", 3, "kes"),
            thread("a", 1, "ann"),
            thread("mine", 4, "me"),
        ];
        rq.threads = rq.base.threads.clone();
        rq.page_read(&before);
        assert!(matches!(
            rq.page_slot(),
            Slot::Waiting(_, 1, Held::NewBelow)
        ));
        // The new one goes to the card's end, whatever the host's order.
        let (shown, new) = card_order(&rq.threads, &rq.page.new_notes);
        assert_eq!(
            shown.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
            ["a", "mine"]
        );
        assert_eq!(new.len(), 1);
        // The held thread drawn 40 px lower: the page scrolls 40 px with it, once.
        let mut anchor = rq.page.anchor.borrow_mut();
        let offset = f32::from(rq.page.scroll.offset().y);
        let y = anchor.drawn("a", 340., offset).expect("moved");
        assert_eq!(y, offset - 40.);
        assert_eq!(anchor.drawn("a", 380., y), None);
    }

    #[test]
    fn the_first_thread_on_screen_is_the_one_held() {
        let tops = HashMap::from([("a".into(), 100.), ("b".into(), 400.), ("c".into(), 700.)]);
        assert_eq!(first_visible(&tops, -200.), Some(("b".into(), 400.)));
        assert_eq!(first_visible(&tops, -900.), None);
        assert_eq!(anchored(-10., 400., 300.), 0., "never past the top");
    }

    #[test]
    fn a_merge_changes_the_state_and_leaves_the_threads_and_facts() {
        let mut rq = page();
        rq.base.approvals = Some(Default::default());
        rq.pipeline = Some(artoo_hosts_api::Pipeline {
            status: "success".into(),
            url: String::new(),
            updated: 0,
            failed: Vec::new(),
        });
        let (threads, approvals, pipeline) = (
            rq.base.threads.clone(),
            rq.base.approvals.clone(),
            rq.pipeline.clone(),
        );
        rq.page_reads();
        rq.merged_here_now(1);
        assert_eq!(rq.list[0].state, MrState::Merged);
        assert!(rq.merged_on_page());
        assert_eq!(rq.base.threads, threads);
        assert_eq!(rq.base.approvals, approvals);
        assert_eq!(rq.pipeline, pipeline);
        assert!(matches!(rq.page_slot(), Slot::Updated(_)));
        assert_eq!(rq.merged_here, [1]);
    }

    #[test]
    fn a_kept_page_comes_back_where_it_was_scrolled() {
        let mut rq = page();
        rq.page.side_in = true;
        rq.page.scroll.set_offset(Point::new(px(0.), px(-240.)));
        rq.forget_open();
        rq.current = Some(2);
        rq.page_opened(2);
        assert_eq!(rq.page.scroll.offset().y, px(0.));
        rq.forget_open();
        rq.current = Some(1);
        rq.page_opened(1);
        assert!(rq.restore(1));
        assert_eq!(rq.page.scroll.offset().y, px(-240.));
        assert!(rq.page.kept && rq.page.side_in);
        rq.page_reads();
        assert!(matches!(rq.page_slot(), Slot::Kept(_)), "from HH:MM");
    }
}
