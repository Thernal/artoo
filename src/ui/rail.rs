//! The project rail on the window's left edge and the title bar's tabs, the tab's side: drawn by the shell
//! (`artoo_shell_api::tabs`); a click goes to the window's tabs, a right click opens the project's menu here.

use super::Workspace;
use super::menu::{Act, Entry};
use super::shell::TabInfo;
use crate::storage::Projects;
use artoo_shell_api::tabs::{self as rail, TabsHost};
pub(super) use artoo_ui_kit::monogram::{hue, hue_at, initials, monogram};
use gpui::{App, Context, IntoElement, Pixels, Point, WeakEntity, Window};
use std::rc::Rc;

/// What a click on the window's tabs does, answered by the tab.
struct Host(WeakEntity<Workspace>);

impl TabsHost for Host {
    fn select(&self, ix: usize, window: &mut Window, cx: &mut App) {
        self.0
            .update(cx, |this, cx| {
                this.with_shell(window, cx, move |s, window, cx| s.select(ix, window, cx))
            })
            .ok();
    }

    fn close(&self, ix: usize, window: &mut Window, cx: &mut App) {
        self.0
            .update(cx, |this, cx| {
                this.with_shell(window, cx, move |s, window, cx| s.close(ix, window, cx))
            })
            .ok();
    }

    fn moved(&self, from: usize, to: usize, window: &mut Window, cx: &mut App) {
        self.0
            .update(cx, |this, cx| {
                this.with_shell(window, cx, move |s, _, cx| s.move_tab(from, to, cx))
            })
            .ok();
    }

    fn menu(&self, ix: usize, at: Point<Pixels>, _: &mut Window, cx: &mut App) {
        self.0
            .update(cx, |this, cx| {
                let tabs = this.tabs.borrow().clone();
                if let Some(tab) = tabs.get(ix) {
                    this.project_menu(ix, tabs.len(), tab, at, cx)
                }
            })
            .ok();
    }

    fn add(&self, _: &mut Window, cx: &mut App) {
        self.0
            .update(cx, |this, cx| {
                this.search(super::palette_lists::PROJECTS, cx)
            })
            .ok();
    }
}

impl Workspace {
    /// The rail; None unless Settings chose it, and in zen.
    pub(super) fn render_project_rail(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        // Zen just began: it is drawn on its way out (#56, 16).
        if super::projects() != Projects::Rail || (self.zen.is_some() && !self.motion.recent("zen"))
        {
            return None;
        }
        let host = self.tabs_host(cx);
        Some(rail::rail(self.tabs.borrow().clone(), host, cx))
    }

    /// The tabs in the title bar; None unless Settings → Appearance chose Tabs.
    pub(super) fn render_title_tabs(&self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        if super::projects() != Projects::Tabs {
            return None;
        }
        let host = self.tabs_host(cx);
        Some(rail::title_tabs(self.tabs.borrow().clone(), host, cx))
    }

    pub(super) fn tabs_host(&self, cx: &mut Context<Self>) -> Rc<dyn TabsHost> {
        Rc::new(Host(cx.weak_entity()))
    }

    /// The right-click menu of project `ix` of `n`.
    pub(super) fn project_menu(
        &mut self,
        ix: usize,
        n: usize,
        tab: &TabInfo,
        at: gpui::Point<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) {
        let mut moves = Vec::new();
        if ix > 0 {
            moves.push(Entry::new("Move up", move_project(ix, ix - 1)));
        }
        if ix + 1 < n {
            moves.push(Entry::new("Move down", move_project(ix, ix + 1)));
        }
        let mut open = Vec::new();
        if let Some(root) = tab.root.clone() {
            open.push(Entry::new(
                if crate::platform::MAC {
                    "Reveal in Finder"
                } else {
                    "Show the folder"
                },
                Act::reveal(root.clone()),
            ));
            open.push(Entry::new("Open in a new window", {
                let root = root.clone();
                Act::new(move |_, _, cx| {
                    let root = root.clone();
                    cx.defer(move |cx| super::open_new_window(root, cx))
                })
            }));
            moves.insert(
                0,
                Entry::new(
                    "Name and colour…",
                    Act::new(move |this, _, cx| {
                        this.open_label(super::labeling::Target::Project(root.clone()), cx)
                    }),
                ),
            );
        }
        let close = vec![Entry::new(
            "Close",
            Act::new(move |this, window, cx| {
                this.with_shell(window, cx, move |s, window, cx| s.close(ix, window, cx))
            }),
        )];
        self.open_menu(at, vec![moves, open, close], cx);
    }
}

/// The rail's Move up and Move down (#49): project `from` to place `to`.
fn move_project(from: usize, to: usize) -> Act {
    Act::new(move |this, window, cx| {
        this.with_shell(window, cx, move |s, _, cx| s.move_tab(from, to, cx))
    })
}
