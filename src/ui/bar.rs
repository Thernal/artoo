//! The page bar (#71): one 44 px line over every page — the path ending in the item's id, each
//! part above it a way up; "‹ Inbox" while a page reached from another space is on screen; the
//! page's view switch; its actions and one primary action. Nothing else: the item's facts are the
//! first block of its content. A request's bar carries the public orange along its top.

use super::nav::Page;
use super::spaces::{BRANCHES, CODE, INBOX, REQUESTS, Space};
use super::{Workspace, px, theme, words};
use artoo_core::route::{Place, Route};
use artoo_requests_api::{RequestRoute, Stand};
use artoo_shell_api::bar::BarAction;
use artoo_shell_api::{MenuItem, NameHover, PagePath, PathUp, bar};
use gpui::{
    AnyElement, App, ClickEvent, Context, FontWeight, Pixels, Point, WeakEntity, Window, div,
    prelude::*,
};
use std::rc::Rc;

/// The folder of Code a part of the path goes up to, for its right click and its note's dot.
fn code_folder(up: &PathUp) -> Option<String> {
    match up.to.as_ref().and_then(Page::from_route) {
        Some(Page::Folder(path)) => Some(path),
        _ => None,
    }
}

impl Workspace {
    /// The bar of the page on screen; None in zen, which has its own line.
    pub(super) fn render_page_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        // Zen takes it away, folding it up as it does the sides.
        if self.zen.is_some() && !self.motion.recent("zen") {
            return None;
        }
        let space = self.nav.space;
        let page = self.shown_page();
        let PagePath {
            up: path,
            name,
            mono,
            hover,
            count,
        } = self.path_of(space, cx);
        // Narrow, the path is ‹ — one level up — and the id (#80).
        let up = (self.compact())
            .then(|| path.last().map(|c| c.to.clone()))
            .flatten();
        let path = if self.compact() { Vec::new() } else { path };
        let count = count.or_else(|| self.count_of(space, cx));
        let public = space == REQUESTS && page.is_some();
        // Narrow, ‹ is one level up; ⌘[ and the title bar still go back (#80).
        let from = self.nav.from.filter(|_| page.is_some() && !self.compact());
        let links: Vec<Option<Route>> = path.iter().map(|c| c.to.clone()).collect();
        let folders: Vec<Option<String>> = path.iter().map(code_folder).collect();
        let crumbs = path
            .into_iter()
            .map(|crumb| {
                // A folder of Code: its right click writes a note for your agent on it, and a note
                // waiting there is its dot, as in the tree (A, 2026-10-23).
                let folder = code_folder(&crumb);
                bar::Crumb {
                    noted: folder
                        .as_deref()
                        .is_some_and(|f| self.agents.folder_waits(f, cx)),
                    open: folder
                        .as_deref()
                        .is_some_and(|f| self.beside.as_ref().is_some_and(|b| b.folder == f)),
                    list: folder.as_deref().and_then(|f| self.render_beside(f, cx)),
                    link: true,
                    folder: folder.is_some(),
                    label: crumb.label,
                    mono: crumb.mono,
                }
            })
            .collect();
        // Another page's left is another width (#186).
        self.bar_room.page(format!(
            "{}|{}|{}|{name}",
            space.id(),
            up.is_some(),
            links.len()
        ));
        let host = Rc::new(BarGlue {
            tab: cx.weak_entity(),
            space,
            up: up.clone(),
            links,
            folders,
        });
        Some(bar::page_bar(
            bar::PageBar {
                from: from.map(|s| super::spaces::label(s, cx)),
                up: up.is_some(),
                crumbs,
                title: title(words::say(&name).into_owned(), mono, hover),
                count,
                view: self.view_of(space, page.as_ref(), cx),
                actions: self.actions_of(space, page.as_ref(), cx),
                more: self.more_of(page.as_ref(), cx),
                room: self.bar_room.clone(),
                public,
            },
            host,
            cx,
        ))
    }

    /// A folder of the path's right click: a note for your agent on it, its path, Finder.
    fn crumb_menu(
        &mut self,
        folder: String,
        at: gpui::Point<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) {
        use super::menu::{Act, Entry};
        let mut place = vec![Entry::new("Copy path", Act::copy(folder.clone()))];
        if let Some(root) = &self.root {
            place.push(Entry::new(
                artoo_ui_kit::controls::REVEAL,
                Act::reveal(root.join(&folder)),
            ));
        }
        self.open_menu(
            at,
            vec![
                vec![Entry::new(
                    "A note for your agent on this folder",
                    Act::new(move |this, _, cx| {
                        this.open_folder_page(folder.clone(), cx);
                        this.start_folder_comment(&folder, cx);
                    }),
                )],
                place,
            ],
            cx,
        )
    }

    /// The path above the page on screen, and its name: what its owner's pages say of it (`Pages::path`); a root's
    /// own name, or a page whose owner says nothing.
    fn path_of(&self, space: Space, cx: &mut Context<Self>) -> PagePath {
        let said = self.nav_page().and_then(|route| {
            let pages = self.pages_drawing(space, Some(route))?;
            (pages.path)(route, &self.drawing(cx), cx)
        });
        said.unwrap_or_else(|| {
            let project = (self.root.as_deref())
                .map(crate::labels::project_name)
                .unwrap_or_default();
            PagePath {
                up: Vec::new(),
                name: space_root(space, &project, cx),
                mono: false,
                hover: None,
                count: None,
            }
        })
    }

    /// The faint count after a root's name.
    fn count_of(&self, space: Space, cx: &App) -> Option<String> {
        let root = self.nav_page().is_none();
        match space {
            INBOX => Some(match self.inbox_unread(cx) {
                0 => "Nothing new".to_owned(),
                n => format!("{n} new"),
            }),
            // Once the filters say it with their counts, the name has none.
            REQUESTS
                if root
                    && self.requests_available(cx)
                    && self.request_login(cx).is_none()
                    && !self.request_list(cx).is_empty() =>
            {
                Some(self.request_list(cx).len().to_string())
            }
            BRANCHES if root => Some(self.branches_count(cx)),
            _ => None,
        }
    }

    /// The page's view switch: the Inbox's filters, the requests list's, a request's section
    /// tabs (#73).
    fn view_of(
        &self,
        space: Space,
        page: Option<&Page>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        match (space, page) {
            (INBOX, _) => Some(self.inbox_filters(cx).into_any_element()),
            (REQUESTS, None) if self.requests_available(cx) => {
                self.request_filters(cx).map(IntoElement::into_any_element)
            }
            _ if self.screen() == artoo_shell_api::Screen::Item => {
                self.section_tabs(cx).map(IntoElement::into_any_element)
            }
            _ => None,
        }
    }

    /// The page's actions, its primary one last; a list's filter (#72). Each but a filter and the approval has its row
    /// in `⋯`, where it goes when the bar has no room for it (#186).
    fn actions_of(
        &self,
        space: Space,
        page: Option<&Page>,
        cx: &mut Context<Self>,
    ) -> Vec<BarAction> {
        let tab = cx.weak_entity();
        let mut out = Vec::new();
        match (space, page) {
            (REQUESTS, None) if self.requests_available(cx) => {
                out.push(BarAction::new("filter", self.bar_filter("Filter", cx)))
            }
            (BRANCHES, None) => out.push(BarAction::new(
                "filter",
                self.bar_filter("Filter branches", cx),
            )),
            (INBOX, _) if self.inbox_unread(cx) > 0 => out.push(
                BarAction::new(
                    "read-all",
                    self.bar_action("bar-read-all", "check", "Mark all read", "")
                        .on_click(
                            cx.listener(|this, _: &ClickEvent, _, cx| this.read_all_notices(cx)),
                        ),
                )
                .folds(MenuItem::new(
                    "Mark all read",
                    on_tab(&tab, |this, cx| this.read_all_notices(cx)),
                )),
            ),
            // A folder's page: a note for your agent on it (#97), then Open in.
            (CODE, Some(Page::Folder(path))) => {
                let (folder, again) = (path.clone(), path.clone());
                out.push(
                    BarAction::new(
                        "folder-note",
                        self.bar_action("bar-folder-note", "bot", "Note for your agent", "C")
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.start_folder_comment(&folder, cx)
                            })),
                    )
                    .folds(
                        MenuItem::new(
                            "Note for your agent",
                            on_tab(&tab, move |this, cx| this.start_folder_comment(&again, cx)),
                        )
                        .key("C"),
                    ),
                );
                out.extend(self.open_in_action(&tab, cx));
            }
            // The file or the project's folder in the app it belongs to, named (#78).
            (CODE, _) => out.extend(self.open_in_action(&tab, cx)),
            (BRANCHES, Some(Page::Branch(refname))) => {
                let ix = self
                    .branch_list(cx)
                    .iter()
                    .position(|b| &b.refname == refname);
                match ix.and_then(|ix| self.request_of_branch(ix, cx).map(|m| m.iid)) {
                    // Its request has a page of its own: the way there.
                    Some(iid) => {
                        let label = format!("{} →", words::number(iid));
                        out.push(
                            BarAction::new(
                                "request",
                                self.bar_action("bar-request", "pull-request", &label, "")
                                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                        this.open_request(iid, cx)
                                    })),
                            )
                            .folds(MenuItem::new(
                                label,
                                on_tab(&tab, move |this, cx| this.open_request(iid, cx)),
                            )),
                        )
                    }
                    // Not while one is being written: its own Create is the primary then.
                    None if self.requests_available(cx) && !self.creating(cx) => out.push(
                        BarAction::new(
                            "create",
                            bar_button("bar-create", "Create merge request", "⌥⌘M", true).on_click(
                                cx.listener(move |this, _: &ClickEvent, _, cx| {
                                    this.open_create(ix, cx)
                                }),
                            ),
                        )
                        .folds(
                            MenuItem::new(
                                "Create merge request",
                                on_tab(&tab, move |this, cx| this.open_create(ix, cx)),
                            )
                            .key("⌥⌘M"),
                        ),
                    ),
                    None => {}
                }
            }
            (REQUESTS, Some(Page::RequestCommit(..)))
            | (BRANCHES, Some(Page::BranchCommit(..))) => {
                for (id, icon, label, key, next) in [
                    ("bar-previous-commit", "back", "Previous", "[", false),
                    ("bar-next-commit", "forward", "Next", "]", true),
                ] {
                    out.push(
                        BarAction::new(
                            id,
                            self.bar_action(id, icon, label, key).on_click(cx.listener(
                                move |this, _: &ClickEvent, _, cx| {
                                    this.step_item_commit(next, cx);
                                },
                            )),
                        )
                        .folds(
                            MenuItem::new(
                                format!("{label} commit"),
                                on_tab(&tab, move |this, cx| {
                                    this.step_item_commit(next, cx);
                                }),
                            )
                            .key(key),
                        ),
                    );
                }
            }
            (REQUESTS, Some(Page::Request(_))) if self.showing_request(cx) => {
                let pending = self.request_drafts(cx).len();
                let label = match pending {
                    0 => "Submit review".to_owned(),
                    n => format!("Submit review · {n}"),
                };
                out.push(
                    BarAction::new(
                        "submit",
                        self.bar_action("bar-submit", "send", &label, "⇧⌘↵")
                            .on_click(
                                cx.listener(|this, _: &ClickEvent, _, cx| this.open_submit(cx)),
                            ),
                    )
                    .folds(
                        MenuItem::new(label, on_tab(&tab, |this, cx| this.open_submit(cx)))
                            .key("⇧⌘↵"),
                    ),
                );
                // A merged or closed one says so where its approval would be (#114).
                let state = self
                    .current_request(cx)
                    .and_then(|iid| self.find_request(iid, cx))
                    .map(|m| m.state)
                    .unwrap_or_default();
                if state == crate::mr::MrState::Opened
                    && let Some(requests) = self.requests_api()
                {
                    // The approval is the request's state: it never folds.
                    out.extend(
                        (requests.approval_chip(self.compact(), cx))
                            .map(|chip| BarAction::new("approval", chip)),
                    );
                    let stand = requests.merge_stand(cx);
                    out.extend(
                        (requests.merge_control(self.compact(), self.on_github(), cx)).map(
                            |control| {
                                let action = BarAction::new("merge", control);
                                match merge_row(stand, &tab) {
                                    Some(row) => action.folds(row),
                                    None => action,
                                }
                            },
                        ),
                    );
                } else {
                    let color = if state == crate::mr::MrState::Merged {
                        theme::accent()
                    } else {
                        theme::removed()
                    };
                    out.push(BarAction::new(
                        "state",
                        div()
                            .flex_none()
                            .px(px(8.))
                            .h(px(22.))
                            .flex()
                            .items_center()
                            .rounded_full()
                            .border_1()
                            .border_color(color)
                            .text_size(px(12.))
                            .text_color(color)
                            .child(state.word()),
                    ));
                }
            }
            _ => {}
        }
        out
    }

    /// Open in, and its row in `⋯`.
    fn open_in_action(&self, tab: &WeakEntity<Self>, cx: &mut Context<Self>) -> Option<BarAction> {
        let label = (self.open_target(cx))
            .and_then(|(path, _)| self.open_app(&path, cx))
            .map_or("Open in…".to_owned(), |app| {
                format!("Open in {}", app.name())
            });
        let button = self.open_in_button(cx)?;
        Some(
            BarAction::new("open-in", button)
                .folds(MenuItem::new(label, on_tab(tab, |this, cx| this.open_in(cx))).key("⌥⌘O")),
        )
    }

    /// The rows only `⋯` holds, after the actions folded into it: a request's page on its host and its link (#186).
    fn more_of(&self, page: Option<&Page>, cx: &mut Context<Self>) -> Vec<MenuItem> {
        let Some(Page::Request(iid)) = page else {
            return Vec::new();
        };
        let tab = cx.weak_entity();
        let mut rows = Vec::new();
        if let (Some(web), Ok(link)) = (
            self.web.as_ref(),
            self.route_link(&RequestRoute { number: *iid }.route(), false, cx),
        ) {
            let url = link.url;
            rows.push(MenuItem::new(
                format!("Open in {} ↗", web.name()),
                move |_, cx| cx.open_url(&url),
            ));
        }
        rows.push(
            MenuItem::new(
                "Copy link",
                on_tab(&tab, |this, cx| this.copy_page_link(cx)),
            )
            .key("⌥⇧⌘C"),
        );
        rows
    }
}

/// Merge's row in `⋯`, by where merging stands: what is in the way said, dimmed; Rebase; Merge…; Cancel auto-merge.
fn merge_row(stand: Option<Stand>, tab: &WeakEntity<Workspace>) -> Option<MenuItem> {
    Some(match stand? {
        Stand::InTheWay(blocks) => MenuItem::off(match blocks.first() {
            Some(block) => format!("Merge · {}", block.detail),
            None => "Merge".to_owned(),
        }),
        Stand::Checking => MenuItem::off("Merge · checking"),
        Stand::Rebase(behind) => MenuItem::new(
            format!("Rebase · {behind} behind"),
            on_tab(tab, |this, cx| this.rebase_request(cx)),
        ),
        Stand::Running | Stand::Ready => {
            MenuItem::new("Merge…", on_tab(tab, |this, cx| this.open_merge(cx)))
        }
        Stand::Auto => MenuItem::new(
            "Cancel auto-merge",
            on_tab(tab, |this, cx| this.cancel_auto_merge(cx)),
        ),
    })
}

/// A row of `⋯` that runs `act` on the tab.
fn on_tab(
    tab: &WeakEntity<Workspace>,
    act: impl Fn(&mut Workspace, &mut Context<Workspace>) + 'static,
) -> impl Fn(&mut Window, &mut App) + 'static {
    let tab = tab.clone();
    move |_, cx| {
        tab.update(cx, |this, cx| act(this, cx)).ok();
    }
}

/// What a click on the page bar does, answered by the tab.
struct BarGlue {
    tab: WeakEntity<Workspace>,
    space: Space,
    /// Where ‹ goes, narrow: a page, or the space's own (`None`).
    up: Option<Option<Route>>,
    /// Where each crumb goes, and the folder of Code it is.
    links: Vec<Option<Route>>,
    folders: Vec<Option<String>>,
}

impl BarGlue {
    fn show(&self, to: Option<Route>, cx: &mut App) {
        let space = self.space;
        self.tab
            .update(cx, |this, cx| {
                this.nav.from = None;
                this.show(space, to, cx)
            })
            .ok();
    }
}

impl bar::BarHost for BarGlue {
    fn back(&self, _: &mut Window, cx: &mut App) {
        self.tab.update(cx, |this, cx| this.go_back(cx)).ok();
    }

    fn up(&self, _: &mut Window, cx: &mut App) {
        if let Some(to) = self.up.clone() {
            self.show(to, cx);
        }
    }

    fn up_to(&self, n: usize, _: &mut Window, cx: &mut App) {
        if let Some(to) = self.links.get(n).cloned() {
            self.show(to, cx);
        }
    }

    fn folder(&self, n: usize, _: &mut Window, cx: &mut App) {
        if let Some(Some(folder)) = self.folders.get(n).cloned() {
            self.tab
                .update(cx, |this, cx| this.open_beside(folder, cx))
                .ok();
        }
    }

    fn folder_menu(&self, n: usize, at: Point<Pixels>, _: &mut Window, cx: &mut App) {
        if let Some(Some(folder)) = self.folders.get(n).cloned() {
            self.tab
                .update(cx, |this, cx| this.crumb_menu(folder, at, cx))
                .ok();
        }
    }

    fn menu(&self, at: Point<Pixels>, groups: Vec<Vec<MenuItem>>, _: &mut Window, cx: &mut App) {
        // The frame's menu, asked once the tab is read: it opens on the tab.
        if let Some(frame) = (self.tab.upgrade()).map(|tab| tab.read(cx).frame.clone()) {
            frame.menu(at, groups, cx);
        }
    }
}

/// The page's title with what a long hover on it says (the developer, 2026-10-23: the header names no author), its
/// owner's (`NameHover`): a request's identity, a commit's or a branch's card, a file's or a folder's whole path.
/// Without one, its own long hover gives the whole title when it is cut.
fn title(text: String, mono: bool, hover: Option<NameHover>) -> AnyElement {
    use super::hover::{CutInTip, Hover};
    let style = |title: gpui::Stateful<gpui::Div>| {
        title
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(theme::text())
            .when(mono, |s| s.font_family(theme::code_font()))
    };
    let title = || {
        style(
            div()
                .id("bar-title")
                .min_w_0()
                .cut_in_tip()
                .child(text.clone()),
        )
    };
    match hover {
        Some(NameHover::Tip(tip)) => title().tip(tip).into_any_element(),
        Some(NameHover::Card(card)) => title()
            .card(move |window, cx| card(window, cx))
            .into_any_element(),
        None => style(super::hover::cut("bar-title", text.clone())).into_any_element(),
    }
}

/// A root's name: the space's, Code's the project's.
fn space_root(space: Space, project: &str, cx: &App) -> String {
    match space {
        CODE => project.to_owned(),
        _ => super::spaces::label(space, cx).to_owned(),
    }
}

impl Workspace {
    /// A secondary action of the bar: its label and key, or in a narrow window its icon alone,
    /// the label and the key in its tooltip (#80).
    pub(super) fn bar_action(
        &self,
        id: &'static str,
        icon: &'static str,
        label: &str,
        key: &'static str,
    ) -> gpui::Stateful<gpui::Div> {
        if !self.compact() {
            return bar_button(id, label, key, false);
        }
        let tip = if key.is_empty() {
            words::say(label).into_owned()
        } else {
            format!("{}  {key}", words::say(label))
        };
        bar_icon(id, icon, tip)
    }
}

/// A bar's button as its icon alone, what it does in its tooltip.
pub(super) fn bar_icon(
    id: &'static str,
    icon: &'static str,
    tip: String,
) -> gpui::Stateful<gpui::Div> {
    use super::hover::Hover;
    div()
        .id(id)
        .flex_none()
        .size(px(28.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(super::ROW_RADIUS))
        .border_1()
        .border_color(theme::island_border())
        .cursor_pointer()
        .hover(|s| s.bg(theme::hover()))
        .child(
            super::icons::icon(icon)
                .size(px(15.))
                .text_color(theme::muted()),
        )
        .tip(tip)
}

/// A button of the bar (the kit's).
pub(super) use artoo_ui_kit::controls::bar_button;
