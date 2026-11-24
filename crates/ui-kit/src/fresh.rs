//! Loading without a jump (round 41; Design `docs/specs/loading-stability.md`): the three shared pieces
//! every screen rests on. The **freshness slot** says in one fixed box how current a list or page is; the
//! **hairline** under a head shows a read that takes long; the **still skeleton** stands where content will
//! be. Their delays live in `motion` (one place to tune); here are the pure clocks, which take the time as
//! an argument, and the elements that draw them. Nothing here waits for a read: the screen sets the state,
//! these draw it.

use crate::{motion, px, theme};
use gpui::{
    Animation, AnimationExt, App, ElementId, InteractiveElement, IntoElement, ParentElement,
    SharedString, StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder,
};
use std::time::{Duration, Instant};

/// The slot's least width: its widest text (`Can't reach · from 09:43 ↻`) fits, so a state change never
/// moves what is beside it.
pub const SLOT_MIN_WIDTH: f32 = 176.;
/// The hairline's height, reserved under a head always (the head is 42 px with it).
pub const HAIRLINE_HEIGHT: f32 = 2.;
/// The head's height with the hairline's room.
pub const HEAD_HEIGHT: f32 = 42.;

/// What the held change is: a request that came in above, one that moved above, one that came in below.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Held {
    NewAbove,
    MovedAbove,
    NewBelow,
}

/// What the slot says. The times are the clock text the screen formats (`09:41`).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Slot {
    /// A kept copy is on screen, nothing asked yet: `from 09:41`.
    Kept(String),
    /// The first read, nothing to show: `Reading…`.
    Reading,
    /// A read under a kept copy: `Refreshing…`.
    Refreshing,
    /// The copy is current: `Updated 09:43`.
    Updated(String),
    /// Changes wait to be shown: `Updated 09:43 · 1 new ↑`; the count is an action.
    Waiting(String, usize, Held),
    /// The host does not answer: `Can't reach · from 09:43 ↻`.
    CantReach(String),
    /// `Token expired · Reconnect`.
    TokenExpired,
    /// Nothing kept, the read failed: `Can't read the list ↻`.
    CantRead,
}

/// What a click on the slot asks the screen for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SlotAction {
    /// Show the held changes.
    ShowHeld,
    /// Read again.
    Retry,
    /// Sign in to the host again.
    Reconnect,
}

/// How a piece of the slot's text is inked.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ink {
    Grey,
    /// The count of held changes: blue, the action.
    Blue,
    /// A failure: amber (`--attention`).
    Amber,
}

impl Slot {
    /// The text in pieces, each with its ink; joined they are the slot's line.
    pub fn parts(&self) -> Vec<(String, Ink)> {
        match self {
            Slot::Kept(at) => vec![(format!("from {at}"), Ink::Grey)],
            Slot::Reading => vec![("Reading…".into(), Ink::Grey)],
            Slot::Refreshing => vec![("Refreshing…".into(), Ink::Grey)],
            Slot::Updated(at) => vec![(format!("Updated {at}"), Ink::Grey)],
            Slot::Waiting(at, n, held) => {
                let (what, arrow) = match held {
                    Held::NewAbove => ("new", "↑"),
                    Held::MovedAbove => ("moved", "↑"),
                    Held::NewBelow => ("new", "↓"),
                };
                vec![
                    (format!("Updated {at} · "), Ink::Grey),
                    (format!("{n} {what} {arrow}"), Ink::Blue),
                ]
            }
            Slot::CantReach(at) => vec![(format!("Can't reach · from {at} ↻"), Ink::Amber)],
            Slot::TokenExpired => vec![("Token expired · Reconnect".into(), Ink::Amber)],
            Slot::CantRead => vec![("Can't read the list ↻".into(), Ink::Amber)],
        }
    }

    /// The slot's line.
    pub fn text(&self) -> String {
        self.parts().into_iter().map(|(text, _)| text).collect()
    }

    /// What a click returns, if the state has an action.
    pub fn action(&self) -> Option<SlotAction> {
        match self {
            Slot::Waiting(..) => Some(SlotAction::ShowHeld),
            Slot::CantReach(_) | Slot::CantRead => Some(SlotAction::Retry),
            Slot::TokenExpired => Some(SlotAction::Reconnect),
            Slot::Kept(_) | Slot::Reading | Slot::Refreshing | Slot::Updated(_) => None,
        }
    }

    /// Whether the slot reports a failure.
    pub fn failed(&self) -> bool {
        matches!(
            self,
            Slot::CantReach(_) | Slot::TokenExpired | Slot::CantRead
        )
    }
}

/// The slot's state kept between frames: when it changes, the old text stays under the new one for
/// `SLOT_FADE`, so the two cross-fade in the same box.
#[derive(Clone, Debug)]
pub struct SlotFade {
    now: Slot,
    before: Option<Slot>,
    changed: Option<Instant>,
    generation: u64,
}

impl SlotFade {
    pub fn new(slot: Slot) -> Self {
        Self {
            now: slot,
            before: None,
            changed: None,
            generation: 0,
        }
    }

    /// Sets the state; a different one starts a cross-fade at `at`.
    pub fn set(&mut self, slot: Slot, at: Instant) {
        if slot != self.now {
            self.before = Some(std::mem::replace(&mut self.now, slot));
            self.changed = Some(at);
            self.generation += 1;
        }
    }

    pub fn current(&self) -> &Slot {
        &self.now
    }

    /// The state fading out, while it still shows.
    pub fn leaving(&self, at: Instant) -> Option<&Slot> {
        let changed = self.changed?;
        (at.saturating_duration_since(changed) < motion::SLOT_FADE)
            .then_some(self.before.as_ref())
            .flatten()
    }

    /// How much of the new text shows, 0…1.
    pub fn arriving(&self, at: Instant) -> f32 {
        match self.changed {
            Some(changed) => fade_in(at.saturating_duration_since(changed), motion::SLOT_FADE),
            None => 1.,
        }
    }

    /// Changes with each new state: a key for the animation that plays it.
    pub fn generation(&self) -> u64 {
        self.generation
    }
}

fn fade_in(elapsed: Duration, over: Duration) -> f32 {
    (elapsed.as_secs_f32() / over.as_secs_f32()).clamp(0., 1.)
}

/// One read's clock for the hairline: nothing before `HAIRLINE_AFTER`, a fade in and out over
/// `HAIRLINE_FADE`; a read that ends sooner than that never draws it.
#[derive(Clone, Copy, Debug, Default)]
pub struct HairlineClock {
    started: Option<Instant>,
    ended: Option<Instant>,
}

impl HairlineClock {
    /// A read begins. One already running goes on from its own start.
    pub fn begin(&mut self, at: Instant) {
        if self.started.is_none() || self.ended.is_some() {
            self.started = Some(at);
            self.ended = None;
        }
    }

    /// The read is over.
    pub fn end(&mut self, at: Instant) {
        if self.started.is_some() && self.ended.is_none() {
            self.ended = Some(at);
        }
    }

    /// The hairline's opacity (0…1) at `at`.
    pub fn opacity(&self, at: Instant) -> f32 {
        let Some(started) = self.started else {
            return 0.;
        };
        let shown = started + motion::HAIRLINE_AFTER;
        let level = |t: Instant| fade_in(t.saturating_duration_since(shown), motion::HAIRLINE_FADE);
        match self.ended {
            None => level(at),
            Some(ended) if ended <= shown => 0.,
            Some(ended) => {
                let left = at.saturating_duration_since(ended);
                level(ended) * (1. - fade_in(left, motion::HAIRLINE_FADE))
            }
        }
    }

    /// Whether the hairline still needs drawing (a read runs, or it is fading out).
    pub fn active(&self, at: Instant) -> bool {
        match self.ended {
            None => self.started.is_some(),
            Some(ended) => at.saturating_duration_since(ended) < motion::HAIRLINE_FADE,
        }
    }
}

/// How much of the skeleton and of the content show.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct SkeletonPhase {
    pub skeleton: f32,
    pub content: f32,
}

/// The skeleton's timing, pure: a read began at `started`; the content arrived at `ready` (`None` while
/// reading). The skeleton shows only after `SKELETON_AFTER`; once shown it stays `SKELETON_HOLD` at least;
/// then it fades into the content over `SKELETON_FADE`. Content that came before the skeleton showed
/// appears at once, with no skeleton at all.
pub fn skeleton_phase(started: Instant, ready: Option<Instant>, at: Instant) -> SkeletonPhase {
    let shown = started + motion::SKELETON_AFTER;
    let reading = SkeletonPhase {
        skeleton: if at >= shown { 1. } else { 0. },
        content: 0.,
    };
    let Some(ready) = ready else {
        return reading;
    };
    if ready <= shown {
        return SkeletonPhase {
            skeleton: 0.,
            content: 1.,
        };
    }
    let handover = ready.max(shown + motion::SKELETON_HOLD);
    if at < handover {
        return reading;
    }
    let t = fade_in(
        at.saturating_duration_since(handover),
        motion::SKELETON_FADE,
    );
    SkeletonPhase {
        skeleton: 1. - t,
        content: t,
    }
}

/// The freshness slot: one box of at least `SLOT_MIN_WIDTH`, caption size, one line, in the head. A click
/// hands the state's action to `on_action`; a state with none is only text.
pub fn slot(
    id: impl Into<ElementId>,
    fade: &SlotFade,
    on_action: impl Fn(SlotAction, &mut Window, &mut App) + 'static,
) -> gpui::AnyElement {
    let id: ElementId = id.into();
    let at = Instant::now();
    let action = fade.current().action();
    let key = fade.generation();
    let line = |slot: &Slot| {
        div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .flex()
            .items_center()
            .justify_end()
            .whitespace_nowrap()
            .children(slot.parts().into_iter().map(|(text, ink)| {
                div()
                    .text_color(ink_color(ink))
                    .child(SharedString::from(text))
            }))
    };
    let mut layers = div()
        .relative()
        .min_w(px(SLOT_MIN_WIDTH))
        .h(px(18.))
        .flex_none()
        .text_size(px(11.));
    if let Some(leaving) = fade.leaving(at) {
        layers = layers.child(motion::appear(
            line(leaving),
            (id.clone(), format!("out-{key}")),
            motion::SLOT_FADE,
            |el, t| el.opacity(1. - t),
        ));
    }
    let incoming = line(fade.current());
    let incoming = if fade.leaving(at).is_some() {
        motion::appear(
            incoming,
            (id.clone(), format!("in-{key}")),
            motion::SLOT_FADE,
            |el, t| el.opacity(t),
        )
        .into_any_element()
    } else {
        incoming.into_any_element()
    };
    layers = layers.child(incoming);
    let clickable = div()
        .id(id)
        .flex_none()
        .child(layers)
        .when_some(action, |el, action| {
            el.cursor_pointer()
                .on_click(move |_, window, cx| on_action(action, window, cx))
        });
    clickable.into_any_element()
}

fn ink_color(ink: Ink) -> gpui::Rgba {
    match ink {
        Ink::Grey => theme::faint(),
        Ink::Blue => theme::accent(),
        Ink::Amber => theme::warning(),
    }
}

/// The hairline's room under a head: `HAIRLINE_HEIGHT` tall, inset 8 px each side, the segment sliding
/// across at `opacity` (from `HairlineClock::opacity`). Put it as the last child of a `relative` head. With
/// reduce motion it is a still 40 % bar.
pub fn hairline(id: impl Into<ElementId>, opacity: f32, cx: &App) -> gpui::AnyElement {
    let room = div()
        .absolute()
        .left(px(8.))
        .right(px(8.))
        .bottom_0()
        .h(px(HAIRLINE_HEIGHT))
        .overflow_hidden();
    if opacity <= 0. {
        return room.into_any_element();
    }
    let bar = |left: f32, width: f32| {
        div()
            .absolute()
            .top_0()
            .bottom_0()
            .left(gpui::relative(left))
            .w(gpui::relative(width))
            .bg(theme::accent())
            .opacity(0.8 * opacity)
    };
    if cx.reduce_motion() {
        return room.child(bar(0., 0.4)).into_any_element();
    }
    room.child(bar(-0.3, 0.3).with_animation(
        id,
        Animation::new(motion::HAIRLINE_SWEEP).repeat(),
        move |bar, t| bar.left(gpui::relative(-0.3 + 1.3 * t)),
    ))
    .into_any_element()
}

/// A still tint where text will be: `width` and the text's height, never a pulse (`--background-04`).
pub fn tint(width: gpui::Length, height: f32) -> gpui::Div {
    div()
        .w(width)
        .max_w_full()
        .h(px(height))
        .rounded(px(3.))
        .bg(theme::hover())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: fn(u64) -> Duration = Duration::from_millis;

    #[test]
    fn the_slot_says_each_state_in_one_line() {
        let at = || "09:43".to_string();
        let said = [
            (Slot::Kept("09:41".into()), "from 09:41"),
            (Slot::Reading, "Reading…"),
            (Slot::Refreshing, "Refreshing…"),
            (Slot::Updated(at()), "Updated 09:43"),
            (
                Slot::Waiting(at(), 1, Held::NewAbove),
                "Updated 09:43 · 1 new ↑",
            ),
            (
                Slot::Waiting(at(), 2, Held::MovedAbove),
                "Updated 09:43 · 2 moved ↑",
            ),
            (
                Slot::Waiting(at(), 1, Held::NewBelow),
                "Updated 09:43 · 1 new ↓",
            ),
            (Slot::CantReach(at()), "Can't reach · from 09:43 ↻"),
            (Slot::TokenExpired, "Token expired · Reconnect"),
            (Slot::CantRead, "Can't read the list ↻"),
        ];
        for (slot, text) in said {
            assert_eq!(slot.text(), text);
            assert!(!slot.text().contains('\n'));
        }
    }

    #[test]
    fn the_count_is_blue_the_failures_amber_the_rest_grey() {
        let waiting = Slot::Waiting("09:43".into(), 3, Held::NewAbove).parts();
        assert_eq!(waiting[0].1, Ink::Grey);
        assert_eq!(waiting[1], ("3 new ↑".to_string(), Ink::Blue));
        for slot in [
            Slot::CantReach("09:43".into()),
            Slot::TokenExpired,
            Slot::CantRead,
        ] {
            assert!(slot.failed());
            assert!(slot.parts().iter().all(|(_, ink)| *ink == Ink::Amber));
        }
        for slot in [
            Slot::Reading,
            Slot::Refreshing,
            Slot::Updated("09:43".into()),
        ] {
            assert!(!slot.failed());
        }
    }

    #[test]
    fn a_click_returns_the_states_action() {
        assert_eq!(Slot::Reading.action(), None);
        assert_eq!(Slot::Updated("09:43".into()).action(), None);
        assert_eq!(
            Slot::Waiting("09:43".into(), 1, Held::NewBelow).action(),
            Some(SlotAction::ShowHeld)
        );
        assert_eq!(Slot::CantRead.action(), Some(SlotAction::Retry));
        assert_eq!(
            Slot::CantReach("09:43".into()).action(),
            Some(SlotAction::Retry)
        );
        assert_eq!(Slot::TokenExpired.action(), Some(SlotAction::Reconnect));
    }

    /// The box is fixed: the widest variant must fit it. At the caption size a character is about 6 px
    /// wide, a pessimistic estimate for the arrows included.
    #[test]
    fn the_widest_variant_fits_the_box() {
        let widest = [
            Slot::Kept("09:41".into()),
            Slot::Updated("09:43".into()),
            Slot::Waiting("09:43".into(), 12, Held::MovedAbove),
            Slot::CantReach("09:43".into()),
            Slot::TokenExpired,
            Slot::CantRead,
        ]
        .iter()
        .map(|slot| slot.text().chars().count())
        .max()
        .unwrap();
        assert!(widest as f32 * 6. <= SLOT_MIN_WIDTH, "{widest} characters");
    }

    #[test]
    fn a_changed_state_fades_in_the_same_box_in_150_ms() {
        let t0 = Instant::now();
        let mut fade = SlotFade::new(Slot::Refreshing);
        assert_eq!(fade.leaving(t0), None);
        assert_eq!(fade.arriving(t0), 1.);
        fade.set(Slot::Refreshing, t0);
        assert_eq!(fade.generation(), 0);
        fade.set(Slot::Updated("09:43".into()), t0);
        assert_eq!(fade.generation(), 1);
        assert_eq!(fade.leaving(t0 + MS(10)), Some(&Slot::Refreshing));
        assert_eq!(fade.arriving(t0), 0.);
        assert!((fade.arriving(t0 + MS(75)) - 0.5).abs() < 1e-4);
        assert_eq!(fade.leaving(t0 + MS(150)), None);
        assert_eq!(fade.arriving(t0 + MS(150)), 1.);
    }

    #[test]
    fn a_read_of_300_ms_draws_no_hairline() {
        let t0 = Instant::now();
        let mut clock = HairlineClock::default();
        clock.begin(t0);
        for ms in [0, 100, 300] {
            assert_eq!(clock.opacity(t0 + MS(ms)), 0.);
        }
        clock.end(t0 + MS(300));
        for ms in [301, 400, 600, 900] {
            assert_eq!(clock.opacity(t0 + MS(ms)), 0.);
        }
        assert!(!clock.active(t0 + MS(900)));
    }

    #[test]
    fn a_long_read_shows_the_hairline_after_400_ms_and_fades_it_out_in_200() {
        let t0 = Instant::now();
        let mut clock = HairlineClock::default();
        assert_eq!(clock.opacity(t0), 0.);
        clock.begin(t0);
        assert_eq!(clock.opacity(t0 + MS(400)), 0.);
        assert!((clock.opacity(t0 + MS(500)) - 0.5).abs() < 1e-4);
        assert_eq!(clock.opacity(t0 + MS(600)), 1.);
        assert_eq!(clock.opacity(t0 + MS(3000)), 1.);
        assert!(clock.active(t0 + MS(3000)));
        clock.end(t0 + MS(3000));
        assert_eq!(clock.opacity(t0 + MS(3000)), 1.);
        assert!((clock.opacity(t0 + MS(3100)) - 0.5).abs() < 1e-4);
        assert_eq!(clock.opacity(t0 + MS(3200)), 0.);
        assert!(clock.active(t0 + MS(3199)) && !clock.active(t0 + MS(3200)));
        // A read that ends midway through the fade-in fades out from where it was.
        let mut clock = HairlineClock::default();
        clock.begin(t0);
        clock.end(t0 + MS(500));
        assert!((clock.opacity(t0 + MS(500)) - 0.5).abs() < 1e-4);
        assert_eq!(clock.opacity(t0 + MS(700)), 0.);
    }

    #[test]
    fn a_read_begun_again_while_running_keeps_its_start() {
        let t0 = Instant::now();
        let mut clock = HairlineClock::default();
        clock.begin(t0);
        clock.begin(t0 + MS(300));
        assert!(clock.opacity(t0 + MS(600)) > 0.9);
        clock.end(t0 + MS(700));
        clock.begin(t0 + MS(2000));
        assert_eq!(clock.opacity(t0 + MS(2100)), 0.);
    }

    #[test]
    fn the_skeleton_shows_after_200_ms_and_not_before() {
        let t0 = Instant::now();
        let at = |ms| skeleton_phase(t0, None, t0 + MS(ms));
        assert_eq!(at(0).skeleton, 0.);
        assert_eq!(at(199).skeleton, 0.);
        assert_eq!(at(200).skeleton, 1.);
        assert_eq!(at(5000).content, 0.);
    }

    #[test]
    fn content_that_comes_before_the_skeleton_shows_at_once() {
        let t0 = Instant::now();
        for ms in [0, 150, 250, 1000] {
            let phase = skeleton_phase(t0, Some(t0 + MS(150)), t0 + MS(ms));
            if ms < 150 {
                assert_eq!(phase.skeleton, 0.);
            } else {
                assert_eq!(
                    phase,
                    SkeletonPhase {
                        skeleton: 0.,
                        content: 1.
                    }
                );
            }
        }
    }

    #[test]
    fn a_skeleton_that_showed_stays_400_ms_then_fades_in_150() {
        let t0 = Instant::now();
        // Content at 250 ms: the skeleton showed at 200, so it stays until 600.
        let at = |ms| skeleton_phase(t0, Some(t0 + MS(250)), t0 + MS(ms));
        assert_eq!(
            at(599),
            SkeletonPhase {
                skeleton: 1.,
                content: 0.
            }
        );
        assert_eq!(
            at(600),
            SkeletonPhase {
                skeleton: 1.,
                content: 0.
            }
        );
        let mid = at(675);
        assert!((mid.skeleton - 0.5).abs() < 1e-4 && (mid.content - 0.5).abs() < 1e-4);
        assert_eq!(
            at(750),
            SkeletonPhase {
                skeleton: 0.,
                content: 1.
            }
        );
        // Content long after: the hold is already served, it fades at once.
        let late = |ms| skeleton_phase(t0, Some(t0 + MS(2000)), t0 + MS(ms));
        assert_eq!(late(1999).skeleton, 1.);
        assert_eq!(
            late(2150),
            SkeletonPhase {
                skeleton: 0.,
                content: 1.
            }
        );
    }
}
