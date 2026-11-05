//! Code's space in one tab: what it reads and holds (`Code`), its tree, its tabs and its pages (`CodeSpace`), and
//! the handle the tab asks it through (`Handle`).

use artoo_code_api::{FilePage, FileRoute, FolderRoute, Host};
use artoo_core::files;
use artoo_core::git::Repo;
use artoo_core::route::{Place, Route};
use artoo_review_api::FileView;
use artoo_shell_api::{Frame, MenuItem, Pages};
use artoo_ui_kit::hover::{CutInTip, Tip, TipOwned};
use artoo_ui_kit::{icons, island, keys, markdown, px, theme};
use git2::Oid;
use gpui::{
    AnyElement, App, ClickEvent, ClipboardItem, Context, Div, Entity, MouseButton, MouseDownEvent,
    SharedString, Task, WeakEntity, div, prelude::*, uniform_list,
};
use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

/// The most files Code's tree lists.
const LIMIT: usize = 20_000;

#[derive(Default)]
pub struct Code {
    /// The revision read, by name: None the working tree, the files on disk.
    pub rev: Option<(String, Oid)>,
    /// The project's files at it, sorted.
    files: Arc<Vec<String>>,
    /// What `files` were read at: the revision, None the working tree.
    read_at: Option<Option<Oid>>,
    task: Option<Task<()>>,
    /// The folders open in the tree.
    open: HashSet<String>,
    /// The folder a click on its row just unfolded: the same double click's second click leaves
    /// it open rather than folding it again.
    unfolded_now: Option<String>,
    /// The files and folders kept open as tabs, in order — a folder's with a slash after it
    /// (`relay/`, the developer, 2026-10-23: a folder page no longer hides the files' tabs);
    /// `preview` is the one the next click replaces.
    pub tabs: Vec<String>,
    pub preview: Option<String>,
    /// The tabs pinned (#20): first in `tabs`, in their order; no close takes one, and they are
    /// kept with the project (`PINS_FILE`).
    pub pins: Vec<String>,
    /// The project whose pins are held.
    pins_of: Option<String>,
    /// The tabs' strip, scrolled to the one on screen.
    tab_scroll: gpui::ScrollHandle,
    /// The root's README at the revision: its path and text.
    readme: Option<(String, String)>,
    /// The folder page's README (#97): the folder, then its README's path and text, read at
    /// `read_at`'s revision.
    folder_readme: Option<(String, Option<(String, String)>)>,
    scroll: gpui::UniformListScrollHandle,
}

impl Code {
    /// The folders above `path` open, so the tree shows it.
    pub fn reveal(&mut self, path: &str) {
        let mut at = 0;
        while let Some(slash) = path[at..].find('/') {
            self.open.insert(path[..at + slash].to_owned());
            at += slash + 1;
        }
    }

    /// The revision as a commit; None the working tree.
    pub fn at(&self) -> Option<Oid> {
        self.rev.as_ref().map(|(_, id)| *id)
    }

    /// Folder `path` in the preview tab, unless it is kept as one.
    pub fn preview_folder(&mut self, path: &str) {
        let key = folder_tab(path);
        if !self.tabs.contains(&key) {
            self.preview = Some(key);
        }
    }

    /// The tabs in their order, the preview one where it was opened.
    fn shown_tabs(&self) -> Vec<(String, bool)> {
        let mut out: Vec<(String, bool)> = self.tabs.iter().map(|t| (t.clone(), false)).collect();
        if let Some(p) = &self.preview {
            out.push((p.clone(), true));
        }
        out
    }

    pub fn pinned(&self, key: &str) -> bool {
        self.pins.iter().any(|p| p == key)
    }

    /// Project `repo`'s pinned tabs, first among the tabs, unless they are the ones held.
    pub fn load_pins(&mut self, repo: &str) {
        if self.pins_of.as_deref() == Some(repo) {
            return;
        }
        self.pins = kept_pins(&artoo_core::storage::read(PINS_FILE), repo);
        let rest: Vec<String> = self
            .tabs
            .drain(..)
            .filter(|t| !self.pins.contains(t))
            .collect();
        self.tabs = self.pins.iter().cloned().chain(rest).collect();
        self.pins_of = Some(repo.to_owned());
    }

    /// Tab `key` pinned, the last of the pinned (a preview kept so), or unpinned, the first after
    /// them.
    fn toggle_pin(&mut self, key: &str) {
        let pin = !self.pinned(key);
        self.pins.retain(|p| p != key);
        self.tabs.retain(|t| t != key);
        self.tabs.insert(self.pins.len(), key.to_owned());
        if pin {
            self.pins.push(key.to_owned());
            if self.preview.as_deref() == Some(key) {
                self.preview = None;
            }
        }
        self.keep_pins();
    }

    fn keep_pins(&self) {
        if let Some(repo) = &self.pins_of {
            let text = artoo_core::storage::read(PINS_FILE);
            artoo_core::storage::save(PINS_FILE, with_pins(&text, repo, &self.pins));
        }
    }
}

/// Each project's pinned tabs: a line per project, its key and then its tabs, apart by tabs.
const PINS_FILE: &str = "code-pins.txt";

/// Project `repo`'s pinned tabs in `text`, in their order.
fn kept_pins(text: &str, repo: &str) -> Vec<String> {
    text.lines()
        .find(|l| l.split('\t').next() == Some(repo))
        .map(|l| {
            l.split('\t')
                .skip(1)
                .filter(|k| !k.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// `text` with project `repo`'s line written anew (none without pins); the others' kept.
fn with_pins(text: &str, repo: &str, pins: &[String]) -> String {
    let mut lines: Vec<String> = text
        .lines()
        .filter(|l| !l.is_empty() && l.split('\t').next() != Some(repo))
        .map(str::to_owned)
        .collect();
    if !pins.is_empty() {
        lines.push(format!("{repo}\t{}", pins.join("\t")));
    }
    lines.join("\n") + "\n"
}

/// One of Code's tabs being dragged: its path.
#[derive(Clone)]
pub struct DragCodeTab(pub String);

/// The tab of folder `path`: its path and a slash.
pub fn folder_tab(path: &str) -> String {
    format!("{path}/")
}

/// The page tab `key` opens: a folder's ends with a slash.
pub fn tab_page(key: &str) -> Page {
    match key.strip_suffix('/') {
        Some(folder) => Page::Folder(folder.to_owned()),
        None => Page::File(key.to_owned()),
    }
}

/// A page of Code's: a file read, or a folder's page.
#[derive(Clone, PartialEq, Debug)]
pub enum Page {
    File(String),
    Folder(String),
}

impl Page {
    /// The page `route` is, when it is one of Code's.
    fn from_route(route: &Route) -> Option<Page> {
        match route.kind.as_str() {
            FileRoute::KIND => Some(Page::File(FileRoute::from_route(route)?.path)),
            FolderRoute::KIND => Some(Page::Folder(FolderRoute::from_route(route)?.path)),
            _ => None,
        }
    }

    fn route(&self) -> Route {
        match self {
            Page::File(path) => FileRoute { path: path.clone() }.route(),
            Page::Folder(path) => FolderRoute { path: path.clone() }.route(),
        }
    }
}

/// The Code space of one tab (#75): what it reads and holds (`Code`), the project it reads, its tree and
/// its pages, drawn as its own view. What it asks of the tab goes through `host`, and what any part asks
/// (a menu, the drawer) through `frame`.
pub struct CodeSpace {
    code: Code,
    /// The project read, and whether it is a plain folder rather than a git repository.
    root: Option<PathBuf>,
    plain: bool,
    /// The page of Code's on screen, a file or a folder; None the project's own.
    shown: Option<Page>,
    host: Rc<dyn Host>,
    frame: Rc<dyn Frame>,
}

impl CodeSpace {
    pub fn new(host: Rc<dyn Host>, frame: Rc<dyn Frame>) -> Self {
        Self {
            code: Code::default(),
            root: None,
            plain: false,
            shown: None,
            host,
            frame,
        }
    }

    /// Its pages as the shell draws them: a file, a folder, the project's own; the tree beside them.
    pub fn pages(this: &Entity<Self>) -> Pages {
        let (side, show) = (this.clone(), this.clone());
        Pages {
            space: artoo_code_api::SPACE,
            kinds: &[FileRoute::KIND, FolderRoute::KIND],
            view: this.clone().into(),
            side: Some(Rc::new(move |_, width, cx| {
                Some(side.update(cx, |c, cx| c.render_tree(width, cx).into_any_element()))
            })),
            show: Rc::new(move |route, opened, cx| {
                show.update(cx, |c, cx| c.show(route, opened, cx));
            }),
        }
    }

    /// The tab's review, which reads and draws the file shown.
    fn file_view(&self) -> Option<Rc<dyn FileView>> {
        self.frame.services().get()
    }

    /// What is on screen (`Pages::show`): a file read at Code's revision — anew when `opened`, else unless it
    /// is the one read —, a folder's page in the preview tab, or the project's page.
    fn show(&mut self, route: Option<&Route>, opened: bool, cx: &mut Context<Self>) {
        self.shown = route.and_then(Page::from_route);
        match self.shown.clone() {
            None => self.read_files(false, cx),
            Some(Page::File(path)) => {
                let file = self.file_view();
                let read = file.as_ref().and_then(|f| f.path(cx)).as_ref() == Some(&path);
                if read && !opened {
                    return;
                }
                let Some(root) = self.root.clone() else {
                    return;
                };
                if !self.code.tabs.contains(&path) && self.code.preview.as_ref() != Some(&path) {
                    self.code.tabs.push(path.clone());
                }
                self.code.reveal(&path);
                self.scroll_to_tab(Some(&path));
                self.read_files(false, cx);
                self.frame
                    .remember(FileRoute { path: path.clone() }.route(), cx);
                let rev = self.code.at().filter(|_| !self.plain);
                if let Some(file) = file {
                    file.read(root, path, rev, cx);
                }
            }
            Some(Page::Folder(path)) => {
                // A folder reached by the path, back or forward, is in the preview tab too.
                self.code.preview_folder(&path);
                self.read_folder_readme(&path, cx);
            }
        }
    }

    /// File `path` opens: as the preview tab (`preview`) the next click replaces, or kept.
    fn open_file(&mut self, path: String, preview: bool, cx: &mut Context<Self>) {
        self.keep_or_preview(&path, preview);
        self.frame.open(FileRoute { path }.route(), cx);
    }

    /// File `path` takes the preview tab (`preview`) unless it has a tab, or a tab of its own.
    fn keep_or_preview(&mut self, path: &str, preview: bool) {
        let code = &mut self.code;
        if !code.tabs.iter().any(|t| t == path) {
            if preview {
                code.preview = Some(path.to_owned());
            } else {
                if code.preview.as_deref() == Some(path) {
                    code.preview = None;
                }
                code.tabs.push(path.to_owned());
            }
        }
    }

    /// Folder `path`'s page opens (#97), unfolded in the tree, in the preview tab.
    fn open_folder(&mut self, path: String, cx: &mut Context<Self>) {
        if self.reveal_folder(&path) {
            self.host.tree_opening(path.clone(), cx);
        }
        self.frame.open(FolderRoute { path }.route(), cx);
    }

    /// Folder `path` shown in the tree, unfolded, and in the preview tab: whether it was folded.
    fn reveal_folder(&mut self, path: &str) -> bool {
        self.code.reveal(path);
        self.code.preview_folder(path);
        self.code.open.insert(path.to_owned())
    }

    /// Tab `key` on screen: its file or its folder's page.
    fn open_tab(&mut self, key: &str, cx: &mut Context<Self>) {
        match tab_page(key) {
            Page::Folder(path) => self.open_folder(path, cx),
            Page::File(path) => self.frame.open(FileRoute { path }.route(), cx),
        }
        self.scroll_to_tab(Some(key));
    }

    /// The tab of the page on screen: a file's path, a folder's with its slash.
    fn shown_tab(&self) -> Option<String> {
        self.shown.as_ref().map(tab_of)
    }

    /// Project `root` read, a plain folder when `plain`, its pinned tabs kept by `key`; another than the one
    /// read (`fresh`), what was read of that one goes.
    fn set_project(&mut self, root: PathBuf, plain: bool, fresh: bool, key: &str) {
        if fresh {
            (self.code, self.shown) = (Code::default(), None);
        }
        self.code.load_pins(key);
        (self.root, self.plain) = (Some(root), plain);
    }

    /// ⌘W: the tab on screen closes.
    fn close_shown(&mut self, cx: &mut Context<Self>) {
        if let Some(key) = self.shown_tab() {
            self.close_tab(&key, cx);
        }
    }

    /// Tab `key` closes, its neighbour shows if it was on screen, else the project's page. A pinned
    /// one stays: unpinned first (#20).
    fn close_tab(&mut self, key: &str, cx: &mut Context<Self>) {
        if self.code.pinned(key) {
            return self.frame.toast("Pinned: unpin it to close it".into(), cx);
        }
        let code = &mut self.code;
        let at = code.shown_tabs().iter().position(|(p, _)| p == key);
        code.tabs.retain(|t| t != key);
        if code.preview.as_deref() == Some(key) {
            code.preview = None;
        }
        if self.shown_tab().as_deref() != Some(key) {
            return cx.notify();
        }
        let rest = self.code.shown_tabs();
        let next = at.and_then(|at| rest.get(at.min(rest.len().saturating_sub(1))).cloned());
        match next {
            Some((next, _)) => self.open_tab(&next, cx),
            None => self.close_all(cx),
        }
    }

    /// The last tab gone: the project's page.
    fn close_all(&mut self, cx: &mut Context<Self>) {
        if let Some(file) = self.file_view() {
            file.close(cx);
        }
        self.frame.open_space(artoo_code_api::SPACE, cx);
    }

    /// Every tab closes but `keep`'s (Close others), or every one (Close all) — the pinned ones
    /// stay.
    fn close_tabs(&mut self, keep: Option<String>, cx: &mut Context<Self>) {
        let code = &mut self.code;
        let keep = keep.filter(|k| code.shown_tabs().iter().any(|(t, _)| t == k));
        code.tabs
            .retain(|t| Some(t) == keep.as_ref() || code.pins.contains(t));
        if code.preview != keep {
            code.preview = None;
        }
        self.after_closing(keep, cx);
    }

    /// Close to the right: the tabs after `key` close, the pinned ones staying.
    fn close_tabs_right(&mut self, key: String, cx: &mut Context<Self>) {
        let code = &mut self.code;
        let tabs = code.shown_tabs();
        let Some(at) = tabs.iter().position(|(t, _)| *t == key) else {
            return;
        };
        let gone: Vec<String> = tabs[at + 1..]
            .iter()
            .map(|(t, _)| t.clone())
            .filter(|t| !code.pinned(t))
            .collect();
        code.tabs.retain(|t| !gone.contains(t));
        if code.preview.as_ref().is_some_and(|p| gone.contains(p)) {
            code.preview = None;
        }
        self.after_closing(Some(key), cx);
    }

    /// Tabs closed: `keep` shows if the one on screen went, the one on screen stays if it did not,
    /// else the project's page.
    fn after_closing(&mut self, keep: Option<String>, cx: &mut Context<Self>) {
        let left = self
            .shown_tab()
            .is_some_and(|s| self.code.shown_tabs().iter().any(|(t, _)| *t == s));
        match keep {
            _ if left => cx.notify(),
            Some(key) => self.open_tab(&key, cx),
            None => self.close_all(cx),
        }
    }

    /// ⌃⇥ and ⌃⇧⇥: the next or the previous tab, going round.
    fn step_tab(&mut self, forward: bool, cx: &mut Context<Self>) {
        let shown = self.code.shown_tabs();
        if shown.is_empty() {
            return;
        }
        let n = shown.len();
        let on = self.shown_tab();
        let at = shown.iter().position(|(p, _)| Some(p) == on.as_ref());
        let to = match at {
            Some(at) => (at + if forward { 1 } else { n - 1 }) % n,
            None => 0,
        };
        let key = shown[to].0.clone();
        self.open_tab(&key, cx);
    }

    /// ⌥⌘P and ⌘K's Pin tab: the tab on screen pinned or unpinned; false when there is none.
    fn pin_shown(&mut self, cx: &mut Context<Self>) -> bool {
        let tabs = self.code.shown_tabs();
        let Some(key) = self
            .shown_tab()
            .filter(|k| tabs.iter().any(|(t, _)| t == k))
        else {
            return false;
        };
        self.pin(&key, cx);
        true
    }

    /// Whether the tab on screen is pinned: ⌘K says Unpin.
    fn shown_pinned(&self) -> bool {
        self.shown_tab().is_some_and(|k| self.code.pinned(&k))
    }

    /// Code reads at `rev` (None: the working tree); the page on screen is read again there.
    fn set_revision(&mut self, rev: Option<(String, Oid)>, cx: &mut Context<Self>) {
        self.code.rev = rev;
        self.read_files(false, cx);
        match self.shown.clone() {
            Some(page @ Page::File(_)) => self.show(Some(&page.route()), true, cx),
            Some(Page::Folder(path)) => {
                self.code.folder_readme = None;
                self.read_folder_readme(&path, cx);
            }
            _ => {}
        }
        cx.notify();
    }

    /// An item of a menu running `act` on this Code space.
    fn item(
        &self,
        label: impl Into<String>,
        cx: &Context<Self>,
        act: impl Fn(&mut Self, &mut Context<Self>) + 'static,
    ) -> MenuItem {
        let this = cx.entity().downgrade();
        MenuItem::new(label, move |_, cx| {
            this.update(cx, |c, cx| act(c, cx)).ok();
        })
    }

    /// Tab `key` pinned — first among the tabs, through every close and a restart — or unpinned,
    /// first after the pinned ones (#20); the strip scrolled to the tab on screen.
    fn pin(&mut self, key: &str, cx: &mut Context<Self>) {
        self.code.toggle_pin(key);
        let shown = self.shown_tab();
        self.scroll_to_tab(shown.as_deref());
        cx.notify();
    }

    /// The strip scrolled to tab `shown`, the one on screen.
    pub fn scroll_to_tab(&self, shown: Option<&str>) {
        if let Some(ix) = self
            .code
            .shown_tabs()
            .iter()
            .position(|(key, _)| Some(key.as_str()) == shown)
        {
            self.code.tab_scroll.scroll_to_item(ix);
        }
    }

    /// Reads the project's files at Code's revision, once per revision (and again with `force`).
    fn read_files(&mut self, force: bool, cx: &mut Context<Self>) {
        let at = self.code.at();
        if !force && self.code.read_at == Some(at) {
            return;
        }
        let Some(root) = self.root.clone() else {
            return;
        };
        let plain = self.plain;
        self.code.read_at = Some(at);
        self.code.task = Some(cx.spawn(async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move {
                    let repo = (!plain).then(|| Repo::open(&root).ok()).flatten();
                    let files = match (&repo, at) {
                        (Some(repo), Some(rev)) => repo.files_of(rev, LIMIT),
                        (Some(repo), None) => repo.working_files(LIMIT),
                        (None, _) => files::on_disk(&root, LIMIT),
                    };
                    let readme = files::readme(&files, "").cloned().and_then(|path| {
                        let text = match (&repo, at) {
                            (Some(repo), Some(rev)) => repo.file_at(rev, &path),
                            _ => std::fs::read_to_string(root.join(&path)).ok(),
                        }?;
                        Some((path, text))
                    });
                    (files, readme)
                })
                .await;
            this.update(cx, |this, cx| {
                if this.code.read_at == Some(at) {
                    this.code.task = None;
                    (this.code.files, this.code.readme) = (Arc::new(read.0), read.1);
                    cx.notify();
                }
            })
            .ok();
        }));
    }

    /// Reads the README of folder `dir` at Code's revision, unless it is the one held.
    fn read_folder_readme(&mut self, dir: &str, cx: &mut Context<Self>) {
        if self
            .code
            .folder_readme
            .as_ref()
            .is_some_and(|(d, _)| d == dir)
        {
            return;
        }
        let Some(root) = self.root.clone() else {
            return;
        };
        let Some(path) = files::readme(&self.code.files, dir).cloned() else {
            self.code.folder_readme = Some((dir.to_owned(), None));
            return;
        };
        let (at, plain, dir) = (self.code.at(), self.plain, dir.to_owned());
        self.code.folder_readme = Some((dir.clone(), None));
        cx.spawn(async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move {
                    match (at, plain) {
                        (Some(rev), false) => Repo::open(&root).ok()?.file_at(rev, &path),
                        _ => std::fs::read_to_string(root.join(&path)).ok(),
                    }
                    .map(|text| (path, text))
                })
                .await;
            this.update(cx, |this, cx| {
                if let Some((d, readme)) = this.code.folder_readme.as_mut()
                    && *d == dir
                {
                    *readme = read;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// Tab `from` dragged onto tab `to`: it takes that place, a pinned one among the pinned.
    fn move_tab(&mut self, from: &str, to: &str, cx: &mut Context<Self>) {
        let code = &mut self.code;
        if code.pinned(from) != code.pinned(to) {
            return;
        }
        let tabs = &mut code.tabs;
        let (Some(a), Some(b)) = (
            tabs.iter().position(|t| t == from),
            tabs.iter().position(|t| t == to),
        ) else {
            return;
        };
        let moved = tabs.remove(a);
        tabs.insert(b, moved);
        if code.pinned(from) {
            code.pins = code.tabs[..code.pins.len()].to_vec();
            code.keep_pins();
        }
        cx.notify();
    }

    /// The preview tab `key` kept: a double click on it, as on a file in the tree.
    fn keep_tab(&mut self, key: &str, cx: &mut Context<Self>) {
        if self.code.preview.as_deref() == Some(key) {
            self.code.preview = None;
            self.code.tabs.push(key.to_owned());
            cx.notify();
        }
    }

    /// What the chip says Code reads: the working tree, or the branch or commit picked.
    fn revision_label(&self) -> String {
        match (&self.code.rev, self.plain) {
            (_, true) => "the files on disk".to_owned(),
            (None, false) => "working tree".to_owned(),
            (Some((name, _)), false) => name.clone(),
        }
    }

    /// The project's tree, `width` wide: its name and the revision chip, Go to file, the folders and files.
    fn render_tree(&self, width: f32, cx: &mut Context<Self>) -> Div {
        let project = self
            .root
            .as_deref()
            .map(|r| artoo_core::labels::project_name(r).to_uppercase())
            .unwrap_or_default();
        let rows = files::rows(&self.code.files, &self.code.open);
        let shown = match &self.shown {
            Some(Page::File(path) | Page::Folder(path)) => Some(path.clone()),
            _ => None,
        };
        let chip = self.revision_label();
        let list = uniform_list(
            "code-tree",
            rows.len(),
            cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                range
                    .filter_map(|ix| rows.get(ix).cloned())
                    .enumerate()
                    .map(|(n, row)| {
                        let path = row.path.clone();
                        let drawn = this.render_tree_row(row, shown.as_deref(), n, cx);
                        this.host.tree_row(&path, drawn, cx)
                    })
                    .collect::<Vec<_>>()
            }),
        )
        .track_scroll(&self.code.scroll)
        .flex_1()
        .pb(px(6.));
        // The first read of the project's files: the tree shimmers rather than reading as empty.
        let list = if self.code.files.is_empty() && self.code.task.is_some() {
            div()
                .flex_1()
                .px(px(14.))
                .py(px(8.))
                .child(artoo_ui_kit::motion::shimmer("code-tree-loading", 8))
                .into_any_element()
        } else {
            list.into_any_element()
        };
        island()
            .w(px(width))
            .flex_none()
            .child(
                div()
                    .flex_none()
                    .h(px(36.))
                    .px(px(12.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .text_size(px(11.))
                    .child(
                        artoo_ui_kit::hover::cut("code-project", project)
                            .flex_1()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme::muted()),
                    )
                    .child(
                        div()
                            .id("code-revision")
                            .flex_none()
                            .px(px(6.))
                            .rounded(px(4.))
                            .text_color(theme::faint())
                            .when(!self.plain, |s| {
                                s.cursor_pointer()
                                    .hover(|s| s.bg(theme::hover()).text_color(theme::text()))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, e: &MouseDownEvent, _, cx| {
                                            this.open_revision_menu(e.position, cx)
                                        }),
                                    )
                            })
                            .child(chip),
                    ),
            )
            .child(
                div().flex_none().px(px(8.)).pb(px(6.)).child(
                    div()
                        .id("code-go-to-file")
                        .h(px(28.))
                        .px(px(8.))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .rounded(px(artoo_ui_kit::ROW_RADIUS))
                        .bg(theme::base())
                        .text_color(theme::faint())
                        .cursor_pointer()
                        .child(
                            icons::icon("search")
                                .size(px(13.))
                                .text_color(theme::faint()),
                        )
                        .child(div().flex_1().child("Go to file"))
                        .child(keys::caps("⌘P", keys::On::Plain))
                        .on_click(
                            cx.listener(|this, _: &ClickEvent, _, cx| this.host.go_to_file(cx)),
                        ),
                ),
            )
            .child(list)
    }

    fn render_tree_row(
        &self,
        row: files::Row,
        shown: Option<&str>,
        _n: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use artoo_ui_kit::hover::Hover;
        let selected = shown == Some(row.path.as_str());
        let preview = self.code.preview.as_deref() == Some(row.path.as_str());
        let path = row.path.clone();
        // A folder whose note for the agent waits (#97).
        let noted = row.folder.is_some() && self.host.note_waits(&row.path, cx);
        artoo_ui_kit::row(SharedString::from(format!("tree-{}", row.path)), selected)
            // Its whole path, whatever the depth (#79).
            .tip(row.path.clone())
            .h(px(26.))
            .pl(px(8. + row.depth as f32 * 14.))
            .gap(px(6.))
            // A folder's chevron, then its icon; a file's icon under the folders' (#95). The
            // chevron alone folds; the row opens the folder's page (#97).
            .child(match row.folder {
                Some(open) => {
                    let fold = row.path.clone();
                    div()
                        .id(SharedString::from(format!("tree-fold-{}", row.path)))
                        .flex_none()
                        .child(
                            icons::icon(if open { "chevron" } else { "chevron-right" })
                                .size(px(12.))
                                .text_color(theme::faint()),
                        )
                        // A double click folds or opens it once, as one click does.
                        .on_click(cx.listener(move |this, e: &ClickEvent, _, cx| {
                            cx.stop_propagation();
                            if e.click_count() > 1 {
                                return;
                            }
                            if !this.code.open.remove(&fold) {
                                this.code.open.insert(fold.clone());
                                this.host.tree_opening(fold.clone(), cx);
                            }
                            cx.notify();
                        }))
                        .into_any_element()
                }
                None => div().w(px(12.)).flex_none().into_any_element(),
            })
            .child(match row.folder {
                Some(_) => icons::folder_icon(&row.path, 16.),
                None => icons::file_icon(&row.path, 16.),
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .cut_in_tip()
                    .when(preview, |s| s.italic())
                    .child(row.name.clone()),
            )
            .children(noted.then(|| {
                div()
                    .flex_none()
                    .size(px(7.))
                    .mr(px(4.))
                    .rounded_full()
                    .bg(theme::agent())
            }))
            // A right click: Reveal in Finder, Copy path, Open in; a folder's, a note for the agent.
            .on_mouse_down(
                gpui::MouseButton::Right,
                cx.listener({
                    let (path, folder) = (row.path.clone(), row.folder.is_some());
                    move |this, e: &gpui::MouseDownEvent, _, cx| {
                        this.tree_context(path.clone(), folder, e.position, cx)
                    }
                }),
            )
            .on_click(cx.listener(move |this, e: &ClickEvent, _, cx| {
                // A folder: one click its page, unfolded; a double click folds one that was
                // open already (the developer, 2026-10-23: everything that folds folds so).
                if row.folder.is_some() {
                    this.frame.close_drawer(cx);
                    if e.click_count() > 1 {
                        if this.code.unfolded_now.take().as_deref() != Some(path.as_str()) {
                            this.code.open.remove(&path);
                            cx.notify();
                        }
                        return;
                    }
                    this.code.unfolded_now =
                        (!this.code.open.contains(&path)).then(|| path.clone());
                    return this.open_folder(path.clone(), cx);
                }
                // A second click keeps it as a tab; picked in the drawer, it goes (#80).
                this.frame.close_drawer(cx);
                this.open_file(path.clone(), e.click_count() < 2, cx)
            }))
            .into_any_element()
    }

    /// A file's or a folder's menu in the tree: show it in Finder, copy its path, open it.
    fn tree_context(
        &mut self,
        path: String,
        folder: bool,
        at: gpui::Point<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) {
        let Some(root) = self.root.clone() else {
            return;
        };
        let full = root.join(&path);
        let mut groups = vec![vec![
            reveal(full.clone()),
            copy("Copy path", path.clone()),
            copy("Copy full path", full.display().to_string()),
        ]];
        groups.extend(
            self.host
                .open_in(&full, cx)
                .map(|(label, open)| vec![MenuItem::new(label, move |_, cx| open(cx))]),
        );
        if folder {
            let note = self.item("Note for your agent…", cx, move |c, cx| {
                c.open_folder(path.clone(), cx);
                c.host.note_folder(path.clone(), cx);
            });
            groups.insert(0, vec![note.key("C")]);
        }
        self.frame.menu(at, groups, cx);
    }

    /// The chip's menu: the working tree, HEAD, the local branches.
    fn open_revision_menu(&mut self, at: gpui::Point<gpui::Pixels>, cx: &mut Context<Self>) {
        let now = self.code.at();
        let all = self.host.branches(cx);
        let read_at = |label: String, rev: Option<(String, Oid)>| {
            self.item(label, cx, move |c, cx| c.set_revision(rev.clone(), cx))
        };
        let mut entries = vec![read_at("Working tree".to_owned(), None).checked(now.is_none())];
        if let Some(head) = all.iter().find(|b| b.is_head) {
            entries.push(
                read_at(
                    format!("HEAD · {}", head.name),
                    Some((head.name.clone(), head.tip)),
                )
                .checked(
                    now == Some(head.tip)
                        && self.code.rev.as_ref().is_some_and(|r| r.0 == head.name),
                ),
            );
        }
        let branches: Vec<MenuItem> = all
            .iter()
            .filter(|b| b.kind == artoo_core::git::RefKind::Local && !b.is_head)
            .map(|b| {
                read_at(b.name.clone(), Some((b.name.clone(), b.tip)))
                    .checked(self.code.rev.as_ref().is_some_and(|r| r.0 == b.name))
            })
            .collect();
        self.frame.menu(at, vec![entries, branches], cx);
    }

    /// Tab `key`'s menu: keep a preview, pin it, close it or others, copy its path, show it in Finder.
    fn tab_menu(
        &self,
        key: &str,
        preview: bool,
        pinned: bool,
        full: Option<PathBuf>,
        cx: &Context<Self>,
    ) -> Vec<Vec<MenuItem>> {
        let mut keep = Vec::new();
        if preview {
            let key = key.to_owned();
            keep.push(self.item("Keep it open", cx, move |c, cx| c.keep_tab(&key, cx)));
        }
        let pin = key.to_owned();
        let label = if pinned { "Unpin tab" } else { "Pin tab" };
        keep.push(
            self.item(label, cx, move |c, cx| c.pin(&pin, cx))
                .key("⌥⌘P"),
        );
        let close = if pinned {
            MenuItem::off("Close — pinned")
        } else {
            let key = key.to_owned();
            self.item("Close", cx, move |c, cx| c.close_tab(&key, cx))
                .key("⌘W")
        };
        let (other, after) = (key.to_owned(), key.to_owned());
        let mut place = vec![copy("Copy path", key.trim_end_matches('/').to_owned())];
        place.extend(full.map(reveal));
        vec![
            keep,
            vec![
                close,
                self.item("Close others", cx, move |c, cx| {
                    c.close_tabs(Some(other.clone()), cx)
                }),
                self.item("Close to the right", cx, move |c, cx| {
                    c.close_tabs_right(after.clone(), cx)
                }),
                self.item("Close all", cx, |c, cx| c.close_tabs(None, cx)),
            ],
            place,
        ]
    }

    /// The island every Code page is drawn in, its tabs on top (the developer, 2026-10-23, A):
    /// the one on screen open into the page under it, so a folder's page, a file and the project's
    /// own page share one shape, and a folder no longer hides the files' tabs.
    fn island(&self, shown: Option<&Page>, cx: &mut Context<Self>) -> Div {
        island().children(self.render_tabs(shown, cx))
    }

    /// The tabs: files and folders kept or previewed, scrolled sideways to the one on screen; ▾ at
    /// the end lists them all. A click shows one, a double click keeps a preview, its right click
    /// closes it or the others.
    fn render_tabs(&self, shown: Option<&Page>, cx: &mut Context<Self>) -> Option<Div> {
        let tabs = self.code.shown_tabs();
        if tabs.is_empty() {
            return None;
        }
        let shown = shown.map(tab_of);
        let rule = theme::island_border();
        let root = self.root.clone();
        let last_pin = self.code.pins.last().cloned();
        // GPUI clips to a rectangle, not to the island's round corners: the strip rounds its own
        // top inside the island's border, the tabs start a corner's width in so the one on screen
        // never fills a corner, and ▾ rounds the other (the developer, 2026-10-23).
        let corner = if artoo_ui_kit::flat() {
            0.
        } else {
            (artoo_ui_kit::ISLAND_RADIUS - 1.).max(0.)
        };
        let strip = div()
            .id("code-tabs")
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .overflow_x_scroll()
            .track_scroll(&self.code.tab_scroll)
            .children(tabs.iter().cloned().flat_map(|(key, preview)| {
                let on = shown.as_ref() == Some(&key);
                let pinned = self.code.pinned(&key);
                // A line after the last pinned tab parts them from the rest (#20, B).
                let parted = last_pin.as_ref() == Some(&key) && tabs.len() > self.code.pins.len();
                let path = key.trim_end_matches('/').to_owned();
                let folder = key.ends_with('/');
                let name = path.rsplit('/').next().unwrap_or(&path).to_owned();
                let (open, close, menu) = (key.clone(), key.clone(), key.clone());
                let (drag, drop) = (key.clone(), key.clone());
                let full = root.as_ref().map(|r| r.join(&path));
                let label = name.clone();
                let whole = key.clone();
                let tab = div()
                    .id(SharedString::from(format!("code-tab-{key}")))
                    .flex_none()
                    .h_full()
                    .pl(px(12.))
                    .pr(px(6.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .border_r_1()
                    .border_color(rule)
                    .text_size(px(12.))
                    .cursor_pointer()
                    // The one on screen opens into the page; the others sit on the strip's line.
                    .when(on, |s| s.bg(theme::editor()).text_color(theme::text()))
                    .when(!on, |s| {
                        s.border_b_1()
                            .text_color(theme::muted())
                            .hover(|s| s.text_color(theme::text()))
                    })
                    // Its whole path in a long hover.
                    .tooltip_show_delay(artoo_ui_kit::hover::LONG)
                    .tooltip(move |_, cx| cx.new(|_| TipOwned(whole.clone().into())).into())
                    // A kept tab dragged onto another takes its place.
                    .drag_over::<DragCodeTab>(|s, _, _, _| s.bg(theme::selected()))
                    .when(!preview, |s| {
                        s.on_drag(DragCodeTab(drag), move |_, _, _, cx| {
                            cx.new(|_| artoo_ui_kit::controls::DragLabel(label.clone()))
                        })
                    })
                    .on_drop(cx.listener(move |this, dragged: &DragCodeTab, _, cx| {
                        this.move_tab(&dragged.0, &drop, cx)
                    }))
                    .child(if folder {
                        icons::folder_icon(&path, 14.)
                    } else {
                        icons::file_icon(&path, 14.)
                    })
                    .child(div().when(preview, |s| s.italic()).child(name))
                    // A pinned tab's pin where × would be: a click unpins it.
                    .child(
                        div()
                            .id(SharedString::from(format!("code-tab-x-{key}")))
                            .size(px(18.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(4.))
                            .hover(|s| s.bg(theme::hover()))
                            .child(
                                icons::icon(if pinned { "pin" } else { "x" })
                                    .size(px(12.))
                                    .text_color(theme::faint()),
                            )
                            .when(pinned, |s| {
                                s.tooltip(|_, cx| cx.new(|_| Tip("Unpin")).into())
                            })
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                cx.stop_propagation();
                                if pinned {
                                    this.pin(&close, cx)
                                } else {
                                    this.close_tab(&close, cx)
                                }
                            })),
                    )
                    .on_click(cx.listener(move |this, e: &ClickEvent, _, cx| {
                        if e.click_count() > 1 {
                            this.keep_tab(&open, cx)
                        } else {
                            this.open_tab(&open, cx)
                        }
                    }))
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, e: &gpui::MouseDownEvent, _, cx| {
                            let groups = this.tab_menu(&menu, preview, pinned, full.clone(), cx);
                            this.frame.menu(e.position, groups, cx)
                        }),
                    );
                let line = parted.then(|| {
                    div()
                        .flex_none()
                        .w(px(2.))
                        .h_full()
                        .bg(theme::border())
                        .into_any_element()
                });
                std::iter::once(tab.into_any_element()).chain(line)
            }))
            // The rest of the strip, on its line.
            .child(
                div()
                    .flex_1()
                    .min_w(px(12.))
                    .h_full()
                    .border_b_1()
                    .border_color(rule),
            );
        let all = div()
            .id("code-tabs-all")
            .flex_none()
            .h_full()
            .px(px(8.))
            .flex()
            .items_center()
            .rounded_tr(px(corner))
            .border_b_1()
            .border_l_1()
            .border_color(rule)
            .cursor_pointer()
            .hover(|s| s.bg(theme::hover()))
            .child(
                icons::icon("chevron")
                    .size(px(14.))
                    .text_color(theme::muted()),
            )
            .tooltip_show_delay(artoo_ui_kit::hover::LONG)
            .tooltip(|_, cx| cx.new(|_| Tip("Every open tab")).into())
            .on_click(cx.listener(move |this, e: &ClickEvent, _, cx| {
                let on = this.shown_tab();
                let listed = this
                    .code
                    .shown_tabs()
                    .into_iter()
                    .map(|(key, _)| {
                        let open = key.clone();
                        this.item(key.clone(), cx, move |c, cx| c.open_tab(&open, cx))
                            .checked(on.as_ref() == Some(&key))
                    })
                    .collect();
                let others = this.item("Close others", cx, move |c, cx| {
                    c.close_tabs(on.clone(), cx)
                });
                let all = this.item("Close all", cx, |c, cx| c.close_tabs(None, cx));
                this.frame
                    .menu(e.position(), vec![listed, vec![others, all]], cx)
            }));
        Some(
            div()
                .flex_none()
                .w_full()
                .h(px(36.))
                .flex()
                .rounded_t(px(corner))
                .bg(theme::panel())
                .child(
                    div()
                        .flex_none()
                        .w(px(corner))
                        .h_full()
                        .border_b_1()
                        .border_color(rule),
                )
                .child(strip)
                .child(all),
        )
    }

    /// A folder's page (#97, Design: notes on folders, B): its notes for your agent and the
    /// composer, its folders and files, its README.
    fn render_dir_page(
        &self,
        path: &str,
        shown: Option<&Page>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let prefix = format!("{path}/");
        let mut folders = std::collections::BTreeSet::new();
        let mut here = Vec::new();
        for f in self
            .code
            .files
            .iter()
            .filter_map(|f| f.strip_prefix(&prefix))
        {
            match f.split_once('/') {
                Some((dir, _)) => {
                    folders.insert(dir.to_owned());
                }
                None => here.push(f.to_owned()),
            }
        }
        let (count, notes) = self.host.folder_notes(path, cx);
        let none = notes.is_empty();
        let heading = |text: String| {
            div()
                .pt(px(14.))
                .pb(px(6.))
                .text_size(px(11.))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(theme::muted())
                .child(text.to_uppercase())
        };
        let entry = |id: SharedString, name: String, folder: bool, to: String| {
            artoo_ui_kit::row(id, false)
                .h(px(28.))
                .gap_2()
                .child(if folder {
                    icons::folder_icon(&to, 16.)
                } else {
                    icons::file_icon(&to, 16.)
                })
                .child(artoo_ui_kit::hover::cut("name", name).flex_1())
                .on_click(cx.listener(move |this, e: &ClickEvent, _, cx| {
                    if folder {
                        this.open_folder(to.clone(), cx)
                    } else {
                        this.open_file(to.clone(), e.click_count() < 2, cx)
                    }
                }))
        };
        let rows: Vec<AnyElement> = folders
            .iter()
            .map(|d| {
                entry(
                    SharedString::from(format!("dir-entry-{path}/{d}")),
                    d.clone(),
                    true,
                    format!("{path}/{d}"),
                )
                .into_any_element()
            })
            .chain(here.iter().map(|f| {
                entry(
                    SharedString::from(format!("dir-entry-{path}/{f}")),
                    f.clone(),
                    false,
                    format!("{path}/{f}"),
                )
                .into_any_element()
            }))
            .collect();
        let readme = self
            .code
            .folder_readme
            .as_ref()
            .filter(|(d, _)| d == path)
            .and_then(|(_, r)| r.clone())
            .map(|(file, text)| {
                let md = file.ends_with(".md") || file.ends_with(".markdown");
                let selecting = self.frame.selecting(cx);
                div()
                    .child(heading(file.rsplit('/').next().unwrap_or(&file).to_owned()))
                    .child(markdown::body(
                        &text,
                        md,
                        format!("folder-readme-{path}"),
                        None,
                        selecting,
                        cx,
                    ))
            });
        self.island(shown, cx)
            .flex_1()
            .min_w_0()
            .bg(theme::editor())
            .child(
                div()
                    .flex_none()
                    .h(px(36.))
                    .px(px(14.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .border_b_1()
                    .border_color(theme::island_border())
                    .text_size(px(12.))
                    // Its path and count are the bar's, its name the tab's (A, 2026-10-23):
                    // what it is read at, as a file's line says.
                    .child(
                        div()
                            .text_color(theme::faint())
                            .child(format!("as of {}", self.revision_label())),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_color(theme::faint())
                            .child("c writes a note for your agent"),
                    ),
            )
            .child(
                div()
                    .id(SharedString::from(format!("dir-page-{path}")))
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px(px(24.))
                    .pb(px(18.))
                    .text_size(px(13.))
                    .child(
                        div()
                            .max_w(px(820.))
                            .child(heading(match count {
                                0 => "For your agent".to_owned(),
                                n => format!("For your agent · {n}"),
                            }))
                            .children(notes)
                            .children(none.then(|| {
                                let folder = path.to_owned();
                                div()
                                    .id("dir-note-start")
                                    .h(px(30.))
                                    .px(px(10.))
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .rounded(px(artoo_ui_kit::ROW_RADIUS))
                                    .border_1()
                                    .border_color(theme::island_border())
                                    .text_size(px(12.))
                                    .text_color(theme::muted())
                                    .cursor_pointer()
                                    .hover(|s| s.bg(theme::hover()).text_color(theme::text()))
                                    .child(
                                        icons::icon("bot").size(px(14.)).text_color(theme::agent()),
                                    )
                                    .child(
                                        "A note for your agent on this folder and everything in it",
                                    )
                                    .child(div().flex_1())
                                    .child(keys::caps("C", keys::On::Plain))
                                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                        this.host.note_folder(folder.clone(), cx)
                                    }))
                            }))
                            .child(heading(format!("In it · {}", folders.len() + here.len())))
                            .children(rows)
                            .children(readme),
                    ),
            )
            .into_any_element()
    }

    /// The folder page at Code's root: its README rendered, else Go to a file and the files
    /// opened lately.
    fn render_folder_page(&self, shown: Option<&Page>, cx: &mut Context<Self>) -> AnyElement {
        let Some((path, text)) = &self.code.readme else {
            return self.render_root(shown, cx).into_any_element();
        };
        let selecting = self.frame.selecting(cx);
        let body = if path.ends_with(".md") || path.ends_with(".markdown") {
            markdown::body(text, true, "folder-readme", None, selecting, cx)
        } else {
            div()
                .font_family(theme::code_font())
                .whitespace_normal()
                .child(markdown::body(
                    text,
                    false,
                    "folder-readme",
                    None,
                    selecting,
                    cx,
                ))
                .into_any_element()
        };
        self.island(shown, cx)
            .flex_1()
            .min_w_0()
            .bg(theme::editor())
            .child(
                div()
                    .flex_none()
                    .h(px(36.))
                    .px(px(14.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .border_b_1()
                    .border_color(theme::island_border())
                    .text_size(px(12.))
                    .child(icons::file_icon(path, 14.))
                    .child(
                        div()
                            .font_family(theme::code_font())
                            .text_color(theme::muted())
                            .child(path.clone()),
                    )
                    .child(div().flex_1())
                    .child(div().text_color(theme::faint()).child("rendered")),
            )
            .child(
                div()
                    .id("folder-page")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px(px(24.))
                    .py(px(18.))
                    .text_size(px(13.))
                    .child(body),
            )
            .into_any_element()
    }

    /// Code's folder page when it has no README: Go to a file, and the files opened lately.
    fn render_root(
        &self,
        shown: Option<&Page>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let recent = self.host.recent_files(cx);
        self.island(shown, cx)
            .flex_1()
            .items_center()
            .bg(theme::editor())
            .child(
                div()
                    .w(px(440.))
                    .mt(px(72.))
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .id("code-go-to-file")
                            .h(px(30.))
                            .px(px(10.))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .rounded(px(artoo_ui_kit::ROW_RADIUS))
                            .border_1()
                            .border_color(theme::island_border())
                            .bg(theme::panel())
                            .text_color(theme::faint())
                            .cursor_pointer()
                            .hover(|s| s.border_color(theme::border()))
                            .child(
                                icons::icon("search")
                                    .size(px(14.))
                                    .text_color(theme::faint()),
                            )
                            .child(div().flex_1().child("Go to a file"))
                            .child(keys::caps("⌘P", keys::On::Plain))
                            .on_click(
                                cx.listener(|this, _: &ClickEvent, _, cx| this.host.go_to_file(cx)),
                            ),
                    )
                    .children((!recent.is_empty()).then(|| {
                        div()
                            .mt_2()
                            .text_size(px(11.))
                            .text_color(theme::faint())
                            .child("Opened lately")
                    }))
                    .children(recent.into_iter().enumerate().map(|(ix, path)| {
                        let open = path.clone();
                        artoo_ui_kit::row(("code-recent", ix), false)
                            .h(px(28.))
                            .gap_2()
                            .child(icons::file_icon(&path, 16.))
                            .child(artoo_ui_kit::hover::cut("path", path).flex_1())
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.frame
                                    .open(FileRoute { path: open.clone() }.route(), cx)
                            }))
                    })),
            )
    }
}

impl Render for CodeSpace {
    /// The page on screen: a file, a folder's page, or the project's.
    fn render(&mut self, _: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        let shown = self.shown.clone();
        match &shown {
            Some(Page::File(path)) => {
                let file = self.host.file(path, cx);
                let island = self
                    .island(shown.as_ref(), cx)
                    .flex_1()
                    .min_w_0()
                    .bg(theme::editor());
                match file {
                    // A file being read that is not on screen yet: a shimmer of lines, not the one before.
                    FilePage::Reading => island
                        .p(px(24.))
                        .child(artoo_ui_kit::motion::shimmer("file-loading", 9))
                        .into_any_element(),
                    FilePage::Read(head, body) => island.child(head).child(body).into_any_element(),
                    FilePage::Empty => island.into_any_element(),
                }
            }
            Some(Page::Folder(path)) => self.render_dir_page(path, shown.as_ref(), cx),
            _ => self.render_folder_page(shown.as_ref(), cx),
        }
    }
}

/// An item copying `text`.
fn copy(label: &str, text: String) -> MenuItem {
    MenuItem::new(label, move |_, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string(text.clone()))
    })
}

/// An item showing `path` in Finder.
fn reveal(path: PathBuf) -> MenuItem {
    MenuItem::new(artoo_ui_kit::controls::REVEAL, move |_, cx| {
        cx.reveal_path(&path)
    })
}

/// The tab of page `page`: a file's path, a folder's with its slash.
fn tab_of(page: &Page) -> String {
    match page {
        Page::File(path) => path.clone(),
        Page::Folder(path) => folder_tab(path),
    }
}

/// Code as the tab's features ask it (`artoo_code_api::Code`).
pub struct Handle(pub WeakEntity<CodeSpace>);

impl Handle {
    fn read<R: Default>(&self, cx: &App, read: impl FnOnce(&CodeSpace) -> R) -> R {
        self.0
            .upgrade()
            .map(|c| read(c.read(cx)))
            .unwrap_or_default()
    }

    fn update<R: Default>(
        &self,
        cx: &mut App,
        change: impl FnOnce(&mut CodeSpace, &mut Context<CodeSpace>) -> R,
    ) -> R {
        self.0.update(cx, change).unwrap_or_default()
    }
}

impl artoo_code_api::Code for Handle {
    fn files(&self, cx: &App) -> Arc<Vec<String>> {
        self.read(cx, |c| c.code.files.clone())
    }

    fn revision(&self, cx: &App) -> Option<(String, Oid)> {
        self.read(cx, |c| c.code.rev.clone())
    }

    fn follow(&self, rev: Option<(String, Oid)>, cx: &mut App) {
        self.update(cx, |c, _| c.code.rev = rev);
    }

    fn read_files(&self, force: bool, cx: &mut App) {
        self.update(cx, |c, cx| c.read_files(force, cx));
    }

    fn set_project(&self, root: PathBuf, plain: bool, fresh: bool, key: &str, cx: &mut App) {
        self.update(cx, |c, _| c.set_project(root, plain, fresh, key));
    }

    fn place_tab(&self, path: &str, preview: bool, cx: &mut App) {
        self.update(cx, |c, _| c.keep_or_preview(path, preview));
    }

    fn reveal_folder(&self, path: &str, cx: &mut App) -> bool {
        self.update(cx, |c, _| c.reveal_folder(path))
    }

    fn pin_shown(&self, cx: &mut App) -> bool {
        self.update(cx, |c, cx| c.pin_shown(cx))
    }

    fn shown_pinned(&self, cx: &App) -> bool {
        self.read(cx, |c| c.shown_pinned())
    }

    fn close_shown(&self, cx: &mut App) {
        self.update(cx, |c, cx| c.close_shown(cx));
    }

    fn step_tab(&self, forward: bool, cx: &mut App) {
        self.update(cx, |c, cx| c.step_tab(forward, cx));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pinned_tab_goes_first_and_is_kept_by_project() {
        let mut code = Code {
            tabs: vec!["a.rs".into(), "b.rs".into(), "src/".into()],
            preview: Some("c.rs".into()),
            ..Default::default()
        };
        code.toggle_pin("src/");
        // A preview pinned is kept.
        code.toggle_pin("c.rs");
        assert_eq!(code.tabs, ["src/", "c.rs", "a.rs", "b.rs"]);
        assert_eq!(code.pins, ["src/", "c.rs"]);
        assert_eq!(code.preview, None);
        // Unpinned, it is the first after the pinned.
        code.toggle_pin("src/");
        assert_eq!(code.tabs, ["c.rs", "src/", "a.rs", "b.rs"]);
        assert_eq!(code.pins, ["c.rs"]);
        // Each project its own line.
        let text = with_pins("other\tx.rs\n", "me", &code.pins);
        assert_eq!(kept_pins(&text, "me"), ["c.rs"]);
        assert_eq!(kept_pins(&text, "other"), ["x.rs"]);
        assert!(kept_pins(&with_pins(&text, "me", &[]), "me").is_empty());
    }

    #[test]
    fn a_folder_is_a_tab_beside_the_files() {
        assert_eq!(tab_page(&folder_tab("relay")), Page::Folder("relay".into()));
        assert_eq!(
            tab_page("relay/wrangler.toml"),
            Page::File("relay/wrangler.toml".into())
        );
        let mut code = Code::default();
        code.tabs.push("a.rs".into());
        // A folder opened takes the preview tab, the files stay.
        code.preview_folder("relay");
        assert_eq!(
            code.shown_tabs(),
            [("a.rs".into(), false), ("relay/".into(), true)]
        );
        // One kept is not previewed again.
        code.tabs.push("src/".into());
        code.preview = None;
        code.preview_folder("src");
        assert_eq!(code.preview, None);
    }
}
