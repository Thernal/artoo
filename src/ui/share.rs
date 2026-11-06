//! Links to what Artoo shows, copied without the browser (#162): a merge request's and its
//! number, a branch's page on the host, a comparison's, a commit's, a file's in a diff the host
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
    Act::copy_said(link.url, format!("Link to {}", link.what))
}

impl Workspace {
    /// Route `route` on the host — with `diff`, where the host shows its diff — as the pages drawing it say; why
    /// not when nothing has one. Asked inside the tab's update: a route's owner reads its own state.
    fn route_link(&self, route: &Route, diff: bool, cx: &App) -> Result<Link, String> {
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
                Ok(link) => this.copy_said(link.url, &format!("Link to {}", link.what), cx),
                Err(why) => this.toast_info(why.into(), cx),
            })
            .ok();
        });
    }

    /// A request's right click, wherever its row is: its link, its number, the host.
    pub(super) fn request_context(&mut self, iid: u64, at: Point<Pixels>, cx: &mut Context<Self>) {
        let (Some(web), Ok(link)) = (
            self.web.clone(),
            self.route_link(&RequestRoute { number: iid }.route(), false, cx),
        ) else {
            return;
        };
        let number = link.what.clone();
        let url = link.url.clone();
        let groups = vec![
            vec![
                Entry::new("Copy link", copied(link)).key("↗"),
                Entry::new(
                    format!("Copy {number}"),
                    Act::copy_said(number.clone(), number),
                ),
            ],
            vec![Entry::new(
                format!("Open in {} ↗", web.name()),
                Act::open_url(url),
            )],
        ];
        self.open_menu(at, groups, cx);
    }

    /// An open row's right click under Branches or Merge requests: its link.
    pub(super) fn item_context(&mut self, item: &Route, at: Point<Pixels>, cx: &mut Context<Self>) {
        let link = match Page::from_route(item) {
            Some(Page::Request(iid)) => return self.request_context(iid, at, cx),
            Some(Page::Branch(refname)) => {
                if let Some(ix) = self
                    .branch_list(cx)
                    .iter()
                    .position(|b| b.refname == refname)
                {
                    self.branch_context(ix, at, cx);
                }
                return;
            }
            Some(_) => return,
            // A comparison: its link is its page's.
            None => match self.nav.opened(item).map(|o| o.page.clone()) {
                Some(page) if matches!(Page::from_route(&page), Some(Page::Compare(..))) => {
                    self.route_link(&page, false, cx)
                }
                _ => return,
            },
        };
        let groups = match (link, &self.web) {
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
            _ => return,
        };
        self.open_menu(at, groups, cx);
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
