//! Toasts stacked at the top right, newest at the bottom, read top to bottom (#198): who raised it and what happened
//! (`Jira · Assigned to you`), the item it is about under it, then its actions — its button, `⌥↵` on the newest one
//! with a button, and `⋯` for what its kind offers —; ✕ in the corner, and a bar for the time left, held while the
//! pointer is on the toast. Errors, news, and news with a button. What a button does is the tab's: an act of its own
//! kind `A` (what an agent left offers to go and see it, #150), handed back to the runner the tab gave, or a closure
//! (Undo of a deleted draft, #76).

use crate::hover::{LONG, Tip};
use crate::{hover, icons, keys, motion, px, theme};
use gpui::{
    App, ClickEvent, Context, Pixels, Point, Render, SharedString, Task, Window, div, prelude::*,
    relative,
};
use std::rc::Rc;
use std::time::{Duration, Instant};

/// How long a toast stays while the pointer is not on it.
const SHOWN: Duration = Duration::from_secs(6);
/// The most toasts on screen; an older one goes when another comes.
const MOST: usize = 4;

/// What `⋯` on a toast opens, at the click: the tab's menu of what the toast's kind offers.
pub type More = Rc<dyn Fn(Point<Pixels>, &mut Window, &mut App)>;

/// What a toast says, top to bottom (#198).
#[derive(Clone, Default)]
pub struct Words {
    /// Who raised it (`Jira`, `GitLab`, `Agents`), first and bold; none for what you just did, or an error.
    pub source: Option<SharedString>,
    /// What happened: `Assigned to you`.
    pub text: SharedString,
    /// What it is about, under it: `PAY-412 Refund rounds down on partial captures`.
    pub about: Option<SharedString>,
    /// `⋯`: what its kind offers — its toasts turned off, Settings.
    pub more: Option<More>,
}

impl Words {
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self {
            text: text.into(),
            ..Self::default()
        }
    }

    pub fn source(self, source: impl Into<SharedString>) -> Self {
        Self {
            source: Some(source.into()),
            ..self
        }
    }

    pub fn about(self, about: impl Into<SharedString>) -> Self {
        Self {
            about: Some(about.into()),
            ..self
        }
    }

    pub fn more(self, more: More) -> Self {
        Self {
            more: Some(more),
            ..self
        }
    }

    /// The same news: the same source saying the same.
    fn same(&self, other: &Words) -> bool {
        self.source == other.source && self.text == other.text && self.about == other.about
    }
}

impl From<SharedString> for Words {
    fn from(text: SharedString) -> Self {
        Self::new(text)
    }
}

impl From<String> for Words {
    fn from(text: String) -> Self {
        Self::new(text)
    }
}

impl From<&'static str> for Words {
    fn from(text: &'static str) -> Self {
        Self::new(text)
    }
}

/// A toast's button.
enum Button<A> {
    /// An act of the tab's, and what it adds up: news of the same act still showing joins this (#150).
    Act(A, usize),
    /// Its label and what it does.
    Run(&'static str, Rc<dyn Fn(&mut App)>),
}

struct Toast<A> {
    id: u64,
    words: Words,
    button: Option<Button<A>>,
    /// News, not an error: no alert mark (#77).
    info: bool,
    /// Dismissed: drawn on its way out until it is removed (#56, 8).
    leaving: bool,
    /// The time it has left, counted from `since`; held (the pointer on it), `since` is none.
    left: Duration,
    since: Option<Instant>,
    /// Dismisses it when its time is up; dropping it (held, ✕, the same news again) cancels that.
    _timer: Task<()>,
}

impl<A> Toast<A> {
    /// The time it has left now.
    fn left_now(&self) -> Duration {
        match self.since {
            // A snapshot's bar does not move between its draws.
            Some(_) if artoo_core::storage::snapshot_run() => self.left,
            Some(since) => self.left.saturating_sub(since.elapsed()),
            None => self.left,
        }
    }
}

/// What the tab does when a button with act `A` is pressed.
pub type OnAct<A> = Rc<dyn Fn(A, &mut App)>;

/// One tab's toasts; `A` is what their buttons do, run by the tab.
pub struct Toasts<A> {
    list: Vec<Toast<A>>,
    next: u64,
    /// An act's button's label.
    label: fn(&A) -> &'static str,
    /// An act's button pressed.
    run: OnAct<A>,
}

impl<A: Clone + PartialEq + 'static> Toasts<A> {
    pub fn new(label: fn(&A) -> &'static str, run: OnAct<A>) -> Self {
        Self {
            list: Vec::new(),
            next: 0,
            label,
            run,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// Shows `words` (`info`: news, else an error); the same news already showing starts over at the bottom instead.
    pub fn show(&mut self, words: impl Into<Words>, info: bool, cx: &mut Context<Self>) {
        self.push(words.into(), None, info, cx);
    }

    /// News with a button `label` doing `run`.
    pub fn show_with(
        &mut self,
        words: impl Into<Words>,
        label: &'static str,
        run: Rc<dyn Fn(&mut App)>,
        cx: &mut Context<Self>,
    ) {
        self.push(words.into(), Some(Button::Run(label, run)), true, cx);
    }

    /// The count of the toasts doing `act` still showing, taken: they go, a toast of `act` about `count` more
    /// coming in their place (`show_act`).
    pub fn take_count(&mut self, act: &A) -> usize {
        let before = self
            .list
            .iter()
            .filter(|t| !t.leaving)
            .filter_map(|t| match &t.button {
                Some(Button::Act(a, n)) if a == act => Some(n),
                _ => None,
            })
            .sum();
        self.list
            .retain(|t| !matches!(&t.button, Some(Button::Act(a, _)) if a == act));
        before
    }

    /// News with a button doing `act`, about `count` things.
    pub fn show_act(
        &mut self,
        words: impl Into<Words>,
        act: A,
        count: usize,
        cx: &mut Context<Self>,
    ) {
        self.push(words.into(), Some(Button::Act(act, count)), true, cx);
    }

    /// The toasts whose button does what `over` picks go now: news another has taken in.
    pub fn drop_acts(&mut self, over: impl Fn(&A) -> bool, cx: &mut Context<Self>) {
        let ids: Vec<u64> = self
            .list
            .iter()
            .filter(|t| matches!(&t.button, Some(Button::Act(a, _)) if over(a)))
            .map(|t| t.id)
            .collect();
        for id in ids {
            self.dismiss(id, cx);
        }
    }

    /// The news of `text` is over: its toast goes now.
    pub fn drop_text(&mut self, text: &str, cx: &mut Context<Self>) {
        let ids: Vec<u64> = self
            .list
            .iter()
            .filter(|t| t.words.text.as_ref() == text)
            .map(|t| t.id)
            .collect();
        for id in ids {
            self.dismiss(id, cx);
        }
    }

    fn push(
        &mut self,
        words: Words,
        button: Option<Button<A>>,
        info: bool,
        cx: &mut Context<Self>,
    ) {
        // Errors go in the app's log, for a report to carry (#118).
        if !info {
            artoo_core::applog::line(&words.text);
        }
        self.list.retain(|t| !t.words.same(&words));
        let id = self.next;
        self.next += 1;
        self.list.push(Toast {
            id,
            words,
            button,
            info,
            leaving: false,
            left: SHOWN,
            since: Some(Instant::now()),
            _timer: Self::timer(id, SHOWN, cx),
        });
        let over = self.list.len().saturating_sub(MOST);
        self.list.drain(..over);
        cx.notify();
    }

    /// Dismisses toast `id` once `left` is up.
    fn timer(id: u64, left: Duration, cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(left).await;
            this.update(cx, |this, cx| this.dismiss(id, cx)).ok();
        })
    }

    /// The pointer came onto toast `id` (`held`) or left it: its time stops, or goes on from where it stopped.
    fn hold(&mut self, id: u64, held: bool, cx: &mut Context<Self>) {
        let Some(toast) = self.list.iter_mut().find(|t| t.id == id && !t.leaving) else {
            return;
        };
        match (held, toast.since) {
            (true, Some(_)) => {
                toast.left = toast.left_now();
                toast.since = None;
                toast._timer = Task::ready(());
            }
            (false, None) => {
                toast.since = Some(Instant::now());
                toast._timer = Self::timer(id, toast.left, cx);
            }
            _ => return,
        }
        cx.notify();
    }

    /// The button of toast `id`: what it says to do, done, and the toast goes.
    fn press(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(toast) = self.list.iter_mut().find(|t| t.id == id) else {
            return;
        };
        let run: Box<dyn FnOnce(&mut App)> = match &toast.button {
            Some(Button::Act(act, _)) => {
                let (act, run) = (act.clone(), self.run.clone());
                Box::new(move |cx| run(act, cx))
            }
            Some(Button::Run(_, run)) => {
                let run = run.clone();
                Box::new(move |cx| run(cx))
            }
            None => return,
        };
        // A closure's button does it once.
        if matches!(toast.button, Some(Button::Run(..))) {
            toast.button = None;
        }
        self.dismiss(id, cx);
        // What it does may reach the tab, whose update may be under way.
        cx.defer(run);
    }

    /// The newest toast with a button still showing: what `⌥↵` presses.
    fn newest_button(&self) -> Option<u64> {
        (self.list.iter().rev())
            .find(|t| !t.leaving && t.button.is_some())
            .map(|t| t.id)
    }

    /// `⌥↵`: the newest toast's button, pressed; false when no toast has one.
    pub fn press_newest(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(id) = self.newest_button() else {
            return false;
        };
        self.press(id, cx);
        true
    }

    /// It slides out, then it is gone.
    fn dismiss(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(toast) = self.list.iter_mut().find(|t| t.id == id && !t.leaving) else {
            return;
        };
        toast.leaving = true;
        toast._timer = cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(motion::BASE + Duration::from_millis(40))
                .await;
            this.update(cx, |this, cx| {
                this.list.retain(|t| t.id != id);
                cx.notify();
            })
            .ok();
        });
        cx.notify();
    }
}

impl<A: Clone + PartialEq + 'static> Render for Toasts<A> {
    /// The stack, over everything else, under the page bar's right end, clear of the right stripe (#198).
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let label = self.label;
        let newest = self.newest_button();
        // The bars move while any toast's time runs.
        if !artoo_core::storage::snapshot_run()
            && self.list.iter().any(|t| t.since.is_some() && !t.leaving)
        {
            window.request_animation_frame();
        }
        let toasts = self.list.iter().map(|t| {
            let id = t.id;
            let leaving = t.leaving;
            let button = match &t.button {
                Some(Button::Act(act, _)) => Some(label(act)),
                Some(Button::Run(label, _)) => Some(*label),
                None => None,
            };
            let more = t.words.more.clone();
            let left = t.left_now().as_secs_f32() / SHOWN.as_secs_f32();
            // `Jira · Assigned to you`, its source bold, as one text that wraps.
            let text = keys::text(&t.words.text).into_owned();
            let (said, bold_len) = match &t.words.source {
                Some(source) => (format!("{source} · {text}"), source.len()),
                None => (text, 0),
            };
            let bold = gpui::HighlightStyle {
                font_weight: Some(gpui::FontWeight::SEMIBOLD),
                ..Default::default()
            };
            let what = div().text_color(theme::text()).child(
                gpui::StyledText::new(said)
                    .with_highlights((bold_len > 0).then_some((0..bold_len, bold))),
            );
            let actions = (button.is_some() || more.is_some()).then(|| {
                div()
                    .mt(px(6.))
                    .ml(px(-6.))
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .children(button.map(|label| {
                        div()
                            .id(("toast-button", id as usize))
                            .flex_none()
                            .flex()
                            .items_baseline()
                            .gap(px(4.))
                            .px(px(6.))
                            .py(px(1.))
                            .rounded(px(6.))
                            .text_size(px(12.))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(theme::accent())
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::hover()))
                            .child(label)
                            .children((newest == Some(id)).then(|| {
                                div()
                                    .text_size(px(11.))
                                    .font_weight(gpui::FontWeight::NORMAL)
                                    .text_color(theme::faint())
                                    .child(keys::text("⌥↵").into_owned())
                            }))
                            .on_click(
                                cx.listener(move |this, _: &ClickEvent, _, cx| this.press(id, cx)),
                            )
                    }))
                    .children(more.map(|more| {
                        div()
                            .id(("toast-more", id as usize))
                            .flex_none()
                            .size(px(24.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(6.))
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::hover()))
                            .child(
                                icons::icon("ellipsis")
                                    .size(px(14.))
                                    .text_color(theme::muted()),
                            )
                            .tooltip_show_delay(LONG)
                            .tooltip(|_, cx| cx.new(|_| Tip("More")).into())
                            .on_click(move |event: &ClickEvent, window, cx| {
                                more(event.position(), window, cx)
                            })
                    }))
            });
            let toast = div()
                .id(("toast", id as usize))
                .occlude()
                .relative()
                .overflow_hidden()
                .w(px(400.))
                .flex()
                .items_start()
                .gap(px(10.))
                .pl(px(12.))
                .pr(px(8.))
                .pt(px(10.))
                .pb(px(12.))
                .rounded(px(10.))
                .border_1()
                .border_color(theme::island_border())
                .bg(theme::panel())
                .shadow_lg()
                .text_size(px(13.))
                .line_height(px(19.))
                .on_hover(cx.listener(move |this, held: &bool, _, cx| this.hold(id, *held, cx)))
                .children((!t.info).then(|| {
                    icons::icon("alert")
                        .size(px(16.))
                        .mt(px(2.))
                        .text_color(theme::removed())
                }))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .child(what)
                        .children(t.words.about.clone().map(|about| {
                            hover::cut_to(("toast-about", id as usize), about, 2)
                                .mt(px(2.))
                                .text_size(px(12.))
                                .line_height(px(17.))
                                .text_color(theme::muted())
                        }))
                        .children(actions),
                )
                .child(
                    div()
                        .id(("toast-close", id as usize))
                        .flex_none()
                        .mt(px(-3.))
                        .size(px(24.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(6.))
                        .cursor_pointer()
                        .hover(|s| s.bg(theme::hover()))
                        .child(icons::icon("x").size(px(14.)).text_color(theme::muted()))
                        .tooltip_show_delay(LONG)
                        .tooltip(|_, cx| cx.new(|_| Tip("Dismiss")).into())
                        .on_click(
                            cx.listener(move |this, _: &ClickEvent, _, cx| this.dismiss(id, cx)),
                        ),
                )
                // The time left: it stops while the pointer is on the toast.
                .child(
                    div()
                        .absolute()
                        .left(px(12.))
                        .right(px(12.))
                        .bottom(px(4.))
                        .h(px(2.))
                        .rounded_full()
                        .overflow_hidden()
                        .bg(theme::hover())
                        .child(
                            div()
                                .h_full()
                                .w(relative(left.clamp(0., 1.)))
                                .bg(theme::accent()),
                        ),
                );
            // In from the right as it fades in, and out the same way (#56, 8).
            motion::toward(
                toast,
                ("toast-motion", id as usize),
                !leaving,
                true,
                motion::BASE,
                |toast, v| toast.relative().left(px(24. * (1. - v))).opacity(v),
            )
        });
        gpui::deferred(
            div()
                .absolute()
                .top(px(92.))
                .right(px(56.))
                .flex()
                .flex_col()
                .gap(px(8.))
                .children(toasts),
        )
        .with_priority(2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toasts(cx: &mut App) -> gpui::Entity<Toasts<u8>> {
        cx.new(|_| Toasts::new(|_| "Open", Rc::new(|_, _| {})))
    }

    #[test]
    fn a_toast_pointed_at_keeps_the_time_it_had() {
        gpui::TestAppContext::single().update(|cx| {
            let toasts = toasts(cx);
            toasts.update(cx, |t, cx| {
                t.show("Assigned to you", true, cx);
                let id = t.list[0].id;
                t.hold(id, true, cx);
                let held = t.list[0].left;
                assert!(t.list[0].since.is_none());
                assert!(held <= SHOWN);
                // Held, its time does not run.
                assert_eq!(t.list[0].left_now(), held);
                t.hold(id, false, cx);
                assert!(t.list[0].since.is_some());
                assert_eq!(t.list[0].left, held);
            });
        });
    }

    #[test]
    fn alt_enter_presses_the_newest_toast_with_a_button() {
        gpui::TestAppContext::single().update(|cx| {
            let toasts = toasts(cx);
            toasts.update(cx, |t, cx| {
                assert!(!t.press_newest(cx));
                t.show_act(
                    Words::new("Claude points at Money.kt").source("Agents"),
                    1,
                    1,
                    cx,
                );
                t.show_act("Ember Lind mentioned you", 2, 1, cx);
                t.show("Copied", true, cx);
                assert_eq!(t.newest_button(), Some(t.list[1].id));
                assert!(t.press_newest(cx));
                // Pressed, it goes: the one before it is the newest now.
                assert_eq!(t.newest_button(), Some(t.list[0].id));
            });
        });
    }

    #[test]
    fn the_same_news_again_replaces_the_one_showing() {
        gpui::TestAppContext::single().update(|cx| {
            let toasts = toasts(cx);
            toasts.update(cx, |t, cx| {
                t.show(Words::new("Assigned to you").source("Jira"), true, cx);
                t.show(Words::new("Assigned to you").source("Linear"), true, cx);
                t.show(Words::new("Assigned to you").source("Jira"), true, cx);
                let sources: Vec<_> = t.list.iter().map(|t| t.words.source.clone()).collect();
                assert_eq!(sources, [Some("Linear".into()), Some("Jira".into())]);
            });
        });
    }
}
