//! A newer Artoo, said in the window (#135): a card over the window the first time a check finds a version —
//! what is new in it and in every release in between, each one's commits folded under it, Download (its page),
//! Skip this version, Later —, then a mark in the title bar that opens it again. The releases scroll between the
//! card's head and its buttons, a fade at their foot while more is below; ↑ ↓ (j k), space, ⇧space, Home and End
//! scroll them. Artoo → Check for Updates…, ⌘K and Settings → General ask at once. Nothing is downloaded or
//! installed. Each tab has its own card, mark and Settings line (`parts`), drawn by the shell (#165).

use crate::{CheckForUpdates, Release};
use artoo_shell_api::{Frame, Overlay, Parts, TitleItem};
use artoo_ui_kit::{ISLAND_RADIUS, ROW_RADIUS, format, hover, icons, keys, markdown, px, theme};
use gpui::{
    App, ClickEvent, Context, Entity, FontWeight, KeyDownEvent, MouseButton, ScrollHandle,
    SharedString, Window, canvas, div, linear_color_stop, linear_gradient, point, prelude::*,
};
use std::cell::Cell;
use std::rc::Rc;

/// How far ↑ ↓ scroll the releases.
const STEP: f32 = 48.;

/// What the feature draws in a tab: the card, the title bar's mark, Settings' line, the check as an action.
pub(crate) fn parts(frame: Rc<dyn Frame>, _: &mut Window, cx: &mut App) -> Parts {
    let open = Rc::new(Cell::new(false));
    let card = cx.new(|_| Card {
        frame,
        open: open.clone(),
        checking: false,
        unfolded: Vec::new(),
        scroll: ScrollHandle::new(),
        more: Rc::default(),
    });
    let mark = cx.new(|cx| Mark::new(card.clone(), cx));
    let status = cx.new(|cx| Status::new(card.clone(), cx));
    let on = |card: &Entity<Card>| card.clone();
    let (key, close, poll, check) = (on(&card), on(&card), on(&card), on(&card));
    let (shot, shot_commits) = (on(&card), on(&card));
    Parts {
        title: Some(TitleItem {
            view: mark.into(),
            shown: Rc::new(|_| crate::newest().is_some()),
        }),
        overlay: Some(Overlay {
            view: card.into(),
            open,
            key: Rc::new(move |event, _, cx| key.update(cx, |card, cx| card.key(event, cx))),
            close: Rc::new(move |cx| close.update(cx, |card, cx| card.hide(cx))),
            // A version a check found and no card showed yet opens the card.
            poll: Some(Rc::new(move |cx| {
                if crate::take_news().is_some() {
                    poll.update(cx, |card, cx| card.show(cx));
                }
            })),
        }),
        tool: None,
        // Under the window root's General.
        settings: vec![artoo_shell_api::settings::Rows::new("general", 10, status)],
        actions: vec![(
            Box::new(CheckForUpdates),
            Rc::new(move |_, cx| check.update(cx, |card, cx| card.check_now(cx))),
        )],
        // A newer release's card, from ARTOO_RELEASES_FIXTURE; with its newest release's commits unfolded.
        snapshots: vec![
            (
                "update",
                Rc::new(move |_, cx| {
                    crate::check();
                    shot.update(cx, |card, cx| card.show(cx));
                }),
            ),
            (
                "update-commits",
                Rc::new(move |_, cx| {
                    crate::check();
                    shot_commits.update(cx, |card, cx| {
                        card.show(cx);
                        card.unfolded.extend(
                            crate::last().and_then(|c| c.newer.first().map(|r| r.version.clone())),
                        );
                    });
                }),
            ),
        ],
        pages: None,
        inputs: Vec::new(),
        pieces: Vec::new(),
        links: Vec::new(),
        searches: Vec::new(),
        lists: Vec::new(),
        ways: Vec::new(),
        setup: Vec::new(),
    }
}

/// The card: whether it is open, the releases whose commits are unfolded, and their scroll; whether a check
/// asked from here is under way.
pub(crate) struct Card {
    frame: Rc<dyn Frame>,
    open: Rc<Cell<bool>>,
    checking: bool,
    unfolded: Vec<semver::Version>,
    scroll: ScrollHandle,
    /// More of the releases below what shows: the fade at their foot. Set as they are drawn.
    more: Rc<Cell<bool>>,
}

impl Card {
    /// Opens it afresh: folded, at the top. The tab is not asked to draw (the caller may be the tab).
    fn show(&mut self, cx: &mut Context<Self>) {
        self.open.set(true);
        self.unfolded.clear();
        self.scroll = ScrollHandle::new();
        self.more.set(false);
        cx.notify();
    }

    fn hide(&mut self, cx: &mut Context<Self>) {
        self.open.set(false);
        cx.notify();
    }

    /// Opened or closed by the card itself: the tab draws it or stops.
    fn show_here(&mut self, cx: &mut Context<Self>) {
        self.show(cx);
        self.frame.redraw(cx);
    }

    fn hide_here(&mut self, cx: &mut Context<Self>) {
        self.hide(cx);
        self.frame.redraw(cx);
    }

    /// The card's keys: esc closes it, the rest scroll the releases.
    fn key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let s = &self.scroll;
        let (y, max) = (f32::from(s.offset().y), f32::from(s.max_offset().y));
        let page = f32::from(s.bounds().size.height) * 0.85;
        let shift = event.keystroke.modifiers.shift;
        let to = match event.keystroke.key.as_str() {
            "escape" => return self.hide(cx),
            "down" | "j" => y - f32::from(px(STEP)),
            "up" | "k" => y + f32::from(px(STEP)),
            "space" if shift => y + page,
            "space" | "pagedown" => y - page,
            "pageup" => y + page,
            "home" => 0.,
            "end" => -max,
            _ => return,
        };
        s.set_offset(point(s.offset().x, gpui::px(to.clamp(-max, 0.))));
        cx.notify();
    }

    /// Artoo → Check for Updates…, ⌘K, Settings' Check now: asks GitHub now and opens the card,
    /// a skipped version too, or says this one is the newest.
    fn check_now(&mut self, cx: &mut Context<Self>) {
        if self.checking {
            return;
        }
        self.checking = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let check = cx
                .background_executor()
                .spawn(async { crate::check() })
                .await;
            this.update(cx, |this, cx| {
                this.checking = false;
                match (&check.error, check.newer.is_empty()) {
                    (_, false) => this.show_here(cx),
                    (Some(e), true) => this
                        .frame
                        .error(format!("Could not ask GitHub for releases: {e}").into(), cx),
                    (None, true) => this.frame.toast(
                        format!("Artoo {} is the newest", crate::current()).into(),
                        cx,
                    ),
                }
                cx.notify();
                cx.refresh_windows();
            })
            .ok();
        })
        .detach();
    }

    /// Skip this version on the left; Later and Download on the right.
    fn render_buttons(&self, newest: &Release, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let url = newest.url.clone();
        let skipped = newest.version.clone();
        div()
            .pt(px(10.))
            .border_t_1()
            .border_color(theme::island_border())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(
                        button("update-skip", "Skip this version", false).on_click(cx.listener(
                            move |this, _: &ClickEvent, _, cx| {
                                crate::skip(&skipped);
                                this.hide_here(cx);
                            },
                        )),
                    )
                    .child(div().flex_1())
                    .child(
                        button("update-later", "Later", false)
                            .child(keys::caps("esc", keys::On::Plain))
                            .on_click(
                                cx.listener(|this, _: &ClickEvent, _, cx| this.hide_here(cx)),
                            ),
                    )
                    .child(
                        button("update-download", "Download", true)
                            .child(
                                icons::icon("external")
                                    .size(px(13.))
                                    .text_color(theme::base()),
                            )
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                cx.open_url(&url);
                                this.hide_here(cx);
                            })),
                    ),
            )
    }
}

impl Render for Card {
    /// The newest version, then what is new in each release newer than this build.
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let newer = crate::last().map(|c| c.newer).unwrap_or_default();
        let Some(newest) = newer.first().filter(|_| self.open.get()).cloned() else {
            return div().into_any_element();
        };
        let current = crate::current();
        let selecting = self.frame.selecting(cx);
        let releases = newer.iter().map(|r| {
            let open = self.unfolded.contains(&r.version);
            let version = r.version.clone();
            div()
                .flex()
                .flex_col()
                .gap(px(6.))
                .pb(px(14.))
                .child(
                    div()
                        .flex()
                        .items_baseline()
                        .gap(px(8.))
                        .child(
                            div()
                                .text_size(px(14.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(r.version.to_string()),
                        )
                        .children(r.published.map(|t| {
                            div()
                                .text_size(px(11.))
                                .text_color(theme::faint())
                                .child(format::ago(t))
                        })),
                )
                .child(if r.notes.is_empty() {
                    div()
                        .text_size(px(12.))
                        .text_color(theme::faint())
                        .child("No notes for this release.")
                        .into_any_element()
                } else {
                    markdown::body(
                        &r.notes,
                        true,
                        format!("update-notes-{}", r.version),
                        None,
                        selecting.clone(),
                        cx,
                    )
                })
                .when(r.count > 0, |s| {
                    s.child(
                        div()
                            .id(SharedString::from(format!("update-commits-{}", r.version)))
                            .flex()
                            .items_center()
                            .gap(px(4.))
                            .text_size(px(12.))
                            .text_color(theme::muted())
                            .cursor_pointer()
                            .hover(|s| s.text_color(theme::text()))
                            .child(
                                icons::icon(if open { "chevron" } else { "chevron-right" })
                                    .size(px(14.))
                                    .text_color(theme::muted()),
                            )
                            .child(format::plural(r.count, "commit"))
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                if let Some(ix) = this.unfolded.iter().position(|v| *v == version) {
                                    this.unfolded.remove(ix);
                                } else {
                                    this.unfolded.push(version.clone());
                                }
                                cx.notify();
                            })),
                    )
                    .when(open, |s| {
                        s.child(div().pl(px(18.)).text_size(px(12.)).child(markdown::body(
                            &r.commits,
                            true,
                            format!("update-commits-{}", r.version),
                            None,
                            selecting.clone(),
                            cx,
                        )))
                    })
                })
        });
        let since = newest
            .published
            .map(|t| format!(" · out {}", format::ago(t)))
            .unwrap_or_default();
        div()
            .id("update-backdrop")
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
                    .w(px(620.))
                    .max_h(gpui::relative(0.86))
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
                                    .child(format!("Artoo {} is out", newest.version)),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(theme::muted())
                                    .child(format!("You have {current}{since}")),
                            ),
                    )
                    .child(
                        div()
                            .relative()
                            .flex_1()
                            .min_h_0()
                            .flex()
                            .flex_col()
                            .border_t_1()
                            .border_color(theme::island_border())
                            .child(
                                div()
                                    .id("update-releases")
                                    .flex_1()
                                    .min_h_0()
                                    .overflow_y_scroll()
                                    .track_scroll(&self.scroll)
                                    .pt(px(12.))
                                    .children(releases),
                            )
                            .children(self.more.get().then(fade))
                            .child(watch_more(self.scroll.clone(), self.more.clone())),
                    )
                    .child(self.render_buttons(&newest, cx)),
            )
            .into_any_element()
    }
}

/// The title bar's mark of a newer version (not one skipped): a click opens the card.
struct Mark {
    card: Entity<Card>,
}

impl Mark {
    fn new(card: Entity<Card>, cx: &mut Context<Self>) -> Self {
        cx.observe(&card, |_, _, cx| cx.notify()).detach();
        Self { card }
    }
}

impl Render for Mark {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(release) = crate::newest() else {
            return div().into_any_element();
        };
        let tip: SharedString = format!(
            "Artoo {} is out — what is new, and the download",
            release.version
        )
        .into();
        div()
            .id("update-mark")
            .flex_none()
            .h(px(24.))
            .flex()
            .items_center()
            .gap(px(5.))
            .px_2()
            .rounded(px(ROW_RADIUS))
            .text_size(px(11.))
            .text_color(theme::accent())
            .cursor_pointer()
            .hover(|s| s.bg(theme::hover()))
            .tooltip_show_delay(hover::LONG)
            .tooltip(move |_, cx| cx.new(|_| hover::TipOwned(tip.clone())).into())
            .child(
                icons::icon("update")
                    .size(px(14.))
                    .text_color(theme::accent()),
            )
            .child(release.version.to_string())
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.card.update(cx, |card, cx| card.show_here(cx));
            }))
            .into_any_element()
    }
}

/// Settings → General, under Check for updates: this build, what the last check said, and Check now.
struct Status {
    card: Entity<Card>,
}

impl Status {
    fn new(card: Entity<Card>, cx: &mut Context<Self>) -> Self {
        cx.observe(&card, |_, _, cx| cx.notify()).detach();
        Self { card }
    }
}

impl Render for Status {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let check = crate::last();
        let said = if self.card.read(cx).checking {
            "checking…".to_owned()
        } else {
            match &check {
                None => "not checked yet".to_owned(),
                Some(c) => {
                    let when =
                        c.at.and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                            .map(|d| format::ago(d.as_secs() as i64))
                            .unwrap_or_default();
                    match (c.newer.first(), &c.error) {
                        (Some(r), _) => format!("{} is out · checked {when}", r.version),
                        (None, Some(e)) => format!("{e} · {when}"),
                        (None, None) => format!("the newest · checked {when}"),
                    }
                }
            }
        };
        let newer = check.is_some_and(|c| !c.newer.is_empty());
        div()
            .flex()
            .items_center()
            .gap(px(8.))
            .text_size(px(12.))
            .text_color(theme::muted())
            .child(
                hover::cut(
                    "version-line",
                    format!("{} — {said}", artoo_core::version()),
                )
                .flex_1(),
            )
            .when(newer, |s| {
                s.child(
                    text_button("update-whats-new", "What's new").on_click(cx.listener(
                        |this, _: &ClickEvent, _, cx| {
                            this.card.update(cx, |card, cx| card.show_here(cx));
                        },
                    )),
                )
            })
            .child(
                text_button("update-check-now", "Check now").on_click(cx.listener(
                    |this, _: &ClickEvent, _, cx| {
                        this.card.update(cx, |card, cx| card.check_now(cx));
                    },
                )),
            )
    }
}

/// The fade at the foot of the releases while more is below.
fn fade() -> impl IntoElement {
    let ground = theme::panel();
    div()
        .absolute()
        .bottom_0()
        .left_0()
        .right_0()
        .h(px(40.))
        .bg(linear_gradient(
            180.,
            linear_color_stop(gpui::Rgba { a: 0., ..ground }, 0.),
            linear_color_stop(ground, 1.),
        ))
}

/// Laid over the releases (their parent `relative`): whether more of them is below, kept in `more`
/// as they are drawn; a change draws the window again, so the fade comes or goes.
fn watch_more(scroll: ScrollHandle, more: Rc<Cell<bool>>) -> impl IntoElement {
    canvas(
        move |_, window, _| {
            let max = f32::from(scroll.max_offset().y);
            let below = max > 1. && f32::from(scroll.offset().y) > 1. - max;
            if more.replace(below) != below {
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

/// A button of the card, as the naming card's.
fn button(id: &'static str, label: &'static str, primary: bool) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .h(px(30.))
        .px(px(12.))
        .flex()
        .items_center()
        .gap(px(6.))
        .rounded(px(6.))
        .text_size(px(12.))
        .cursor_pointer()
        .when(primary, |s| {
            s.bg(theme::accent())
                .text_color(theme::base())
                .font_weight(FontWeight::SEMIBOLD)
        })
        .when(!primary, |s| {
            s.text_color(theme::muted())
                .hover(|s| s.bg(theme::hover()).text_color(theme::text()))
        })
        .child(label)
}

/// A quiet word that acts, in a settings line.
fn text_button(id: &'static str, label: &'static str) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .flex_none()
        .px(px(8.))
        .h(px(24.))
        .flex()
        .items_center()
        .rounded(px(ROW_RADIUS))
        .text_color(theme::accent())
        .cursor_pointer()
        .hover(|s| s.bg(theme::hover()))
        .child(label)
}
