//! The open list in every state of round 41 (Design `docs/specs/loading-stability.md`, `requests.html?s=…`): the
//! slot and the hairline in the head, the skeleton drawn as the real rows and preview with tints, and the two panes
//! that say a list could not be read or that nothing is in it.

use crate::list::{BAND, ListLook};
use crate::requests::{Act, Acts, Requests, Scope};
use artoo_hosts_api::{Mr, MrState};
use artoo_ui_kit::fresh::{self, HairlineClock, SkeletonPhase, SlotAction, SlotFade};
use artoo_ui_kit::{ROW_RADIUS, motion, px, theme, words};
use gpui::{AnyElement, App, ClickEvent, Div, IntoElement, canvas, div, prelude::*};
use std::cell::{Cell, RefCell};
use std::time::Instant;

/// What the head and the list keep between frames.
#[derive(Default)]
pub struct Freshness {
    pub(crate) fade: RefCell<Option<SlotFade>>,
    pub(crate) hair: Cell<HairlineClock>,
    /// The first read with nothing to show: when it began, when it answered.
    first: Cell<Option<(Instant, Option<Instant>)>>,
}

impl Freshness {
    /// A read begins; `nothing` when there is no list to show under it.
    pub(crate) fn begin(&self, nothing: bool, at: Instant) {
        let mut hair = self.hair.get();
        hair.begin(at);
        self.hair.set(hair);
        // A read asked again while the first one runs goes on with its tints.
        let running = matches!(self.first.get(), Some((_, None)));
        if !(nothing && running) {
            self.first.set(nothing.then_some((at, None)));
        }
    }

    /// The read answered, or failed.
    pub(crate) fn end(&self, at: Instant) {
        let mut hair = self.hair.get();
        hair.end(at);
        self.hair.set(hair);
        if let Some((started, None)) = self.first.get() {
            self.first.set(Some((started, Some(at))));
        }
    }

    /// How much of the skeleton and of the content show at `at`.
    pub(crate) fn phase(&self, at: Instant) -> SkeletonPhase {
        match self.first.get() {
            Some((started, ready)) => fresh::skeleton_phase(started, ready, at),
            None => SkeletonPhase {
                skeleton: 0.,
                content: 1.,
            },
        }
    }

    /// Whether the skeleton's timing still moves by itself: waiting to show, held, or fading.
    pub(crate) fn skeleton_moves(&self, at: Instant) -> bool {
        match self.first.get() {
            Some((started, None)) => at < started + motion::SKELETON_AFTER,
            Some((_, Some(_))) => self.phase(at).content < 1.,
            None => false,
        }
    }
}

/// What stands where the list's rows are.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stage {
    /// No answer yet: tints (or, for a moment, nothing).
    Loading,
    /// Nothing kept and the read failed.
    Failed,
    /// An answer with nothing of this tab's.
    Empty,
    List,
}

/// The stage of the open list; `none_shown` when the scope and filter leave no row. Never `Empty` before an answer.
pub fn stage(rq: &Requests, none_shown: bool) -> Stage {
    if rq.list.is_empty() && rq.listed_at.is_none() {
        return if rq.error.is_some() && !rq.loading {
            Stage::Failed
        } else {
            Stage::Loading
        };
    }
    if none_shown {
        Stage::Empty
    } else {
        Stage::List
    }
}

/// Asks for the next frame while something timed is on its way.
pub(crate) fn frames() -> impl IntoElement {
    canvas(
        |_, window, _| window.request_animation_frame(),
        |_, _, _, _| {},
    )
    .absolute()
    .size_0()
}

/// The slot at the head's right: its text per `Requests::slot`, its action done through `acts` (a held change's
/// through `apply`).
pub(crate) fn head_slot(
    rq: &Requests,
    acts: Acts,
    apply: impl Fn(&mut App) + 'static,
) -> AnyElement {
    let at = Instant::now();
    let slot = rq.slot();
    let mut kept = rq.fresh.fade.borrow_mut();
    let fade = kept.get_or_insert_with(|| SlotFade::new(slot.clone()));
    fade.set(slot, at);
    let element = fresh::slot("requests-fresh", fade, move |action, _, cx| match action {
        SlotAction::ShowHeld => apply(cx),
        SlotAction::Retry => acts(Act::Relist, cx),
        SlotAction::Reconnect => acts(Act::RenewToken, cx),
    });
    div().mr(px(8.)).child(element).into_any_element()
}

/// The hairline under the head, while a read takes long.
pub(crate) fn head_hairline(rq: &Requests, cx: &App) -> AnyElement {
    let at = Instant::now();
    let clock = rq.fresh.hair.get();
    let opacity = clock.opacity(at);
    div()
        .absolute()
        .left_0()
        .right_0()
        .bottom_0()
        .h(px(fresh::HAIRLINE_HEIGHT))
        .child(fresh::hairline("requests-hairline", opacity, cx))
        .when(clock.active(at) && opacity < 1., |s| s.child(frames()))
        .into_any_element()
}

/// A message centred in a pane: what is the case, why (when it changes what to do), and the way on.
fn message(title: String, why: Option<String>, action: Option<AnyElement>) -> Div {
    div()
        .flex_1()
        .size_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(8.))
        .px(px(24.))
        .text_size(px(13.))
        .text_color(theme::muted())
        .child(div().text_center().child(title))
        .children(why.map(|why| {
            div()
                .text_center()
                .text_size(px(11.))
                .text_color(theme::faint())
                .child(why)
        }))
        .children(action)
}

/// The preview pane with nothing picked.
pub(crate) fn unselected() -> AnyElement {
    message("No request selected".to_owned(), None, None).into_any_element()
}

/// Why the list could not be read, in the failed pane's words.
fn why(rq: &Requests) -> String {
    if rq.refused {
        return "Token expired".to_owned();
    }
    let error = rq.error.as_deref().unwrap_or_default();
    error.trim_end_matches('.').replace(" — ", " · ")
}

/// The list pane when nothing is kept and the read failed.
pub(crate) fn failed(rq: &Requests, acts: &Acts) -> Div {
    let acts = acts.clone();
    let retry = div()
        .id("requests-failed-retry")
        .cursor_pointer()
        .text_color(theme::accent())
        .child("Retry ⌘R")
        .on_click(move |_: &ClickEvent, _, cx| acts(Act::Relist, cx));
    message(
        "The list could not be read".to_owned(),
        Some(why(rq)),
        Some(retry.into_any_element()),
    )
}

/// The list pane when an answer holds nothing of this tab's.
pub(crate) fn nothing(scope: Scope) -> Div {
    let (title, why) = match scope {
        Scope::ReviewMe => (
            "Nothing needs your review".to_owned(),
            Some("No open request is waiting for you".to_owned()),
        ),
        Scope::Mine => ("None of yours is open".to_owned(), None),
        Scope::All => (words::say("No open merge requests").into_owned(), None),
    };
    message(title, why, None)
}

/// A tint of `width` standing for a line of text `size` tall, centred in a box of the line's height.
fn tint(width: f32, height: f32, line: f32) -> Div {
    div()
        .h(px(line))
        .flex_none()
        .flex()
        .items_center()
        .child(fresh::tint(px(width).into(), height))
}

/// A group's band as tints.
fn skeleton_band(name: f32, top: bool) -> Div {
    div()
        .h(px(BAND))
        .flex_none()
        .flex()
        .items_center()
        .gap(px(8.))
        .pl(px(16.))
        .pr(px(10.))
        .when(!top, |s| s.border_t_1())
        .border_b_1()
        .border_color(theme::island_border())
        .child(fresh::tint(px(14.).into(), 14.))
        .child(fresh::tint(px(name).into(), 8.))
        .child(fresh::tint(px(8.).into(), 8.))
}

/// A row as tints, in the real row's boxes: the dot's room, the number and title, the time; the author, the
/// discussions and the pipeline.
fn skeleton_row(title: f32, author: f32, threads: bool) -> Div {
    let line = 20.;
    div().px(px(6.)).py(px(1.)).child(
        div()
            .px(px(10.))
            .py(px(8.))
            .flex()
            .items_start()
            .gap(px(10.))
            .rounded(px(ROW_RADIUS))
            .child(div().flex_none().size(px(7.)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(tint(28., 8., line))
                            .child(tint(title, 8., line))
                            .child(div().flex_1())
                            .child(tint(20., 8., line)),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .h(px(16.))
                            .child(tint(author, 8., 16.))
                            .child(div().flex_1())
                            .when(threads, |s| s.child(tint(8., 8., 16.)))
                            .child(fresh::tint(px(14.).into(), 14.).rounded_full()),
                    ),
            ),
    )
}

/// A skeleton row's title and author widths, and whether it has discussions.
type SkeletonRow = (f32, f32, bool);

/// The list pane's skeleton: four groups of rows.
pub(crate) fn skeleton_list(look: &ListLook) -> Div {
    let groups: [(f32, &[SkeletonRow]); 4] = [
        (
            92.,
            &[(228., 76., false), (192., 66., true), (180., 55., true)],
        ),
        (136., &[(186., 66., true), (174., 76., false)]),
        (116., &[(150., 55., false)]),
        (28., &[(198., 18., true)]),
    ];
    let mut pane = div()
        .relative()
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .overflow_hidden()
        .child(look.widths.measure("requests"));
    for (n, (name, rows)) in groups.into_iter().enumerate() {
        pane = pane.child(skeleton_band(name, n == 0));
        for &(title, author, threads) in rows {
            pane = pane.child(skeleton_row(title, author, threads));
        }
    }
    pane
}

/// The preview pane's skeleton: head, title, description, branches, the Open button and its chips.
pub(crate) fn skeleton_preview() -> Div {
    let pill = |w: f32, h: f32| fresh::tint(px(w).into(), h).rounded(px(ROW_RADIUS));
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
                .gap(px(10.))
                .child(tint(24., 8., 22.))
                .child(pill(50., 22.))
                .child(tint(18., 8., 22.))
                .child(fresh::tint(px(16.).into(), 16.).rounded_full())
                .child(tint(72., 8., 22.)),
        )
        .child(tint(264., 12., 26.))
        .child(
            div()
                .flex()
                .flex_col()
                .child(tint(390., 8., 20.))
                .child(tint(300., 8., 20.)),
        )
        .child(tint(190., 8., 16.))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(pill(66., 28.))
                .child(pill(42., 28.))
                .child(pill(42., 28.))
                .child(pill(54., 28.)),
        )
}

impl Requests {
    /// The open list as a state of `requests.html?s=` for a snapshot: what a host's answer would have left, with no
    /// host. Needs a list read before it (a fixture's); the reads under way are dropped.
    pub(crate) fn force_fresh(&mut self, state: &str) {
        let now = Instant::now();
        // Long enough ago that the hairline and the skeleton have come.
        let long = now - std::time::Duration::from_secs(2);
        self.task = None;
        self.loading = false;
        self.error = None;
        self.refused = false;
        self.from_disk = false;
        self.held = None;
        self.fresh = Freshness::default();
        let time = self.listed_at.unwrap_or_else(artoo_ui_kit::format::now);
        self.listed_at = Some(time);
        let fresh_one = |list: &[Mr]| {
            let mut m = list.first()?.clone();
            m.iid = 418;
            m.title = "Cache refund quotes for ten minutes".to_owned();
            m.author = "Ember Lind".to_owned();
            m.updated = artoo_ui_kit::format::now() - 120;
            Some(m)
        };
        match state {
            "loading" => {
                self.list.clear();
                self.listed_at = None;
                self.loading = true;
                self.fresh.begin(true, long);
            }
            "kept" => self.from_disk = true,
            "refreshing" => {
                self.loading = true;
                self.fresh.begin(false, long);
            }
            "new" => {
                let Some(new) = fresh_one(&self.list) else {
                    return;
                };
                let mut list = self.list.clone();
                list.insert(0, new);
                self.held = Some(crate::held::Held {
                    new: vec![418],
                    list,
                    at: Some(fresh::Held::NewAbove),
                    ..Default::default()
                });
            }
            "applied" => {
                if let Some(new) = fresh_one(&self.list) {
                    self.list.insert(0, new);
                }
            }
            "empty" => self.list.clear(),
            "error" | "failed" => {
                self.error =
                    Some("Can't reach gitlab.payments.example — is the VPN on?".to_owned());
                if state == "failed" {
                    self.list.clear();
                    self.listed_at = None;
                }
            }
            // `loaded`, and anything else.
            _ => {}
        }
    }
}

/// A merged or closed list has its own words and reads (#114): none of this.
pub(crate) fn open_view(rq: &Requests) -> bool {
    rq.view == MrState::Opened
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn the_empty_state_waits_for_an_answer() {
        let mut rq = Requests::default();
        // Nothing asked, asking, asking again after a failure: never empty.
        assert_eq!(stage(&rq, true), Stage::Loading);
        rq.loading = true;
        assert_eq!(stage(&rq, true), Stage::Loading);
        rq.loading = false;
        rq.error = Some("Can't reach gitlab.example — is the VPN on?".into());
        assert_eq!(stage(&rq, true), Stage::Failed);
        assert_eq!(why(&rq), "Can't reach gitlab.example · is the VPN on?");
        rq.loading = true;
        assert_eq!(stage(&rq, true), Stage::Loading);
        // An answer with nothing: empty, and it stays so when a later refresh fails.
        rq.loading = false;
        rq.listed_at = Some(1);
        assert_eq!(stage(&rq, true), Stage::Empty);
        assert_eq!(stage(&rq, false), Stage::List);
    }

    #[test]
    fn a_quick_answer_draws_no_skeleton_and_a_slow_one_is_held() {
        let fresh = Freshness::default();
        let start = Instant::now();
        let ms = |n| start + Duration::from_millis(n);
        fresh.begin(true, start);
        assert_eq!(fresh.phase(ms(100)).skeleton, 0.);
        fresh.end(ms(100));
        assert_eq!(fresh.phase(ms(101)).content, 1.);
        fresh.begin(true, start);
        assert_eq!(fresh.phase(ms(300)).skeleton, 1.);
        fresh.end(ms(300));
        assert_eq!(fresh.phase(ms(400)).content, 0.);
        assert!(fresh.skeleton_moves(ms(400)));
        assert_eq!(fresh.phase(ms(800)).content, 1.);
        // Asked again while it runs: the tints stay.
        fresh.begin(true, start);
        fresh.begin(true, ms(300));
        assert_eq!(fresh.phase(ms(350)).skeleton, 1.);
        fresh.end(ms(360));
        // A read over a list shown is no first read.
        fresh.begin(false, ms(900));
        assert_eq!(fresh.phase(ms(901)).content, 1.);
    }
}
