//! A window's repository tabs, the tab's side: the tabs are the window's (`tabs.rs`); a tab is a
//! workspace, and the window says to agents that it is there and what it has open (#153).

use super::Workspace;
use super::tabs::{Tab, Tabs};
pub(super) use artoo_shell_api::tabs::{TabInfo, TabModel};
use gpui::{App, Context, FocusHandle, WeakEntity, Window};
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

/// A window: its tabs.
pub type Shell = Tabs<Workspace>;

impl Tab for Workspace {
    fn make(
        path: Option<PathBuf>,
        first: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Workspace::new(path, first, window, cx)
    }

    fn join(&mut self, tabs: WeakEntity<Shell>, model: TabModel) -> Rc<Cell<bool>> {
        self.shell = Some(tabs);
        self.tabs = model;
        self.on_screen.clone()
    }

    fn root(&self) -> Option<PathBuf> {
        self.root.clone()
    }

    fn project(&self) -> Option<PathBuf> {
        self.project.clone()
    }

    fn info(&self, cx: &App) -> (String, bool, usize) {
        (self.tab_title(), self.has_news(cx), self.inbox_unread(cx))
    }

    fn focus(&self) -> FocusHandle {
        self.focus.clone()
    }
}

/// A window's tabs (`primary`: the first window, which keeps the session and opens what agents ask for).
pub(super) fn new(
    path: Option<PathBuf>,
    primary: bool,
    window: &mut Window,
    cx: &mut Context<Shell>,
) -> Shell {
    let shell = Tabs::new(path, primary, window, cx);
    // Agents read whether this window is there and what it has open; the first window opens
    // what they ask for (#153).
    let beat = Rc::new(RefCell::new(Beat::default()));
    cx.spawn_in(window, async move |this, cx| {
        loop {
            cx.background_executor()
                .timer(std::time::Duration::from_secs(2))
                .await;
            let beat = beat.clone();
            if this
                .update_in(cx, |this, window, cx| {
                    beat.borrow_mut().beat(this, window, cx)
                })
                .is_err()
            {
                return;
            }
        }
    })
    .detach();
    let key = cx.entity_id().as_u64();
    let folder = crate::bus::folder(cx);
    cx.on_release(move |_, _| {
        folder.absent(std::process::id(), key);
        super::system_notes::set_front(key, false);
    })
    .detach();
    // While a window is in front, news is not sent to the system's notifications (#26).
    super::system_notes::set_front(key, window.is_window_active());
    cx.observe_window_activation(window, move |_, window, _| {
        super::system_notes::set_front(key, window.is_window_active());
    })
    .detach();
    // The first start: Set up over the welcome (#143); a release that adds a part, that part.
    let parts = if primary {
        super::setup::due()
    } else {
        Vec::new()
    };
    if !parts.is_empty() {
        shell.active().update(cx, |w, cx| w.open_setup(parts, cx));
    }
    shell
}

/// What a window last said to agents.
#[derive(Default)]
struct Beat {
    /// The projects it said it has open, and when.
    said: Vec<PathBuf>,
    said_at: i64,
    /// Projects opened for an agent whose tabs are still loading (a tab has its root once loaded),
    /// with when: not opened twice meanwhile.
    opening: Vec<(PathBuf, i64)>,
}

impl Beat {
    /// Every 2 s: the window says it is there and what it has open (#153); the first window also
    /// opens a project an agent asked for, or acted on, that no window has, as a tab behind the one
    /// on screen — its workspace then takes the agent's event.
    fn beat(&mut self, shell: &mut Shell, window: &mut Window, cx: &mut Context<Shell>) {
        let folder = crate::bus::folder(cx);
        let mut projects = shell.projects(cx);
        let now = crate::bus::now();
        // Said again when it changes, and every 10 s so it stays fresh.
        if projects != self.said || now - self.said_at >= 10 {
            let presence = crate::bus::Presence {
                pid: std::process::id(),
                projects: projects.clone(),
                beat: now,
            };
            if folder.present(cx.entity_id().as_u64(), &presence) {
                self.said = projects.clone();
                self.said_at = now;
            }
        }
        if !shell.primary() {
            return;
        }
        let pid = std::process::id();
        self.opening
            .retain(|(path, at)| !projects.contains(path) && now - at < 30);
        let elsewhere: Vec<PathBuf> = folder
            .running()
            .into_iter()
            .filter(|p| p.pid == pid)
            .flat_map(|p| p.projects)
            .chain(self.opening.iter().map(|(path, _)| path.clone()))
            .collect();
        for event in folder.events() {
            let project = event.project.clone();
            if !projects.contains(&project) && !elsewhere.contains(&project) {
                if !project.is_dir() {
                    folder.take_event(&event.uid);
                    continue;
                }
                shell.open_behind(project.clone(), window, cx);
                projects.push(project.clone());
                self.opening.push((project.clone(), now));
                let said = format!(
                    "{} opened {} in a tab",
                    event.agent,
                    crate::labels::project_name(&project)
                );
                shell
                    .active()
                    .update(cx, |w, cx| w.toast_info(said.into(), cx));
            }
            if event.what == crate::bus::What::Open {
                folder.take_event(&event.uid);
            }
        }
    }
}

impl Workspace {
    pub(super) fn tab_title(&self) -> String {
        // Its own name when it has one (the developer, 2026-10-23), else its folder's.
        self.root
            .as_deref()
            .map_or_else(|| "New tab".to_owned(), crate::labels::project_name)
    }

    pub(super) fn has_news(&self, cx: &gpui::App) -> bool {
        self.branches().is_some_and(|b| b.has_news(cx))
    }

    /// Runs `f` on the shell this workspace is a tab of.
    pub(super) fn with_shell(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut Shell, &mut Window, &mut Context<Shell>) + 'static,
    ) {
        // After the event: the shell reads its tabs' workspaces (closing one reads its path), and
        // this workspace is still being updated now — reading it then panics.
        if let Some(shell) = self.shell.as_ref().and_then(WeakEntity::upgrade) {
            window.defer(cx, move |window, cx| {
                shell.update(cx, |s, cx| f(s, window, cx));
            });
        }
    }
}
