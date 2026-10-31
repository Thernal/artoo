//! Where the window is (#71). Each space keeps one page, and its stack follows from it: a page's
//! parent is the page above it (`Page::parent`) — Merge requests › !398 › a commit of it. Esc goes
//! one page up and a space's root has none above it; ⌘[ ⌘] walk every page seen, across spaces; a
//! page reached from another space by a link says where it came from (‹ Inbox). The requests and
//! branches opened stay under their space in the menu, each with the page, file and scroll it was
//! left at; × or ⌘W closes one. A project reopens on the space and pages it was left on
//! (`nav.txt`).
//!
//! The page decides what is loaded: `show` loads it when the loaded branch, selection or request
//! differ from it, and the places that open something (a list's row, ⌘K, a notification, a link)
//! say where they went through `arrive`.

use super::spaces::Space;
use super::{Selection, Workspace, recents, zones};
use artoo_branches::opening::Opening;
use artoo_core::route::Route;
use artoo_ui_kit::format;
use git2::Oid;
use gpui::{App, Context, ListOffset};

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Page {
    /// A file read in Code.
    File(String),
    /// A folder of the project in Code, by its path: its notes for the agent, its files, its
    /// README (#97).
    Folder(String),
    /// A branch, by its refname: off the main line its Commits · Changes (#74), the main line its
    /// commits with one of them shown.
    Branch(String),
    /// A commit of a branch off the main line: a level under its Commits (#74).
    BranchCommit(String, Oid),
    /// A run of a branch's commits picked with shift, newest and oldest: one diff, a level under
    /// its Commits.
    BranchCommits(String, Oid, Oid),
    /// Two versions of a branch compared.
    Versions(String, usize, usize),
    /// What is not committed, against `HEAD`.
    WorkingTree,
    /// Two refs compared (⌥⌘C): what the head changed against the base, since their merge base
    /// or directly (#147).
    Compare((String, Oid), (String, Oid), bool),
    Request(u64),
    /// A commit of a request.
    RequestCommit(u64, Oid),
    /// A run of a request's commits picked with shift, newest and oldest.
    RequestCommits(u64, Oid, Oid),
    /// Two versions of a request compared.
    RequestVersions(u64, usize, usize),
}

impl Page {
    pub fn space(&self) -> Space {
        match self {
            Page::File(_) | Page::Folder(_) => Space::Code,
            Page::Branch(_)
            | Page::BranchCommit(..)
            | Page::BranchCommits(..)
            | Page::Versions(..)
            | Page::WorkingTree
            | Page::Compare(..) => Space::Branches,
            Page::Request(_)
            | Page::RequestCommit(..)
            | Page::RequestCommits(..)
            | Page::RequestVersions(..) => Space::Requests,
        }
    }

    /// The page one up; None when that is the space's root.
    pub fn parent(&self) -> Option<Page> {
        match self {
            Page::Versions(refname, ..)
            | Page::BranchCommit(refname, _)
            | Page::BranchCommits(refname, ..) => Some(Page::Branch(refname.clone())),
            Page::RequestCommit(iid, _)
            | Page::RequestCommits(iid, ..)
            | Page::RequestVersions(iid, ..) => Some(Page::Request(*iid)),
            // A file's or a folder's is the folder it is in, as the path says (A, 2026-10-23);
            // at the top, the project's page.
            Page::File(path) | Page::Folder(path) => path
                .rsplit_once('/')
                .map(|(dir, _)| Page::Folder(dir.to_owned())),
            Page::Branch(_) | Page::WorkingTree | Page::Compare(..) | Page::Request(_) => None,
        }
    }

    /// The request or branch it belongs to, kept open under its space.
    pub fn item(&self) -> Option<Item> {
        match self {
            Page::Branch(refname)
            | Page::BranchCommit(refname, _)
            | Page::BranchCommits(refname, ..)
            | Page::Versions(refname, ..) => Some(Item::Branch(refname.clone())),
            Page::Request(iid)
            | Page::RequestCommit(iid, _)
            | Page::RequestCommits(iid, ..)
            | Page::RequestVersions(iid, ..) => Some(Item::Request(*iid)),
            Page::WorkingTree => Some(Item::WorkingTree),
            Page::Compare(base, head, _) => Some(Item::compare(&base.0, &head.0)),
            Page::File(_) | Page::Folder(_) => None,
        }
    }

    /// A page as `nav.txt` kept it before routes (#165): read, never written.
    fn parse_key(key: &str) -> Option<Page> {
        let (kind, rest) = key.split_once(':').unwrap_or((key, ""));
        let mut parts = rest.splitn(4, ':');
        let mut next = || parts.next();
        Some(match kind {
            "file" => Page::File(rest.to_owned()),
            "folder" => Page::Folder(rest.to_owned()),
            "branch" => Page::Branch(rest.to_owned()),
            "branch-commit" => {
                let (id, refname) = rest.split_once(':')?;
                Page::BranchCommit(refname.to_owned(), Oid::from_str(id).ok()?)
            }
            "branch-commits" => {
                let (a, b) = (Oid::from_str(next()?).ok()?, Oid::from_str(next()?).ok()?);
                Page::BranchCommits(next()?.to_owned(), a, b)
            }
            "versions" => {
                let (a, b) = (next()?.parse().ok()?, next()?.parse().ok()?);
                Page::Versions(next()?.to_owned(), a, b)
            }
            "working-tree" => Page::WorkingTree,
            "compare" | "compare-direct" => {
                let (a, b) = (Oid::from_str(next()?).ok()?, Oid::from_str(next()?).ok()?);
                let (base, head) = (next()?.to_owned(), next()?.to_owned());
                Page::Compare((base, a), (head, b), kind == "compare")
            }
            "request" => Page::Request(rest.parse().ok()?),
            "request-commit" => {
                Page::RequestCommit(next()?.parse().ok()?, Oid::from_str(next()?).ok()?)
            }
            "request-commits" => Page::RequestCommits(
                next()?.parse().ok()?,
                Oid::from_str(next()?).ok()?,
                Oid::from_str(next()?).ok()?,
            ),
            "request-versions" => Page::RequestVersions(
                next()?.parse().ok()?,
                next()?.parse().ok()?,
                next()?.parse().ok()?,
            ),
            _ => return None,
        })
    }
}

/// What stays open under a space in the menu.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Item {
    Request(u64),
    Branch(String),
    /// Two refs compared (#149), by their names whichever is the base: Swap and Direct keep its
    /// row.
    Compare(String, String),
    /// What is not committed (#149).
    WorkingTree,
}

impl Item {
    pub fn space(&self) -> Space {
        match self {
            Item::Request(_) => Space::Requests,
            Item::Branch(_) | Item::Compare(..) | Item::WorkingTree => Space::Branches,
        }
    }

    /// The comparison of `a` and `b`, in either order.
    pub fn compare(a: &str, b: &str) -> Item {
        let (a, b) = if a <= b { (a, b) } else { (b, a) };
        Item::Compare(a.to_owned(), b.to_owned())
    }

    /// Its page when it is opened anew; a comparison's needs its sides' commits.
    fn page(&self) -> Option<Page> {
        match self {
            Item::Request(iid) => Some(Page::Request(*iid)),
            Item::Branch(refname) => Some(Page::Branch(refname.clone())),
            Item::WorkingTree => Some(Page::WorkingTree),
            Item::Compare(..) => None,
        }
    }

    /// An item as `nav.txt` kept it before routes (#165): read, never written.
    fn parse(key: &str) -> Option<Item> {
        match key.split_once(':')? {
            ("request", iid) => Some(Item::Request(iid.parse().ok()?)),
            ("branch", refname) => Some(Item::Branch(refname.to_owned())),
            _ => None,
        }
    }
}

/// A request's section tabs (#73): tabs, not levels — the request is one page, a commit of it a
/// level under Commits.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Section {
    Overview,
    Commits,
    Changes,
}

impl Section {
    pub const ALL: [Section; 3] = [Section::Overview, Section::Commits, Section::Changes];

    pub fn label(self) -> &'static str {
        match self {
            Section::Overview => "Overview",
            Section::Commits => "Commits",
            Section::Changes => "Changes",
        }
    }
}

pub use artoo_branches::roots::BranchRow;

/// A request or branch open under its space, and where it was left.
#[derive(Clone)]
pub struct Opened {
    pub item: Item,
    /// The page it was on.
    pub page: Page,
    /// The commit its branch page showed.
    commit: Option<Oid>,
    /// The file and scroll it was at.
    place: Option<(String, ListOffset)>,
    /// The files of its diff when it was last shown: an agent draft on one is unsent work here.
    pub paths: Vec<String>,
    /// A request's section tab (#73).
    pub section: Section,
}

/// What `nav.txt` keeps of a project.
struct Saved {
    repo: String,
    space: Space,
    pages: [Option<Page>; 4],
    /// Each with its page when the item alone cannot say it: a comparison's.
    open: Vec<(Item, Option<Page>)>,
}

/// A page seen: ⌘[ ⌘] come back to it, with the commit a branch's page showed.
#[derive(Clone, PartialEq, Debug)]
struct Moment {
    space: Space,
    page: Option<Page>,
    commit: Option<Oid>,
}

pub struct Nav {
    pub space: Space,
    /// Each space's page; None at its root.
    pages: [Option<Page>; 4],
    time: Vec<Moment>,
    at: usize,
    /// The space a link came from, while its page is on screen (‹ Inbox).
    pub from: Option<Space>,
    pub open: Vec<Opened>,
    /// The spaces whose open items the menu folds away (Branches at first).
    pub folded: [bool; 4],
    /// A space and page to show once the request list is in (a project reopened on one).
    pending_show: Option<(Space, Option<Page>)>,
    /// The commit a request's Commits section has selected (#73), and with shift the other end
    /// of a run of them.
    pub pick_commit: Option<Oid>,
    pub pick_to: Option<Oid>,
    /// The menu's open items picked with shift, the first the one it began from: ⌘W closes them.
    pub picked: Vec<Item>,
    /// Loading what a page needs, or coming back to a moment: no new moment, no link.
    restoring: bool,
}

impl Default for Nav {
    fn default() -> Self {
        Self {
            space: Space::Branches,
            pages: Default::default(),
            time: Vec::new(),
            at: 0,
            from: None,
            open: Vec::new(),
            folded: [false, false, true, false],
            pending_show: None,
            pick_commit: None,
            picked: Vec::new(),
            pick_to: None,
            restoring: false,
        }
    }
}

impl Nav {
    pub fn can_back(&self) -> bool {
        self.at > 0
    }

    pub fn can_forward(&self) -> bool {
        self.at + 1 < self.time.len()
    }

    /// A new moment; the ones ahead of it are dropped, the same one twice is one. A branch's page
    /// shown before its commit was in becomes the one with the commit.
    fn remember(&mut self, moment: Moment) {
        if let Some(last) = self.time.get_mut(self.at) {
            if *last == moment {
                return;
            }
            if last.space == moment.space && last.page == moment.page && last.commit.is_none() {
                *last = moment;
                return;
            }
        }
        let keep = (self.at + 1).min(self.time.len());
        self.time.truncate(keep);
        self.time.push(moment);
        // A long day is a long list: the oldest go.
        if self.time.len() > 200 {
            self.time.remove(0);
        }
        self.at = self.time.len() - 1;
    }

    pub(super) fn opened(&self, item: &Item) -> Option<&Opened> {
        self.open.iter().find(|o| &o.item == item)
    }

    fn opened_mut(&mut self, item: &Item) -> Option<&mut Opened> {
        self.open.iter_mut().find(|o| &o.item == item)
    }

    /// `nav.txt`'s line for project `repo`: the space, each space's page, the open items.
    fn line(&self, repo: &str) -> String {
        let pages: Vec<String> = self
            .pages
            .iter()
            .map(|p| {
                p.as_ref()
                    .map_or_else(|| "-".to_owned(), |p| p.route().text())
            })
            .collect();
        // An item as its own page; a comparison as the page it was left on: its sides' commits come
        // back with it (#149).
        let open: Vec<String> = self
            .open
            .iter()
            .map(|o| {
                o.item
                    .page()
                    .unwrap_or_else(|| o.page.clone())
                    .route()
                    .text()
            })
            .collect();
        format!(
            "{repo}\t{}\t{}\t{}",
            self.space.ix(),
            pages.join("\t"),
            open.join(" ")
        )
    }

    /// What `line` kept: the space, the pages and the open items.
    fn read(line: &str) -> Option<Saved> {
        let mut cols = line.split('\t');
        let repo = cols.next()?.to_owned();
        let space = Space::ALL
            .get(cols.next()?.parse::<usize>().ok()?)
            .copied()?;
        // Each kept as a route; a line written before routes as the keys it had then.
        let page_of = |text: &str| {
            Route::parse(text)
                .and_then(|route| Page::from_route(&route))
                .or_else(|| Page::parse_key(text))
        };
        let mut pages: [Option<Page>; 4] = Default::default();
        for page in pages.iter_mut() {
            *page = page_of(cols.next()?);
        }
        let open = cols
            .next()
            .unwrap_or_default()
            .split(' ')
            .filter_map(|key| match page_of(key) {
                // A request's or a branch's own page is the item alone.
                Some(page @ (Page::Request(_) | Page::Branch(_))) => Some((page.item()?, None)),
                Some(page) => Some((page.item()?, Some(page))),
                None => Some((Item::parse(key)?, None)),
            })
            .collect();
        Some(Saved {
            repo,
            space,
            pages,
            open,
        })
    }
}

const FILE: &str = "nav.txt";

impl Workspace {
    /// The page of the space on screen; None at its root.
    pub(super) fn nav_page(&self) -> Option<&Page> {
        self.nav.pages[self.nav.space.ix()].as_ref()
    }

    /// The page space `space` keeps.
    pub(super) fn page_of(&self, space: Space) -> Option<&Page> {
        self.nav.pages[space.ix()].as_ref()
    }

    /// At the root of Branches or Merge requests: their list is the page.
    pub(super) fn at_root_list(&self) -> bool {
        matches!(self.nav.space, Space::Branches | Space::Requests) && self.nav_page().is_none()
    }

    /// What is loaded, as a page of Branches or Merge requests (`History::loaded_route`).
    pub(super) fn loaded_page(&self, cx: &App) -> Option<Page> {
        let shown = self.review.read(cx).diff.as_ref().map(|d| &d.header);
        let on_screen = self.nav_page().map(Page::route);
        let route = self.history.read(cx).loaded_route(
            shown,
            self.requests.read(cx).current,
            on_screen.as_ref(),
        )?;
        Page::from_route(&route)
    }

    /// The commit the page shows, when it shows one.
    fn shown_commit(&self, cx: &App) -> Option<Oid> {
        match self.history.read(cx).selection {
            Selection::Commit(ix) => self.history.read(cx).commits.get(ix).map(|c| c.id),
            _ => None,
        }
    }

    /// Something was opened: its space and page come on screen, over the Inbox or Settings. From
    /// another space it was a link, and the page says where it came from.
    pub(super) fn arrive(&mut self, page: Page, cx: &App) {
        let space = page.space();
        if !self.nav.restoring && space != self.nav.space {
            self.nav.from = Some(self.nav.space);
        }
        self.inbox = space == Space::Inbox;
        self.settings_open = false;
        self.set_page(page, cx);
    }

    /// The section a request opened anew starts on: Overview when you have never been inside it,
    /// else Changes (#73).
    fn first_section(&self, item: &Item, cx: &App) -> Section {
        match item {
            Item::Request(iid) if !self.opened_before(*iid, cx) => Section::Overview,
            _ => Section::Changes,
        }
    }

    /// The section tabs of the page's item: a request's three (#73), a branch off the main line's
    /// Commits and Changes (#74); none for anything else.
    pub(super) fn item_sections(&self, cx: &App) -> &'static [Section] {
        match self.nav_page() {
            Some(
                Page::Request(_)
                | Page::RequestCommit(..)
                | Page::RequestCommits(..)
                | Page::RequestVersions(..),
            ) => &Section::ALL,
            Some(
                Page::Branch(refname)
                | Page::BranchCommit(refname, _)
                | Page::BranchCommits(refname, ..),
            ) if self.history.read(cx).tabbed(refname) => &[Section::Commits, Section::Changes],
            _ => &[],
        }
    }

    /// The section tab of the item on screen: a commit of it is under Commits, two versions
    /// compared under Changes.
    pub(super) fn page_section(&self, cx: &App) -> Option<Section> {
        if self.item_sections(cx).is_empty() {
            return None;
        }
        let item = self.nav_page()?.item()?;
        Some(
            self.nav
                .opened(&item)
                .map_or(Section::Changes, |o| o.section),
        )
    }

    /// The item's tab on screen when it is the tabbed page itself, not a level under it.
    pub(super) fn page_tab(&self, cx: &App) -> Option<Section> {
        matches!(self.nav_page(), Some(Page::Request(_) | Page::Branch(_)))
            .then(|| self.page_section(cx))
            .flatten()
    }

    /// Section `section` of the item on screen; from a level under it, the item's page again.
    pub(super) fn show_section(&mut self, section: Section, cx: &mut Context<Self>) {
        if !self.item_sections(cx).contains(&section) {
            return;
        }
        let Some(page) = self.nav_page().cloned() else {
            return;
        };
        let Some(item) = page.item() else {
            return;
        };
        if let Some(open) = self.nav.opened_mut(&item) {
            open.section = section;
        }
        let top = item.page().unwrap_or_else(|| page.clone());
        if page != top {
            self.show(page.space(), Some(top), cx);
        }
        cx.notify();
    }

    /// ⌃⇥ and ⌃⇧⇥: the next or the previous section tab, going round.
    pub(super) fn step_section(&mut self, forward: bool, cx: &mut Context<Self>) {
        // In Code they walk the files open as tabs (#75).
        if self.nav.space == Space::Code {
            return self.step_code_tab(forward, cx);
        }
        let (sections, Some(at)) = (self.item_sections(cx), self.page_section(cx)) else {
            return;
        };
        let n = sections.len();
        let ix = sections.iter().position(|s| *s == at).unwrap_or(0);
        self.show_section(sections[(ix + if forward { 1 } else { n - 1 }) % n], cx);
    }

    /// `page` becomes its space's and the space the one on screen; the request or branch it is of
    /// stays open in the menu; a moment of time.
    fn set_page(&mut self, page: Page, cx: &App) {
        let space = page.space();
        self.keep_place(cx);
        self.nav.space = space;
        self.nav.pages[space.ix()] = Some(page.clone());
        if let Some(item) = page.item() {
            // The commit shown is this item's only when it is the one loaded.
            let commit = self
                .shown_commit(cx)
                .filter(|_| self.loaded_page(cx).and_then(|p| p.item()).as_ref() == Some(&item));
            match self.nav.opened_mut(&item) {
                Some(open) => {
                    open.page = page.clone();
                    open.commit = commit.or(open.commit);
                }
                None => {
                    let section = self.first_section(&item, cx);
                    self.nav.open.push(Opened {
                        item: item.clone(),
                        page: page.clone(),
                        commit,
                        place: None,
                        paths: Vec::new(),
                        section,
                    })
                }
            }
            // A commit of a request is a level under Commits, picked there when esc comes back to
            // it; two versions compared are Changes (#73).
            let section = match &page {
                Page::RequestCommit(_, id) | Page::BranchCommit(_, id) => {
                    self.nav.pick_commit = Some(*id);
                    self.nav.pick_to = None;
                    Some(Section::Commits)
                }
                // A run is picked there as it was, newest first.
                Page::RequestCommits(_, newest, oldest)
                | Page::BranchCommits(_, newest, oldest) => {
                    self.nav.pick_commit = Some(*newest);
                    self.nav.pick_to = Some(*oldest);
                    Some(Section::Commits)
                }
                Page::RequestVersions(..) => Some(Section::Changes),
                _ => None,
            };
            if let Some(section) = section
                && let Some(open) = self.nav.opened_mut(&item)
            {
                open.section = section;
            }
        }
        self.remember_moment(cx);
        self.save_nav();
    }

    /// What is loaded comes on screen as its page, from wherever the window was (a ⌘K result, a
    /// link to a commit).
    pub(super) fn follow_loaded(&mut self, cx: &App) {
        if let Some(page) = self.loaded_page(cx) {
            self.arrive(page, cx);
        }
    }

    /// What is selected inside the open request or branch changed (a commit, versions): the page
    /// follows. Not while a request is still loading, nor while a page is being put back.
    pub(super) fn record(&mut self, cx: &App) {
        if self.history.read(cx).pending_request.is_some() || self.nav.restoring {
            return;
        }
        let Some(page) = self.loaded_page(cx) else {
            return;
        };
        if page.space() == self.nav.space && page.item() == self.nav_page().and_then(Page::item) {
            self.set_page(page, cx);
        }
    }

    fn remember_moment(&mut self, cx: &App) {
        if self.nav.restoring {
            return;
        }
        let moment = Moment {
            space: self.nav.space,
            page: self.nav_page().cloned(),
            commit: self
                .shown_commit(cx)
                .filter(|_| matches!(self.nav_page(), Some(Page::Branch(_)))),
        };
        self.nav.remember(moment);
    }

    /// Before the loaded request or branch makes way: where it was, for coming back to it.
    fn keep_place(&mut self, cx: &App) {
        let Some(page) = self.loaded_page(cx) else {
            return;
        };
        let Some(item) = page.item() else {
            return;
        };
        let place = self.review.read(cx).scroll_place();
        let commit = self.shown_commit(cx);
        let paths: Vec<String> = self
            .review
            .read(cx)
            .diff
            .as_ref()
            .map(|d| d.files.iter().map(|f| f.path().to_owned()).collect())
            .unwrap_or_default();
        if let Some(open) = self.nav.opened_mut(&item) {
            open.page = page;
            if place.is_some() {
                open.place = place;
            }
            open.commit = commit.or(open.commit);
            if !paths.is_empty() {
                open.paths = paths;
            }
        }
    }

    /// Shows `page` of `space`, loading what it needs. The menu, esc, ⌘[ ⌘] and the open items
    /// come here.
    pub(super) fn show(&mut self, space: Space, page: Option<Page>, cx: &mut Context<Self>) {
        self.keep_place(cx);
        self.nav.space = space;
        self.nav.pages[space.ix()] = page.clone();
        self.history.update(cx, |h, _| h.waiting = None);
        self.inbox = space == Space::Inbox;
        self.settings_open = false;
        self.nav.restoring = true;
        match page {
            None => self.enter_root(space, cx),
            Some(page) => self.load_page(page, cx),
        }
        self.nav.restoring = false;
        self.remember_moment(cx);
        self.save_nav();
        cx.notify();
    }

    /// A space's root: the Inbox's list, Code's project, the list of branches or requests.
    fn enter_root(&mut self, space: Space, cx: &mut Context<Self>) {
        match space {
            Space::Inbox => {
                // The newest row: a notice, or an agent's answer (#116).
                match self.inbox_rows(cx).first().copied() {
                    Some(super::inbox::InboxRow::Agent(id)) => {
                        self.inbox_view.update(cx, |v, _| v.agent = Some(id))
                    }
                    Some(super::inbox::InboxRow::Notice(ix)) => {
                        self.inbox_view.update(cx, |v, _| v.agent = None);
                        self.notices.update(cx, |n, _| n.at = ix);
                    }
                    None => self.notices.update(cx, |n, _| n.at = 0),
                }
                self.refresh_notices(cx);
            }
            Space::Code => self.show_pages(space, None, false, cx),
            Space::Branches => {
                let first = self.first_branch_row(cx);
                self.history
                    .update(cx, |h, _| h.roots.picked = h.roots.picked.or(first));
                self.zone = zones::Zone::Branches;
                self.read_branch_facts(cx);
                self.read_tree_files(cx);
            }
            Space::Requests => {
                self.pick_first_request(cx);
                self.read_preview(cx);
                self.zone = zones::Zone::Branches;
                // A project without a token: the connect panel takes the keyboard.
                if self.requests.read(cx).connectable {
                    self.start_field(super::lists::Field::Token, cx);
                }
            }
        }
    }

    /// Loads what `page` shows, unless it is what is loaded.
    fn load_page(&mut self, page: Page, cx: &mut Context<Self>) {
        // A feature's pages load what they show themselves.
        if self.pages_drawing(page.space(), Some(&page)).is_some() {
            return self.show_pages(page.space(), Some(&page), false, cx);
        }
        let loaded = self.loaded_page(cx);
        if loaded.as_ref() == Some(&page) {
            return;
        }
        let opened = page.item().and_then(|item| self.nav.opened(&item).cloned());
        let same_item =
            loaded.as_ref().and_then(Page::item) == page.item() && page.item().is_some();
        if !same_item {
            self.review.update(cx, |rv, _| {
                rv.restore_scroll = opened.as_ref().and_then(|o| o.place.clone())
            });
        }
        // A branch's or a request's kinds: what showing it takes is the history's (`History::opening`).
        let remembered = opened.as_ref().and_then(|o| o.commit);
        if let Some(opening) = self
            .history
            .update(cx, |h, _| h.opening(&page.route(), same_item, remembered))
        {
            match opening {
                Opening::Branch { ix, keep } => self.select_branch(ix, keep, cx),
                Opening::Changes => self.run_branch_compare(cx),
                Opening::Commit(ix) => self.select_commit(ix, cx),
                Opening::Versions(from, to) => self.select_versions(from, to, cx),
                Opening::Finish => {
                    self.finish_pending(cx);
                }
                Opening::WorkingTree => self.select_working_tree(cx),
                Opening::Compare(base, head, since) => {
                    self.run_compare(base, head, since, None, cx)
                }
                Opening::Request(iid) => self.select_request(iid, cx),
                Opening::RequestChanges((base, head, iid)) => {
                    self.run_compare(base, head, true, Some(iid), cx)
                }
                Opening::Stay => {}
            }
        }
    }

    fn branch_ix(&self, refname: &str, cx: &App) -> Option<usize> {
        self.history
            .read(cx)
            .branches
            .iter()
            .position(|b| b.refname == refname)
    }

    /// What a page waited for is in (a request's commits, a branch's versions): the page is shown
    /// (`History::finish_waiting`). Whether it was.
    pub(super) fn finish_pending(&mut self, cx: &mut Context<Self>) -> bool {
        let review = self.review_handle();
        let Some(finished) = self
            .history
            .update(cx, |h, cx| h.finish_waiting(&*review, cx))
        else {
            return false;
        };
        self.record(cx);
        if let Some(commit) = finished.commit {
            self.remember_place(
                recents::Key::Commit(commit.id),
                commit.summary.clone(),
                format!("commit {}", format::short(commit.id)),
            );
        }
        cx.notify();
        true
    }

    /// Esc, once nothing smaller is open: one page up; nothing at a space's root.
    pub(super) fn go_up(&mut self, cx: &mut Context<Self>) {
        let Some(page) = self.nav_page().cloned() else {
            return;
        };
        let up = page.parent();
        if up.is_none() {
            // Back at the root: the list selects what was open.
            match (&page, page.item()) {
                (Page::WorkingTree, _) => self
                    .history
                    .update(cx, |h, _| h.roots.picked = Some(BranchRow::WorkingTree)),
                (_, Some(Item::Request(iid))) => {
                    self.requests.update(cx, |rq, _| rq.rows.picked = Some(iid));
                    self.requests.read(cx).rows.reveal.set(true);
                }
                (_, Some(Item::Branch(refname))) => {
                    let row = self.branch_ix(&refname, cx).map(BranchRow::Branch);
                    self.history.update(cx, |h, _| h.roots.picked = row);
                }
                (_, Some(Item::Compare(..) | Item::WorkingTree) | None) => {}
            }
            self.nav.from = None;
        }
        self.show(self.nav.space, up, cx);
    }

    /// ⌘[ and ⌘]: the page seen before or after, in whatever space.
    pub(super) fn go_back(&mut self, cx: &mut Context<Self>) {
        if self.nav.can_back() {
            self.nav.at -= 1;
            self.travel(cx);
        }
    }

    pub(super) fn go_forward(&mut self, cx: &mut Context<Self>) {
        if self.nav.can_forward() {
            self.nav.at += 1;
            self.travel(cx);
        }
    }

    fn travel(&mut self, cx: &mut Context<Self>) {
        let Some(moment) = self.nav.time.get(self.nav.at).cloned() else {
            return;
        };
        let at = self.nav.at;
        // The commit a branch's page showed then is the one it shows again.
        if let (Some(Page::Branch(refname)), Some(id)) = (&moment.page, moment.commit) {
            let item = Item::Branch(refname.clone());
            if let Some(open) = self.nav.opened_mut(&item) {
                open.commit = Some(id);
            }
            let loaded = self.loaded_page(cx).and_then(|p| p.item()) == Some(item);
            if loaded && self.page_of(Space::Branches) == moment.page.as_ref() {
                self.nav.space = Space::Branches;
                self.nav.restoring = true;
                if let Some(ix) = self
                    .history
                    .read(cx)
                    .commits
                    .iter()
                    .position(|c| c.id == id)
                {
                    self.select_commit(ix, cx);
                }
                self.nav.restoring = false;
                self.inbox = false;
                self.nav.at = at;
                return cx.notify();
            }
        }
        self.nav.from = None;
        self.show(moment.space, moment.page, cx);
        // Showing it is not a new moment.
        self.nav.at = at;
    }

    /// A space's row or ⌃⌘1…4: the page it was left on; its row again, its root.
    pub(super) fn enter_space(&mut self, space: Space, cx: &mut Context<Self>) {
        let page = if space == self.nav.space && !self.settings_open {
            None
        } else {
            self.page_of(space).cloned()
        };
        self.nav.from = None;
        self.show(space, page, cx);
    }

    /// An open item's row in the menu: back to it, as it was left.
    pub(super) fn show_open(&mut self, item: &Item, cx: &mut Context<Self>) {
        // One an agent gathered starts where its comments are (#148).
        if let Item::Request(iid) = item
            && self.agent_fresh.remove(iid)
        {
            return self.open_gathered(*iid, cx);
        }
        let Some(page) = self.nav.opened(item).map(|o| o.page.clone()) else {
            return;
        };
        self.nav.from = None;
        self.show(item.space(), Some(page), cx);
    }

    /// `page`'s request, comparison or working tree open under its space without loading it, on
    /// Changes, after the others (#148, #149). One open already keeps its place; a comparison not
    /// on screen goes to the commits `page` has. Whether it was added.
    pub(super) fn open_quietly(&mut self, page: Page, cx: &App) -> bool {
        let Some(item) = page.item() else {
            return false;
        };
        let loaded = self.loaded_page(cx).and_then(|p| p.item()).as_ref() == Some(&item);
        if let Some(open) = self.nav.opened_mut(&item) {
            if !loaded && matches!(item, Item::Compare(..)) {
                open.page = page;
            }
            return false;
        }
        self.nav.open.push(Opened {
            item,
            page,
            commit: None,
            place: None,
            paths: Vec::new(),
            section: Section::Changes,
        });
        true
    }

    /// × on an open item, or ⌘W on the one on screen: it leaves the menu; on screen, its space's
    /// root comes.
    pub(super) fn close_open(&mut self, item: &Item, cx: &mut Context<Self>) {
        self.nav.open.retain(|o| &o.item != item);
        self.nav.picked.retain(|o| o != item);
        let space = item.space();
        let shown = self.page_of(space).and_then(Page::item).as_ref() == Some(item);
        if shown {
            self.nav.pages[space.ix()] = None;
            if let Item::Request(iid) = item
                && self.requests.read(cx).current == Some(*iid)
            {
                self.drop_request(cx);
            }
            if space == self.nav.space {
                self.nav.from = None;
                self.show(space, None, cx);
            }
        }
        self.save_nav();
        cx.notify();
    }

    /// ⌘W: closes the open items picked in the menu, else what is open on screen — a request, a
    /// branch, a file's or a folder's tab in Code.
    pub(super) fn close_shown(&mut self, cx: &mut Context<Self>) {
        if self.close_picked(cx) {
            return;
        }
        match self.nav_page().cloned() {
            Some(Page::File(_) | Page::Folder(_)) => self.close_code_tab(cx),
            Some(page) => match page.item() {
                Some(item) => self.close_open(&item, cx),
                None => self.go_up(cx),
            },
            None => {}
        }
    }

    /// The request list came in: requests open in the menu that are not in it any more (merged,
    /// closed) stay there, what became of them read once (#114: no pin needed); a page waiting
    /// for the list is shown.
    pub(super) fn requests_listed(&mut self, cx: &mut Context<Self>) {
        if self.requests.read(cx).error.is_none() {
            let gone: Vec<u64> = self
                .nav
                .open
                .iter()
                .filter_map(|o| match o.item {
                    Item::Request(iid) => Some(iid),
                    _ => None,
                })
                .filter(|iid| self.requests.read(cx).find(*iid).is_none())
                .collect();
            self.read_left(gone, cx);
        }
        if let Some((space, page)) = self.nav.pending_show.take() {
            self.show(space, page, cx);
        }
        // The list came in under its root: the first request is picked and previewed (#72).
        if self.nav.space == Space::Requests && self.at_root_list() {
            self.pick_first_request(cx);
            self.read_preview(cx);
        }
    }

    /// The project just opened: the space and pages it was left on, else its checked-out branch.
    pub(super) fn restore_nav(&mut self, head: Option<usize>, cx: &mut Context<Self>) {
        let repo = self.nav_repo();
        let saved = crate::storage::read(FILE)
            .lines()
            .filter_map(Nav::read)
            .find(|saved| Some(&saved.repo) == repo.as_ref());
        self.nav = Nav::default();
        self.history.update(cx, |h, _| h.waiting = None);
        let Some(Saved {
            space, pages, open, ..
        }) = saved
        else {
            if let Some(ix) = head {
                self.open_branch(ix, None, cx);
            }
            return;
        };
        self.nav.pages = pages;
        for (item, kept) in open {
            let Some(page) = self
                .nav
                .pages
                .iter()
                .flatten()
                .find(|p| p.item().as_ref() == Some(&item))
                .cloned()
                .or(kept)
                .or_else(|| item.page())
            else {
                continue;
            };
            self.nav.open.push(Opened {
                item,
                page,
                commit: None,
                place: None,
                paths: Vec::new(),
                section: Section::Changes,
            });
        }
        let page = self.nav.pages[space.ix()].clone();
        // Underneath a page that loads nothing of its own, the checked-out branch, as before:
        // ⌘K's commits and the files' revision come from it.
        if !matches!(
            page.as_ref().map(Page::space),
            Some(Space::Branches | Space::Requests)
        ) && let Some(ix) = head
        {
            self.nav.restoring = true;
            self.select_branch(ix, None, cx);
            self.nav.restoring = false;
        }
        let needs_list = matches!(page.as_ref().map(Page::space), Some(Space::Requests));
        if needs_list && self.requests.read(cx).loading {
            self.nav.space = space;
            self.nav.pages[space.ix()] = None;
            self.nav.pending_show = Some((space, page));
        } else {
            self.show(space, page, cx);
        }
    }

    fn nav_repo(&self) -> Option<String> {
        self.root.as_ref().map(|r| crate::review::key(r))
    }

    pub(super) fn save_nav(&self) {
        let Some(repo) = self.nav_repo() else {
            return;
        };
        let mut lines: Vec<String> = crate::storage::read(FILE)
            .lines()
            .filter(|l| l.split('\t').next() != Some(repo.as_str()))
            .map(str::to_owned)
            .collect();
        lines.push(self.nav.line(&repo));
        crate::storage::save(FILE, lines.join("\n") + "\n");
    }

    /// ⌥⌘→ ⌥⌘← (Ctrl+PageDown, Ctrl+PageUp elsewhere): the next or the previous item open under
    /// the space — its requests or branches, Code's tabs —, going round.
    pub(super) fn step_open_item(&mut self, forward: bool, cx: &mut Context<Self>) {
        let space = self.nav.space;
        if space == Space::Code {
            return self.step_code_tab(forward, cx);
        }
        let items: Vec<Item> = self
            .open_items(space)
            .into_iter()
            .map(|o| o.item.clone())
            .collect();
        let n = items.len();
        if n == 0 {
            return;
        }
        let at = self
            .page_of(space)
            .and_then(Page::item)
            .and_then(|i| items.iter().position(|x| *x == i));
        let to = match at {
            Some(at) => (at + if forward { 1 } else { n - 1 }) % n,
            None if forward => 0,
            None => n - 1,
        };
        self.nav.picked.clear();
        self.show_open(&items[to], cx);
    }

    /// The open items of `space`, oldest first.
    pub(super) fn open_items(&self, space: Space) -> Vec<&Opened> {
        self.nav
            .open
            .iter()
            .filter(|o| o.item.space() == space)
            .collect()
    }

    /// Shift and a click on an open item in the menu: the items of its space from the one it began
    /// from (the first picked, else the one on screen) to this one, picked to close with ⌘W.
    pub(super) fn pick_open(&mut self, item: Item, cx: &mut Context<Self>) {
        let items: Vec<Item> = self
            .open_items(item.space())
            .into_iter()
            .map(|o| o.item.clone())
            .collect();
        let from = self
            .nav
            .picked
            .first()
            .cloned()
            .or_else(|| self.page_of(item.space()).and_then(Page::item))
            .filter(|f| items.contains(f))
            .unwrap_or_else(|| item.clone());
        if let Some(picked) = run(&items, &from, &item) {
            self.nav.picked = picked;
            cx.notify();
        }
    }

    /// ⌘W with open items picked in the menu: they all close.
    pub(super) fn close_picked(&mut self, cx: &mut Context<Self>) -> bool {
        if self.nav.picked.is_empty() {
            return false;
        }
        for item in std::mem::take(&mut self.nav.picked) {
            self.close_open(&item, cx);
        }
        true
    }

    /// Whether `item` is the one on screen.
    pub(super) fn is_shown(&self, item: &Item) -> bool {
        self.nav_page().and_then(Page::item).as_ref() == Some(item)
    }

    /// The menu's open items of a space shown or folded away.
    pub(super) fn toggle_open_items(&mut self, space: Space, cx: &mut Context<Self>) {
        self.mark_motion(&format!("opens-{}", space.id()), cx);
        self.nav.folded[space.ix()] = !self.nav.folded[space.ix()];
        cx.notify();
    }

    /// Opens branch `ix` as a page of Branches (a list's row, ⌘K, a link).
    pub(super) fn open_branch(&mut self, ix: usize, keep: Option<Oid>, cx: &mut Context<Self>) {
        let Some(refname) = self
            .history
            .read(cx)
            .branches
            .get(ix)
            .map(|b| b.refname.clone())
        else {
            return;
        };
        let item = Item::Branch(refname.clone());
        let place = (!self.is_shown(&item))
            .then(|| self.nav.opened(&item).and_then(|o| o.place.clone()))
            .flatten();
        self.arrive(Page::Branch(refname), cx);
        self.review.update(cx, |rv, _| rv.restore_scroll = place);
        self.select_branch(ix, keep, cx);
    }
}

/// The items from `from` to `to` in `items`' order, `from` first; None when either is missing.
fn run<T: PartialEq + Clone>(items: &[T], from: &T, to: &T) -> Option<Vec<T>> {
    let a = items.iter().position(|i| i == from)?;
    let b = items.iter().position(|i| i == to)?;
    let mut out = items[a.min(b)..=a.max(b)].to_vec();
    if a > b {
        out.reverse();
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shift_picks_the_run_from_where_it_began() {
        let items = [1, 2, 3, 4, 5];
        assert_eq!(run(&items, &2, &4), Some(vec![2, 3, 4]));
        assert_eq!(run(&items, &4, &2), Some(vec![4, 3, 2]));
        assert_eq!(run(&items, &3, &3), Some(vec![3]));
        assert_eq!(run(&items, &9, &3), None);
    }

    #[test]
    fn a_page_knows_its_space_and_the_page_above_it() {
        let id = Oid::from_str("0123456789012345678901234567890123456789").unwrap();
        assert_eq!(Page::RequestCommit(398, id).space(), Space::Requests);
        assert_eq!(
            Page::RequestCommit(398, id).parent(),
            Some(Page::Request(398))
        );
        assert_eq!(Page::Request(398).parent(), None);
        assert_eq!(
            Page::Versions("refs/heads/x".into(), 0, 2).parent(),
            Some(Page::Branch("refs/heads/x".into()))
        );
        assert_eq!(
            Page::BranchCommit("refs/heads/x".into(), id).parent(),
            Some(Page::Branch("refs/heads/x".into()))
        );
        assert_eq!(Page::File("a/b.kt".into()).space(), Space::Code);
        // A file or a folder goes up to its folder, the top ones to the project.
        assert_eq!(
            Page::File("a/b/c.kt".into()).parent(),
            Some(Page::Folder("a/b".into()))
        );
        assert_eq!(
            Page::Folder("a/b".into()).parent(),
            Some(Page::Folder("a".into()))
        );
        assert_eq!(Page::File("README.md".into()).parent(), None);
        assert_eq!(Page::WorkingTree.item(), Some(Item::WorkingTree));
        assert_eq!(Page::File("a".into()).item(), None);
        // A comparison's row is its two sides, whichever is the base (#149).
        let id = Oid::ZERO_SHA1;
        assert_eq!(
            Page::Compare(("main".into(), id), ("x".into(), id), true).item(),
            Page::Compare(("x".into(), id), ("main".into(), id), false).item()
        );
        assert_eq!(
            Page::RequestVersions(7, 0, 1).item(),
            Some(Item::Request(7))
        );
    }

    #[test]
    fn pages_and_open_items_survive_nav_txt() {
        let id = Oid::from_str("0123456789012345678901234567890123456789").unwrap();
        let pages = [
            Page::File("src/a b.rs".into()),
            Page::Folder("core/money".into()),
            Page::Branch("refs/heads/feature/x".into()),
            Page::BranchCommit("refs/heads/feature/x".into(), id),
            Page::Versions("refs/remotes/origin/x".into(), 1, 3),
            Page::WorkingTree,
            Page::Compare(("main".into(), id), ("origin/x".into(), id), true),
            Page::Compare(("main".into(), id), ("origin/x".into(), id), false),
            Page::Request(398),
            Page::RequestCommit(398, id),
            Page::RequestVersions(398, 0, 2),
        ];
        for page in pages {
            assert_eq!(Page::from_route(&page.route()), Some(page.clone()));
            assert_eq!(
                Route::parse(&page.route().text()).and_then(|r| Page::from_route(&r)),
                Some(page)
            );
        }
        // A kind nothing draws (a plugin gone) is dropped.
        let gone = Route::parse(r#"plugin.page{"x":1}"#).unwrap();
        assert_eq!(Page::from_route(&gone), None);
        // nav.txt before routes (#165): its keys still read.
        assert_eq!(
            Page::parse_key(&format!("branch-commit:{id}:refs/heads/feature/x")),
            Some(Page::BranchCommit("refs/heads/feature/x".into(), id))
        );
        assert_eq!(
            Page::parse_key("file:src/a b.rs"),
            Some(Page::File("src/a b.rs".into()))
        );
        // A compare kept before Direct was kept with it is since the merge base (#147).
        assert_eq!(
            Page::parse_key(&format!("compare:{id}:{id}:main:origin/x")),
            Some(Page::Compare(
                ("main".into(), id),
                ("origin/x".into(), id),
                true
            ))
        );
        let mut nav = Nav {
            space: Space::Requests,
            ..Nav::default()
        };
        nav.pages[Space::Code.ix()] = Some(Page::File("README.md".into()));
        nav.pages[Space::Requests.ix()] = Some(Page::RequestCommit(398, id));
        let compare = Page::Compare(("main".into(), id), ("origin/x".into(), id), false);
        for (item, page) in [
            (Item::Request(398), Page::Request(398)),
            (Item::Branch("refs/heads/main".into()), Page::WorkingTree),
            (Item::compare("main", "origin/x"), compare.clone()),
            (Item::WorkingTree, Page::WorkingTree),
        ] {
            nav.open.push(Opened {
                page,
                item,
                commit: None,
                place: None,
                paths: Vec::new(),
                section: Section::Changes,
            });
        }
        let Saved {
            repo,
            space,
            pages,
            open,
        } = Nav::read(&nav.line("_Users_me_repo")).unwrap();
        assert_eq!(repo, "_Users_me_repo");
        assert_eq!(space, Space::Requests);
        assert_eq!(
            pages[Space::Code.ix()],
            Some(Page::File("README.md".into()))
        );
        assert_eq!(pages[Space::Branches.ix()], None);
        assert_eq!(
            pages[Space::Requests.ix()],
            Some(Page::RequestCommit(398, id))
        );
        // A comparison comes back with its page, its sides' commits in it.
        assert_eq!(
            open,
            [
                (Item::Request(398), None),
                (Item::Branch("refs/heads/main".into()), None),
                (Item::compare("main", "origin/x"), Some(compare)),
                (Item::WorkingTree, Some(Page::WorkingTree)),
            ]
        );
    }

    #[test]
    fn a_line_written_before_routes_still_reads() {
        let id = "0123456789012345678901234567890123456789";
        let line = format!(
            "_Users_me_repo\t3\t-\tfile:README.md\t-\trequest-commit:398:{id}\trequest:398 branch:refs/heads/main compare-direct:{id}:{id}:main:origin/x working-tree"
        );
        let Saved {
            space, pages, open, ..
        } = Nav::read(&line).unwrap();
        let id = Oid::from_str(id).unwrap();
        assert_eq!(space, Space::Requests);
        assert_eq!(
            pages[Space::Code.ix()],
            Some(Page::File("README.md".into()))
        );
        assert_eq!(
            pages[Space::Requests.ix()],
            Some(Page::RequestCommit(398, id))
        );
        let compare = Page::Compare(("main".into(), id), ("origin/x".into(), id), false);
        assert_eq!(
            open,
            [
                (Item::Request(398), None),
                (Item::Branch("refs/heads/main".into()), None),
                (Item::compare("main", "origin/x"), Some(compare)),
                (Item::WorkingTree, Some(Page::WorkingTree)),
            ]
        );
    }

    #[test]
    fn time_drops_what_was_ahead_and_keeps_one_of_a_kind() {
        let mut nav = Nav::default();
        let at = |space, page: Option<Page>| Moment {
            space,
            page,
            commit: None,
        };
        nav.remember(at(Space::Inbox, None));
        nav.remember(at(Space::Requests, Some(Page::Request(1))));
        nav.remember(at(Space::Requests, Some(Page::Request(1))));
        nav.remember(at(Space::Branches, None));
        assert_eq!(nav.time.len(), 3);
        // The branch's page, then the same with its commit in: one moment.
        let id = Oid::from_str("0123456789012345678901234567890123456789").unwrap();
        let branch = Some(Page::Branch("refs/heads/main".into()));
        nav.remember(at(Space::Branches, branch.clone()));
        nav.remember(Moment {
            commit: Some(id),
            ..at(Space::Branches, branch)
        });
        assert_eq!(nav.time.len(), 4);
        assert_eq!(nav.time[3].commit, Some(id));
        nav.time.truncate(3);
        nav.at = 2;
        nav.at = 0;
        assert!(nav.can_forward() && !nav.can_back());
        nav.remember(at(Space::Code, None));
        assert_eq!(nav.time.len(), 2);
        assert!(!nav.can_forward());
    }
}
