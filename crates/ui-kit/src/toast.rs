//! Toasts stacked at the top right, newest at the bottom, each with ✕ and gone after a few seconds: errors, news,
//! and news with a button. What a button does is the tab's: an act of its own kind `A` (what an agent left offers
//! to go and see it, #150), handed back to the runner the tab gave, or a closure (Undo of a deleted draft, #76).

use crate::hover::{LONG, Tip};
use crate::{icons, keys, motion, px, theme};
use gpui::{App, ClickEvent, Context, Render, SharedString, Task, Window, div, prelude::*};
use std::rc::Rc;
use std::time::Duration;

/// How long a toast stays.
const SHOWN: Duration = Duration::from_secs(6);
/// The most toasts on screen; an older one goes when another comes.
const MOST: usize = 4;

/// A toast's button.
enum Button<A> {
    /// An act of the tab's, and what it adds up: news of the same act still showing joins this (#150).
    Act(A, usize),
    /// Its label and what it does.
    Run(&'static str, Rc<dyn Fn(&mut App)>),
}

struct Toast<A> {
    id: u64,
    text: SharedString,
    button: Option<Button<A>>,
    /// News, not an error: no alert mark (#77).
    info: bool,
    /// Dismissed: drawn on its way out until it is removed (#56, 8).
    leaving: bool,
    /// Dismisses it when `SHOWN` is up; dropping it (✕, the same text again) cancels that.
    _timer: Task<()>,
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

    /// Shows `text` (`info`: news, else an error); the same text already showing starts over at the bottom instead.
    pub fn show(&mut self, text: SharedString, info: bool, cx: &mut Context<Self>) {
        self.push(text, None, info, cx);
    }

    /// News with a button `label` doing `run`.
    pub fn show_with(
        &mut self,
        text: SharedString,
        label: &'static str,
        run: Rc<dyn Fn(&mut App)>,
        cx: &mut Context<Self>,
    ) {
        self.push(text, Some(Button::Run(label, run)), true, cx);
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
    pub fn show_act(&mut self, text: SharedString, act: A, count: usize, cx: &mut Context<Self>) {
        self.push(text, Some(Button::Act(act, count)), true, cx);
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
            .filter(|t| t.text.as_ref() == text)
            .map(|t| t.id)
            .collect();
        for id in ids {
            self.dismiss(id, cx);
        }
    }

    fn push(
        &mut self,
        text: SharedString,
        button: Option<Button<A>>,
        info: bool,
        cx: &mut Context<Self>,
    ) {
        // Errors go in the app's log, for a report to carry (#118).
        if !info {
            artoo_core::applog::line(&text);
        }
        self.list.retain(|t| t.text != text);
        let id = self.next;
        self.next += 1;
        let timer = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SHOWN).await;
            this.update(cx, |this, cx| this.dismiss(id, cx)).ok();
        });
        self.list.push(Toast {
            id,
            text,
            button,
            info,
            leaving: false,
            _timer: timer,
        });
        let over = self.list.len().saturating_sub(MOST);
        self.list.drain(..over);
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

    /// The newest toast with an act's button, pressed (the snapshot checks).
    pub fn press_newest(&mut self, cx: &mut Context<Self>) {
        let newest = self
            .list
            .iter()
            .rev()
            .find(|t| matches!(t.button, Some(Button::Act(..))));
        if let Some(id) = newest.map(|t| t.id) {
            self.press(id, cx);
        }
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
    /// The stack, over everything else, under the title bar's right end.
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let label = self.label;
        let toasts = self.list.iter().map(|t| {
            let id = t.id;
            let leaving = t.leaving;
            let button = match &t.button {
                Some(Button::Act(act, _)) => Some(label(act)),
                Some(Button::Run(label, _)) => Some(*label),
                None => None,
            };
            let info = t.info;
            let toast = div()
                .id(("toast", id as usize))
                .occlude()
                .w(px(360.))
                .flex()
                .items_start()
                .gap(px(10.))
                .pl(px(12.))
                .pr(px(6.))
                .py(px(10.))
                .rounded(px(10.))
                .border_1()
                .border_color(theme::island_border())
                .bg(theme::panel())
                .shadow_lg()
                .text_size(px(13.))
                .line_height(px(19.))
                .children((!info).then(|| {
                    icons::icon("alert")
                        .size(px(16.))
                        .mt(px(2.))
                        .text_color(theme::removed())
                }))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_color(theme::text())
                        .child(keys::text(&t.text).into_owned()),
                )
                .children(button.map(|label| {
                    div()
                        .id(("toast-button", id as usize))
                        .flex_none()
                        .px(px(6.))
                        .rounded(px(6.))
                        .text_color(theme::accent())
                        .cursor_pointer()
                        .hover(|s| s.bg(theme::hover()))
                        .child(label)
                        .on_click(
                            cx.listener(move |this, _: &ClickEvent, _, cx| this.press(id, cx)),
                        )
                }))
                .child(
                    div()
                        .id(("toast-close", id as usize))
                        .flex_none()
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
                .top(px(52.))
                .right(px(12.))
                .flex()
                .flex_col()
                .gap(px(8.))
                .children(toasts),
        )
        .with_priority(2)
    }
}
