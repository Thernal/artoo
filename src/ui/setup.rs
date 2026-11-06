//! Set up's parts the tab offers (#143 B; the page is the shell's, `artoo_shell_api::setup`): Keys — the keymaps to
//! start from, then the keys that matter by group — and Look — Appearance's rows' short form, Panes, the faces and
//! the sizes. Each is its Settings section at its shortest, drawn by the same code (`key_presets`, `panes_chips`,
//! `font_rows`, `settings::Rows::short`): what is set here is set there. Agents is the agents' part.

use super::keymap::Keymap;
use super::keys::{self, On};
use super::settings::item;
use super::{ROW_RADIUS, Workspace, px, theme};
use artoo_shell_api::setup::{Part, SetUp};
use gpui::{AnyElement, App, Context, FontWeight, WeakEntity, div, prelude::*};
use std::rc::Rc;

/// The keys that matter, by group, as the Keys part lists them.
const GROUPS: [(&str, &[&str]); 4] = [
    (
        "Move",
        &["NextFile", "PreviousFile", "NextChange", "SearchFiles"],
    ),
    ("Find", &["SearchEverywhere", "FindInFiles", "OpenRecent"]),
    (
        "Review",
        &["CommentLine", "MarkReviewed", "SendToAgent", "SubmitReview"],
    ),
    ("Window", &["FocusDiff", "ToggleAgents", "ShowShortcuts"]),
];

/// The tab's parts of Set up: Keys and Look, after the agents'.
pub(super) fn parts(tab: WeakEntity<Workspace>) -> Vec<Part> {
    let draw = |tab: WeakEntity<Workspace>, look: bool| -> artoo_shell_api::setup::Body {
        Rc::new(move |_, cx| {
            tab.update(cx, |this, cx| {
                if look {
                    this.render_setup_look(cx)
                } else {
                    this.render_setup_keys(cx)
                }
            })
            .unwrap_or_else(|_| div().into_any_element())
        })
    };
    let looked = tab.clone();
    vec![
        Part {
            key: "keys",
            name: "Keys",
            order: 1,
            // A keymap chosen, or a key changed.
            done: Rc::new(|cx| {
                let map = cx.global::<Keymap>();
                map.preset() != super::keymap::Preset::Artoo || map.changed_count() > 0
            }),
            body: draw(tab.clone(), false),
            shown: None,
            again: None,
        },
        Part {
            key: "look",
            name: "Look",
            order: 2,
            // A look other than the first.
            done: Rc::new(move |cx| {
                looked
                    .upgrade()
                    .is_some_and(|tab| tab.read(cx).look_changed())
            }),
            body: draw(tab, true),
            shown: None,
            again: None,
        },
    ]
}

impl Workspace {
    /// Set up in this tab, the shell's.
    fn setup_page(&self) -> Option<Rc<dyn SetUp>> {
        self.services.get()
    }

    /// The parts its features offer and its own handed to Set up, once.
    pub(super) fn offer_setup(&self, cx: &mut App) {
        let parts = self
            .parts
            .iter()
            .flat_map(|p| p.setup.iter().cloned())
            .collect();
        if let Some(set_up) = self.setup_page() {
            set_up.offer(parts, cx);
        }
    }

    /// Opens Set up on every part (Help → Set Up Artoo…), on part `at` when given; Settings goes.
    pub(super) fn open_setup(&mut self, at: Option<&str>, cx: &mut Context<Self>) {
        if let Some(set_up) = self.setup_page() {
            set_up.open(at, cx);
        }
        self.settings_open = false;
        cx.notify();
    }

    /// The first start: the parts due, if any; Settings goes.
    pub(super) fn open_setup_due(&mut self, cx: &mut Context<Self>) {
        if let Some(set_up) = self.setup_page() {
            set_up.open_due(cx);
            if set_up.is_open(cx) {
                self.settings_open = false;
                cx.notify();
            }
        }
    }

    pub(super) fn setup_open(&self, cx: &App) -> bool {
        self.setup_page().is_some_and(|s| s.is_open(cx))
    }

    /// Later, Done, esc and Settings opened: what was under it comes back. Whether it was open.
    pub(super) fn close_setup(&mut self, cx: &mut Context<Self>) -> bool {
        let open = self.setup_page().is_some_and(|s| s.close(cx));
        if open {
            cx.notify();
        }
        open
    }

    /// ↵: the next part, or Done on the last.
    pub(super) fn setup_next(&mut self, cx: &mut Context<Self>) {
        if let Some(set_up) = self.setup_page() {
            set_up.next(cx);
        }
        cx.notify();
    }

    /// The page, while it is open.
    pub(super) fn render_setup(&self, cx: &App) -> AnyElement {
        match self.setup_page().and_then(|s| s.view(cx)) {
            Some(view) => view.into_any_element(),
            None => div().into_any_element(),
        }
    }

    /// Look's state other than the first: a theme, code colours, faces, a size, flat panes.
    fn look_changed(&self) -> bool {
        let choice = theme::choice();
        choice.window != "one-dark"
            || choice.code.is_some()
            || theme::faces() != theme::Faces::default()
            || self.settings.ui_zoom != 100
            || self.settings.zoom != 100
            || self.settings.flat
    }

    /// Keys: the keymaps to start from, then the keys that matter by group — a mark and
    /// Artoo's own where the one picked differs.
    fn render_setup_keys(&self, cx: &mut Context<Self>) -> AnyElement {
        let presets = self.key_presets(cx);
        let map = cx.global::<Keymap>();
        let entries = map.entries();
        let differ = |e: &super::keymap::Entry| e.keys != map.shipped(e.name);
        let differing = entries.iter().filter(|e| differ(e)).count();
        let caps = |keys: &[String]| {
            div().flex().gap(px(4.)).children(
                keys.first()
                    .map(|k| keys::caps(&super::keymap::symbols(k), On::Plain)),
            )
        };
        let group = |name: &'static str, actions: &[&str]| {
            div()
                .flex()
                .flex_col()
                .child(
                    div()
                        .pt(px(12.))
                        .pb(px(4.))
                        .text_size(px(11.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::muted())
                        .child(name.to_uppercase()),
                )
                .children(actions.iter().enumerate().filter_map(|(i, action)| {
                    let e = entries.iter().find(|e| e.name == *action)?;
                    let shipped = map.shipped(e.name);
                    let own = differ(e).then(|| {
                        shipped
                            .first()
                            .map(|k| super::keymap::symbols(k))
                            .unwrap_or_else(|| "no key".to_owned())
                    });
                    Some(
                        div()
                            .h(px(30.))
                            .px(px(10.))
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .rounded(px(ROW_RADIUS))
                            .when(i % 2 == 1, |r| r.bg(theme::panel()))
                            .children(own.is_some().then(|| {
                                div()
                                    .flex_none()
                                    .size(px(6.))
                                    .rounded_full()
                                    .bg(theme::accent())
                            }))
                            .child(div().flex_1().min_w_0().child(e.label.clone()))
                            .children(own.map(|own| {
                                div()
                                    .text_size(px(11.))
                                    .text_color(theme::faint())
                                    .child(format!("Artoo {}", keys::text(&own)))
                            }))
                            .child(caps(&e.keys)),
                    )
                }))
        };
        let [(m, move_), (f, find), (r, review), (w, window)] = GROUPS;
        div()
            .flex()
            .flex_col()
            .child(presets)
            .child(
                div()
                    .flex()
                    .gap(px(16.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(group(m, move_))
                            .child(group(f, find)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(group(r, review))
                            .child(group(w, window)),
                    ),
            )
            .child(
                div()
                    .pt(px(10.))
                    .text_size(px(12.))
                    .text_color(theme::muted())
                    .child(format!(
                        "{differing} of {} keys differ from Artoo's · every key in Settings → Keyboard.",
                        entries.len()
                    )),
            )
            .into_any_element()
    }

    /// Look: what Appearance's owner draws of it (the Window themes as Settings draws them, then Code colours), then
    /// Panes, the faces and the sizes.
    fn render_setup_look(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .children(
                self.settings_rows(super::settings::APPEARANCE)
                    .into_iter()
                    .filter_map(|r| r.short),
            )
            .child(item(
                "Panes",
                "Islands float with gaps; flat ones touch.",
                self.panes_chips(cx),
            ))
            .children(self.font_rows(cx))
            .into_any_element()
    }
}
