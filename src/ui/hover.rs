//! Cards on a long hover (#53, Design `docs/specs/hover.md`): a person (#51) and a branch, which
//! stay while the pointer is on them and hold their actions (copy, open, mention). A commit's card is
//! the branches' (`artoo_branches::card`), tooltips and the cut the kit's (`artoo_ui_kit::hover`):
//! both re-exported here.

use super::{Workspace, format, px, theme};
pub(super) use artoo_ui_kit::hover::*;
use gpui::{
    AnyView, App, ClickEvent, Context, FontWeight, IntoElement, Render, StatefulInteractiveElement,
    WeakEntity, Window, div, prelude::*,
};

/// The whole text F1 opened at a row (#184): the row's tooltip, and where the row was drawn.
pub(super) struct Whole {
    text: gpui::SharedString,
    at: gpui::Bounds<gpui::Pixels>,
    /// A row low in the window has its whole text over it, not under it.
    above: bool,
}

/// A person (#51): who they are here, and a way to mention them or open their page.
pub struct PersonCard {
    pub name: String,
    pub login: String,
    /// What they are in the open request: author, reviewer (approved), …
    pub role: Option<String>,
    /// What they did last in it: "Pushed v4 35 min ago · wrote 12 min ago" (X58).
    pub activity: Option<String>,
    pub url: Option<String>,
    pub ws: WeakEntity<Workspace>,
}

impl PersonCard {
    pub fn clone_of(other: &PersonCard) -> PersonCard {
        PersonCard {
            name: other.name.clone(),
            login: other.login.clone(),
            role: other.role.clone(),
            activity: other.activity.clone(),
            url: other.url.clone(),
            ws: other.ws.clone(),
        }
    }
}

impl Render for PersonCard {
    // It fades in after the long rest (#56, 7).
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        super::motion::appear(self.body(), "card-in", super::motion::FAST, |c, v| {
            c.opacity(v)
        })
    }
}

impl PersonCard {
    fn body(&mut self) -> gpui::Div {
        let login = self.login.clone();
        let ws = self.ws.clone();
        let url = self.url.clone();
        card_frame()
            .w(px(300.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(super::discussion::avatar(&self.name, false, 32.))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(self.name.clone()),
                            )
                            .children((!self.login.is_empty()).then(|| {
                                div()
                                    .text_color(theme::muted())
                                    .child(format!("@{}", self.login))
                            })),
                    ),
            )
            .children(
                self.role
                    .clone()
                    .map(|r| div().text_color(theme::muted()).child(r)),
            )
            .children(
                self.activity
                    .clone()
                    .map(|a| div().text_size(px(11.)).text_color(theme::faint()).child(a)),
            )
            .child(
                div()
                    .flex()
                    .gap(px(6.))
                    .children((!login.is_empty()).then(|| {
                        card_action("card-mention", "Mention").on_click(
                            move |_: &ClickEvent, _, cx| {
                                ws.update(cx, |this, cx| this.mention(&login, cx)).ok();
                            },
                        )
                    }))
                    .children(url.map(|url| {
                        card_action("card-open-person", "Open ↗")
                            .on_click(move |_: &ClickEvent, _, cx| cx.open_url(&url))
                    })),
            )
    }
}

/// A mention under the pointer in comment text, and the person's card once the pointer rested on it
/// (#51, X62): the card stays while the pointer is on the mention or on the card.
pub struct MentionHover {
    login: String,
    /// Where the mention is drawn, in the window.
    spot: gpui::Bounds<gpui::Pixels>,
    card: Option<AnyView>,
    _wait: gpui::Task<()>,
}

impl MentionHover {
    /// Whether `at` is on the mention, or between it and its card.
    fn near(&self, at: gpui::Point<gpui::Pixels>) -> bool {
        self.spot.dilate(gpui::px(8.)).contains(&at)
    }
}

impl Workspace {
    /// The card of the person named `name` (`login` when known): their part in the open request.
    pub(super) fn person_card(
        &self,
        name: &str,
        login: &str,
        ws: WeakEntity<Workspace>,
        cx: &App,
    ) -> PersonCard {
        let login = if login.is_empty() {
            self.request_members(cx)
                .iter()
                .find(|(_, n)| n == name)
                .map(|(l, _)| l.clone())
                .unwrap_or_default()
        } else {
            login.to_owned()
        };
        let m = self
            .current_request(cx)
            .and_then(|iid| self.find_request(iid, cx));
        let approved = self
            .requests_api()
            .and_then(|r| r.approvals(cx))
            .as_ref()
            .is_some_and(|a| a.by.iter().any(|b| b == name));
        let role = m.as_ref().and_then(|m| {
            let mine = |l: &str| !login.is_empty() && l == login;
            if mine(&m.author_login) || m.author == name {
                Some("Author of this merge request".to_owned())
            } else if m
                .reviewers
                .iter()
                .any(|r| mine(r) || r == name || m.reviewer_name(r) == Some(name))
                || approved
            {
                Some(if approved {
                    "Reviewer · approved".to_owned()
                } else {
                    "Reviewer · reviewing".to_owned()
                })
            } else {
                None
            }
        });
        // What they did last here: the author's last push, anyone's last note.
        let theirs = |l: &str, n: &str| (!login.is_empty() && l == login) || n == name;
        let pushed = m
            .as_ref()
            .filter(|m| theirs(&m.author_login, &m.author))
            .and_then(|_| self.branches().and_then(|b| b.latest_version(cx)))
            .filter(|v| v.time > 0)
            .map(|v| format!("Pushed v{} {}", v.number, format::ago(v.time)));
        let wrote = m
            .and_then(|_| {
                self.request_threads(cx)
                    .iter()
                    .flat_map(|t| &t.notes)
                    .filter(|n| theirs(&n.login, &n.author))
                    .map(|n| n.created)
                    .max()
            })
            .map(|t| format!("wrote {}", format::ago(t)));
        let activity = match (pushed, wrote) {
            (Some(p), Some(w)) => Some(format!("{p} · {w}")),
            (Some(p), None) => Some(p),
            (None, Some(w)) => Some(format!("{}{}", w[..1].to_uppercase(), &w[1..])),
            (None, None) => None,
        };
        let url = self.web.as_ref().filter(|_| !login.is_empty()).map(|w| {
            let host_end = w
                .base
                .find("://")
                .and_then(|s| w.base[s + 3..].find('/').map(|i| s + 3 + i))
                .unwrap_or(w.base.len());
            format!("{}/{login}", &w.base[..host_end])
        });
        PersonCard {
            name: name.to_owned(),
            login,
            role,
            activity,
            url,
            ws,
        }
    }

    /// The pointer is on mention `hit` (or on none): its card comes after a long rest there.
    pub(super) fn hover_mention(
        &mut self,
        hit: Option<(String, gpui::Bounds<gpui::Pixels>)>,
        cx: &mut Context<Self>,
    ) {
        match hit {
            Some((login, spot)) => {
                if self
                    .mention_hover
                    .as_ref()
                    .is_some_and(|h| h.login == login && h.spot == spot)
                {
                    return;
                }
                let wait = cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(LONG).await;
                    this.update(cx, |this, cx| this.show_mention_card(cx)).ok();
                });
                self.mention_hover = Some(MentionHover {
                    login,
                    spot,
                    card: None,
                    _wait: wait,
                });
            }
            // Off the mention before its card came: it does not come. A card shown waits for the
            // pointer to leave it.
            None => {
                if self
                    .mention_hover
                    .as_ref()
                    .is_some_and(|h| h.card.is_none())
                {
                    self.mention_hover = None;
                }
            }
        }
        cx.notify();
    }

    fn show_mention_card(&mut self, cx: &mut Context<Self>) {
        let Some(login) = self.mention_hover.as_ref().map(|h| h.login.clone()) else {
            return;
        };
        let name = self
            .request_members(cx)
            .iter()
            .find(|(l, _)| *l == login)
            .map_or_else(|| login.clone(), |(_, n)| n.clone());
        let card = self.person_card(&name, &login, cx.entity().downgrade(), cx);
        let view: AnyView = cx.new(|_| card).into();
        if let Some(h) = &mut self.mention_hover {
            h.card = Some(view);
        }
        cx.notify();
    }

    /// The pointer moved: off the mention and off its card, the card goes.
    pub(super) fn mention_pointer(
        &mut self,
        at: gpui::Point<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) {
        if self
            .mention_hover
            .as_ref()
            .is_some_and(|h| !h.near(at) && h.card.is_some())
        {
            self.mention_hover = None;
            cx.notify();
        }
    }

    /// The card of the mention the pointer rests on, under it.
    pub(super) fn render_mention_card(&self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        let h = self.mention_hover.as_ref()?;
        let card = h.card.clone()?;
        Some(
            gpui::deferred(
                gpui::anchored()
                    .position(gpui::point(h.spot.left(), h.spot.bottom()))
                    .snap_to_window_with_margin(gpui::px(8.))
                    .child(
                        div()
                            .id("mention-card")
                            .occlude()
                            .on_hover(cx.listener(|this, on: &bool, window, cx| {
                                let at = window.mouse_position();
                                if !*on && this.mention_hover.as_ref().is_some_and(|h| !h.near(at))
                                {
                                    this.mention_hover = None;
                                    cx.notify();
                                }
                            }))
                            .child(card),
                    ),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }

    /// F1, ⌘K's `Show the whole text` (#184): the row picked last opens its whole text — its tooltip — at
    /// the row; F1 again closes it.
    pub(super) fn show_whole(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.whole = match self.whole.take() {
            Some(_) => None,
            None => picked_row(window, cx).map(|(text, at)| Whole {
                text,
                at,
                above: at.bottom() > window.viewport_size().height * 0.6,
            }),
        };
        cx.notify();
    }

    /// A key while the whole text is open, before anything else hears it: `⌘C` copies it, `esc` closes
    /// it; F1 is its own action's; any other key closes it and does what it does (↓ picks the next row).
    pub(super) fn whole_key(&mut self, key: &gpui::Keystroke, cx: &mut Context<Self>) {
        let Some(whole) = &self.whole else {
            return;
        };
        let m = &key.modifiers;
        match key.key.as_str() {
            "f1" | "shift" | "control" | "alt" | "platform" | "function" => {}
            "c" if m.secondary() && !m.shift && !m.alt => {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(whole.text.to_string()));
                cx.stop_propagation();
            }
            "escape" => {
                self.whole = None;
                cx.stop_propagation();
                cx.notify();
            }
            _ => {
                self.whole = None;
                cx.notify();
            }
        }
    }

    /// The whole text F1 opened, under its row.
    pub(super) fn render_whole(&self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        let whole = self.whole.as_ref()?;
        let left = whole.at.left() + px(8.);
        let place = if whole.above {
            gpui::anchored()
                .anchor(gpui::Anchor::BottomLeft)
                .position(gpui::point(left, whole.at.top() - px(8.)))
        } else {
            gpui::anchored().position(gpui::point(left, whole.at.bottom()))
        };
        Some(
            gpui::deferred(
                place.snap_to_window_with_margin(px(8.)).child(
                    div()
                        .id("whole-text")
                        .occlude()
                        .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                            this.whole = None;
                            cx.notify();
                        }))
                        .child(whole_card(&whole.text)),
                ),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }

    /// `@login ` into the comment being written, or onto the clipboard when none is.
    pub(super) fn mention(&mut self, login: &str, cx: &mut Context<Self>) {
        let tag = format!("@{login} ");
        match self.compose.as_mut() {
            Some(c) => c.edit.insert(&tag),
            None => cx.write_to_clipboard(gpui::ClipboardItem::new_string(tag)),
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    /// Every `.rs` file under `dir`, at any depth.
    fn sources(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                sources(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }

    /// The app's sources and every crate's, the helpers left out: the kit's `hover.rs` and this
    /// file, which names the cuts it looks for.
    fn views() -> Vec<(std::path::PathBuf, String)> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut files = Vec::new();
        sources(&root.join("src"), &mut files);
        for krate in std::fs::read_dir(root.join("crates")).unwrap().flatten() {
            sources(&krate.path().join("src"), &mut files);
        }
        let helpers = [
            root.join("crates/ui-kit/src/hover.rs"),
            root.join("src/ui/hover.rs"),
        ];
        files.sort();
        files
            .into_iter()
            .filter(|p| !helpers.contains(p))
            .map(|p| {
                let text = std::fs::read_to_string(&p).unwrap_or_default();
                (p.strip_prefix(root).unwrap_or(&p).to_path_buf(), text)
            })
            .collect()
    }

    /// Every text cut short gives the whole on a long hover (#161) and says it was cut with an
    /// ellipsis at the side that matters (#184): views cut through the kit's `hover` helpers, never
    /// with a bare `truncate`, `line_clamp` or `text_ellipsis…`.
    #[test]
    fn text_is_cut_only_through_the_helpers() {
        let cuts = [
            ".truncate()",
            ".line_clamp(",
            ".text_ellipsis()",
            ".text_ellipsis_start()",
            ".text_ellipsis_middle()",
        ];
        let mut bare = Vec::new();
        for (path, text) in views() {
            for (n, line) in text.lines().enumerate() {
                if cuts.iter().any(|cut| line.contains(cut)) {
                    bare.push(format!("{}:{}", path.display(), n + 1));
                }
            }
        }
        assert!(bare.is_empty(), "cut without hover::cut: {bare:#?}");
    }

    /// Text is never clipped without an ellipsis (#184): an element chain holding both
    /// `overflow_hidden` and `whitespace_nowrap` cuts what does not fit with nothing to say so.
    /// One that means it (a field scrolling with its caret) says why in a `// clip:` comment above
    /// its head or inside it.
    #[test]
    fn text_is_never_clipped_without_saying_so() {
        let (hidden, nowrap) = (".overflow_hidden()", ".whitespace_nowrap()");
        let mut silent = Vec::new();
        for (path, text) in views() {
            let lines: Vec<&str> = text.lines().collect();
            let mut n = 0;
            while n < lines.len() {
                if !lines[n].trim_start().starts_with('.') {
                    let line = lines[n];
                    if line.contains(hidden) && line.contains(nowrap) && !line.contains("// clip:")
                    {
                        silent.push(format!("{}:{}", path.display(), n + 1));
                    }
                    n += 1;
                    continue;
                }
                // A run of `.method()` lines: one chain, or the part of it between two children.
                let start = n;
                while n < lines.len() && lines[n].trim_start().starts_with('.') {
                    n += 1;
                }
                let run = &lines[start..n];
                if !(run.iter().any(|l| l.contains(hidden))
                    && run.iter().any(|l| l.contains(nowrap)))
                {
                    continue;
                }
                // The head (`div()`), then the comment lines right above it.
                let above = lines[..start]
                    .iter()
                    .rev()
                    .enumerate()
                    .take_while(|(k, l)| *k == 0 || l.trim_start().starts_with("//"));
                let said = above
                    .map(|(_, l)| l)
                    .chain(run)
                    .any(|l| l.contains("// clip:"));
                if !said {
                    silent.push(format!("{}:{}", path.display(), start + 1));
                }
            }
        }
        assert!(
            silent.is_empty(),
            "clipped with no ellipsis and no `// clip:` reason: {silent:#?}"
        );
    }

    /// Every tooltip opens after the same delay, `hover::LONG` (800 ms, #202): a `.tooltip(` with no
    /// `tooltip_show_delay` on its own chain, within the four lines above it.
    #[test]
    fn every_tooltip_waits_the_same_delay() {
        let mut quick = Vec::new();
        for (path, text) in views() {
            let lines: Vec<&str> = text.lines().collect();
            for (n, line) in lines.iter().enumerate() {
                if !line.contains(".tooltip(") || line.contains("fn tooltip") {
                    continue;
                }
                let back = &lines[n.saturating_sub(4)..=n];
                if !back.iter().any(|l| l.contains("tooltip_show_delay")) {
                    quick.push(format!("{}:{}", path.display(), n + 1));
                }
            }
        }
        assert!(
            quick.is_empty(),
            "a tooltip without the shared delay: {quick:#?}"
        );
    }
}
