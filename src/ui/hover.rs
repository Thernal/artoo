//! Cards on a long hover (#53, Design `docs/specs/hover.md`): a person (#51) and a branch, which
//! stay while the pointer is on them and hold their actions (copy, open, mention). A commit's card is
//! the branches' (`artoo_branches::card`), tooltips and the cut the kit's (`artoo_ui_kit::hover`):
//! both re-exported here.

use super::{Workspace, format, px, theme};
use crate::git::CommitInfo;
pub(super) use artoo_branches::card::CommitCard;
pub(super) use artoo_ui_kit::hover::*;
use gpui::{
    AnyView, App, ClickEvent, Context, FontWeight, IntoElement, Render, StatefulInteractiveElement,
    WeakEntity, Window, div, prelude::*,
};

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

/// A branch: its upstream and how far apart, its last commit.
pub struct BranchCard {
    pub name: String,
    pub tracking: Option<(String, usize, usize)>,
    /// ↑ ↓ against the main line, and its request: what its row shows when it has room (#79).
    pub against: Option<(String, usize, usize)>,
    pub request: Option<String>,
    pub last: Option<CommitInfo>,
}

impl Render for BranchCard {
    // It fades in after the long rest (#56, 7).
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        super::motion::appear(self.body(), "card-in", super::motion::FAST, |c, v| {
            c.opacity(v)
        })
    }
}

impl BranchCard {
    fn body(&mut self) -> gpui::Div {
        card_frame()
            .child(
                div()
                    .font_family(theme::code_font())
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(self.name.clone()),
            )
            .child(
                div()
                    .text_color(theme::muted())
                    .child(match &self.tracking {
                        Some((up, 0, 0)) => format!("Up to date with {up}"),
                        Some((up, ahead, behind)) => {
                            format!("{up} · ↑{ahead} ahead · ↓{behind} behind")
                        }
                        None => "No upstream".to_owned(),
                    }),
            )
            .children(
                self.against
                    .as_ref()
                    .filter(|(on, ..)| self.tracking.as_ref().is_none_or(|(up, ..)| up != on))
                    .map(|(on, ahead, behind)| {
                        div()
                            .text_color(theme::muted())
                            .child(format!("↑{ahead} ↓{behind} against {on}"))
                    }),
            )
            .children(
                self.request
                    .clone()
                    .map(|r| div().text_color(theme::accent()).child(r)),
            )
            .children(self.last.as_ref().map(|c| {
                div().flex().flex_col().child(c.summary.clone()).child(
                    div()
                        .text_size(px(11.))
                        .text_color(theme::faint())
                        .child(format!(
                            "{} · {} · {}",
                            format::short(c.id),
                            c.author,
                            full_date(c.time)
                        )),
                )
            }))
    }
}

impl Workspace {
    /// The card of commit `ix` of the list.
    pub(super) fn commit_card(&self, ix: usize, cx: &App) -> Option<CommitCard> {
        Some(self.card_of(self.history.read(cx).commits.get(ix)?.clone(), cx))
    }

    /// The card of commit `info`, of whatever list.
    pub(super) fn card_of(&self, info: crate::git::CommitInfo, cx: &App) -> CommitCard {
        CommitCard {
            refs: (self.history.read(cx).decor.get(&info.id))
                .cloned()
                .unwrap_or_default(),
            url: self.web.as_ref().map(|w| w.commit(&info.id.to_string())),
            info,
            root: self.root.clone(),
            stat: None,
        }
    }

    /// The card of the person named `name` (`login` when known): their part in the open request.
    pub(super) fn person_card(
        &self,
        name: &str,
        login: &str,
        ws: WeakEntity<Workspace>,
        cx: &App,
    ) -> PersonCard {
        let login = if login.is_empty() {
            self.requests
                .read(cx)
                .members
                .iter()
                .find(|(_, n)| n == name)
                .map(|(l, _)| l.clone())
                .unwrap_or_default()
        } else {
            login.to_owned()
        };
        let m = self
            .requests
            .read(cx)
            .current
            .and_then(|iid| self.requests.read(cx).find(iid));
        let approved = self
            .requests
            .read(cx)
            .approvals
            .as_ref()
            .is_some_and(|a| a.by.iter().any(|b| b == name));
        let role = m.and_then(|m| {
            let mine = |l: &str| !login.is_empty() && l == login;
            if mine(&m.author_login) || m.author == name {
                Some("Author of this merge request".to_owned())
            } else if m.reviewers.iter().any(|r| mine(r) || r == name) || approved {
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
            .filter(|m| theirs(&m.author_login, &m.author))
            .and_then(|_| self.history.read(cx).versions.last())
            .filter(|v| v.time > 0)
            .map(|v| format!("Pushed v{} {}", v.number, format::ago(v.time)));
        let wrote = m
            .and_then(|_| {
                self.requests
                    .read(cx)
                    .threads
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

    /// What builds the card of branch `ix` of the list when it shows: the repository is read then,
    /// not at every frame.
    pub(super) fn branch_card(
        &self,
        ix: usize,
        cx: &App,
    ) -> Option<impl Fn(&mut Window, &mut App) -> AnyView + 'static> {
        let b = self.history.read(cx).branches.get(ix)?;
        let (root, name, tip) = (self.root.clone()?, b.name.clone(), b.tip);
        let against = self
            .history
            .read(cx)
            .roots
            .facts
            .get(&b.refname)
            .and_then(|f| f.against.clone());
        let request = self
            .request_of_branch(ix, cx)
            .map(|m| format!("{} {}", super::words::number(m.iid), m.title));
        Some(move |_: &mut Window, cx: &mut App| {
            let repo = crate::git::Repo::open(&root).ok();
            let card = BranchCard {
                name: name.clone(),
                tracking: repo.as_ref().and_then(|r| r.tracking(&name)),
                against: against.clone(),
                request: request.clone(),
                last: repo.as_ref().and_then(|r| r.commit(tip).ok()),
            };
            cx.new(|_| card).into()
        })
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
            .requests
            .read(cx)
            .members
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
    /// Every text cut short gives the whole on a long hover (#161): views cut through this file's
    /// helpers (the kit's `hover`), never with a bare `truncate`, `line_clamp` or `text_ellipsis`.
    #[test]
    fn text_is_cut_only_through_the_helpers() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut bare = Vec::new();
        let dirs = [root.join("src/ui"), root.join("crates/ui-kit/src")];
        for entry in dirs.iter().flat_map(|d| std::fs::read_dir(d).unwrap()) {
            let path = entry.unwrap().path();
            // The helpers (the kit's) and this test's own list of cuts.
            if path.file_name().is_some_and(|n| n == "hover.rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            for (n, line) in text.lines().enumerate() {
                if [".truncate()", ".line_clamp(", ".text_ellipsis()"]
                    .iter()
                    .any(|cut| line.contains(cut))
                {
                    bare.push(format!("{}:{}", path.display(), n + 1));
                }
            }
        }
        assert!(bare.is_empty(), "cut without hover::cut: {bare:#?}");
    }
}
