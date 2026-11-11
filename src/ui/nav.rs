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

use super::spaces::{BRANCHES, CODE, INBOX, KEPT, REQUESTS, Space};
use super::{Selection, Workspace, recents, zones};
use artoo_branches_api::Shows;
use artoo_core::route::Route;
use artoo_ui_kit::format;
use git2::Oid;
use gpui::{App, Context, ListOffset};
use std::collections::{HashMap, HashSet};

/// The pages the tab still coordinates, as it reads their routes (`Page::from_route`): Code's, the branches' and the
/// requests' item screen — its bar, its sections, its links. What any page is under, what stays open of it and
/// which space it is in are its pages' (`Drawn`); a route of another kind is kept and shown all the same.
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

/// An item open under a space as `nav.txt` kept it before routes (#165): read, never written.
fn item_key(key: &str) -> Option<Route> {
    match key.split_once(':')? {
        ("request", iid) => Some(Page::Request(iid.parse().ok()?).route()),
        ("branch", refname) => Some(Page::Branch(refname.to_owned()).route()),
        _ => None,
    }
}

/// What the tab's pages say of the routes they draw (`Pages`): the space each kind is in, its page one up, what
/// stays open of it. A kind no page draws in the tab is none of theirs: `nav.txt` drops it.
#[derive(Default)]
pub(super) struct Drawn(Vec<Drawer>);

#[derive(Clone, Copy)]
struct Drawer {
    space: Space,
    kinds: &'static [&'static str],
    parent: fn(&Route) -> Option<Route>,
    item: fn(&Route) -> Option<Route>,
    screen: fn(Option<&Route>) -> artoo_shell_api::Screen,
}

impl Drawn {
    pub(super) fn of<'a>(pages: impl IntoIterator<Item = &'a artoo_shell_api::Pages>) -> Self {
        Self(
            pages
                .into_iter()
                .map(|p| Drawer {
                    space: Space(p.space),
                    kinds: p.kinds,
                    parent: p.parent,
                    item: p.item,
                    screen: p.screen,
                })
                .collect(),
        )
    }

    fn drawer(&self, route: &Route) -> Option<&Drawer> {
        self.0
            .iter()
            .find(|d| d.kinds.contains(&route.kind.as_str()))
    }

    /// Whether a page of the tab draws `route`.
    pub(super) fn has(&self, route: &Route) -> bool {
        self.drawer(route).is_some()
    }

    /// The space `route` is in.
    pub(super) fn space(&self, route: &Route) -> Option<Space> {
        self.drawer(route).map(|d| d.space)
    }

    /// The page one up from `route`; none when that is its space's own page.
    pub(super) fn parent(&self, route: &Route) -> Option<Route> {
        (self.drawer(route)?.parent)(route)
    }

    /// What stays open under its space once `route` was seen.
    pub(super) fn item(&self, route: &Route) -> Option<Route> {
        (self.drawer(route)?.item)(route)
    }

    /// What the tab frames `route` as (`Pages::screen`); a page of its own when no page draws it.
    pub(super) fn screen(&self, route: &Route) -> artoo_shell_api::Screen {
        self.drawer(route)
            .map_or(artoo_shell_api::Screen::Own, |d| (d.screen)(Some(route)))
    }

    /// The page item `item` opens on when nothing kept another: itself when it is a page (a request, a branch),
    /// none when no page draws it (a comparison, whose page is the one it was left on).
    fn item_page(&self, item: &Route) -> Option<Route> {
        self.has(item).then(|| item.clone())
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

pub use artoo_branches_api::BranchRow;

/// A request or branch open under its space, and where it was left.
#[derive(Clone)]
pub struct Opened {
    /// What it is (`Drawn::item`): a request's page, a branch's, a comparison's key.
    pub item: Route,
    /// The page it was on.
    pub page: Route,
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
    pages: HashMap<Space, Route>,
    /// Each with its page when the item alone cannot say it: a comparison's.
    open: Vec<(Route, Option<Route>)>,
}

/// A page seen: ⌘[ ⌘] come back to it.
#[derive(Clone, PartialEq, Debug)]
struct Moment {
    space: Space,
    page: Option<Route>,
}

pub struct Nav {
    pub space: Space,
    /// Each space's page; none at its root.
    pages: HashMap<Space, Route>,
    time: Vec<Moment>,
    at: usize,
    /// The space a link came from, while its page is on screen (‹ Inbox).
    pub from: Option<Space>,
    pub open: Vec<Opened>,
    /// The spaces whose open items the menu folds away (Branches at first).
    pub folded: HashSet<Space>,
    /// A space and page to show once the request list is in (a project reopened on one).
    pending_show: Option<(Space, Option<Route>)>,
    /// The menu's open items picked with shift, the first the one it began from: ⌘W closes them.
    pub picked: Vec<Route>,
    /// Loading what a page needs, or coming back to a moment: no new moment, no link.
    restoring: bool,
}

impl Default for Nav {
    fn default() -> Self {
        Self {
            space: BRANCHES,
            pages: Default::default(),
            time: Vec::new(),
            at: 0,
            from: None,
            open: Vec::new(),
            folded: HashSet::from([BRANCHES]),
            pending_show: None,
            picked: Vec::new(),
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

    /// A new moment; the ones ahead of it are dropped, the same one twice is one.
    fn remember(&mut self, moment: Moment) {
        if self.time.get(self.at) == Some(&moment) {
            return;
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

    pub(super) fn opened(&self, item: &Route) -> Option<&Opened> {
        self.open.iter().find(|o| &o.item == item)
    }

    fn opened_mut(&mut self, item: &Route) -> Option<&mut Opened> {
        self.open.iter_mut().find(|o| &o.item == item)
    }

    /// `nav.txt`'s line for project `repo`: the space, each space's page, the open items, then the pages of any
    /// other space. The spaces with a column are `KEPT`, the one on screen by its place there; another is kept by
    /// its id.
    fn line(&self, repo: &str, drawn: &Drawn) -> String {
        let pages: Vec<String> = KEPT
            .iter()
            .map(|space| {
                self.pages
                    .get(space)
                    .map_or_else(|| "-".to_owned(), Route::text)
            })
            .collect();
        let space = KEPT
            .iter()
            .position(|s| *s == self.space)
            .map_or_else(|| self.space.name().to_owned(), |ix| ix.to_string());
        // An item as its own page; a comparison as the page it was left on: its sides' commits come
        // back with it (#149).
        let open: Vec<String> = self
            .open
            .iter()
            .map(|o| {
                drawn
                    .item_page(&o.item)
                    .unwrap_or_else(|| o.page.clone())
                    .text()
            })
            .collect();
        // Each a route, which says its space: a line written before them ends at the open items.
        let mut others: Vec<(&str, String)> = self
            .pages
            .iter()
            .filter(|(space, _)| !KEPT.contains(space))
            .map(|(space, page)| (space.name(), page.text()))
            .collect();
        others.sort();
        let others: String = others.into_iter().map(|(_, p)| format!("\t{p}")).collect();
        format!(
            "{repo}\t{space}\t{}\t{}{others}",
            pages.join("\t"),
            open.join(" ")
        )
    }

    /// What `line` kept: the space, the pages and the open items. A space kept by its id is one of `known`; a
    /// route of a kind no page of the tab draws is dropped.
    fn read(line: &str, known: &[Space], drawn: &Drawn) -> Option<Saved> {
        let mut cols = line.split('\t');
        let repo = cols.next()?.to_owned();
        let space = cols.next()?;
        let space = match space.parse::<usize>() {
            Ok(ix) => *KEPT.get(ix)?,
            Err(_) => *known.iter().find(|s| s.name() == space)?,
        };
        // Each kept as a route; a line written before routes as the keys it had then.
        let route_of = |text: &str| {
            Route::parse(text)
                .or_else(|| Page::parse_key(text).map(|p| p.route()))
                .filter(|route| drawn.has(route))
        };
        let mut pages = HashMap::new();
        for space in KEPT {
            if let Some(page) = route_of(cols.next()?) {
                pages.insert(space, page);
            }
        }
        let open = cols
            .next()
            .unwrap_or_default()
            .split(' ')
            .filter_map(|key| match route_of(key) {
                // An item's own page is the item alone.
                Some(page) => {
                    let item = drawn.item(&page)?;
                    Some(if item == page {
                        (item, None)
                    } else {
                        (item, Some(page))
                    })
                }
                None => Some((item_key(key).filter(|r| drawn.has(r))?, None)),
            })
            .collect();
        for page in cols.filter_map(route_of) {
            if let Some(space) = drawn.space(&page).filter(|s| !KEPT.contains(s)) {
                pages.insert(space, page);
            }
        }
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
    pub(super) fn nav_page(&self) -> Option<&Route> {
        self.nav.pages.get(&self.nav.space)
    }

    /// The page on screen as the tab reads it, when it is one it coordinates.
    pub(super) fn shown_page(&self) -> Option<Page> {
        self.nav_page().and_then(Page::from_route)
    }

    /// The page space `space` keeps.
    pub(super) fn page_of(&self, space: Space) -> Option<&Route> {
        self.nav.pages.get(&space)
    }

    /// What the tab's pages say of the routes they draw.
    pub(super) fn drawn(&self) -> Drawn {
        Drawn::of(self.parts.iter().filter_map(|p| p.pages.as_ref()))
    }

    /// What stays open of `route` under its space (`Drawn::item`).
    pub(super) fn item_of(&self, route: &Route) -> Option<Route> {
        self.drawn().item(route)
    }

    /// What the page on screen is framed as, as its pages say (`Pages::screen`); a page of its own when none draws
    /// it.
    pub(super) fn screen(&self) -> artoo_shell_api::Screen {
        let route = self.nav_page();
        self.pages_drawing(self.nav.space, route)
            .map_or(artoo_shell_api::Screen::Own, |p| (p.screen)(route))
    }

    /// At a list of the tab's: the space's own page is its list (the branches', the requests').
    pub(super) fn at_root_list(&self) -> bool {
        self.screen() == artoo_shell_api::Screen::List
    }

    /// At the Inbox's list: its Filter in the bar, as a list's (#197).
    pub(super) fn at_inbox(&self) -> bool {
        self.nav.space == INBOX && self.nav_page().is_none()
    }

    /// What is loaded, as a page of Branches or Merge requests (`History::loaded_route`).
    pub(super) fn loaded_page(&self, cx: &App) -> Option<Route> {
        self.branches()?
            .loaded(self.nav_page(), cx)
            .filter(|route| self.drawn().has(route))
    }

    /// The review as a page asks it (`Review`), and what the tab reads most of it: the diff shown, the file it is at
    /// and its path, the open file's rows, whether the diff is narrow, the view's options, the diff's list, a code
    /// character's width, the repository it reads.
    pub(super) fn review_api(&self) -> Option<std::rc::Rc<dyn artoo_review_api::Review>> {
        self.services.get()
    }

    pub(super) fn shown_diff(
        &self,
        cx: &App,
    ) -> Option<std::sync::Arc<artoo_review_api::diff::Diff>> {
        self.review_api()?.shown(cx)
    }

    pub(super) fn diff_file(&self, cx: &App) -> usize {
        self.review_api().map_or(0, |r| r.file(cx))
    }

    pub(super) fn current_path(&self, cx: &App) -> Option<String> {
        self.review_api()?.current_path(cx)
    }

    pub(super) fn diff_data(&self, cx: &App) -> Option<std::sync::Arc<crate::rows::FileData>> {
        self.review_api()?.data(cx)
    }

    pub(super) fn tight(&self, cx: &App) -> bool {
        self.review_api().is_some_and(|r| r.tight(cx))
    }

    pub(super) fn diff_narrow(&self, cx: &App) -> bool {
        self.review_api().is_some_and(|r| r.narrow(cx))
    }

    pub(super) fn view_options(&self, cx: &App) -> crate::storage::ViewOptions {
        self.review_api().map(|r| r.options(cx)).unwrap_or_default()
    }

    pub(super) fn diff_list(&self, cx: &App) -> gpui::ListState {
        match self.review_api() {
            Some(r) => r.diff_list(cx),
            None => gpui::ListState::new(0, gpui::ListAlignment::Top, gpui::px(0.)),
        }
    }

    pub(super) fn char_width(&self, cx: &App) -> f32 {
        self.review_api().map_or(0., |r| r.char_width(cx))
    }

    pub(super) fn review_root(&self, cx: &App) -> Option<std::path::PathBuf> {
        self.review_api()?.root(cx)
    }

    /// The review, which every tab has (its parts make it): what the tab asks of the diff.
    pub(super) fn rv(&self) -> std::rc::Rc<dyn artoo_review_api::Review> {
        self.services
            .get()
            .expect("a tab's review: artoo_review::install")
    }

    /// The requests as a page asks them.
    pub(super) fn requests_api(&self) -> Option<std::rc::Rc<dyn artoo_requests_api::Requests>> {
        self.services.get()
    }

    /// What the tab reads most of the requests (`Requests`): the one open, a request by its number, the list, the
    /// open one's discussions and your pending comments on it.
    pub(super) fn current_request(&self, cx: &App) -> Option<u64> {
        self.requests_api()?.current(cx)
    }

    pub(super) fn find_request(&self, number: u64, cx: &App) -> Option<crate::mr::Mr> {
        self.requests_api()?.find(number, cx)
    }

    pub(super) fn request_list(&self, cx: &App) -> Vec<crate::mr::Mr> {
        self.requests_api().map(|r| r.list(cx)).unwrap_or_default()
    }

    pub(super) fn requests_available(&self, cx: &App) -> bool {
        self.requests_api().is_some_and(|r| r.available(cx))
    }

    pub(super) fn requests_listing(&self, cx: &App) -> bool {
        self.requests_api().is_some_and(|r| r.listing(cx))
    }

    pub(super) fn request_login(&self, cx: &App) -> Option<String> {
        self.requests_api()?.login(cx)
    }

    pub(super) fn request_members(&self, cx: &App) -> Vec<(String, String)> {
        self.requests_api()
            .map(|r| r.members(cx))
            .unwrap_or_default()
    }

    pub(super) fn request_threads(&self, cx: &App) -> Vec<crate::mr::Thread> {
        self.requests_api()
            .map(|r| r.threads(cx))
            .unwrap_or_default()
    }

    pub(super) fn request_thread(&self, ix: usize, cx: &App) -> Option<crate::mr::Thread> {
        self.requests_api()?.thread(ix, cx)
    }

    pub(super) fn request_drafts(&self, cx: &App) -> Vec<crate::mr::Draft> {
        self.requests_api()
            .map(|r| r.drafts(cx))
            .unwrap_or_default()
    }

    /// The branches as a page asks them.
    pub(super) fn branches(&self) -> Option<std::rc::Rc<dyn artoo_branches_api::Branches>> {
        self.services.get()
    }

    /// What the tab reads most of the branches (`Branches`): the list, the branch open, what the diff shows of it,
    /// its commits and versions.
    pub(super) fn branch_list(&self, cx: &App) -> std::sync::Arc<Vec<crate::git::BranchRef>> {
        self.branches().map(|b| b.list(cx)).unwrap_or_default()
    }

    pub(super) fn branch_ix(&self, cx: &App) -> Option<usize> {
        self.branches()?.branch(cx)
    }

    pub(super) fn selection(&self, cx: &App) -> Selection {
        self.branches().map_or(Selection::None, |b| b.selection(cx))
    }

    pub(super) fn commit_count(&self, cx: &App) -> usize {
        self.branches().map_or(0, |b| b.commit_count(cx))
    }

    pub(super) fn commit_info(&self, ix: usize, cx: &App) -> Option<crate::git::CommitInfo> {
        self.branches()?.commit(ix, cx)
    }

    pub(super) fn commit_at(&self, id: Oid, cx: &App) -> Option<usize> {
        self.branches()?.commit_of(id, cx)
    }

    pub(super) fn version_at(&self, ix: usize, cx: &App) -> Option<artoo_core::git::Version> {
        self.branches()?.version(ix, cx)
    }

    /// Something was opened: its space and page come on screen, over the Inbox or Settings. From
    /// another space it was a link, and the page says where it came from.
    pub(super) fn arrive(&mut self, page: Route, cx: &mut App) {
        let Some(space) = self.drawn().space(&page) else {
            return;
        };
        if !self.nav.restoring && space != self.nav.space {
            self.nav.from = Some(self.nav.space);
        }
        self.inbox = space == INBOX;
        self.settings_open = false;
        self.set_page(page, cx);
    }

    /// The section a request opened anew starts on: Overview when you have never been inside it,
    /// else Changes (#73).
    fn first_section(&self, item: &Route, cx: &App) -> Section {
        match Page::from_route(item) {
            Some(Page::Request(iid)) if !self.opened_before(iid, cx) => Section::Overview,
            _ => Section::Changes,
        }
    }

    /// The section tabs of the page's item: a request's three (#73), a branch off the main line's
    /// Commits and Changes (#74); none for anything else.
    pub(super) fn item_sections(&self, cx: &App) -> &'static [Section] {
        match self.shown_page() {
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
            ) if self.branches().is_some_and(|b| b.tabbed(&refname, cx)) => {
                &[Section::Commits, Section::Changes]
            }
            _ => &[],
        }
    }

    /// The section tab of the item on screen: a commit of it is under Commits, two versions
    /// compared under Changes.
    pub(super) fn page_section(&self, cx: &App) -> Option<Section> {
        if self.item_sections(cx).is_empty() {
            return None;
        }
        let item = self.item_of(self.nav_page()?)?;
        Some(
            self.nav
                .opened(&item)
                .map_or(Section::Changes, |o| o.section),
        )
    }

    /// The item's tab on screen when it is the tabbed page itself, not a level under it.
    pub(super) fn page_tab(&self, cx: &App) -> Option<Section> {
        matches!(self.shown_page(), Some(Page::Request(_) | Page::Branch(_)))
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
        let drawn = self.drawn();
        let (Some(item), Some(space)) = (drawn.item(&page), drawn.space(&page)) else {
            return;
        };
        if let Some(open) = self.nav.opened_mut(&item) {
            open.section = section;
        }
        let top = drawn.item_page(&item).unwrap_or_else(|| page.clone());
        if page != top {
            self.show(space, Some(top), cx);
        }
        cx.notify();
    }

    /// ⌃⇥ and ⌃⇧⇥: the next or the previous section tab, going round.
    pub(super) fn step_section(&mut self, forward: bool, cx: &mut Context<Self>) {
        // In Code they walk the files open as tabs (#75).
        if self.nav.space == CODE {
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
    fn set_page(&mut self, page: Route, cx: &mut App) {
        let drawn = self.drawn();
        let Some(space) = drawn.space(&page) else {
            return;
        };
        self.keep_place(cx);
        self.nav.space = space;
        self.nav.pages.insert(space, page.clone());
        if let Some(item) = drawn.item(&page) {
            match self.nav.opened_mut(&item) {
                Some(open) => open.page = page.clone(),
                None => {
                    let section = self.first_section(&item, cx);
                    self.nav.open.push(Opened {
                        item: item.clone(),
                        page: page.clone(),
                        place: None,
                        paths: Vec::new(),
                        section,
                    })
                }
            }
            // A commit of a request is a level under Commits, picked there when esc comes back to
            // it; two versions compared are Changes (#73).
            let section = match Page::from_route(&page) {
                Some(Page::RequestCommit(_, id) | Page::BranchCommit(_, id)) => {
                    if let Some(branches) = self.branches() {
                        branches.pick_commits(id, None, cx);
                    }
                    Some(Section::Commits)
                }
                // A run is picked there as it was, newest first.
                Some(
                    Page::RequestCommits(_, newest, oldest)
                    | Page::BranchCommits(_, newest, oldest),
                ) => {
                    if let Some(branches) = self.branches() {
                        branches.pick_commits(newest, Some(oldest), cx);
                    }
                    Some(Section::Commits)
                }
                Some(Page::RequestVersions(..)) => Some(Section::Changes),
                _ => None,
            };
            if let Some(section) = section
                && let Some(open) = self.nav.opened_mut(&item)
            {
                open.section = section;
            }
        }
        self.remember_moment();
        self.save_nav();
    }

    /// Page `page` opened from where the tab is (a pick, a step, a part's `Frame::open`): it comes
    /// on screen, kept in the history, and what it shows is loaded (`load_page`).
    pub(super) fn go_page(&mut self, page: Route, cx: &mut Context<Self>) {
        self.arrive(page.clone(), cx);
        self.load_page(page, true, cx);
        cx.notify();
    }

    /// `selection` of the branch open picked — a commit, a run, two versions —: opened as the page
    /// it is (`History::route_of`); loaded as it is where it is no page of the item on screen.
    pub(super) fn pick(&mut self, selection: Selection, cx: &mut Context<Self>) {
        let drawn = self.drawn();
        let item = self.nav_page().and_then(|p| drawn.item(p));
        let page = self
            .branches()
            .and_then(|b| b.route_of(selection, cx))
            .filter(|page| drawn.space(page) == Some(self.nav.space) && drawn.item(page) == item);
        match (page, selection) {
            (Some(page), _) => self.go_page(page, cx),
            (None, Selection::Commit(ix)) => self.select_commit(ix, cx),
            (None, Selection::Commits { newest, oldest }) => {
                self.select_commits(newest, oldest, cx)
            }
            (None, Selection::Versions { from, to }) => self.select_versions(from, to, cx),
            (None, Selection::None | Selection::WorkingTree) => {}
        }
    }

    /// Commit `ix` of the list opened from wherever the window is (a ⌘K result, a link, a place
    /// opened lately): its page, kept in the history and loaded; where it is none, loaded and
    /// followed.
    pub(super) fn open_commit(&mut self, ix: usize, cx: &mut Context<Self>) {
        match self
            .branches()
            .and_then(|b| b.route_of(Selection::Commit(ix), cx))
            .filter(|route| self.drawn().has(route))
        {
            Some(page) => self.go_page(page, cx),
            None => {
                self.select_commit(ix, cx);
                self.follow_loaded(cx);
            }
        }
    }

    /// What is loaded comes on screen as its page, from wherever the window was (a ⌘K result, a
    /// file of the diff).
    pub(super) fn follow_loaded(&mut self, cx: &mut App) {
        if let Some(page) = self.loaded_page(cx) {
            self.arrive(page, cx);
        }
    }

    /// What is selected inside the open request or branch changed (a commit, versions): the page
    /// follows. Not while a request is still loading, nor while a page is being put back.
    pub(super) fn record(&mut self, cx: &mut App) {
        if self.branches().is_some_and(|b| b.settling(cx)) || self.nav.restoring {
            return;
        }
        let Some(page) = self.loaded_page(cx) else {
            return;
        };
        let drawn = self.drawn();
        if drawn.space(&page) == Some(self.nav.space)
            && drawn.item(&page) == self.nav_page().and_then(|p| drawn.item(p))
        {
            self.set_page(page, cx);
        }
    }

    fn remember_moment(&mut self) {
        if self.nav.restoring {
            return;
        }
        let moment = Moment {
            space: self.nav.space,
            page: self.nav_page().cloned(),
        };
        self.nav.remember(moment);
    }

    /// Before the loaded request or branch makes way: where it was, for coming back to it.
    fn keep_place(&mut self, cx: &App) {
        let Some(page) = self.loaded_page(cx) else {
            return;
        };
        let Some(item) = self.item_of(&page) else {
            return;
        };
        let review = self.rv();
        let place = review.place(cx);
        let paths: Vec<String> = review
            .shown(cx)
            .map(|d| d.files.iter().map(|f| f.path().to_owned()).collect())
            .unwrap_or_default();
        if let Some(open) = self.nav.opened_mut(&item) {
            open.page = page;
            if place.is_some() {
                open.place = place;
            }
            if !paths.is_empty() {
                open.paths = paths;
            }
        }
    }

    /// Shows `page` of `space`, loading what it needs. The menu, esc, ⌘[ ⌘] and the open items
    /// come here.
    pub(super) fn show(&mut self, space: Space, page: Option<Route>, cx: &mut Context<Self>) {
        self.keep_place(cx);
        self.nav.space = space;
        match &page {
            Some(page) => self.nav.pages.insert(space, page.clone()),
            None => self.nav.pages.remove(&space),
        };
        if let Some(branches) = self.branches() {
            branches.stop_waiting(cx);
        }
        self.inbox = space == INBOX;
        self.settings_open = false;
        self.nav.restoring = true;
        match page {
            None => self.enter_root(space, cx),
            Some(page) => self.load_page(page, false, cx),
        }
        self.nav.restoring = false;
        self.remember_moment();
        self.save_nav();
        cx.notify();
    }

    /// A space's root: the Inbox's list, the list of branches or requests; Code's project, and any other space's
    /// own page as its pages draw it.
    fn enter_root(&mut self, space: Space, cx: &mut Context<Self>) {
        match space {
            INBOX => {
                // The newest row: a notice, or an agent's answer (#116).
                let first = self.inbox_rows(cx).first().copied();
                if let Some(requests) = self.requests_api() {
                    requests.inbox_pick(first, cx);
                }
                self.refresh_notices(cx);
            }
            BRANCHES => {
                self.zone = zones::Zone::List;
                self.show_pages(space, None, false, cx);
            }
            REQUESTS => {
                self.pick_first_request(cx);
                self.read_preview(cx);
                self.zone = zones::Zone::List;
                // A project without a token: the connect panel takes the keyboard.
                if self.requests_api().is_some_and(|r| r.connectable(cx))
                    && let Some(connecting) = self.connecting()
                {
                    connecting.take_keyboard(cx);
                }
            }
            _ => self.show_pages(space, None, false, cx),
        }
    }

    /// Loads what `page` shows: its feature's pages read it (`Pages::show`), `opened` anew or gone
    /// back to. An item shown in place of another comes back where it was left.
    fn load_page(&mut self, page: Route, opened: bool, cx: &mut Context<Self>) {
        let drawn = self.drawn();
        if let Some(item) = drawn.item(&page) {
            let loaded = self.loaded_page(cx).and_then(|p| drawn.item(&p));
            if loaded.as_ref() != Some(&item) {
                let place = self.nav.opened(&item).and_then(|o| o.place.clone());
                self.rv().restore_at(place, cx);
            }
        }
        if let Some(space) = drawn.space(&page) {
            self.show_pages(space, Some(&page), opened, cx);
        }
        // A branch's or a request's: what the tab does after (`History::show_opened`) — the request
        // on the branch, the watch, the page following what is shown, the places opened lately.
        match self.branches().and_then(|b| b.take_shown(cx)) {
            Some(Shows::Branch { ix, read }) => self.branch_opened(ix, read, cx),
            Some(Shows::Request(iid)) => self.select_request(iid, cx),
            Some(Shows::Shown { commit, follow }) => {
                if follow {
                    self.record(cx);
                }
                if let Some(commit) = commit {
                    self.remember_commit(&commit);
                }
            }
            Some(Shows::Stay) | None => {}
        }
    }

    /// What a page waited for is in (a request's commits, a branch's versions): the page is shown
    /// (`History::finish_waiting`). Whether it was.
    pub(super) fn finish_pending(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(finished) = self.branches().and_then(|b| b.finish_waiting(cx)) else {
            return false;
        };
        self.record(cx);
        if let Some(commit) = finished.commit {
            self.remember_commit(&commit);
        }
        cx.notify();
        true
    }

    /// Commit `commit` kept among the places opened lately.
    pub(super) fn remember_commit(&mut self, commit: &artoo_core::git::CommitInfo) {
        self.remember_place(
            recents::Key::Commit(commit.id),
            commit.summary.clone(),
            format!("commit {}", format::short(commit.id)),
        );
    }

    /// Esc, once nothing smaller is open: one page up; nothing at a space's root.
    pub(super) fn go_up(&mut self, cx: &mut Context<Self>) {
        let Some(page) = self.nav_page().cloned() else {
            return;
        };
        let drawn = self.drawn();
        let up = drawn.parent(&page);
        if up.is_none() {
            // Back at the root: the list selects what was open.
            match drawn.item(&page).as_ref().and_then(Page::from_route) {
                Some(Page::Request(iid)) => {
                    if let Some(requests) = self.requests_api() {
                        requests.back_to_list(iid, cx);
                    }
                }
                Some(Page::Branch(_) | Page::WorkingTree) => {
                    if let Some(branches) = self.branches() {
                        branches.back_to_list(&page, cx);
                    }
                }
                _ => {}
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
    pub(super) fn show_open(&mut self, item: &Route, cx: &mut Context<Self>) {
        // One an agent gathered starts where its comments are (#148).
        if let Some(Page::Request(iid)) = Page::from_route(item)
            && self.agent_fresh.remove(&iid)
        {
            return self.open_gathered(iid, cx);
        }
        let Some(page) = self.nav.opened(item).map(|o| o.page.clone()) else {
            return;
        };
        let Some(space) = self.drawn().space(&page) else {
            return;
        };
        self.nav.from = None;
        self.show(space, Some(page), cx);
    }

    /// `page`'s request, comparison or working tree open under its space without loading it, on
    /// Changes, after the others (#148, #149). One open already keeps its place; a comparison not
    /// on screen goes to the commits `page` has. Whether it was added.
    pub(super) fn open_quietly(&mut self, page: Route, cx: &App) -> bool {
        let drawn = self.drawn();
        let Some(item) = drawn.item(&page) else {
            return false;
        };
        let loaded = self.loaded_page(cx).and_then(|p| drawn.item(&p)).as_ref() == Some(&item);
        if let Some(open) = self.nav.opened_mut(&item) {
            // One no page draws (a comparison) is the page it is given.
            if !loaded && !drawn.has(&item) {
                open.page = page;
            }
            return false;
        }
        self.nav.open.push(Opened {
            item,
            page,
            place: None,
            paths: Vec::new(),
            section: Section::Changes,
        });
        true
    }

    /// × on an open item, or ⌘W on the one on screen: it leaves the menu; on screen, its space's
    /// root comes.
    pub(super) fn close_open(&mut self, item: &Route, cx: &mut Context<Self>) {
        let drawn = self.drawn();
        let space = self.item_space(item);
        self.nav.open.retain(|o| &o.item != item);
        self.nav.picked.retain(|o| o != item);
        let shown = space
            .and_then(|space| self.page_of(space))
            .and_then(|p| drawn.item(p))
            .as_ref()
            == Some(item);
        if let Some(space) = space.filter(|_| shown) {
            self.nav.pages.remove(&space);
            if let Some(Page::Request(iid)) = Page::from_route(item)
                && self.requests_api().and_then(|r| r.current(cx)) == Some(iid)
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
        let Some(page) = self.nav_page() else {
            return;
        };
        if matches!(
            Page::from_route(page),
            Some(Page::File(_) | Page::Folder(_))
        ) {
            return self.close_code_tab(cx);
        }
        match self.item_of(page) {
            Some(item) => self.close_open(&item, cx),
            None => self.go_up(cx),
        }
    }

    /// The request list came in: requests open in the menu that are not in it any more (merged,
    /// closed) stay there, what became of them read once (#114: no pin needed); a page waiting
    /// for the list is shown.
    pub(super) fn requests_listed(&mut self, cx: &mut Context<Self>) {
        let open: Vec<u64> = self
            .nav
            .open
            .iter()
            .filter_map(|o| match Page::from_route(&o.item) {
                Some(Page::Request(iid)) => Some(iid),
                _ => None,
            })
            .collect();
        if let Some(requests) = self.requests_api() {
            let gone = requests.unlisted(&open, cx);
            self.read_left(gone, cx);
        }
        if let Some((space, page)) = self.nav.pending_show.take() {
            self.show(space, page, cx);
        }
        // The list came in under its root: the first request is picked and previewed (#72).
        if self.nav.space == REQUESTS && self.at_root_list() {
            self.pick_first_request(cx);
            self.read_preview(cx);
        }
    }

    /// The project just opened: the space and pages it was left on, else its checked-out branch.
    pub(super) fn restore_nav(&mut self, head: Option<usize>, cx: &mut Context<Self>) {
        let repo = self.nav_repo();
        let known: Vec<Space> = artoo_shell_api::spaces::all(cx)
            .iter()
            .map(|s| s.space)
            .collect();
        let drawn = self.drawn();
        let saved = crate::storage::read(FILE)
            .lines()
            .filter_map(|line| Nav::read(line, &known, &drawn))
            .find(|saved| Some(&saved.repo) == repo.as_ref());
        self.nav = Nav::default();
        if let Some(branches) = self.branches() {
            branches.stop_waiting(cx);
        }
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
                .values()
                .find(|p| drawn.item(p).as_ref() == Some(&item))
                .cloned()
                .or(kept)
                .or_else(|| drawn.item_page(&item))
            else {
                continue;
            };
            self.nav.open.push(Opened {
                item,
                page,
                place: None,
                paths: Vec::new(),
                section: Section::Changes,
            });
        }
        let page = self.nav.pages.get(&space).cloned();
        // Underneath a page that loads nothing of its own, the checked-out branch, as before:
        // ⌘K's commits and the files' revision come from it.
        let page_space = page.as_ref().and_then(|p| drawn.space(p));
        let item = page.as_ref().map(|p| drawn.screen(p)) == Some(artoo_shell_api::Screen::Item);
        if !item && let Some(ix) = head {
            self.nav.restoring = true;
            self.select_branch(ix, None, cx);
            self.nav.restoring = false;
        }
        let needs_list = page_space == Some(REQUESTS);
        if needs_list && self.requests_api().is_some_and(|r| r.listing(cx)) {
            self.nav.space = space;
            self.nav.pages.remove(&space);
            self.nav.pending_show = Some((space, page));
        } else {
            self.show(space, page, cx);
        }
    }

    fn nav_repo(&self) -> Option<String> {
        self.root.as_ref().map(|r| crate::comment::key(r))
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
        lines.push(self.nav.line(&repo, &self.drawn()));
        crate::storage::save(FILE, lines.join("\n") + "\n");
    }

    /// ⌥⌘→ ⌥⌘← (Ctrl+PageDown, Ctrl+PageUp elsewhere): the next or the previous item open under
    /// the space — its requests or branches, Code's tabs —, going round.
    pub(super) fn step_open_item(&mut self, forward: bool, cx: &mut Context<Self>) {
        let space = self.nav.space;
        if space == CODE {
            return self.step_code_tab(forward, cx);
        }
        let items: Vec<Route> = self
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
            .and_then(|p| self.item_of(p))
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
        let drawn = self.drawn();
        self.nav
            .open
            .iter()
            .filter(|o| drawn.space(&o.page) == Some(space))
            .collect()
    }

    /// The space item `item` is open under: its page's.
    fn item_space(&self, item: &Route) -> Option<Space> {
        let drawn = self.drawn();
        self.nav
            .opened(item)
            .and_then(|o| drawn.space(&o.page))
            .or_else(|| drawn.space(item))
    }

    /// Shift and a click on an open item in the menu: the items of its space from the one it began
    /// from (the first picked, else the one on screen) to this one, picked to close with ⌘W.
    pub(super) fn pick_open(&mut self, item: Route, cx: &mut Context<Self>) {
        let Some(space) = self.item_space(&item) else {
            return;
        };
        let items: Vec<Route> = self
            .open_items(space)
            .into_iter()
            .map(|o| o.item.clone())
            .collect();
        let from = self
            .nav
            .picked
            .first()
            .cloned()
            .or_else(|| self.page_of(space).and_then(|p| self.item_of(p)))
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
    pub(super) fn is_shown(&self, item: &Route) -> bool {
        self.nav_page().and_then(|p| self.item_of(p)).as_ref() == Some(item)
    }

    /// The menu's open items of a space shown or folded away.
    pub(super) fn toggle_open_items(&mut self, space: Space, cx: &mut Context<Self>) {
        self.mark_motion(&format!("opens-{}", space.id()), cx);
        if !self.nav.folded.remove(&space) {
            self.nav.folded.insert(space);
        }
        cx.notify();
    }

    /// Opens branch `ix` as a page of Branches (a list's row, ⌘K, a link).
    pub(super) fn open_branch(&mut self, ix: usize, keep: Option<Oid>, cx: &mut Context<Self>) {
        let Some(refname) = self
            .branches()
            .and_then(|b| b.list(cx).get(ix).map(|b| b.refname.clone()))
        else {
            return;
        };
        let item = Page::Branch(refname).route();
        let place = (!self.is_shown(&item))
            .then(|| self.nav.opened(&item).and_then(|o| o.place.clone()))
            .flatten();
        self.arrive(item, cx);
        self.rv().restore_at(place, cx);
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

    /// The tab's pages as their owners draw them (`Pages::parent`, `Pages::item`; each owner tests its own), and a
    /// plugin's space of its own.
    fn drawn() -> Drawn {
        fn parent(route: &Route) -> Option<Route> {
            match Page::from_route(route)? {
                Page::RequestCommit(n, _)
                | Page::RequestCommits(n, ..)
                | Page::RequestVersions(n, ..) => Some(Page::Request(n).route()),
                Page::BranchCommit(b, _) | Page::BranchCommits(b, ..) | Page::Versions(b, ..) => {
                    Some(Page::Branch(b).route())
                }
                Page::File(path) | Page::Folder(path) => {
                    Some(Page::Folder(path.rsplit_once('/')?.0.to_owned()).route())
                }
                _ => None,
            }
        }
        fn item(route: &Route) -> Option<Route> {
            Some(match Page::from_route(route)? {
                Page::Request(n)
                | Page::RequestCommit(n, _)
                | Page::RequestCommits(n, ..)
                | Page::RequestVersions(n, ..) => Page::Request(n).route(),
                Page::Branch(b)
                | Page::BranchCommit(b, _)
                | Page::BranchCommits(b, ..)
                | Page::Versions(b, ..) => Page::Branch(b).route(),
                Page::WorkingTree => Page::WorkingTree.route(),
                Page::Compare((a, _), (b, _), _) => comparison(&a, &b),
                Page::File(_) | Page::Folder(_) => return None,
            })
        }
        let jira: fn(&Route) -> Option<Route> = |r| Some(r.clone());
        Drawn(vec![
            Drawer {
                space: CODE,
                kinds: &["code.file", "code.folder"],
                parent,
                item,
                screen: |_| artoo_shell_api::Screen::Own,
            },
            Drawer {
                space: BRANCHES,
                kinds: &[
                    "branches.branch",
                    "branches.commit",
                    "branches.commits",
                    "branches.versions",
                    "branches.working-tree",
                    "branches.compare",
                ],
                parent,
                item,
                screen: |_| artoo_shell_api::Screen::Item,
            },
            Drawer {
                space: REQUESTS,
                kinds: &[
                    "requests.request",
                    "requests.commit",
                    "requests.commits",
                    "requests.versions",
                ],
                parent,
                item,
                screen: |_| artoo_shell_api::Screen::Item,
            },
            Drawer {
                space: Space("jira"),
                kinds: &["jira.issue"],
                parent: |_| None,
                item: jira,
                screen: |_| artoo_shell_api::Screen::Own,
            },
        ])
    }

    /// Two refs compared as the branches keep them open: by their names, either way round.
    fn comparison(a: &str, b: &str) -> Route {
        let (a, b) = if a <= b { (a, b) } else { (b, a) };
        Route {
            kind: "branches.comparison".to_owned(),
            args: format!(r#"{{"a":"{a}","b":"{b}"}}"#),
        }
    }

    #[test]
    fn a_page_is_in_the_space_of_the_pages_that_draw_it() {
        let drawn = drawn();
        let id = Oid::from_str("0123456789012345678901234567890123456789").unwrap();
        assert_eq!(
            drawn.space(&Page::RequestCommit(398, id).route()),
            Some(REQUESTS)
        );
        assert_eq!(
            drawn.space(&Page::File("a/b.kt".into()).route()),
            Some(CODE)
        );
        let gone = Route::parse(r#"plugin.page{"x":1}"#).unwrap();
        assert!(!drawn.has(&gone) && drawn.space(&gone).is_none());
        // An item that is a page opens on itself; one no page draws (a comparison) on the page it was left on.
        let request = Page::Request(398).route();
        assert_eq!(drawn.item_page(&request), Some(request));
        assert_eq!(drawn.item_page(&comparison("main", "x")), None);
    }

    #[test]
    fn a_page_is_framed_as_its_pages_say() {
        use artoo_shell_api::Screen;
        let drawn = drawn();
        assert_eq!(drawn.screen(&Page::Request(398).route()), Screen::Item);
        assert_eq!(drawn.screen(&Page::WorkingTree.route()), Screen::Item);
        assert_eq!(
            drawn.screen(&Page::File("a/b.kt".into()).route()),
            Screen::Own
        );
        // A plugin's page has its own; one nobody draws has no islands but the tool window.
        let issue = Route::parse(r#"jira.issue{"key":"PAY-1"}"#).unwrap();
        assert_eq!(drawn.screen(&issue), Screen::Own);
        let gone = Route::parse(r#"plugin.page{"x":1}"#).unwrap();
        assert_eq!(drawn.screen(&gone), Screen::Own);
    }

    #[test]
    fn pages_and_open_items_survive_nav_txt() {
        let drawn = drawn();
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
            space: REQUESTS,
            ..Nav::default()
        };
        nav.pages
            .insert(CODE, Page::File("README.md".into()).route());
        nav.pages
            .insert(REQUESTS, Page::RequestCommit(398, id).route());
        let compare = Page::Compare(("main".into(), id), ("origin/x".into(), id), false).route();
        for (item, page) in [
            (Page::Request(398).route(), Page::Request(398).route()),
            (
                Page::Branch("refs/heads/main".into()).route(),
                Page::WorkingTree.route(),
            ),
            (comparison("main", "origin/x"), compare.clone()),
            (Page::WorkingTree.route(), Page::WorkingTree.route()),
        ] {
            nav.open.push(Opened {
                page,
                item,
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
        } = Nav::read(&nav.line("_Users_me_repo", &drawn), &[], &drawn).unwrap();
        assert_eq!(repo, "_Users_me_repo");
        assert_eq!(space, REQUESTS);
        assert_eq!(
            pages.get(&CODE).cloned(),
            Some(Page::File("README.md".into()).route())
        );
        assert_eq!(pages.get(&BRANCHES), None);
        assert_eq!(
            pages.get(&REQUESTS).cloned(),
            Some(Page::RequestCommit(398, id).route())
        );
        // A comparison comes back with its page, its sides' commits in it; an item that is a page alone.
        assert_eq!(
            open,
            [
                (Page::Request(398).route(), None),
                (Page::Branch("refs/heads/main".into()).route(), None),
                (comparison("main", "origin/x"), Some(compare)),
                (Page::WorkingTree.route(), None),
            ]
        );
    }

    /// A route of a kind no page of the tab draws (a plugin gone) is dropped where it is read, column or item.
    #[test]
    fn a_route_nobody_draws_is_dropped() {
        let drawn = drawn();
        let gone = r#"plugin.page{"x":1}"#;
        let line = format!("r\t1\t-\t{gone}\t-\t-\t{gone} requests.request{{\"number\":7}}");
        let Saved { pages, open, .. } = Nav::read(&line, &[], &drawn).unwrap();
        assert!(pages.is_empty());
        assert_eq!(open, [(Page::Request(7).route(), None)]);
    }

    /// A space `nav.txt` has no column for is kept by its id, and its page after the open items; both read back
    /// while its owner offers them.
    #[test]
    fn a_space_of_its_own_is_kept_by_its_id() {
        let (drawn, jira) = (drawn(), Space("jira"));
        let issue = Route::parse(r#"jira.issue{"key":"DCT-123"}"#).unwrap();
        let mut nav = Nav {
            space: jira,
            ..Nav::default()
        };
        nav.pages.insert(jira, issue.clone());
        let line = nav.line("r", &drawn);
        assert!(line.starts_with("r\tjira\t"), "{line}");
        assert!(line.ends_with(&format!("\t{}", issue.text())), "{line}");
        let saved = Nav::read(&line, &[jira], &drawn).unwrap();
        assert_eq!(saved.space, jira);
        assert_eq!(saved.pages.get(&jira), Some(&issue));
        assert!(Nav::read(&line, &[], &drawn).is_none());
        // Its pages gone, the line still reads: only what they drew is dropped.
        let none = Drawn::default();
        let saved = Nav::read(&line, &[jira], &none).unwrap();
        assert!(saved.pages.is_empty());
    }

    #[test]
    fn a_line_written_before_routes_still_reads() {
        let drawn = drawn();
        let id = "0123456789012345678901234567890123456789";
        let line = format!(
            "_Users_me_repo\t3\t-\tfile:README.md\t-\trequest-commit:398:{id}\trequest:398 branch:refs/heads/main compare-direct:{id}:{id}:main:origin/x working-tree"
        );
        let Saved {
            space, pages, open, ..
        } = Nav::read(&line, &[], &drawn).unwrap();
        let id = Oid::from_str(id).unwrap();
        assert_eq!(space, REQUESTS);
        assert_eq!(
            pages.get(&CODE).cloned(),
            Some(Page::File("README.md".into()).route())
        );
        assert_eq!(
            pages.get(&REQUESTS).cloned(),
            Some(Page::RequestCommit(398, id).route())
        );
        let compare = Page::Compare(("main".into(), id), ("origin/x".into(), id), false).route();
        assert_eq!(
            open,
            [
                (Page::Request(398).route(), None),
                (Page::Branch("refs/heads/main".into()).route(), None),
                (comparison("main", "origin/x"), Some(compare)),
                (Page::WorkingTree.route(), None),
            ]
        );
    }

    #[test]
    fn time_drops_what_was_ahead_and_keeps_one_of_a_kind() {
        let mut nav = Nav::default();
        let at = |space, page: Option<Page>| Moment {
            space,
            page: page.map(|p| p.route()),
        };
        nav.remember(at(INBOX, None));
        nav.remember(at(REQUESTS, Some(Page::Request(1))));
        nav.remember(at(REQUESTS, Some(Page::Request(1))));
        nav.remember(at(BRANCHES, None));
        assert_eq!(nav.time.len(), 3);
        // A commit of the main line is a page of its own: a moment after its branch's.
        let id = Oid::from_str("0123456789012345678901234567890123456789").unwrap();
        let main = "refs/heads/main".to_owned();
        nav.remember(at(BRANCHES, Some(Page::Branch(main.clone()))));
        nav.remember(at(BRANCHES, Some(Page::BranchCommit(main, id))));
        assert_eq!(nav.time.len(), 5);
        nav.time.truncate(3);
        nav.at = 0;
        assert!(nav.can_forward() && !nav.can_back());
        nav.remember(at(CODE, None));
        assert_eq!(nav.time.len(), 2);
        assert!(!nav.can_forward());
    }
}
