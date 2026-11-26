//! The menu on the left, the tab's side: the spaces the features offer (#70, #171 phase 6) — Inbox, Code, Branches
//! and Merge requests — with what waits in each and what the project lacks for one. What is on screen sits under its
//! space: the request, else the branch, and the file read in Code. Drawn by the shell (`artoo_shell_api::spaces`); a
//! click comes back here. The menu folds to icons only when the user folds it — the title bar's first button or ⌘B
//! (D2); Settings is the title bar's gear. Until the spaces get pages of their own (#72, #75), Code opens search
//! everywhere on Files, Branches and Merge requests the picker beside the menu.

use super::grouping::Tabs;
use super::groups::{Groups, List, Row, Unit};
use super::nav::{Opened, Page};
use super::requests::Scope;
use super::{Workspace, icons, px, theme};
use artoo_core::route::Route;
use artoo_shell_api::ItemRow;
use artoo_shell_api::spaces::{
    self, DropAt, Drops, Head, Open, SpaceInfo, SpaceRow, SpacesHost, SpacesMenu, ordered,
};
pub use artoo_shell_api::spaces::{GROUPS, Places, Space};

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
use gpui::{App, Context, Pixels, Point, SharedString, WeakEntity, Window, div, prelude::*};
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
pub(super) struct OpenRow {
    /// The request or branch; None for the page the space keeps when it belongs to no item (Code's file).
    item: Option<Route>,
    icon: gpui::AnyElement,
    pub(super) label: String,
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
    /// A group's head rather than a page (#246).
    head: Option<(u64, Head)>,
    /// How deep in groups it stands (Design round 48).
    depth: usize,
}

impl OpenRow {
    /// What it is to a click.
    fn kind(&self) -> Kind {
        match (&self.head, &self.item) {
            (Some((id, _)), _) => Kind::Group(*id),
            (None, item) => Kind::Page(item.clone()),
        }
    }
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

    /// The rows open under `space`: the page it keeps when that belongs to no item (Code's file), then its pages and
    /// groups as the tree has them (`groups`, synced), a folded group's own left out; each page as its tab's pages say
    /// it (`Pages::row`). The Groups tab's are its groups, of pages of any tab.
    fn open_rows(&self, space: Space, groups: &Groups, cx: &App) -> Vec<OpenRow> {
        let mut rows = Vec::new();
        if space != GROUPS
            && let Some(page) = self.page_of(space).filter(|p| self.item_of(p).is_none())
            && let Some(row) = self
                .space_pages(space)
                .and_then(|p| (p.row)(page, page, cx))
        {
            rows.push(OpenRow {
                item: None,
                icon: row.icon,
                label: row.label,
                mono: row.mono,
                tip: row.tip,
                on: self.space() == Some(space),
                drafts: false,
                preview: false,
                news: row.news,
                agent: None,
                call: None,
                head: None,
                depth: 0,
            });
        }
        // The groups the page on screen is under: a folded one's head is marked for it.
        let mut above = Vec::new();
        if let Some(item) = self.nav_page().and_then(|p| self.item_of(p)) {
            let mut at = Unit::Page(item);
            while let Some((List::Group(id), _)) = groups.find(&at) {
                above.push(id);
                at = Unit::Group(id);
            }
        }
        for (depth, row) in groups.rows(&super::grouping::list_of(space)) {
            match row {
                Row::Page(item) => {
                    if let Some(open) = self.nav.opened(item) {
                        let mut row = self.item_row(open, cx);
                        row.depth = depth;
                        rows.push(row);
                    }
                }
                Row::Head(group) => rows.push(OpenRow {
                    item: None,
                    icon: div().into_any_element(),
                    label: group.name.clone(),
                    mono: false,
                    tip: format!("{}  double-click renames", group.name),
                    on: group.shut && above.contains(&group.id),
                    drafts: false,
                    preview: false,
                    news: false,
                    agent: None,
                    call: None,
                    head: Some((
                        group.id,
                        Head {
                            count: groups.pages(group.id).len(),
                            shut: group.shut,
                        },
                    )),
                    depth,
                }),
            }
        }
        rows
    }

    /// Open page `open`'s row, as its tab's pages say it (`Pages::row`), one they say nothing of by its kind, with
    /// what the tab adds: its dots, the name the reviewer gave it, the agent that gathered it.
    pub(super) fn item_row(&self, open: &Opened, cx: &App) -> OpenRow {
        let space = self.item_space(&open.item);
        let on = space.is_some() && self.space() == space && self.is_shown(&open.item);
        let pages = space.and_then(|s| self.space_pages(s));
        let loaded = space
            .and_then(|s| self.page_of(s))
            .and_then(|p| self.item_of(p));
        // The one loaded says its files as they are now.
        let now: Vec<String>;
        let paths = match &self.shown_diff(cx) {
            Some(diff) if loaded.as_ref() == Some(&open.item) => {
                now = diff.files.iter().map(|f| f.path().to_owned()).collect();
                &now
            }
            _ => &open.paths,
        };
        // Unsent drafts on its files, or a note a plugin says its item holds (an issue's).
        let drafts = self
            .comments(cx)
            .iter()
            .any(|c| c.state == crate::comment::State::Draft && paths.contains(&c.path))
            || artoo_shell_api::spaces::has_note(cx, &open.item);
        let row =
            (pages.and_then(|p| (p.row)(&open.item, &open.page, cx))).unwrap_or_else(|| ItemRow {
                icon: icons::icon("layers")
                    .size(px(14.))
                    .text_color(theme::faint())
                    .into_any_element(),
                label: open.page.kind.clone(),
                mono: true,
                tip: open.page.kind.clone(),
                news: false,
            });
        // A name the reviewer gave it replaces its title, its number kept.
        let named = self.page_name(&open.item).map(str::to_owned);
        let (row_label, row_tip) = match &named {
            Some(name) => (
                super::nav::named_label(&row.label, name),
                format!("{}  named by you", row.tip),
            ),
            None => (row.label.clone(), row.tip.clone()),
        };
        // A request an agent gathered: its line under the title, its mark and its comments waiting (#148).
        let gathered = match Page::from_route(&open.item) {
            Some(Page::Request(iid)) => self.gathered(iid, cx).map(|g| (iid, g)),
            _ => None,
        };
        let tip = match gathered.and_then(|(_, g)| g.note.as_deref().map(|n| (g, n))) {
            Some((g, note)) if !note.is_empty() => format!("{row_tip}\n{}: {note}", g.agent),
            Some((g, _)) => format!("{row_tip}\nGathered by {}", g.agent),
            None => row_tip,
        };
        let agent = gathered.map(|(iid, g)| {
            let (letter, hue) = self.agents.agent_mark(&g.agent, cx);
            (letter, hue, self.agent_waiting(iid, &g.agent, cx))
        });
        OpenRow {
            item: Some(open.item.clone()),
            icon: row.icon,
            label: row_label,
            mono: row.mono && named.is_none(),
            tip,
            on,
            drafts,
            preview: open.preview,
            news: row.news,
            agent,
            call: self.agent_calls.get(&open.item).filter(|_| !on).cloned(),
            head: None,
            depth: 0,
        }
    }

    /// The spaces the menu offers, the Groups tab after them while it holds a group (Design round 48).
    pub(super) fn offered_spaces(&self, cx: &App) -> Vec<SpaceInfo> {
        let mut offered = spaces::all(cx);
        if !self.nav.groups.shared.is_empty() {
            offered.push(SpaceInfo {
                space: GROUPS,
                label: "Groups",
                icon: "layers",
                order: u16::MAX,
                go: "",
            });
        }
        offered
    }

    /// The title of open item `item`'s row, without its number: what Rename… starts from.
    pub(super) fn row_title(&self, item: &Route, cx: &App) -> Option<String> {
        let row = self.item_row(self.nav.opened(item)?, cx);
        Some(super::nav::split_number(&row.label).1.to_owned())
    }

    /// The row of `item` while it is renamed: its number, then the field its name is typed in.
    fn rename_row(&self, item: &Route, cx: &Context<Self>) -> Option<gpui::AnyElement> {
        if self.renaming.as_ref() != Some(item) || self.field != Some(super::lists::Field::Rename) {
            return None;
        }
        let label = self.item_row(self.nav.opened(item)?, cx).label;
        let number = super::nav::split_number(&label).0.to_owned();
        let field = self.name_field(cx);
        Some(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .items_center()
                .gap(px(6.))
                .children((!number.is_empty()).then(|| div().flex_none().child(number)))
                .child(field)
                .into_any_element(),
        )
    }

    /// The field a page's or a group's name is typed in, in its row.
    fn name_field(&self, cx: &Context<Self>) -> gpui::AnyElement {
        div()
            .id("rename-field")
            .flex_1()
            .min_w_0()
            .flex()
            .items_center()
            // No box of its own: the row it stands in is ringed (the design's rename in place).
            .h(px(24.))
            .text_color(theme::text())
            .cursor_text()
            .child(self.field_look(
                &self.label_name,
                true,
                "Name",
                super::input::Look::default(),
                cx.entity().downgrade(),
            ))
            .on_mouse_down_out(cx.listener(|this, _: &gpui::MouseDownEvent, _, cx| {
                if this.renaming.is_some() || this.renaming_group.is_some() {
                    this.save_rename(cx);
                }
            }))
            .into_any_element()
    }

    /// An open item's words for ⌘E (#194): its row's label, the review's part as a glyph with its sentence
    /// (`3/8`, Reviewed by you: 3 of 8 files), and its dots, drafts and news, as its row in the menu wears them.
    pub(super) fn open_words(&self, open: &super::nav::Opened, cx: &App) -> OpenWords {
        use artoo_shell_api::palette::Dot;
        let space = self.item_space(&open.item);
        let pages = space.and_then(|s| self.space_pages(s));
        let row = pages.and_then(|p| (p.row)(&open.item, &open.page, cx));
        let news = row.as_ref().is_some_and(|r| r.news);
        let label = row.map_or_else(|| open.page.kind.clone(), |row| row.label);
        let drafts = (self.comments(cx).iter())
            .any(|c| c.state == crate::comment::State::Draft && open.paths.contains(&c.path));
        let review = match Page::from_route(&open.item) {
            Some(Page::Request(iid)) => {
                let head = (self.requests_api())
                    .and_then(|r| r.find(iid, cx))
                    .and_then(|m| git2::Oid::from_str(&m.head).ok());
                match head {
                    Some(h) if self.rv().head_reviewed(h, cx) => {
                        Some(("all".to_owned(), "Reviewed by you: every file".to_owned()))
                    }
                    Some(h) => self.rv().head_part(h, cx).map(|(done, total)| {
                        (
                            format!("{done}/{total}"),
                            format!("Reviewed by you: {done} of {total} files"),
                        )
                    }),
                    None => None,
                }
            }
            _ => None,
        };
        let dots = [(drafts, Dot::Drafts), (news, Dot::News)]
            .into_iter()
            .filter_map(|(on, dot)| on.then_some(dot))
            .collect();
        OpenWords {
            label,
            detail: if open.preview {
                "looked at".to_owned()
            } else {
                String::new()
            },
            review,
            dots,
        }
    }

    /// `from` dragged onto `to` in the menu: it takes `to`'s place, kept in `layout.txt`.
    fn move_space(&mut self, from: Space, to: Space, cx: &mut Context<Self>) {
        let mut order: Vec<Space> = ordered(&self.layout.spaces, &self.offered_spaces(cx))
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
        // The Groups tab has no page: a click shows its tree (Design round 48).
        if space == GROUPS {
            if self.nav.folded.contains(&GROUPS) {
                self.toggle_open_items(GROUPS, cx);
            }
            return;
        }
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
        // The tree as the pages are open now, and the row being dragged: where it may go is said on each place.
        let (groups, tabs): (Groups, Tabs) = (self.synced_groups(), self.open_tabs());
        let dragged = spaces::dragging(cx).and_then(|key| Unit::read(&key));
        let dragged = dragged.as_ref();
        for info in ordered(&self.layout.spaces, &self.offered_spaces(cx)) {
            let space = info.space;
            // The key that goes there in force: one changed in Settings → Keyboard shows the new one.
            let key = actions
                .iter()
                .find(|a| a.id == info.go)
                .zip(keymap)
                .map(|(a, keymap)| keymap.key_now(a.act.as_ref()))
                .unwrap_or_default();
            let missing = self.missing(space, cx);
            let opens = self.open_rows(space, &groups, cx);
            items.push((space, opens.iter().map(OpenRow::kind).collect()));
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
                    editing: match open.head {
                        Some((id, _)) => (self.renaming_group == Some(id)
                            && self.field == Some(super::lists::Field::Rename))
                        .then(|| self.name_field(cx)),
                        None => open.item.as_ref().and_then(|i| self.rename_row(i, &*cx)),
                    },
                    key: match (&open.head, &open.item) {
                        (Some((id, _)), _) => Unit::Group(*id).key().into(),
                        (None, Some(item)) => Unit::Page(item.clone()).key().into(),
                        (None, None) => Default::default(),
                    },
                    drops: match (&open.head, &open.item) {
                        (Some((id, _)), _) => {
                            self.row_drops(&groups, &tabs, dragged, &Unit::Group(*id))
                        }
                        (None, Some(item)) => {
                            self.row_drops(&groups, &tabs, dragged, &Unit::Page(item.clone()))
                        }
                        (None, None) => Drops::default(),
                    },
                    head: open.head.map(|(_, head)| head),
                    depth: open.depth,
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
                drops: self.list_drops(&groups, &tabs, dragged, space),
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
                drops: Drops::default(),
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
                end: match dragged {
                    Some(Unit::Group(_)) => self.list_drops(&groups, &tabs, dragged, GROUPS),
                    _ => Drops::default(),
                },
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

/// What a row under a space is.
#[derive(Clone)]
enum Kind {
    /// Its request or branch; none for Code's file.
    Page(Option<Route>),
    /// A group's head (#246).
    Group(u64),
}

/// What a click on the menu does, answered by the tab.
struct Glue {
    tab: WeakEntity<Workspace>,
    /// What each row under a space is.
    items: Vec<(Space, Vec<Kind>)>,
}

impl Glue {
    /// Row `n` under `space`, when it is there.
    fn item(&self, space: Space, n: usize) -> Option<Kind> {
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
                Kind::Page(Some(item)) if shift => this.pick_open(item, cx),
                Kind::Page(Some(item)) => {
                    this.nav.picked.clear();
                    this.show_open(&item, cx)
                }
                Kind::Page(None) => this.go_space(space, cx),
                Kind::Group(id) => this.fold_group(id, cx),
            })
            .ok();
    }

    fn close(&self, space: Space, n: usize, _: &mut Window, cx: &mut App) {
        let Some(item) = self.item(space, n) else {
            return;
        };
        self.tab
            .update(cx, |this, cx| match &item {
                Kind::Page(Some(item)) => this.close_open(item, cx),
                Kind::Page(None) => this.close_shown(cx),
                // A head has no ×.
                Kind::Group(_) => {}
            })
            .ok();
    }

    fn rename(&self, space: Space, n: usize, _: &mut Window, cx: &mut App) {
        let Some(item) = self.item(space, n) else {
            return;
        };
        self.tab
            .update(cx, |this, cx| match item {
                Kind::Page(Some(item)) => this.start_rename(item, cx),
                Kind::Page(None) => {}
                // The first of the two clicks folded it: back, and its name typed.
                Kind::Group(id) => {
                    this.fold_group(id, cx);
                    this.start_group_rename(id, cx)
                }
            })
            .ok();
    }

    fn dropped(
        &self,
        _: Space,
        from: SharedString,
        to: SharedString,
        at: DropAt,
        _: &mut Window,
        cx: &mut App,
    ) {
        self.tab
            .update(cx, |this, cx| {
                if let Some(to) = Unit::read(&to) {
                    this.drop_unit(&from, super::groups::Target::Row(to, at.into()), cx)
                }
            })
            .ok();
    }

    fn dropped_on(&self, from: SharedString, space: Space, _: &mut Window, cx: &mut App) {
        let to = super::groups::Target::List(super::grouping::list_of(space));
        self.tab
            .update(cx, |this, cx| this.drop_unit(&from, to, cx))
            .ok();
    }

    fn row_menu(&self, space: Space, n: usize, at: Point<Pixels>, _: &mut Window, cx: &mut App) {
        match self.item(space, n) {
            Some(Kind::Page(Some(item))) => {
                self.tab
                    .update(cx, |this, cx| this.item_context(&item, at, cx))
                    .ok();
            }
            Some(Kind::Group(id)) => {
                self.tab
                    .update(cx, |this, cx| this.group_context(id, at, cx))
                    .ok();
            }
            _ => {}
        }
    }
}

/// An open item as ⌘E lists it.
pub(super) struct OpenWords {
    pub label: String,
    /// What is said at its end: `looked at`, or nothing.
    pub detail: String,
    /// How far the review got: the short word and its sentence.
    pub review: Option<(String, String)>,
    pub dots: Vec<artoo_shell_api::palette::Dot>,
}
