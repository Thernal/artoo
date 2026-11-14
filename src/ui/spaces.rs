//! The menu on the left, the tab's side: the spaces the features offer (#70, #171 phase 6) — Inbox, Code, Branches
//! and Merge requests — with what waits in each and what the project lacks for one. What is on screen sits under its
//! space: the request, else the branch, and the file read in Code. Drawn by the shell (`artoo_shell_api::spaces`); a
//! click comes back here. The menu folds to icons only when the user folds it — the title bar's first button or ⌘B
//! (D2); Settings is the title bar's gear. Until the spaces get pages of their own (#72, #75), Code opens search
//! everywhere on Files, Branches and Merge requests the picker beside the menu.

use super::nav::Page;
use super::requests::Scope;
use super::{Workspace, icons, px, theme};
use artoo_core::route::Route;
use artoo_shell_api::ItemRow;
use artoo_shell_api::spaces::{self, Open, SpaceRow, SpacesHost, SpacesMenu, ordered};
pub use artoo_shell_api::spaces::{Places, Space};

/// The spaces the tab coordinates, by their owners' ids: what each lacks and what waits in it are still the tab's;
/// their pages say the rest — their bar, their zones, their rows here (#171 phase 7).
pub(super) const INBOX: Space = Space(artoo_requests_api::INBOX);
pub(super) const CODE: Space = Space(artoo_code_api::SPACE);
pub(super) const BRANCHES: Space = Space(artoo_branches_api::SPACE);
pub(super) const REQUESTS: Space = Space(artoo_requests_api::SPACE);

/// The spaces `nav.txt` keeps a page of, in its columns' order; the space on screen by its place here.
pub(super) const KEPT: [Space; 4] = [INBOX, CODE, BRANCHES, REQUESTS];

/// A space's name as its owner offered it (`Merge requests`); its id when nobody did.
pub(super) fn label(space: Space, cx: &App) -> &'static str {
    spaces::known(cx)
        .into_iter()
        .find(|s| s.space == space)
        .map_or(space.name(), |s| s.label)
}
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

/// What is open under a space, as its row in the menu: what its pages say of it (`Pages::row`) and what the tab adds.
struct OpenRow {
    /// The request or branch; None for the page the space keeps when it belongs to no item (Code's file).
    item: Option<Route>,
    icon: gpui::AnyElement,
    label: String,
    mono: bool,
    tip: String,
    /// It is the page on screen.
    on: bool,
    /// Agent comments not sent yet on its files.
    drafts: bool,
    /// A request only looked at (#194).
    preview: bool,
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

    /// What the project lacks for `space`. The Inbox lacks nothing: its news comes from GitLab or GitHub, plugins
    /// and agents, and what one source lacks the others do not.
    fn missing(&self, space: Space, cx: &App) -> Option<Missing> {
        match space {
            BRANCHES | REQUESTS if self.plain => Some(Missing::Git),
            REQUESTS if self.requests_available(cx) => None,
            REQUESTS if self.requests_api().is_some_and(|r| r.connectable(cx)) => {
                Some(Missing::Token)
            }
            REQUESTS => Some(Missing::Remote),
            _ => None,
        }
    }

    /// What waits in a space: the Inbox's unread, the requests that ask for your review.
    fn waiting(&self, space: Space, cx: &App) -> usize {
        match space {
            INBOX => self.inbox_unread(cx),
            REQUESTS => self
                .requests_api()
                .map_or(0, |r| r.count_in(Scope::ReviewMe, cx)),
            _ => 0,
        }
    }

    /// The rows open under `space`: its items (its requests, its branches), and the page it keeps when that belongs
    /// to no item (Code's file); each as its pages say (`Pages::row`), one they say nothing of by its kind.
    fn open_rows(&self, space: Space, cx: &App) -> Vec<OpenRow> {
        let here = self.space() == Some(space);
        let pages = self.space_pages(space);
        let said = |item: &Route, page: &Route| pages.and_then(|p| (p.row)(item, page, cx));
        let mut rows = Vec::new();
        if let Some(page) = self.page_of(space).filter(|p| self.item_of(p).is_none())
            && let Some(row) = said(page, page)
        {
            rows.push(OpenRow {
                item: None,
                icon: row.icon,
                label: row.label,
                mono: row.mono,
                tip: row.tip,
                on: here,
                drafts: false,
                preview: false,
                news: row.news,
                agent: None,
                call: None,
            });
        }
        let loaded = self.page_of(space).and_then(|p| self.item_of(p));
        rows.extend(self.open_items(space).into_iter().map(|open| {
            let on = here && self.is_shown(&open.item);
            // The one loaded says its files as they are now.
            let now: Vec<String>;
            let paths = match &self.shown_diff(cx) {
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
            let row = said(&open.item, &open.page).unwrap_or_else(|| ItemRow {
                icon: icons::icon("layers")
                    .size(px(14.))
                    .text_color(theme::faint())
                    .into_any_element(),
                label: open.page.kind.clone(),
                mono: true,
                tip: open.page.kind.clone(),
                news: false,
            });
            // A request an agent gathered: its line under the title, its mark and its comments waiting (#148).
            let gathered = match Page::from_route(&open.item) {
                Some(Page::Request(iid)) => self.gathered(iid, cx).map(|g| (iid, g)),
                _ => None,
            };
            let tip = match gathered.and_then(|(_, g)| g.note.as_deref().map(|n| (g, n))) {
                Some((g, note)) if !note.is_empty() => format!("{}\n{}: {note}", row.tip, g.agent),
                Some((g, _)) => format!("{}\nGathered by {}", row.tip, g.agent),
                None => row.tip,
            };
            let agent = gathered.map(|(iid, g)| {
                let (letter, hue) = self.agents.agent_mark(&g.agent, cx);
                (letter, hue, self.agent_waiting(iid, &g.agent, cx))
            });
            OpenRow {
                item: Some(open.item.clone()),
                icon: row.icon,
                label: row.label,
                mono: row.mono,
                tip,
                on,
                drafts,
                preview: open.preview,
                news: row.news,
                agent,
                call: self.agent_calls.get(&open.item).filter(|_| !on).cloned(),
            }
        }));
        rows
    }

    /// An open item's words for ⌘E (#194): its row's, and how far its review got, what it is; its dots, drafts
    /// and news, as its row in the menu wears them.
    pub(super) fn open_words(
        &self,
        open: &super::nav::Opened,
        cx: &App,
    ) -> (String, String, Vec<artoo_shell_api::palette::Dot>) {
        use artoo_shell_api::palette::Dot;
        let space = self.item_space(&open.item);
        let pages = space.and_then(|s| self.space_pages(s));
        let row = pages.and_then(|p| (p.row)(&open.item, &open.page, cx));
        let news = row.as_ref().is_some_and(|r| r.news);
        let label = row.map_or_else(|| open.page.kind.clone(), |row| row.label);
        let drafts = (self.comments(cx).iter())
            .any(|c| c.state == crate::comment::State::Draft && open.paths.contains(&c.path));
        let mut detail = match Page::from_route(&open.item) {
            Some(Page::Request(iid)) => {
                let head = (self.requests_api())
                    .and_then(|r| r.find(iid, cx))
                    .and_then(|m| git2::Oid::from_str(&m.head).ok());
                match head {
                    Some(h) if self.rv().head_reviewed(h, cx) => "reviewed".to_owned(),
                    Some(h) => match self.rv().head_part(h, cx) {
                        Some((done, total)) => format!("{done} of {total} reviewed"),
                        None => "not reviewed".to_owned(),
                    },
                    None => "merge request".to_owned(),
                }
            }
            Some(Page::Branch(_)) => "branch".to_owned(),
            _ => open.page.kind.clone(),
        };
        if open.preview {
            detail.push_str(" · looked at");
        }
        let dots = [(drafts, Dot::Drafts), (news, Dot::News)]
            .into_iter()
            .filter_map(|(on, dot)| on.then_some(dot))
            .collect();
        (label, detail, dots)
    }

    /// `from` dragged onto `to` in the menu: it takes `to`'s place, kept in `layout.txt`.
    fn move_space(&mut self, from: Space, to: Space, cx: &mut Context<Self>) {
        let mut order: Vec<Space> = ordered(&self.layout.spaces, &spaces::all(cx))
            .into_iter()
            .map(|s| s.space)
            .collect();
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
    /// for opens too, and says so in its middle (#210).
    pub(super) fn go_space(&mut self, space: Space, cx: &mut Context<Self>) {
        let offered = spaces::known(cx).iter().any(|s| s.space == space);
        match self.missing(space, cx) {
            _ if self.root.is_none() || !offered => {}
            Some(Missing::Token) => self.show(REQUESTS, None, cx),
            Some(Missing::Remote | Missing::Git) | None => self.enter_space(space, cx),
        }
    }

    /// What a space says in its middle when the project lacks what it needs (#210): a plain folder's Branches and
    /// Merge requests `Not a git repository`, a repository without a remote's Merge requests `No remote`, each with
    /// what is missing under it, once. No button: the agent does the setup.
    pub(super) fn missing_page(&self, space: Space, cx: &App) -> Option<gpui::Div> {
        let name = (self.root.as_ref())
            .and_then(|r| r.file_name())
            .map_or_else(
                || "This folder".to_owned(),
                |n| n.to_string_lossy().into_owned(),
            );
        let (icon, title, text) = match (space, self.missing(space, cx)?) {
            (_, Missing::Token) => return None,
            (_, Missing::Git) => (
                "branch",
                "Not a git repository",
                format!("{name} has no .git folder"),
            ),
            (_, Missing::Remote) => (
                "pull-request",
                "No remote",
                format!("Merge requests need a GitLab or GitHub remote; {name} has none"),
            ),
        };
        Some(artoo_ui_kit::island().flex_1().bg(theme::editor()).child(
            artoo_ui_kit::controls::empty_state(icon, title.to_owned(), text, None),
        ))
    }

    /// The menu; None without a repository and in zen.
    pub(super) fn render_spaces(&self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        // Zen just began: it slides away (#56, 16).
        if self.root.is_none() || (self.zen.is_some() && !self.motion.recent("zen")) {
            return None;
        }
        let (mut rows, mut items) = (Vec::new(), Vec::new());
        let keymap = cx.try_global::<super::keymap::Keymap>();
        let actions = artoo_shell_api::actions(cx);
        for info in ordered(&self.layout.spaces, &spaces::all(cx)) {
            let space = info.space;
            // The key that goes there in force: one changed in Settings → Keyboard shows the new one.
            let key = actions
                .iter()
                .find(|a| a.id == info.go)
                .zip(keymap)
                .map(|(a, keymap)| keymap.key_now(a.act.as_ref()))
                .unwrap_or_default();
            let missing = self.missing(space, cx);
            let opens = self.open_rows(space, cx);
            items.push((space, opens.iter().map(|o| o.item.clone()).collect()));
            let opens = opens
                .into_iter()
                .map(|open| Open {
                    // Each by the icon its owner draws: Code's file by its own (#95).
                    icon: open.icon,
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
                    preview: open.preview,
                    news: open.news,
                    call: open.call,
                })
                .collect();
            rows.push(SpaceRow {
                space,
                label: info.label,
                icon: info.icon,
                key,
                // Only a token to connect is said under a space: what the project lacks, its page says (#210).
                missing: missing.filter(|m| *m == Missing::Token).map(Missing::line),
                inert: false,
                waiting: self.waiting(space, cx),
                shut: self.nav.folded.contains(&space),
                opens,
                opens_moving: self.motion.recent(&format!("opens-{}", space.id())),
            });
        }
        // The foot's rows (#176): the app's, the same from every project; what is open under them is their page.
        let foot = spaces::feet(cx)
            .into_iter()
            .map(|foot| SpaceRow {
                space: foot.info.space,
                label: foot.info.label,
                icon: foot.info.icon,
                key: actions
                    .iter()
                    .find(|a| a.id == foot.info.go)
                    .zip(keymap)
                    .map(|(a, keymap)| keymap.key_now(a.act.as_ref()))
                    .unwrap_or_default(),
                missing: None,
                inert: false,
                waiting: (foot.waiting)(cx),
                shut: true,
                opens: Vec::new(),
                opens_moving: false,
            })
            .collect();
        let host = Rc::new(Glue {
            tab: cx.weak_entity(),
            items,
        });
        Some(spaces::menu(
            SpacesMenu {
                folded: self.menu_folded(),
                width: self.layout.menu,
                edge: (!self.menu_folded())
                    .then(|| self.handle(super::Split::Menu, cx).into_any_element()),
                moving: self.motion.recent("places-fold"),
                here: self.space(),
                rows,
                foot,
                // The projects inside the menu of spaces went (#205).
                projects: Vec::new(),
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
    items: Vec<(Space, Vec<Option<Route>>)>,
}

impl Glue {
    /// Row `n` under `space`, when it is there.
    fn item(&self, space: Space, n: usize) -> Option<Option<Route>> {
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
