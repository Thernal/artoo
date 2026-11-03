//! The menu on the left, the tab's side: the four spaces of the shell (#70) — Inbox, Code, Branches and
//! Merge requests — with what waits in each and what the project lacks for one. What is on screen sits under its
//! space: the request, else the branch, and the file read in Code. Drawn by the shell (`artoo_shell_api::spaces`); a
//! click comes back here. The menu folds to icons only when the user folds it — the title bar's first button or ⌘B
//! (D2); Settings is the title bar's gear. Until the spaces get pages of their own (#72, #75), Code opens search
//! everywhere on Files, Branches and Merge requests the picker beside the menu.

use super::nav::{Item, Page};
use super::requests::Scope;
use super::{Workspace, icons, px, theme};
use artoo_shell_api::spaces::{self, Open, SpaceRow, SpacesHost, SpacesMenu, ordered};
pub use artoo_shell_api::spaces::{Places, Space};
use gpui::{App, Context, Pixels, Point, WeakEntity, Window, prelude::*};
use std::rc::Rc;

/// What a project lacks for a space.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Missing {
    /// No GitLab or GitHub remote.
    Remote,
    /// A remote but no token: the space opens the connect panel.
    Token,
    /// A plain folder, not a git repository (#75).
    Git,
}

impl Missing {
    fn line(self) -> &'static str {
        match self {
            Missing::Remote => "Needs a GitLab or GitHub remote",
            Missing::Token => "Connect a GitLab token",
            Missing::Git => "Not a git repository · git init",
        }
    }
}

/// What is open under a space, as its row in the menu.
struct OpenRow {
    /// The request or branch; None for Code's file.
    item: Option<Item>,
    icon: &'static str,
    label: String,
    mono: bool,
    tip: String,
    /// It is the page on screen.
    on: bool,
    /// Agent comments not sent yet on its files.
    drafts: bool,
    /// Notifications of it not read yet.
    news: bool,
    /// The agent that gathered it (#148): its mark, and how many of its pending comments wait.
    agent: Option<(char, gpui::Rgba, usize)>,
    /// What an agent put there for the reviewer, not opened since (#160).
    call: Option<String>,
}

impl Workspace {
    /// The space on screen; none while Settings shows or no repository is open.
    pub(super) fn space(&self) -> Option<Space> {
        (!self.settings_open && self.root.is_some()).then_some(self.nav.space)
    }

    fn missing(&self, space: Space, cx: &App) -> Option<Missing> {
        match space {
            Space::Branches if self.plain => Some(Missing::Git),
            Space::Code | Space::Branches => None,
            Space::Inbox | Space::Requests if self.requests_available(cx) => None,
            Space::Inbox | Space::Requests if self.requests_api().is_some_and(|r| r.connectable(cx)) => {
                Some(Missing::Token)
            }
            Space::Inbox | Space::Requests => Some(Missing::Remote),
        }
    }

    /// What waits in a space: the Inbox's unread, the requests that ask for your review.
    fn waiting(&self, space: Space, cx: &App) -> usize {
        match space {
            Space::Inbox => self.inbox_unread(cx),
            Space::Requests => self
                .requests_api()
                .map_or(0, |r| r.count_in(Scope::ReviewMe, cx)),
            Space::Code | Space::Branches => 0,
        }
    }

    /// The rows open under `space`: its requests or branches, Code's file.
    fn open_rows(&self, space: Space, cx: &App) -> Vec<OpenRow> {
        let here = self.space() == Some(space);
        if space == Space::Code {
            return match self.page_of(Space::Code) {
                Some(Page::File(path) | Page::Folder(path)) => vec![OpenRow {
                    item: None,
                    icon: if matches!(self.page_of(Space::Code), Some(Page::Folder(_))) {
                        "folder"
                    } else {
                        "file"
                    },
                    label: path.rsplit('/').next().unwrap_or(path).to_owned(),
                    mono: false,
                    tip: path.clone(),
                    on: here,
                    drafts: false,
                    news: false,
                    agent: None,
                    call: None,
                }],
                _ => Vec::new(),
            };
        }
        let loaded = self.page_of(space).and_then(Page::item);
        self.open_items(space)
            .into_iter()
            .map(|open| {
                let on = here && self.is_shown(&open.item);
                // The one loaded says its files as they are now.
                let now: Vec<String>;
                let paths = match &self.review.read(cx).diff {
                    Some(diff) if loaded.as_ref() == Some(&open.item) => {
                        now = diff.files.iter().map(|f| f.path().to_owned()).collect();
                        &now
                    }
                    _ => &open.paths,
                };
                let drafts = self
                    .comments(cx)
                    .iter()
                    .any(|c| c.state == crate::comment::State::Draft && paths.contains(&c.path));
                match &open.item {
                    Item::Request(iid) => {
                        let m = self.find_request(*iid, cx);
                        let title = m.as_ref().map(|m| m.title.clone()).unwrap_or_default();
                        // One merged or closed since stays here, saying so, until closed (#114).
                        let state = m
                            .map(|m| m.state)
                            .filter(|s| *s != crate::mr::MrState::Opened)
                            .map(|s| format!(" · {}", s.word().to_lowercase()))
                            .unwrap_or_default();
                        let label = format!("{} {title}{state}", super::words::number(*iid));
                        // Gathered by an agent: its line under the title (#148).
                        let gathered = self.gathered(*iid);
                        let tip = match gathered.and_then(|g| g.note.as_deref().map(|n| (g, n))) {
                            Some((g, note)) if !note.is_empty() => {
                                format!("{label}\n{}: {note}", g.agent)
                            }
                            Some((g, _)) => format!("{label}\nGathered by {}", g.agent),
                            None => label.clone(),
                        };
                        let agent = gathered.map(|g| {
                            let (letter, hue) = self.agent_mark(&g.agent, cx);
                            (letter, hue, self.agent_waiting(*iid, &g.agent, cx))
                        });
                        OpenRow {
                            item: Some(open.item.clone()),
                            icon: "pull-request",
                            tip,
                            label,
                            mono: false,
                            on,
                            drafts,
                            news: self.requests_api().is_some_and(|r| r.unread_of(*iid, cx)),
                            agent,
                            call: self.agent_calls.get(&open.item).filter(|_| !on).cloned(),
                        }
                    }
                    // Named as their page is (#149).
                    Item::Compare(..) => {
                        let label = match &open.page {
                            Page::Compare(base, head, _) => format!("{} … {}", base.0, head.0),
                            _ => String::new(),
                        };
                        OpenRow {
                            item: Some(open.item.clone()),
                            icon: "layers",
                            tip: label.clone(),
                            label,
                            mono: true,
                            on,
                            drafts,
                            news: false,
                            agent: None,
                            call: self.agent_calls.get(&open.item).filter(|_| !on).cloned(),
                        }
                    }
                    Item::WorkingTree => OpenRow {
                        item: Some(open.item.clone()),
                        icon: "files",
                        label: "Working tree".to_owned(),
                        mono: false,
                        tip: "Working tree".to_owned(),
                        on,
                        drafts,
                        news: false,
                        agent: None,
                        call: self.agent_calls.get(&open.item).filter(|_| !on).cloned(),
                    },
                    Item::Branch(refname) => {
                        let name = self
                            .branch_list(cx)
                            .iter()
                            .find(|b| &b.refname == refname)
                            .map_or_else(|| refname.clone(), |b| b.name.clone());
                        OpenRow {
                            item: Some(open.item.clone()),
                            icon: "branch",
                            tip: name.clone(),
                            label: name,
                            mono: true,
                            on,
                            drafts,
                            news: false,
                            agent: None,
                            call: self.agent_calls.get(&open.item).filter(|_| !on).cloned(),
                        }
                    }
                }
            })
            .collect()
    }

    /// `from` dragged onto `to` in the menu: it takes `to`'s place, kept in `layout.txt`.
    fn move_space(&mut self, from: Space, to: Space, cx: &mut Context<Self>) {
        let mut order = ordered(&self.layout.spaces);
        let (Some(a), Some(b)) = (
            order.iter().position(|s| *s == from),
            order.iter().position(|s| *s == to),
        ) else {
            return;
        };
        let moved = order.remove(a);
        order.insert(b, moved);
        self.layout.spaces = order.iter().map(|s| s.name()).collect::<Vec<_>>().join(",");
        self.save_layout();
        cx.notify();
    }

    /// A space's row or ⌃⌘1…4: the page it was left on, its root when it is already on screen.
    /// One the project lacks a token for opens the connect panel; one it lacks a remote or git
    /// for does nothing.
    pub(super) fn go_space(&mut self, space: Space, cx: &mut Context<Self>) {
        match self.missing(space, cx) {
            _ if self.root.is_none() => {}
            Some(Missing::Remote | Missing::Git) => {}
            Some(Missing::Token) => self.show(Space::Requests, None, cx),
            None => self.enter_space(space, cx),
        }
    }

    /// The menu; None without a repository and in zen.
    pub(super) fn render_spaces(&self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        // Zen just began: it slides away (#56, 16).
        if self.root.is_none() || (self.zen.is_some() && !self.motion.recent("zen")) {
            return None;
        }
        let (mut rows, mut items) = (Vec::new(), Vec::new());
        for space in ordered(&self.layout.spaces) {
            let missing = self.missing(space, cx);
            let opens = self.open_rows(space, cx);
            items.push((space, opens.iter().map(|o| o.item.clone()).collect()));
            let opens = opens
                .into_iter()
                .map(|open| Open {
                    // Code's file by its own icon (#95).
                    icon: match open.item {
                        None if open.icon == "folder" => icons::folder_icon(&open.tip, 14.),
                        None => icons::file_icon(&open.tip, 14.),
                        Some(_) => icons::icon(open.icon)
                            .size(px(14.))
                            .text_color(theme::faint())
                            .into_any_element(),
                    },
                    picked: open
                        .item
                        .as_ref()
                        .is_some_and(|i| self.nav.picked.contains(i)),
                    agent: open.agent.map(|(letter, hue, waiting)| {
                        let badge = artoo_agents_api::view::chat_badge(letter, hue, false, cx)
                            .size(px(14.))
                            .text_size(px(8.));
                        (badge.into_any_element(), waiting)
                    }),
                    label: open.label,
                    mono: open.mono,
                    tip: open.tip,
                    on: open.on,
                    drafts: open.drafts,
                    news: open.news,
                    call: open.call,
                })
                .collect();
            rows.push(SpaceRow {
                space,
                missing: missing.map(Missing::line),
                inert: missing == Some(Missing::Remote),
                waiting: self.waiting(space, cx),
                shut: self.nav.folded[space.ix()],
                opens,
                opens_moving: self.motion.recent(&format!("opens-{}", space.id())),
            });
        }
        let host = Rc::new(Glue {
            tab: cx.weak_entity(),
            items,
        });
        Some(spaces::menu(
            SpacesMenu {
                folded: self.menu_folded(),
                moving: self.motion.recent("places-fold"),
                here: self.space(),
                rows,
                projects: self.menu_projects(),
                tabs: self.tabs_host(cx),
                panel_icons: self.render_panel_icons(cx),
                zen: self.zen.is_some(),
                zen_moving: self.motion.recent("zen"),
                places: &self.places,
                folds: &self.folds,
                heights: &self.heights,
            },
            host,
            cx,
        ))
    }
}

/// What a click on the menu does, answered by the tab.
struct Glue {
    tab: WeakEntity<Workspace>,
    /// What each row under a space is: its request or branch; None for Code's file.
    items: Vec<(Space, Vec<Option<Item>>)>,
}

impl Glue {
    /// Row `n` under `space`, when it is there.
    fn item(&self, space: Space, n: usize) -> Option<Option<Item>> {
        let (_, items) = self.items.iter().find(|(s, _)| *s == space)?;
        items.get(n).cloned()
    }
}

impl SpacesHost for Glue {
    fn go(&self, space: Space, _: &mut Window, cx: &mut App) {
        self.tab
            .update(cx, |this, cx| this.go_space(space, cx))
            .ok();
    }

    fn toggle(&self, space: Space, _: &mut Window, cx: &mut App) {
        self.tab
            .update(cx, |this, cx| this.toggle_open_items(space, cx))
            .ok();
    }

    fn moved(&self, from: Space, to: Space, _: &mut Window, cx: &mut App) {
        self.tab
            .update(cx, |this, cx| this.move_space(from, to, cx))
            .ok();
    }

    fn open(&self, space: Space, n: usize, shift: bool, _: &mut Window, cx: &mut App) {
        let Some(item) = self.item(space, n) else {
            return;
        };
        self.tab
            .update(cx, |this, cx| match item {
                Some(item) if shift => this.pick_open(item, cx),
                Some(item) => {
                    this.nav.picked.clear();
                    this.show_open(&item, cx)
                }
                None => this.go_space(space, cx),
            })
            .ok();
    }

    fn close(&self, space: Space, n: usize, _: &mut Window, cx: &mut App) {
        let Some(item) = self.item(space, n) else {
            return;
        };
        self.tab
            .update(cx, |this, cx| match &item {
                Some(item) => this.close_open(item, cx),
                None => this.close_shown(cx),
            })
            .ok();
    }

    fn row_menu(&self, space: Space, n: usize, at: Point<Pixels>, _: &mut Window, cx: &mut App) {
        if let Some(Some(item)) = self.item(space, n) {
            self.tab
                .update(cx, |this, cx| this.item_context(&item, at, cx))
                .ok();
        }
    }
}
