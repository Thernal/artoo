//! The project rail on the window's left edge and the title bar's tabs, the tab's side: drawn by the shell
//! (`artoo_shell::rail`); a click goes to the window's tabs, a right click opens the project's menu here.

use super::Workspace;
use super::menu::{Act, Entry};
use super::shell::TabInfo;
use crate::storage::Projects;
use artoo_shell::rail::{self, TabsHost};
pub(super) use artoo_shell::rail::{hue, hue_at, initials, project_mark};
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
                this.open_palette(super::palette::Kind::Projects, cx)
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
        Some(rail::rail(self.tabs.borrow().clone(), self.tabs_host(cx)))
    }

    /// The tabs in the title bar; None unless Settings → Appearance chose Tabs.
    pub(super) fn render_title_tabs(&self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        if super::projects() != Projects::Tabs {
            return None;
        }
        Some(rail::title_tabs(
            self.tabs.borrow().clone(),
            self.tabs_host(cx),
        ))
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
            moves.push(Entry::new(
                "Move up",
                Act::MoveProject {
                    from: ix,
                    to: ix - 1,
                },
            ));
        }
        if ix + 1 < n {
            moves.push(Entry::new(
                "Move down",
                Act::MoveProject {
                    from: ix,
                    to: ix + 1,
                },
            ));
        }
        let mut open = Vec::new();
        if let Some(root) = tab.root.clone() {
            open.push(Entry::new(
                if crate::platform::MAC {
                    "Reveal in Finder"
                } else {
                    "Show the folder"
                },
                Act::Reveal(root.clone()),
            ));
            open.push(Entry::new(
                "Open in a new window",
                Act::NewWindow(root.clone()),
            ));
            moves.insert(0, Entry::new("Name and colour…", Act::NameProject(root)));
        }
        let close = vec![Entry::new("Close", Act::CloseProject(ix))];
        self.open_menu(at, vec![moves, open, close], cx);
    }

    /// The projects in the menu of spaces (Settings → Appearance → Menu): one row each.
    pub(super) fn menu_projects(&self) -> Vec<(usize, TabInfo)> {
        if super::projects() != Projects::Menu {
            return Vec::new();
        }
        self.tabs.borrow().iter().cloned().enumerate().collect()
    }
}
