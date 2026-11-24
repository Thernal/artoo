//! Toasts stacked at the top right, newest at the bottom, read top to bottom (#198): who raised it and what happened
//! (`Jira · Assigned to you`), the item it is about under it, then its actions — its button, `⌥↵` on the newest one
//! with a button, and `⋯` for what its kind offers —; ✕ in the corner, and a bar for the time left, held while the
//! pointer is on the toast. Errors, news, and news with a button. What a button does is the tab's: an act of its own
//! kind `A` (what an agent left offers to go and see it, #150), handed back to the runner the tab gave, or a closure
//! (Undo of a deleted draft, #76).

use crate::hover::{LONG, TipOwned};
use crate::{hover, icons, keys, motion, px, theme};
use gpui::{
    AnyElement, App, ClickEvent, Context, Pixels, Point, Render, Rgba, SharedString, Task, Window,
    div, prelude::*, relative,
};
use std::rc::Rc;
use std::time::{Duration, Instant};

/// How long a toast stays while the pointer is not on it.
const SHOWN: Duration = Duration::from_secs(6);
/// The most toasts on screen; an older one goes when another comes.
const MOST: usize = 4;
/// Every toast's width, and its mark's slot.
const WIDTH: f32 = 400.;
const MARK: f32 = 24.;

/// What a toast's mark says: the tone is the glyph, never a stripe (round 40).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Tone {
    #[default]
    Info,
    Success,
    Warning,
    Error,
}

impl Tone {
    fn glyph(self) -> &'static str {
        match self {
            Tone::Info => "info",
            Tone::Success => "check",
            Tone::Warning => "alert",
            Tone::Error => "circle-x",
        }
    }
}

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
    /// A chat's toast (a chat began listening): its face with a live dot as the mark, what happened with its
    /// first letters bold as the title, the branch as the body.
    pub flat: Option<Flat>,
    /// The tone's glyph in the mark; none: news is `Info`, an error `Error` (what `show` was told).
    pub tone: Option<Tone>,
    /// A source's monogram in the mark, its letter on a hue of its own; none: the source's first letter, or the
    /// tone's glyph when there is no source.
    pub mark: Option<(char, Rgba)>,
    /// A glyph in a badge on the mark's corner: a person is the cause (`user`).
    pub badge: Option<&'static str>,
    /// A live dot on the mark: a chat that is listening.
    pub live: bool,
    /// What the ⋯'s tooltip says: the kind's sentence (`Turn off toasts for Jira · Assigned to you`).
    pub more_tip: Option<SharedString>,
}

/// What a flat toast adds to its words.
#[derive(Clone)]
pub struct Flat {
    /// The chat's face: its agent's letter on a hue of its own.
    pub mark: (char, Rgba),
    /// How many bytes of the text, from its start, are the chat's name, in bold.
    pub bold: usize,
    /// The branch the chat works on, after the words.
    pub branch: SharedString,
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

    pub fn flat(self, flat: Flat) -> Self {
        Self {
            flat: Some(flat),
            ..self
        }
    }

    pub fn tone(self, tone: Tone) -> Self {
        Self {
            tone: Some(tone),
            ..self
        }
    }

    pub fn mark(self, letter: char, hue: Rgba) -> Self {
        Self {
            mark: Some((letter, hue)),
            ..self
        }
    }

    pub fn badge(self, glyph: &'static str) -> Self {
        Self {
            badge: Some(glyph),
            ..self
        }
    }

    pub fn live(self) -> Self {
        Self { live: true, ..self }
    }

    pub fn more_tip(self, tip: impl Into<SharedString>) -> Self {
        Self {
            more_tip: Some(tip.into()),
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
    /// A second button after the first (Show, then Stop listening): an act of the tab's.
    second: Option<A>,
    /// The mark's glyph.
    tone: Tone,
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

/// Whether button `b` undoes: its words, or its act's (`label`), start with Undo.
fn undoes<A>(b: &Button<A>, label: fn(&A) -> &'static str) -> bool {
    match b {
        Button::Run(words, _) => words.starts_with("Undo"),
        Button::Act(act, _) => label(act).starts_with("Undo"),
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

    /// News with a button doing `act` and a second one doing `second`, about `count` things.
    pub fn show_acts(
        &mut self,
        words: impl Into<Words>,
        act: A,
        second: A,
        count: usize,
        cx: &mut Context<Self>,
    ) {
        self.push(words.into(), Some(Button::Act(act, count)), true, cx);
        if let Some(t) = self.list.last_mut() {
            t.second = Some(second);
        }
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
        let tone = words
            .tone
            .unwrap_or(if info { Tone::Info } else { Tone::Error });
        let words = match &words.flat {
            // A chat that is listening: its face and a live dot.
            Some(flat) => Words {
                mark: Some(flat.mark),
                live: true,
                ..words
            },
            None => words,
        };
        let id = self.next;
        self.next += 1;
        self.list.push(Toast {
            id,
            words,
            button,
            second: None,
            tone,
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

    /// The second button of toast `id`: its act done, and the toast goes.
    fn press_second(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(act) = self
            .list
            .iter()
            .find(|t| t.id == id)
            .and_then(|t| t.second.clone())
        else {
            return;
        };
        let run = self.run.clone();
        self.dismiss(id, cx);
        cx.defer(move |cx| run(act, cx));
    }

    /// The newest toast with a button still showing: what `⌥↵` presses.
    fn newest_button(&self) -> Option<u64> {
        (self.list.iter().rev())
            .find(|t| !t.leaving && t.button.is_some())
            .map(|t| t.id)
    }

    /// The newest toast with an Undo button still showing: what `⌘Z` presses (#191) — the app's own, or one whose
    /// button is an action named Undo (a plugin's toast, raised through the frame).
    fn newest_undo(&self) -> Option<u64> {
        let label = self.label;
        (self.list.iter().rev())
            .find(|t| !t.leaving && t.button.as_ref().is_some_and(|b| undoes(b, label)))
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

    /// `⌘Z`: the newest toast whose button undoes (`Undo …`), pressed; false when none shows (#191).
    pub fn press_undo(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(id) = self.newest_undo() else {
            return false;
        };
        self.press(id, cx);
        true
    }

    /// `esc`: the newest toast still showing goes; false when none shows.
    pub fn dismiss_newest(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(id) = self.list.iter().rev().find(|t| !t.leaving).map(|t| t.id) else {
            return false;
        };
        self.dismiss(id, cx);
        true
    }

    /// It slides out, then it is gone.
    fn dismiss(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(toast) = self.list.iter_mut().find(|t| t.id == id && !t.leaving) else {
            return;
        };
        toast.leaving = true;
        toast._timer = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(motion::BASE).await;
            this.update(cx, |this, cx| {
                this.list.retain(|t| t.id != id);
                cx.notify();
            })
            .ok();
        });
        cx.notify();
    }
}

/// In from the right as it fades in, and out the same way (#56, 8).
fn arriving(toast: gpui::Stateful<gpui::Div>, id: u64, leaving: bool) -> AnyElement {
    motion::toward(
        toast,
        ("toast-motion", id as usize),
        !leaving,
        true,
        motion::BASE,
        |toast, v| toast.relative().left(px(24. * (1. - v))).opacity(v),
    )
    .into_any_element()
}

/// The mark's ink for a tone: the glyph's colour, never a stripe at the edge.
fn tone_ink(tone: Tone) -> Rgba {
    match tone {
        Tone::Info => theme::accent(),
        Tone::Success => theme::added(),
        Tone::Warning => theme::warning(),
        Tone::Error => theme::removed(),
    }
}

/// The 24 px slot every toast has at its left: a source's monogram (with a badge when a person is the cause, or a
/// live dot for a chat that is listening), or the tone's glyph.
fn mark_slot(words: &Words, tone: Tone) -> gpui::Div {
    let (ink, inside) = match (&words.mark, &words.source) {
        (Some((letter, hue)), _) => (*hue, Some(letter.to_string())),
        (None, Some(source)) => (
            theme::muted(),
            Some(
                source
                    .chars()
                    .next()
                    .unwrap_or('?')
                    .to_uppercase()
                    .to_string(),
            ),
        ),
        (None, None) => (tone_ink(tone), None),
    };
    let slot = div()
        .relative()
        .flex_none()
        .size(px(MARK))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.))
        .bg(Rgba { a: 0.22, ..ink })
        .text_color(ink)
        .text_size(px(12.))
        .font_weight(gpui::FontWeight::SEMIBOLD);
    let slot = match inside {
        Some(letter) => slot.child(letter),
        None => slot.child(icons::icon(tone.glyph()).size(px(14.)).text_color(ink)),
    };
    slot.children(words.badge.map(|glyph| {
        div()
            .absolute()
            .right(px(-5.))
            .bottom(px(-5.))
            .size(px(13.))
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .bg(theme::base())
            .border_1()
            .border_color(theme::island_border())
            .child(icons::icon(glyph).size(px(9.)).text_color(theme::muted()))
    }))
    .children(words.live.then(|| {
        div()
            .absolute()
            .right(px(-3.))
            .bottom(px(-3.))
            .size(px(8.))
            .rounded_full()
            .border_2()
            .border_color(theme::panel())
            .bg(theme::added())
    }))
}

/// A toast's actions in the order they are drawn: the secondary first, the primary last.
fn in_order<T>(secondary: Option<T>, primary: Option<T>) -> Vec<T> {
    secondary.into_iter().chain(primary).collect()
}

/// Whether the toast's one action sits on the title line: no body and one action. Else the actions are on its last line.
fn on_title_line(has_body: bool, has_second: bool, has_button: bool) -> bool {
    !has_body && !has_second && has_button
}

/// A toast's button: the primary one filled, the secondary one outlined; its key at its right.
fn action_button(
    id: gpui::ElementId,
    label: &'static str,
    key: Option<&'static str>,
    primary: bool,
) -> gpui::Stateful<gpui::Div> {
    let button = div()
        .id(id)
        .flex_none()
        .h(px(24.))
        .px(px(10.))
        .flex()
        .items_center()
        .gap(px(6.))
        .rounded(px(6.))
        .text_size(px(12.))
        .font_weight(gpui::FontWeight::MEDIUM)
        .cursor_pointer()
        .child(label);
    let button = if primary {
        button
            .bg(theme::accent())
            .text_color(theme::on_accent())
            .hover(|s| s.opacity(0.9))
    } else {
        button
            .border_1()
            .border_color(theme::border())
            .text_color(theme::text())
            .hover(|s| s.bg(theme::hover()))
    };
    button.children(key.map(|key| {
        keys::caps(
            key,
            if primary {
                keys::On::Filled
            } else {
                keys::On::Plain
            },
        )
    }))
}

/// A 24 px square control (⋯, ✕) with its tooltip.
fn control(
    id: gpui::ElementId,
    glyph: &'static str,
    tip: SharedString,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .flex_none()
        .size(px(24.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.))
        .cursor_pointer()
        .hover(|s| s.bg(theme::hover()))
        .child(icons::icon(glyph).size(px(14.)).text_color(theme::muted()))
        .tooltip_show_delay(LONG)
        .tooltip(move |_, cx| cx.new(|_| TipOwned(tip.clone())).into())
}

impl<A: Clone + PartialEq + 'static> Render for Toasts<A> {
    /// The stack, over everything else, under the page bar's right end, clear of the right stripe (#198).
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let label = self.label;
        let (newest, undo) = (self.newest_button(), self.newest_undo());
        // The bars move while any toast's time runs.
        if !artoo_core::storage::snapshot_run()
            && self.list.iter().any(|t| t.since.is_some() && !t.leaving)
        {
            window.request_animation_frame();
        }
        let toasts = self.list.iter().map(|t| {
            let id = t.id;
            let button = match &t.button {
                Some(Button::Act(act, _)) => Some(label(act)),
                Some(Button::Run(label, _)) => Some(*label),
                None => None,
            };
            let second = t.second.as_ref().map(label);
            let words = &t.words;
            let more = words.more.clone();
            let left = t.left_now().as_secs_f32() / SHOWN.as_secs_f32();
            // A flat chat toast is the one layout too: its words, the branch as its body.
            let (title, body) = match &words.flat {
                Some(flat) => (
                    keys::text(&words.text).into_owned(),
                    (!flat.branch.is_empty()).then(|| flat.branch.clone()),
                ),
                None => (keys::text(&words.text).into_owned(), words.about.clone()),
            };
            // `Jira · Assigned to you`, its source bold; a chat's name is bold too.
            let (said, bold_len) = match (&words.source, &words.flat) {
                (Some(source), _) => (format!("{source} · {title}"), source.len()),
                (None, Some(flat)) => {
                    let bold = flat.bold.min(title.len());
                    (title, bold)
                }
                (None, None) => (title, 0),
            };
            let bold = gpui::HighlightStyle {
                font_weight: Some(gpui::FontWeight::SEMIBOLD),
                ..Default::default()
            };
            let title = hover::cut_with(
                ("toast-title", id as usize),
                said.clone(),
                gpui::StyledText::new(said)
                    .with_highlights((bold_len > 0).then_some((0..bold_len, bold))),
            )
            .flex_1()
            .text_color(theme::text());
            // Its key: ⌘Z on the newest Undo (#191), else ⌥↵ on the newest button.
            let key = if undo == Some(id) {
                Some("⌘Z")
            } else {
                (newest == Some(id)).then_some("⌥↵")
            };
            // One action and no body: it sits on the title line; else on the last line, the secondary first.
            let inline = on_title_line(body.is_some(), second.is_some(), button.is_some());
            let primary = button.map(|label| {
                action_button(("toast-button", id as usize).into(), label, key, true)
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.press(id, cx)))
            });
            let secondary = second.map(|label| {
                action_button(("toast-second", id as usize).into(), label, None, false).on_click(
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.press_second(id, cx)),
                )
            });
            let (on_title, on_foot) = if inline {
                (primary, None)
            } else {
                (None, primary)
            };
            let more_tip = words.more_tip.clone().unwrap_or_else(|| "More".into());
            let head =
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .min_h(px(24.))
                    .child(title)
                    .children(on_title)
                    .child(
                        div()
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap(px(2.))
                            .children(more.map(|more| {
                                control(("toast-more", id as usize).into(), "ellipsis", more_tip)
                                    .on_click(move |event: &ClickEvent, window, cx| {
                                        more(event.position(), window, cx)
                                    })
                            }))
                            .child(
                                control(
                                    ("toast-close", id as usize).into(),
                                    "x",
                                    "Dismiss  esc".into(),
                                )
                                .on_click(cx.listener(
                                    move |this, _: &ClickEvent, _, cx| this.dismiss(id, cx),
                                )),
                            ),
                    );
            let toast = div()
                .id(("toast", id as usize))
                .occlude()
                .relative()
                .overflow_hidden()
                .w(px(WIDTH))
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
                .child(mark_slot(words, t.tone))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .child(head)
                        .children(body.map(|body| {
                            hover::cut_to(("toast-body", id as usize), body, 2)
                                .text_size(px(12.))
                                .line_height(px(17.))
                                .text_color(theme::muted())
                        }))
                        .when(secondary.is_some() || on_foot.is_some(), |d| {
                            d.child(div().flex().justify_end().gap(px(8.)).mt(px(4.)).children(
                                in_order(
                                    secondary.map(IntoElement::into_any_element),
                                    on_foot.map(IntoElement::into_any_element),
                                ),
                            ))
                        }),
                )
                // The time left: 2 px along the foot, held while the pointer is on the toast.
                .child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .bottom_0()
                        .h(px(2.))
                        .child(div().h_full().w(relative(left.clamp(0., 1.))).bg(Rgba {
                            a: 0.6,
                            ..theme::faint()
                        })),
                );
            arriving(toast, id, t.leaving)
        });
        gpui::deferred(
            div()
                .absolute()
                .top(px(96.))
                .right(px(55.))
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
    fn every_toast_is_one_width_and_the_secondary_action_comes_first() {
        assert_eq!(WIDTH, 400.);
        assert_eq!(
            in_order(Some("Stop listening"), Some("Show")),
            ["Stop listening", "Show"]
        );
        assert_eq!(in_order(None, Some("Open")), ["Open"]);
        // No body and one action: on the title line; a body or a second action: on the last line.
        assert!(on_title_line(false, false, true));
        assert!(!on_title_line(true, false, true));
        assert!(!on_title_line(false, true, true));
        assert!(!on_title_line(false, false, false));
    }

    #[test]
    fn the_tone_is_told_by_the_call_and_esc_dismisses_the_newest() {
        gpui::TestAppContext::single().update(|cx| {
            let toasts = toasts(cx);
            toasts.update(cx, |t, cx| {
                t.show("Saved", true, cx);
                t.show("Disk is full", false, cx);
                t.show(Words::new("Resolved").tone(Tone::Success), true, cx);
                let tones: Vec<_> = t.list.iter().map(|t| t.tone).collect();
                assert_eq!(tones, [Tone::Info, Tone::Error, Tone::Success]);
                assert!(t.dismiss_newest(cx));
                assert!(t.list[2].leaving && !t.list[1].leaving);
                assert!(t.dismiss_newest(cx));
                assert!(t.list[1].leaving);
            });
        });
    }

    #[test]
    fn a_chat_that_is_listening_has_its_face_and_a_live_dot() {
        gpui::TestAppContext::single().update(|cx| {
            let toasts = toasts(cx);
            toasts.update(cx, |t, cx| {
                let flat = Flat {
                    mark: ('X', theme::accent()),
                    bold: 5,
                    branch: "feature/ledger-cache".into(),
                };
                t.show(Words::new("Codex started listening").flat(flat), true, cx);
                assert!(t.list[0].words.live && t.list[0].words.mark.is_some());
            });
        });
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
    fn cmd_z_presses_an_undo_that_is_an_act_too() {
        gpui::TestAppContext::single().update(|cx| {
            // A plugin's toast reaches the stack as an act whose label is its button's words.
            let toasts = cx.new(|_| {
                Toasts::new(
                    |a: &u8| if *a == 1 { "Undo" } else { "Open" },
                    Rc::new(|_, _| {}),
                )
            });
            toasts.update(cx, |t, cx| {
                t.show_act("Assigned to Ember", 2, 1, cx);
                assert!(t.newest_undo().is_none());
                t.show_act("Reassigned to Ember", 1, 1, cx);
                let undo = t.newest_undo();
                assert_eq!(undo, Some(t.list[1].id));
                assert!(t.press_undo(cx));
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
