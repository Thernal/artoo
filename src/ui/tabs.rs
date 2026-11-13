//! Repository tabs: several tabs in one window, each with its own repository, selection, filter and scroll
//! position. `⌘T` opens one, `⌘W` closes, `⌘1…9` switch, `⌘⇧T` reopens the last closed. In Tabs mode they sit in
//! the title bar (D12, D21), drawn by the frame (`artoo_shell_api::tabs`). A tab is a workspace (`Tab`, `shell.rs`); the
//! window, the root, keeps them, which is on screen, the session.

use artoo_shell_api::tabs::{TabInfo, TabModel};
use gpui::{
    App, AppContext, Context, Entity, EntityId, FocusHandle, Render, WeakEntity, Window, div,
    prelude::*,
};
use std::cell::Cell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// What a window asks of its tabs.
pub trait Tab: Render + Sized {
    /// A tab on `path`; `first`, the one a window starts with when no session comes back (it may open what was
    /// open last).
    fn make(
        path: Option<PathBuf>,
        first: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self;
    /// It is one of `tabs`, `model` the window's tabs as they last said; what it gives back is whether it is the
    /// one on screen, set by the window.
    fn join(&mut self, tabs: WeakEntity<Tabs<Self>>, model: TabModel) -> Rc<Cell<bool>>;
    /// Its repository, once loaded.
    fn root(&self) -> Option<PathBuf>;
    /// Its project: the main working tree, else its folder.
    fn project(&self) -> Option<PathBuf>;
    /// Its title, whether it has news, and its unread notifications.
    fn info(&self, cx: &App) -> (String, bool, usize);
    fn focus(&self) -> FocusHandle;
}

/// A window's tabs.
pub struct Tabs<T: Tab> {
    tabs: Vec<Entity<T>>,
    active: usize,
    /// Each tab's "on screen", shared with it: one behind another tab polls its repository seldom.
    shown: HashMap<EntityId, Rc<Cell<bool>>>,
    /// The folder each tab was opened on: what it has open until it has loaded.
    asked: HashMap<EntityId, PathBuf>,
    closed: Vec<PathBuf>,
    model: TabModel,
    /// The session as last written, so it is written again only when it changes.
    saved: String,
    /// The first window keeps the session; one opened from the rail does not.
    primary: bool,
}

impl<T: Tab> Tabs<T> {
    /// The tabs of a window: `path` alone, or without one the tabs of the last session (a window opened from the
    /// rail is not `primary`: it does not write the session).
    pub fn new(
        path: Option<PathBuf>,
        primary: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut this = Self {
            tabs: Vec::new(),
            active: 0,
            shown: HashMap::new(),
            asked: HashMap::new(),
            closed: Vec::new(),
            model: Rc::default(),
            saved: String::new(),
            primary,
        };
        // An explicit path opens just that; otherwise the tabs of the last session come back.
        let (tabs, active) = if path.is_some() {
            (Vec::new(), 0)
        } else {
            artoo_core::storage::session()
        };
        if tabs.is_empty() {
            let tab = this.make(path, true, window, cx);
            this.attach(tab, window, cx);
        } else {
            for path in tabs {
                let tab = this.make(Some(path), false, window, cx);
                this.attach(tab, window, cx);
            }
            this.select(active.min(this.tabs.len() - 1), window, cx);
        }
        this
    }

    pub fn primary(&self) -> bool {
        self.primary
    }

    /// A new tab on `path`, kept as what it has open while it loads.
    fn make(
        &mut self,
        path: Option<PathBuf>,
        first: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<T> {
        let tab = cx.new(|cx| T::make(path.clone(), first, window, cx));
        if let Some(path) = path {
            self.asked.insert(tab.entity_id(), path);
        }
        tab
    }

    /// Wraps a new tab into the window and shows it.
    fn attach(&mut self, tab: Entity<T>, window: &mut Window, cx: &mut Context<Self>) {
        self.wrap(tab, cx);
        self.active = self.tabs.len() - 1;
        self.focus_active(window, cx);
        self.mark_shown();
        cx.notify();
    }

    /// Wraps a new tab into the window as its last, the one on screen staying.
    fn wrap(&mut self, tab: Entity<T>, cx: &mut Context<Self>) {
        let weak = cx.weak_entity();
        let model = self.model.clone();
        let shown = tab.update(cx, |t, _| t.join(weak, model));
        self.shown.insert(tab.entity_id(), shown);
        cx.observe(&tab, |_, _, cx| cx.notify()).detach();
        self.tabs.push(tab);
        self.mark_shown();
        cx.notify();
    }

    /// Opens `path` as the last tab, behind the one on screen, and gives it back.
    pub fn open_behind(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<T> {
        let tab = self.make(Some(path), false, window, cx);
        self.wrap(tab.clone(), cx);
        tab
    }

    /// Each tab's project: its main working tree, else its folder; a tab still loading, the folder it was
    /// opened on, so an agent's news about it opens no second tab however long it loads.
    pub fn projects(&self, cx: &App) -> Vec<PathBuf> {
        self.tabs
            .iter()
            .filter_map(|tab| {
                let t = tab.read(cx);
                (t.project().or_else(|| t.root()))
                    .or_else(|| self.asked.get(&tab.entity_id()).cloned())
            })
            .collect()
    }

    /// Tells each tab whether it is the one on screen (no tab is touched: one may be mid-update).
    fn mark_shown(&self) {
        for (ix, tab) in self.tabs.iter().enumerate() {
            if let Some(shown) = self.shown.get(&tab.entity_id()) {
                shown.set(ix == self.active);
            }
        }
    }

    pub fn active_ix(&self) -> usize {
        self.active
    }

    /// The tab on screen.
    pub fn active(&self) -> Entity<T> {
        self.tabs[self.active].clone()
    }

    /// The first tab (the snapshot checks drive it).
    #[cfg(feature = "snapshot")]
    pub fn first(&self) -> Entity<T> {
        self.tabs[0].clone()
    }

    fn focus_active(&self, window: &mut Window, cx: &mut Context<Self>) {
        let focus = self.tabs[self.active].read(cx).focus();
        window.focus(&focus, cx);
    }

    /// The tab `id`.
    pub fn tab_ix(&self, id: EntityId) -> Option<usize> {
        self.tabs.iter().position(|t| t.entity_id() == id)
    }

    /// The tab that has `path` open, as the tabs last said (a tab may be mid-update).
    pub fn tab_of(&self, path: &Path) -> Option<usize> {
        let same = |root: &Path| {
            root == path
                || matches!((root.canonicalize(), path.canonicalize()), (Ok(a), Ok(b)) if a == b)
        };
        self.model
            .borrow()
            .iter()
            .position(|t| t.root.as_deref().is_some_and(same))
    }

    /// Opens `path` in a tab of its own, and gives it back; a project already open is gone to, not opened twice.
    pub fn open_tab(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<T> {
        if let Some(ix) = self.tab_of(&path).filter(|&ix| ix < self.tabs.len()) {
            self.select(ix, window, cx);
            return self.tabs[ix].clone();
        }
        let tab = self.make(Some(path), false, window, cx);
        self.attach(tab.clone(), window, cx);
        tab
    }

    pub fn select(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        if ix < self.tabs.len() && ix != self.active {
            self.active = ix;
            self.focus_active(window, cx);
            self.mark_shown();
            cx.notify();
        }
    }

    pub fn step(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let n = self.tabs.len();
        let ix = if forward {
            (self.active + 1) % n
        } else {
            (self.active + n - 1) % n
        };
        self.select(ix, window, cx);
    }

    pub fn close(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        if ix >= self.tabs.len() {
            return;
        }
        if self.tabs.len() == 1 {
            cx.quit();
            return;
        }
        // The tab being closed may be mid-update (a click inside it): only its path is read.
        let root = self.tabs[ix].read(cx).root();
        self.closed.extend(root);
        let gone = self.tabs.remove(ix);
        self.shown.remove(&gone.entity_id());
        self.asked.remove(&gone.entity_id());
        if self.active > ix || self.active >= self.tabs.len() {
            self.active = self.active.saturating_sub(1);
        }
        self.mark_shown();
        self.focus_active(window, cx);
        cx.notify();
    }

    /// Writes the open repositories and the active tab when they differ from what was written.
    fn save_session(&mut self, cx: &mut Context<Self>) {
        if !self.primary {
            return;
        }
        let mut roots = Vec::new();
        let mut active = 0;
        for (ix, tab) in self.tabs.iter().enumerate() {
            // A tab still loading has no root yet; it keeps the last session's place.
            if let Some(root) = tab.read(cx).root() {
                if ix == self.active {
                    active = roots.len();
                }
                roots.push(root);
            }
        }
        if roots.is_empty() {
            return;
        }
        let key = format!("{active} {roots:?}");
        if key != self.saved {
            artoo_core::storage::save_session(&roots, active);
            self.saved = key;
        }
    }

    /// Moves tab `from` to `to` (the rail's Move up / down and its drag); ⌘1…9 follow.
    pub fn move_tab(&mut self, from: usize, to: usize, cx: &mut Context<Self>) {
        let n = self.tabs.len();
        if from >= n || to >= n || from == to {
            return;
        }
        let active = self.tabs[self.active].clone();
        let tab = self.tabs.remove(from);
        self.tabs.insert(to, tab);
        self.active = self.tabs.iter().position(|t| *t == active).unwrap_or(0);
        cx.notify();
    }

    pub fn reopen(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(path) = self.closed.pop() {
            let tab = self.make(Some(path), false, window, cx);
            self.attach(tab, window, cx);
        }
    }
}

impl<T: Tab> Render for Tabs<T> {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // A new drawing: the rows picked in it say where they are (F1, #184).
        artoo_ui_kit::hover::drawing(window, cx);
        let infos = self
            .tabs
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let t = t.read(cx);
                let (title, news, unread) = t.info(cx);
                TabInfo {
                    title,
                    news: i != self.active && news,
                    active: i == self.active,
                    root: t.root(),
                    unread,
                }
            })
            .collect();
        *self.model.borrow_mut() = infos;
        self.save_session(cx);
        div().size_full().child(self.tabs[self.active].clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tab that never loads: no root, no project.
    struct Loading(FocusHandle);

    impl Render for Loading {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
        }
    }

    impl Tab for Loading {
        fn make(_: Option<PathBuf>, _: bool, _: &mut Window, cx: &mut Context<Self>) -> Self {
            Loading(cx.focus_handle())
        }
        fn join(&mut self, _: WeakEntity<Tabs<Self>>, _: TabModel) -> Rc<Cell<bool>> {
            Rc::default()
        }
        fn root(&self) -> Option<PathBuf> {
            None
        }
        fn project(&self) -> Option<PathBuf> {
            None
        }
        fn info(&self, _: &App) -> (String, bool, usize) {
            (String::new(), false, 0)
        }
        fn focus(&self) -> FocusHandle {
            self.0.clone()
        }
    }

    /// A tab still loading has the folder it was opened on open, so an agent's news about that folder opens
    /// no second tab: the window's beat opened one every 30 s while the tabs waited (a rail of copies).
    #[test]
    fn a_tab_still_loading_has_its_folder_open() {
        let mut cx = gpui::TestAppContext::single();
        let window =
            cx.add_window(|window, cx| Tabs::<Loading>::new(Some("/a".into()), false, window, cx));
        window
            .update(&mut cx, |tabs, window, cx| {
                tabs.open_behind("/b".into(), window, cx);
                assert_eq!(
                    tabs.projects(cx),
                    [PathBuf::from("/a"), PathBuf::from("/b")]
                );
                tabs.close(0, window, cx);
                assert_eq!(tabs.projects(cx), [PathBuf::from("/b")]);
            })
            .unwrap();
    }
}
