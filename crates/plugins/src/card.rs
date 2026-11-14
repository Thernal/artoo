//! The card at the start after a plugin ended the app (#178): which one it was and what it did, Turn it off, and
//! Report, its own repository's new issue filled in. One tab says it.

use crate::crash::{self, Crash};
use crate::load::State;
use crate::state::{self, Loaded};
use artoo_shell_api::{Frame, Overlay, Parts, Snap, Toast};
use artoo_ui_kit::controls::{outline_button, primary_button};
use artoo_ui_kit::{ISLAND_RADIUS, ROW_RADIUS, px, theme};
use gpui::{
    App, AppContext, ClickEvent, Context, FontWeight, IntoElement, KeyDownEvent, MouseButton,
    Render, Window, div, prelude::*,
};
use std::cell::Cell;
use std::rc::Rc;

/// What the card says.
#[derive(Clone)]
struct Shown {
    crash: Crash,
    name: String,
    version: String,
    repository: String,
    /// Off already: it ended two starts.
    off: bool,
}

/// The crash, in the words of the plugin as it was found this start.
fn shown(crash: Crash, cx: &App) -> Shown {
    let loaded = cx.global::<Loaded>();
    let plugin = loaded.plugins.iter().find(|p| p.id() == crash.id);
    let stamp = plugin.and_then(|p| p.stamp.as_ref());
    Shown {
        name: stamp.map_or_else(|| crash.id.clone(), |s| s.name.clone()),
        version: stamp.map(|s| s.version.clone()).unwrap_or_default(),
        repository: stamp.map(|s| s.repository.clone()).unwrap_or_default(),
        off: plugin.is_some_and(|p| p.state == State::Struck) || loaded.off.contains(&crash.id),
        crash,
    }
}

pub(crate) fn parts(frame: Rc<dyn Frame>, _: &mut Window, cx: &mut App) -> Parts {
    let open = Rc::new(Cell::new(false));
    let card = cx.new(|_| Card {
        frame,
        open: open.clone(),
        shown: None,
    });
    let (key, close, poll, shot) = (card.clone(), card.clone(), card.clone(), card.clone());
    let sample: Snap = Rc::new(move |_, cx| {
        let sample = Crash {
            id: "hello".to_owned(),
            what: "panic: panicked at src/lib.rs:41:9: index out of bounds: the len is 2 but the index is 7".to_owned(),
            frames: vec!["libhello.dylib+0x1f0a".to_owned()],
        };
        let mut shown = shown(sample, cx);
        shown.name = "Hello".to_owned();
        shown.version = "0.1.0".to_owned();
        shown.repository = "https://github.com/someone/artoo-hello".to_owned();
        shot.update(cx, |card, cx| card.show(shown, cx));
    });
    Parts {
        overlay: Some(Overlay {
            view: card.into(),
            open,
            key: Rc::new(move |event, _, cx| key.update(cx, |card, cx| card.key(event, cx))),
            close: Rc::new(move |cx| close.update(cx, |card, cx| card.hide(cx))),
            poll: Some(Rc::new(move |cx| {
                if let Some(crash) = cx.global_mut::<Loaded>().crash.take() {
                    let said = shown(crash, cx);
                    poll.update(cx, |card, cx| card.show(said, cx));
                }
            })),
        }),
        snapshots: vec![("plugin-crash", sample)],
        ..Default::default()
    }
}

struct Card {
    frame: Rc<dyn Frame>,
    open: Rc<Cell<bool>>,
    shown: Option<Shown>,
}

impl Card {
    fn show(&mut self, shown: Shown, cx: &mut Context<Self>) {
        self.shown = Some(shown);
        self.open.set(true);
        cx.notify();
    }

    fn hide(&mut self, cx: &mut Context<Self>) {
        self.open.set(false);
        cx.notify();
    }

    fn hide_here(&mut self, cx: &mut Context<Self>) {
        self.hide(cx);
        self.frame.redraw(cx);
    }

    fn key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        if event.keystroke.key == "escape" {
            self.hide(cx);
        }
    }

    fn turn_off(&mut self, cx: &mut Context<Self>) {
        let Some(shown) = self.shown.clone() else {
            return;
        };
        state::turn_off(&shown.crash.id, cx);
        self.frame.toast(
            Toast::new(format!("{} is off from the next start", shown.name)),
            cx,
        );
        self.hide_here(cx);
    }
}

impl Render for Card {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(shown) = self.shown.clone().filter(|_| self.open.get()) else {
            return div().into_any_element();
        };
        let line = if shown.off {
            "It ended Artoo as it started, so it is off now. The Plugins page, at the foot of the menu, turns it on again."
        } else {
            "It was running when Artoo ended. Turned off, it stays out from the next start."
        };
        let title = format!("{} stopped Artoo", shown.name);
        let url = (!shown.repository.is_empty()).then(|| {
            let body = crash::issue_body(&shown.crash, &shown.version);
            let title = format!("{} {} stopped Artoo", shown.name, shown.version);
            crash::issue_url(&shown.repository, &title, &body)
        });
        div()
            .id("plugin-crash-backdrop")
            .absolute()
            .size_full()
            .flex()
            .justify_center()
            .items_start()
            .pt(px(72.))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| this.hide_here(cx)),
            )
            .child(
                div()
                    .occlude()
                    .w(px(520.))
                    .flex()
                    .flex_col()
                    .gap_3()
                    .p_4()
                    .rounded(px(ISLAND_RADIUS))
                    .border_1()
                    .border_color(theme::island_border())
                    .bg(theme::panel())
                    .shadow_lg()
                    .text_size(px(13.))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(
                                div()
                                    .text_size(px(15.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(title),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(theme::muted())
                                    .child(line),
                            ),
                    )
                    .child(
                        div()
                            .px(px(10.))
                            .py(px(8.))
                            .rounded(px(ROW_RADIUS))
                            .bg(theme::base())
                            .font_family(theme::code_font())
                            .text_size(px(12.))
                            // What went wrong is never cut (#184): it wraps whole.
                            .child(shown.crash.what.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .children(url.map(|url| {
                                outline_button("Report")
                                    .id("plugin-crash-report")
                                    .on_click(move |_: &ClickEvent, _, cx| cx.open_url(&url))
                            }))
                            .child(div().flex_1())
                            .child(outline_button("Close").id("plugin-crash-close").on_click(
                                cx.listener(|this, _: &ClickEvent, _, cx| this.hide_here(cx)),
                            ))
                            .when(!shown.off, |row| {
                                row.child(
                                    primary_button("plugin-crash-off", "Turn it off", "").on_click(
                                        cx.listener(|this, _: &ClickEvent, _, cx| {
                                            this.turn_off(cx)
                                        }),
                                    ),
                                )
                            }),
                    ),
            )
            .into_any_element()
    }
}
