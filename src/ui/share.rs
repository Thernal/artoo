//! Links to what Artoo shows, copied without the browser (#162): a merge request's (its menu is
//! a link's, `super::links`), a branch's page on the host, a comparison's, a commit's, a file's in a diff the host
//! anchors, and the page on screen (⌘K, ⌥⇧⌘C). What each page is on the host its owner says
//! (`Pages::link`, `Pages::called`); the tab offers it in its menus. A toast says what was copied;
//! what has no link (a branch only on this Mac, what is not committed) says why instead.

use super::Workspace;
use super::menu::{Act, Entry};
use super::nav::Page;
use artoo_branches_api::{BranchCommitRoute, BranchRoute};
use artoo_core::route::{Place, Route};
use artoo_requests_api::RequestRoute;
use artoo_shell_api::Link;
use gpui::{App, Context, Pixels, Point};

/// No GitLab or GitHub remote: nothing here has a link.
const NO_HOST: &str = "No GitLab or GitHub remote: nothing here has a link";

/// Link `link` copied, its toast saying what it is.
pub(super) fn copied(link: Link) -> Act {
    Act::new(move |this, _, cx| this.link_copied(link.url.clone(), &link.what, cx))
}

impl Workspace {
    /// Route `route` on the host — with `diff`, where the host shows its diff — as the pages drawing it say; why
    /// not when nothing has one. Asked inside the tab's update: a route's owner reads its own state.
    pub(super) fn route_link(&self, route: &Route, diff: bool, cx: &App) -> Result<Link, String> {
        let web = self.web.as_ref().ok_or_else(|| NO_HOST.to_owned())?;
        let pages = (self.parts.iter())
            .filter_map(|p| p.pages.as_ref())
            .find(|p| p.kinds.contains(&route.kind.as_str()));
        match pages {
            Some(pages) => (pages.link)(Some(route), web, diff, cx),
            None => Err("Nothing here has a link".to_owned()),
        }
    }

    /// What the page on screen is, for ⌘K's "Copy link to …", as its pages say.
    pub(super) fn page_kind(&self) -> &'static str {
        self.space_pages(self.nav.space)
            .map_or("this page", |p| (p.called)(self.nav_page()))
    }

    /// ⌘K's and ⌥⇧⌘C's: the page on screen's link copied, or why it has none. Asked once the tab's update is
    /// over, as the page's owner may read the tab for it (the Inbox's notes an agent answered, Code's branches).
    pub(super) fn copy_page_link(&mut self, cx: &mut Context<Self>) {
        let (web, route) = (self.web.clone(), self.nav_page().cloned());
        let link = self.space_pages(self.nav.space).map(|p| p.link.clone());
        let tab = cx.weak_entity();
        let app: &mut App = cx;
        app.defer(move |cx| {
            let said = match (&web, link) {
                (None, _) => Err(NO_HOST.to_owned()),
                (Some(web), Some(link)) => link(route.as_ref(), web, false, cx),
                (Some(_), None) => Err("Nothing here has a link".to_owned()),
            };
            tab.update(cx, |this, cx| match said {
                Ok(link) => this.link_copied(link.url, &link.what, cx),
                Err(why) => this.toast_info(why.into(), cx),
            })
            .ok();
        });
    }

    /// A request's number clicked: its page here, or with ⌘ (Ctrl elsewhere) its page on the host in the browser.
    pub(super) fn open_request_or_browser(
        &mut self,
        iid: u64,
        browser: bool,
        cx: &mut Context<Self>,
    ) {
        match self.web.as_ref().filter(|_| browser) {
            Some(web) => cx.open_url(&web.request(iid)),
            None => self.open_request(iid, cx),
        }
    }

    /// A request's right click, wherever its row is: its link's menu.
    pub(super) fn request_context(&mut self, iid: u64, at: Point<Pixels>, cx: &mut Context<Self>) {
        if let Some(url) = self.request_url(iid, cx) {
            self.link_menu(&url, at, cx);
        }
    }

    /// Request `iid`'s page on the host: the one the host gave, else made from the remote.
    pub(super) fn request_url(&self, iid: u64, cx: &App) -> Option<String> {
        self.web.as_ref()?;
        let link = self.route_link(&RequestRoute { number: iid }.route(), false, cx);
        link.ok().map(|l| l.url)
    }

    /// An open row's right click under Branches or Merge requests: its link, then closing it, the others, all of
    /// them, the merged ones (#194).
    pub(super) fn item_context(&mut self, item: &Route, at: Point<Pixels>, cx: &mut Context<Self>) {
        let mut groups = self.item_groups(item, cx);
        // Rename… first, alone (Design round 10, U1).
        let rename = {
            let item = item.clone();
            Entry::new(
                "Rename\u{2026}",
                Act::new(move |this, _, cx| this.start_rename(item.clone(), cx)),
            )
            .icon("pencil")
            .key("or double-click")
        };
        // Move to ▸ beside it (#246, Design round 48).
        let first = vec![
            rename,
            self.move_to_entry(super::groups::Unit::Page(item.clone()), cx),
        ];
        groups.insert(0, first);
        groups.push(self.close_entries(item, cx));
        self.open_menu(at, groups, cx);
    }

    /// An open row's link entries.
    fn item_groups(&self, item: &Route, cx: &App) -> Vec<Vec<Entry>> {
        let link = match Page::from_route(item) {
            Some(Page::Request(iid)) => {
                let linked = (self.request_url(iid, cx)).and_then(|url| self.linked(&url, cx));
                return linked.map(|l| self.link_groups(&l, cx)).unwrap_or_default();
            }
            Some(Page::Branch(refname)) => {
                return (self.branch_list(cx).iter())
                    .position(|b| b.refname == refname)
                    .and_then(|ix| self.branch_groups(ix, cx))
                    .unwrap_or_default();
            }
            Some(_) => return Vec::new(),
            // A comparison: its link is its page's.
            None => match self.nav.opened(item).map(|o| o.page.clone()) {
                Some(page) if matches!(Page::from_route(&page), Some(Page::Compare(..))) => {
                    self.route_link(&page, false, cx)
                }
                _ => return Vec::new(),
            },
        };
        match (link, &self.web) {
            (Ok(link), Some(web)) => {
                let open = Entry::new(
                    format!("Open in {} ↗", web.name()),
                    Act::open_url(link.url.clone()),
                );
                vec![
                    vec![Entry::new("Copy link", copied(link)).key("↗")],
                    vec![open],
                ]
            }
            (Err(_), Some(_)) => vec![vec![Entry::off("No link: only on this Mac")]],
            _ => Vec::new(),
        }
    }

    /// Closing from an open row's menu: it (⌘W), the others under its space, all of them, the merged or closed
    /// requests (#194).
    fn close_entries(&self, item: &Route, cx: &App) -> Vec<Entry> {
        let Some(space) = self.item_space(item) else {
            return Vec::new();
        };
        let items: Vec<Route> = (self.open_items(space).into_iter())
            .map(|o| o.item.clone())
            .collect();
        let gone: Vec<Route> = (items.iter())
            .filter(|i| match Page::from_route(i) {
                Some(Page::Request(iid)) => (self.requests_api())
                    .and_then(|r| r.find(iid, cx))
                    .is_some_and(|m| m.state != artoo_core::mr::MrState::Opened),
                _ => false,
            })
            .cloned()
            .collect();
        let others: Vec<Route> = items.iter().filter(|i| *i != item).cloned().collect();
        let closing = |items: Vec<Route>| {
            Act::new(move |this, _, cx| {
                for item in &items {
                    this.close_open(item, cx);
                }
            })
        };
        let mut entries = vec![
            Entry::new("Close", closing(vec![item.clone()])).key("⌘W"),
            if others.is_empty() {
                Entry::off("Close others")
            } else {
                Entry::new("Close others", closing(others))
            },
            Entry::new("Close all", closing(items)),
        ];
        if space == super::spaces::REQUESTS {
            entries.push(if gone.is_empty() {
                Entry::off("Close merged")
            } else {
                Entry::new(format!("Close merged ({})", gone.len()), closing(gone))
            });
        }
        entries
    }

    /// The link of file `path` in the diff on screen, where the host anchors its files.
    pub(super) fn diff_file_link(&self, path: &str, cx: &App) -> Option<Link> {
        let web = self.web.as_ref()?;
        let page = self.route_link(&self.loaded_page(cx)?, true, cx).ok()?;
        let name = path.rsplit('/').next().unwrap_or(path);
        Some(page.at(&web.file_anchor(path), name))
    }

    /// A commit's link entry, for its right click: Copy link, or why there is none.
    pub(super) fn commit_link_entry(&self, sha: git2::Oid, cx: &App) -> Option<Entry> {
        self.web.as_ref()?;
        // A commit's page on the host is the same whatever branch it is listed under.
        let route = BranchCommitRoute {
            refname: String::new(),
            commit: sha.to_string(),
        }
        .route();
        Some(match self.route_link(&route, false, cx) {
            Ok(link) => Entry::new("Copy link", copied(link)).key("↗"),
            Err(_) => Entry::off("No link: only on this Mac"),
        })
    }

    /// Branch `refname`'s page on the host, or why it has none.
    pub(super) fn branch_link(&self, refname: &str, cx: &App) -> Result<Link, String> {
        let route = BranchRoute {
            refname: refname.to_owned(),
        };
        self.route_link(&route.route(), false, cx)
    }

    /// A branch's link entry, for its right click: Copy link, or why there is none.
    pub(super) fn branch_link_entry(&self, refname: &str, cx: &App) -> Option<Entry> {
        self.web.as_ref()?;
        Some(match self.branch_link(refname, cx) {
            Ok(link) => Entry::new("Copy link", copied(link)).key("↗"),
            Err(_) => Entry::off("No link: only on this Mac"),
        })
    }
}
