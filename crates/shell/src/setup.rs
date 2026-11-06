//! Set up (#143 B): the page — the title and Later, the parts' tabs with a check on a part done at the top, the part
//! in the column between, which alone scrolls, and the count and Back · Skip · Next at the foot — and which parts it
//! offers when: each once (`setup.txt`), on the first start those not offered before and those offered again, a
//! release that adds one that one alone, every part from Help → Set Up Artoo…. One per tab, behind
//! `artoo_shell_api::setup::SetUp` in its services; the parts are their owners' (`Parts::setup`).

use artoo_core::storage;
use artoo_shell_api::setup::{Part, SetUp};
use artoo_shell_api::{Frame, Parts};
use artoo_ui_kit::controls::{chip, group};
use artoo_ui_kit::discussion::{Kind, button};
use artoo_ui_kit::{GAP, ISLAND_RADIUS, icons, px, theme};
use gpui::{
    AnyElement, AnyView, App, AppContext, ClickEvent, Context, Entity, FontWeight, Render,
    WeakEntity, Window, div, prelude::*,
};
use std::rc::{Rc, Weak};

/// The page open: the parts it offers now (indexes into those offered), and the one on.
struct Open {
    parts: Vec<usize>,
    at: usize,
}

/// Set up in one tab.
struct Machine {
    /// Held weakly: the tab holds its frame, whose services hold this.
    frame: Weak<dyn Frame>,
    /// Every part offered in the tab, in their order.
    offered: Vec<Part>,
    open: Option<Open>,
}

impl Machine {
    fn redraw(&self, cx: &mut Context<Self>) {
        if let Some(frame) = self.frame.upgrade() {
            frame.redraw(cx);
        }
        cx.notify();
    }

    /// The page opened on `parts`, which are offered now (`setup.txt`), at `at`.
    fn open_on(&mut self, parts: Vec<usize>, at: usize, cx: &mut Context<Self>) {
        if parts.is_empty() {
            return;
        }
        let keys: Vec<&str> = parts.iter().map(|&ix| self.offered[ix].key).collect();
        keep_seen(&self.keys(), &keys);
        self.open = Some(Open { parts, at: 0 });
        self.go(at, cx);
    }

    fn keys(&self) -> Vec<&'static str> {
        self.offered.iter().map(|p| p.key).collect()
    }

    /// Part `at` on: what it shows read again.
    fn go(&mut self, at: usize, cx: &mut Context<Self>) {
        let Some(open) = &mut self.open else {
            return;
        };
        open.at = at.min(open.parts.len() - 1);
        if let Some(shown) = self.offered[open.parts[open.at]].shown.clone() {
            shown(cx);
        }
        self.redraw(cx);
    }

    fn close(&mut self, cx: &mut Context<Self>) -> bool {
        let open = self.open.take().is_some();
        if open {
            self.redraw(cx);
        }
        open
    }

    /// The first start's parts: those not offered before, then those offered again.
    fn due(&self, cx: &App) -> Vec<usize> {
        let seen = storage::read("setup.txt");
        let keys = self.keys();
        let unseen = unseen(&keys, &seen, !storage::read("settings.txt").is_empty());
        if seen.is_empty() && unseen.is_empty() {
            keep_seen(&keys, &keys);
        }
        let mut due: Vec<usize> = (0..self.offered.len())
            .filter(|&ix| unseen.contains(&self.offered[ix].key))
            .collect();
        for (ix, part) in self.offered.iter().enumerate() {
            if !due.contains(&ix) && part.again.as_ref().is_some_and(|again| again(cx)) {
                due.push(ix);
            }
        }
        due.sort();
        due
    }
}

/// The keys of `offered` in `text` (`setup.txt`: a part's key a line).
fn parse_seen(offered: &[&'static str], text: &str) -> Vec<&'static str> {
    text.lines()
        .filter_map(|l| offered.iter().copied().find(|k| *k == l.trim()))
        .collect()
}

/// The parts to offer: those not seen — none when Artoo was used before Set up came (its settings kept, no
/// `setup.txt`): what the parts set is in Settings already.
fn unseen(offered: &[&'static str], seen: &str, used_before: bool) -> Vec<&'static str> {
    if seen.is_empty() && used_before {
        return Vec::new();
    }
    let seen = parse_seen(offered, seen);
    offered
        .iter()
        .copied()
        .filter(|k| !seen.contains(k))
        .collect()
}

/// `parts` kept as offered, with those offered before.
fn keep_seen(offered: &[&'static str], parts: &[&str]) {
    let seen = parse_seen(offered, &storage::read("setup.txt"));
    let text: String = offered
        .iter()
        .filter(|k| seen.contains(k) || parts.contains(k))
        .map(|k| format!("{k}\n"))
        .collect();
    storage::save("setup.txt", text);
}

impl Render for Machine {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(open) = &self.open else {
            return div().into_any_element();
        };
        let parts: Vec<Part> = open
            .parts
            .iter()
            .map(|&ix| self.offered[ix].clone())
            .collect();
        let at = open.at;
        let tabs = parts.iter().map(|p| (p.name, (p.done)(cx))).collect();
        let body = (parts[at].body)(window, cx);
        page(tabs, at, body, cx.entity().downgrade())
    }
}

/// Set up's handle in the tab's services.
struct Handle(Entity<Machine>);

impl SetUp for Handle {
    fn offer(&self, mut parts: Vec<Part>, cx: &mut App) {
        parts.sort_by_key(|p| p.order);
        self.0.update(cx, |m, _| m.offered = parts);
    }

    fn open(&self, at: Option<&str>, cx: &mut App) {
        self.0.update(cx, |m, cx| {
            let at = at
                .and_then(|key| m.offered.iter().position(|p| p.key == key))
                .unwrap_or(0);
            m.open_on((0..m.offered.len()).collect(), at, cx);
        });
    }

    fn open_due(&self, cx: &mut App) {
        self.0.update(cx, |m, cx| {
            let due = m.due(cx);
            m.open_on(due, 0, cx);
        });
    }

    fn is_open(&self, cx: &App) -> bool {
        self.0.read(cx).open.is_some()
    }

    fn next(&self, cx: &mut App) {
        self.0.update(cx, |m, cx| {
            let Some(open) = &m.open else {
                return;
            };
            if open.at + 1 < open.parts.len() {
                let at = open.at + 1;
                m.go(at, cx);
            } else {
                m.close(cx);
            }
        });
    }

    fn close(&self, cx: &mut App) -> bool {
        self.0.update(cx, |m, cx| m.close(cx))
    }

    fn view(&self, cx: &App) -> Option<AnyView> {
        self.0
            .read(cx)
            .open
            .is_some()
            .then(|| self.0.clone().into())
    }
}

/// A tab's Set up, its handle in the tab's services.
pub(crate) fn parts(frame: Rc<dyn Frame>, _: &mut Window, cx: &mut App) -> Parts {
    let machine = cx.new(|_| Machine {
        frame: Rc::downgrade(&frame),
        offered: Vec::new(),
        open: None,
    });
    frame
        .services()
        .set::<Rc<dyn SetUp>>(Rc::new(Handle(machine)));
    Parts::default()
}

/// The page's column: the top, the part and the foot line up in it.
fn column() -> gpui::Div {
    div().w_full().max_w(px(760.)).mx_auto().px(px(24.))
}

/// The page: each part's tab with whether it is done, part `at`'s `body`.
fn page(
    tabs: Vec<(&'static str, bool)>,
    at: usize,
    body: AnyElement,
    this: WeakEntity<Machine>,
) -> AnyElement {
    let n = tabs.len();
    let last = at + 1 == n;
    let go_to = move |this: &WeakEntity<Machine>, at: usize, cx: &mut App| {
        this.update(cx, |m, cx| m.go(at, cx)).ok();
    };
    let shut = |this: &WeakEntity<Machine>, cx: &mut App| {
        this.update(cx, |m, cx| {
            m.close(cx);
        })
        .ok();
    };
    let tabs = tabs
        .into_iter()
        .enumerate()
        .fold(group(), |group, (i, (name, done))| {
            let go = this.clone();
            group.child(
                chip(("setup-tab", i), name, i == at)
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .children(done.then(|| {
                        icons::icon("check")
                            .size(px(12.))
                            .text_color(theme::added())
                    }))
                    .on_click(move |_: &ClickEvent, _, cx| go_to(&go, i, cx)),
            )
        });
    let back = (at > 0).then(|| {
        let go = this.clone();
        button("setup-back", "Back", None, Kind::Plain)
            .on_click(move |_: &ClickEvent, _, cx| go_to(&go, at - 1, cx))
    });
    let next = at + 1;
    let (done, skip, forward, later) = (this.clone(), this.clone(), this.clone(), this);
    let forward: Vec<AnyElement> = if last {
        vec![
            button(
                "setup-done",
                "Done",
                Some("↵"),
                Kind::Primary(theme::accent()),
            )
            .on_click(move |_: &ClickEvent, _, cx| shut(&done, cx))
            .into_any_element(),
        ]
    } else {
        vec![
            button("setup-skip", "Skip", None, Kind::Plain)
                .on_click(move |_: &ClickEvent, _, cx| go_to(&skip, next, cx))
                .into_any_element(),
            button(
                "setup-next",
                "Next",
                Some("↵"),
                Kind::Primary(theme::accent()),
            )
            .on_click(move |_: &ClickEvent, _, cx| go_to(&forward, next, cx))
            .into_any_element(),
        ]
    };
    div()
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .m(px(GAP))
        .mt_0()
        .rounded(px(ISLAND_RADIUS))
        .bg(theme::editor())
        .child(
            div().flex_none().pt(px(24.)).child(
                column()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .mb(px(12.))
                            .child(
                                div()
                                    .flex_1()
                                    .text_size(px(18.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Set up Artoo"),
                            )
                            .child(
                                button("setup-later", "Later", Some("esc"), Kind::Plain)
                                    .on_click(move |_: &ClickEvent, _, cx| shut(&later, cx)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .pb(px(10.))
                            .border_b_1()
                            .border_color(theme::island_border())
                            .child(tabs),
                    ),
            ),
        )
        .child(
            div()
                .id("setup-body")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .child(column().pt(px(16.)).pb(px(24.)).child(body)),
        )
        .child(
            div()
                .flex_none()
                .py(px(12.))
                .border_t_1()
                .border_color(theme::island_border())
                .child(
                    column()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(
                            div()
                                .flex_1()
                                .text_size(px(12.))
                                .text_color(theme::faint())
                                .child(format!("{} of {n}", at + 1)),
                        )
                        .children(back)
                        .children(forward),
                ),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_part_is_offered_once_and_not_to_who_used_artoo_before() {
        let all = ["agents", "keys", "look"];
        // The first start: every part.
        assert_eq!(unseen(&all, "", false), all);
        // Artoo used before Set up came: none.
        assert!(unseen(&all, "", true).is_empty());
        // A release that adds a part: that part alone.
        assert_eq!(unseen(&all, "agents\nkeys\n", true), ["look"]);
        assert_eq!(parse_seen(&all, "keys\nwhat\n look"), ["keys", "look"]);
    }
}
