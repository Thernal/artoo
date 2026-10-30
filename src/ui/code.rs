//! The Code space (#75): the project's tree beside its page — the folder page with its README
//! rendered, or files as the editor's tabs — read at a revision: the working tree by default,
//! HEAD or a branch picked on the tree's chip. A plain folder, not a git repository, has only
//! this space, its files as they are on disk.

use super::hover::CutInTip;
use super::menu::{Act, Entry};
use super::nav::Page;
use super::spaces::Space;
use super::{Workspace, icons, island, keys, markdown, px, theme};
use crate::files;
use crate::git::{BranchRef, Repo};
use git2::Oid;
use gpui::{
    AnyElement, App, ClickEvent, Context, Div, MouseButton, MouseDownEvent, SharedString, Task,
    WeakEntity, div, prelude::*, uniform_list,
};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// The most files Code's tree lists.
const LIMIT: usize = 20_000;

#[derive(Default)]
pub struct Code {
    /// The revision read, by name: None the working tree, the files on disk.
    pub rev: Option<(String, Oid)>,
    /// The project's files at it, sorted.
    files: Vec<String>,
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
    /// The project's files as the tree lists them.
    pub fn files(&self) -> &[String] {
        &self.files
    }

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
        self.pins = kept_pins(&crate::storage::read(PINS_FILE), repo);
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
            let text = crate::storage::read(PINS_FILE);
            crate::storage::save(PINS_FILE, with_pins(&text, repo, &self.pins));
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

/// The Code space of one tab (#75): what it reads and holds (`Code`), the project it reads, its tree and
/// its pages, drawn as its own view. What it asks of the tab goes through `host`.
pub struct CodeSpace {
    pub code: Code,
    /// The project read, and whether it is a plain folder rather than a git repository.
    pub root: Option<PathBuf>,
    pub plain: bool,
    host: Rc<dyn Host>,
}

impl CodeSpace {
    pub fn new(host: Rc<dyn Host>) -> Self {
        Self {
            code: Code::default(),
            root: None,
            plain: false,
            host,
        }
    }

    /// Tab `key` pinned or unpinned, the strip scrolled to the tab on screen.
    fn pin(&mut self, key: &str, cx: &mut Context<Self>) {
        self.code.toggle_pin(key);
        let shown = self.host.shown(cx).as_ref().and_then(tab_of);
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
                    (this.code.files, this.code.readme) = read;
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

    /// The project's tree, `width` wide, `shown` the page on screen: its name and the revision chip, Go to file, the folders and files.
    pub(super) fn render_tree(
        &self,
        width: f32,
        shown: Option<Page>,
        cx: &mut Context<Self>,
    ) -> Div {
        let project = self
            .root
            .as_deref()
            .map(|r| crate::labels::project_name(r).to_uppercase())
            .unwrap_or_default();
        let rows = files::rows(&self.code.files, &self.code.open);
        let shown = match shown {
            Some(Page::File(path) | Page::Folder(path)) => Some(path),
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
                .child(super::motion::shimmer("code-tree-loading", 8))
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
                        super::hover::cut("code-project", project)
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
                        .rounded(px(super::ROW_RADIUS))
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
        use super::hover::Hover;
        let selected = shown == Some(row.path.as_str());
        let preview = self.code.preview.as_deref() == Some(row.path.as_str());
        let path = row.path.clone();
        // A folder whose note for the agent waits (#97).
        let noted = row.folder.is_some() && self.host.note_waits(&row.path, cx);
        super::row(SharedString::from(format!("tree-{}", row.path)), selected)
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
                    this.host.close_drawer(cx);
                    if e.click_count() > 1 {
                        if this.code.unfolded_now.take().as_deref() != Some(path.as_str()) {
                            this.code.open.remove(&path);
                            cx.notify();
                        }
                        return;
                    }
                    this.code.unfolded_now =
                        (!this.code.open.contains(&path)).then(|| path.clone());
                    return this.host.open_folder(path.clone(), cx);
                }
                // A second click keeps it as a tab; picked in the drawer, it goes (#80).
                this.host.close_drawer(cx);
                this.host.open_file(path.clone(), e.click_count() < 2, cx)
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
            Entry::new(super::open_in::REVEAL, Act::Reveal(full.clone())),
            Entry::new("Copy path", Act::Copy(path.clone())),
            Entry::new("Copy full path", Act::Copy(full.display().to_string())),
        ]];
        if let Some(app) = self.host.open_app(&full, cx) {
            groups.push(vec![Entry::new(
                format!("Open in {}", app.name()),
                Act::OpenPathWith(full, app),
            )]);
        }
        if folder {
            groups.insert(
                0,
                vec![Entry::new("Note for your agent…", Act::FolderNote(path)).key("C")],
            );
        }
        self.host.menu(at, groups, cx);
    }

    /// The chip's menu: the working tree, HEAD, the local branches.
    fn open_revision_menu(&mut self, at: gpui::Point<gpui::Pixels>, cx: &mut Context<Self>) {
        let now = self.code.at();
        let all = self.host.branches(cx);
        let mut entries =
            vec![Entry::new("Working tree", Act::CodeRevision(None)).checked(now.is_none())];
        if let Some(head) = all.iter().find(|b| b.is_head) {
            entries.push(
                Entry::new(
                    format!("HEAD · {}", head.name),
                    Act::CodeRevision(Some((head.name.clone(), head.tip))),
                )
                .checked(
                    now == Some(head.tip)
                        && self.code.rev.as_ref().is_some_and(|r| r.0 == head.name),
                ),
            );
        }
        let branches: Vec<Entry> = all
            .iter()
            .filter(|b| b.kind == crate::git::RefKind::Local && !b.is_head)
            .map(|b| {
                Entry::new(
                    b.name.clone(),
                    Act::CodeRevision(Some((b.name.clone(), b.tip))),
                )
                .checked(self.code.rev.as_ref().is_some_and(|r| r.0 == b.name))
            })
            .collect();
        self.host.menu(at, vec![entries, branches], cx);
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
        let shown = shown.and_then(tab_of);
        let rule = theme::island_border();
        let root = self.root.clone();
        let last_pin = self.code.pins.last().cloned();
        // GPUI clips to a rectangle, not to the island's round corners: the strip rounds its own
        // top inside the island's border, the tabs start a corner's width in so the one on screen
        // never fills a corner, and ▾ rounds the other (the developer, 2026-10-23).
        let corner = if super::flat() {
            0.
        } else {
            (super::ISLAND_RADIUS - 1.).max(0.)
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
                    .tooltip_show_delay(super::hover::LONG)
                    .tooltip(move |_, cx| cx.new(|_| super::TipOwned(whole.clone().into())).into())
                    // A kept tab dragged onto another takes its place.
                    .drag_over::<DragCodeTab>(|s, _, _, _| s.bg(theme::selected()))
                    .when(!preview, |s| {
                        s.on_drag(DragCodeTab(drag), move |_, _, _, cx| {
                            cx.new(|_| super::spaces::DragLabel(label.clone()))
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
                                s.tooltip(|_, cx| cx.new(|_| super::Tip("Unpin")).into())
                            })
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                cx.stop_propagation();
                                if pinned {
                                    this.pin(&close, cx)
                                } else {
                                    this.host.close_tab(close.clone(), cx)
                                }
                            })),
                    )
                    .on_click(cx.listener(move |this, e: &ClickEvent, _, cx| {
                        if e.click_count() > 1 {
                            this.keep_tab(&open, cx)
                        } else {
                            this.host.open_tab(open.clone(), cx)
                        }
                    }))
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, e: &gpui::MouseDownEvent, _, cx| {
                            let mut keep = Vec::new();
                            if preview {
                                keep.push(Entry::new(
                                    "Keep it open",
                                    Act::KeepCodeTab(menu.clone()),
                                ));
                            }
                            keep.push(
                                Entry::new(
                                    if pinned { "Unpin tab" } else { "Pin tab" },
                                    Act::PinCodeTab(menu.clone()),
                                )
                                .key("⌥⌘P"),
                            );
                            let close = if pinned {
                                Entry::off("Close — pinned")
                            } else {
                                Entry::new("Close", Act::CloseCodeTab(menu.clone())).key("⌘W")
                            };
                            let mut groups = vec![keep];
                            groups.push(vec![
                                close,
                                Entry::new("Close others", Act::CloseCodeTabs(Some(menu.clone()))),
                                Entry::new(
                                    "Close to the right",
                                    Act::CloseCodeTabsRight(menu.clone()),
                                ),
                                Entry::new("Close all", Act::CloseCodeTabs(None)),
                            ]);
                            let mut place = vec![Entry::new(
                                "Copy path",
                                Act::Copy(menu.trim_end_matches('/').to_owned()),
                            )];
                            if let Some(full) = &full {
                                place.push(Entry::new(
                                    super::open_in::REVEAL,
                                    Act::Reveal(full.clone()),
                                ));
                            }
                            groups.push(place);
                            this.host.menu(e.position, groups, cx)
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
            .tooltip_show_delay(super::hover::LONG)
            .tooltip(|_, cx| cx.new(|_| super::Tip("Every open tab")).into())
            .on_click(cx.listener(move |this, e: &ClickEvent, _, cx| {
                let on = this.host.shown(cx).as_ref().and_then(tab_of);
                let listed = this
                    .code
                    .shown_tabs()
                    .into_iter()
                    .map(|(key, _)| {
                        Entry::new(key.clone(), Act::CodeTab(key.clone()))
                            .checked(on.as_ref() == Some(&key))
                    })
                    .collect();
                this.host.menu(
                    e.position(),
                    vec![
                        listed,
                        vec![
                            Entry::new("Close others", Act::CloseCodeTabs(on)),
                            Entry::new("Close all", Act::CloseCodeTabs(None)),
                        ],
                    ],
                    cx,
                )
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
            super::row(id, false)
                .h(px(28.))
                .gap_2()
                .child(if folder {
                    icons::folder_icon(&to, 16.)
                } else {
                    icons::file_icon(&to, 16.)
                })
                .child(super::hover::cut("name", name).flex_1())
                .on_click(cx.listener(move |this, e: &ClickEvent, _, cx| {
                    if folder {
                        this.host.open_folder(to.clone(), cx)
                    } else {
                        this.host.open_file(to.clone(), e.click_count() < 2, cx)
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
                let selecting = self.host.selecting(cx);
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
                                    .rounded(px(super::ROW_RADIUS))
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
        let selecting = self.host.selecting(cx);
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
                            .rounded(px(super::ROW_RADIUS))
                            .border_1()
                            .border_color(theme::island_border())
                            .bg(theme::panel())
                            .text_color(theme::faint())
                            .cursor_pointer()
                            .hover(|s| s.border_color(theme::border()))
                            .child(
                                super::icons::icon("search")
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
                        super::row(("code-recent", ix), false)
                            .h(px(28.))
                            .gap_2()
                            .child(super::icons::file_icon(&path, 16.))
                            .child(super::hover::cut("path", path).flex_1())
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.host.show_file(open.clone(), cx)
                            }))
                    })),
            )
    }
}

impl Render for CodeSpace {
    /// The page on screen: a file, a folder's page, or the project's.
    fn render(&mut self, _: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        let shown = self.host.shown(cx);
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
                        .child(super::motion::shimmer("file-loading", 9))
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

/// The file Code shows, as the tab has it: being read, read (its head line and what is under it), or none.
pub enum FilePage {
    Reading,
    Read(AnyElement, AnyElement),
    Empty,
}

/// What Code asks of its tab (`CodeHost`). What it reads is read while Code draws or answers a click, never
/// while the tab is updating it; what it changes is changed once the update under way is over.
pub trait Host {
    /// The page of Code's on screen, a file or a folder.
    fn shown(&self, cx: &App) -> Option<Page>;
    /// File `path` as the tab reads it.
    fn file(&self, path: &str, cx: &mut App) -> FilePage;
    /// The notes for the agent on folder `folder` (how many) and what is drawn of them, the composer with them.
    fn folder_notes(&self, folder: &str, cx: &App) -> (usize, Vec<AnyElement>);
    /// Whether folder `folder` has a note for the agent waiting (#97).
    fn note_waits(&self, folder: &str, cx: &App) -> bool;
    /// The files opened lately, the latest first.
    fn recent_files(&self, cx: &App) -> Vec<String>;
    /// The tab's text selection, for the text Code draws; `None` once the tab is gone.
    fn selecting(&self, cx: &App) -> Option<markdown::Selecting>;
    /// Row `row` of the tree for `path`, fading in under a folder just opened (#110).
    fn tree_row(&self, path: &str, row: AnyElement, cx: &App) -> AnyElement;
    /// The repository's branches, for the revision's menu.
    fn branches(&self, cx: &App) -> Vec<BranchRef>;
    /// The app `path` opens in.
    fn open_app(&self, path: &Path, cx: &App) -> Option<super::open_in::App>;
    /// File `path` opened in a tab: the preview one (`preview`) or kept.
    fn open_file(&self, path: String, preview: bool, cx: &mut App);
    /// File `path` shown as Code shows a file opened from elsewhere.
    fn show_file(&self, path: String, cx: &mut App);
    /// Folder `path`'s page opened.
    fn open_folder(&self, path: String, cx: &mut App);
    /// Tab `key` shown.
    fn open_tab(&self, key: String, cx: &mut App);
    /// Tab `key` closed.
    fn close_tab(&self, key: String, cx: &mut App);
    /// A note for the agent on folder `folder` started.
    fn note_folder(&self, folder: String, cx: &mut App);
    /// Menu `groups` opened at `at`.
    fn menu(&self, at: gpui::Point<gpui::Pixels>, groups: Vec<Vec<Entry>>, cx: &mut App);
    /// Go to file (⌘P).
    fn go_to_file(&self, cx: &mut App);
    /// The tree's drawer closed, when narrow (#80).
    fn close_drawer(&self, cx: &mut App);
    /// Folder `folder` of the tree opened: its rows fade in.
    fn tree_opening(&self, folder: String, cx: &mut App);
}

/// The tab of page `page`: a file's path, a folder's with its slash.
fn tab_of(page: &Page) -> Option<String> {
    match page {
        Page::File(path) => Some(path.clone()),
        Page::Folder(path) => Some(folder_tab(path)),
        _ => None,
    }
}

impl Workspace {
    /// Reads the project's files at Code's revision, once per revision (and again with `force`).
    pub(super) fn read_code_files(&mut self, force: bool, cx: &mut Context<Self>) {
        self.code.update(cx, |c, cx| c.read_files(force, cx));
    }

    /// Reads Code at `rev` (None: the working tree); the file open is read again there.
    fn read_code_at(&mut self, rev: Option<(String, Oid)>, cx: &mut Context<Self>) {
        self.code.update(cx, |c, _| c.code.rev = rev);
        self.read_code_files(false, cx);
        match self.nav_page().cloned() {
            Some(Page::File(path)) => self.open_file_view(path, cx),
            Some(Page::Folder(path)) => {
                self.code.update(cx, |c, _| c.code.folder_readme = None);
                self.read_folder_readme(&path, cx);
            }
            _ => {}
        }
        cx.notify();
    }

    /// Folder `path` opens as Code's page (#97), unfolded in the tree, in the preview tab.
    pub(super) fn open_folder_page(&mut self, path: String, cx: &mut Context<Self>) {
        let unfolded = self.code.update(cx, |c, _| {
            c.code.reveal(&path);
            c.code.preview_folder(&path);
            c.code.open.insert(path.clone())
        });
        if unfolded {
            self.tree_opening(&path, cx);
        }
        self.nav.space = Space::Code;
        self.arrive(Page::Folder(path.clone()), cx);
        self.read_folder_readme(&path, cx);
        cx.notify();
    }

    /// Reads the README of folder `dir` at Code's revision, unless it is the one held.
    pub(super) fn read_folder_readme(&mut self, dir: &str, cx: &mut Context<Self>) {
        self.code.update(cx, |c, cx| c.read_folder_readme(dir, cx));
    }

    /// File `path` opens in Code: as the preview tab (`preview`) the next click replaces, or kept.
    pub(super) fn open_code_file(&mut self, path: String, preview: bool, cx: &mut Context<Self>) {
        self.code.update(cx, |c, _| {
            let code = &mut c.code;
            if !code.tabs.contains(&path) {
                if preview {
                    code.preview = Some(path.clone());
                } else {
                    if code.preview.as_ref() == Some(&path) {
                        code.preview = None;
                    }
                    code.tabs.push(path.clone());
                }
            }
        });
        self.open_file_view(path, cx);
    }

    /// Tab `key` pinned — first among the tabs, through every close and a restart — or unpinned,
    /// first after the pinned ones (#20).
    pub(super) fn toggle_pin(&mut self, key: &str, cx: &mut Context<Self>) {
        let shown = self.shown_tab();
        self.code.update(cx, |c, _| {
            c.code.toggle_pin(key);
            c.scroll_to_tab(shown.as_deref());
        });
        cx.notify();
    }

    /// ⌥⌘P and ⌘K's Pin tab: the tab on screen in Code pinned or unpinned; false when there is
    /// none.
    pub(super) fn pin_shown_tab(&mut self, cx: &mut Context<Self>) -> bool {
        let tabs = self.code.read(cx).code.shown_tabs();
        let Some(key) = self
            .shown_tab()
            .filter(|k| tabs.iter().any(|(t, _)| t == k))
        else {
            return false;
        };
        self.toggle_pin(&key, cx);
        true
    }

    /// Whether Code's tab on screen is pinned: ⌘K says Unpin.
    pub(super) fn shown_tab_pinned(&self, cx: &App) -> bool {
        self.shown_tab()
            .is_some_and(|k| self.code.read(cx).code.pinned(&k))
    }

    /// The tab of the page on screen in Code: a file's path, a folder's with its slash.
    fn shown_tab(&self) -> Option<String> {
        self.nav_page().and_then(tab_of)
    }

    /// Tab `key` on screen: its file or its folder's page.
    pub(super) fn open_code_tab(&mut self, key: &str, cx: &mut Context<Self>) {
        match tab_page(key) {
            Page::Folder(path) => self.open_folder_page(path, cx),
            Page::File(path) => self.open_file_view(path, cx),
            _ => {}
        }
        self.scroll_to_code_tab(cx);
    }

    /// The strip scrolled to the tab on screen.
    pub(super) fn scroll_to_code_tab(&self, cx: &App) {
        let shown = self.shown_tab();
        self.code.read(cx).scroll_to_tab(shown.as_deref());
    }

    /// The preview tab `key` kept: a double click on it, as on a file in the tree.
    pub(super) fn keep_code_tab(&mut self, key: &str, cx: &mut Context<Self>) {
        self.code.update(cx, |c, cx| c.keep_tab(key, cx));
    }

    /// ⌘W in Code: the tab on screen closes, its neighbour shows, else the project's page. A
    /// pinned one stays: unpinned first (#20).
    pub(super) fn close_code_tab(&mut self, key: &str, cx: &mut Context<Self>) {
        if self.code.read(cx).code.pinned(key) {
            return self.toast_info("Pinned: unpin it to close it".into(), cx);
        }
        let at = self.code.update(cx, |c, _| {
            let code = &mut c.code;
            let at = code.shown_tabs().iter().position(|(p, _)| p == key);
            code.tabs.retain(|t| t != key);
            if code.preview.as_deref() == Some(key) {
                code.preview = None;
            }
            at
        });
        if self.shown_tab().as_deref() != Some(key) {
            return cx.notify();
        }
        let rest = self.code.read(cx).code.shown_tabs();
        let next = at.and_then(|at| rest.get(at.min(rest.len().saturating_sub(1))).cloned());
        match next {
            Some((next, _)) => self.open_code_tab(&next, cx),
            None => {
                self.review.update(cx, |rv, _| rv.file_view = None);
                self.show(Space::Code, None, cx);
            }
        }
    }

    /// Every tab closes but `keep`'s (Close others), or every one (Close all) — the pinned ones
    /// stay.
    pub(super) fn close_code_tabs(&mut self, keep: Option<String>, cx: &mut Context<Self>) {
        let keep = keep.filter(|k| {
            self.code
                .read(cx)
                .code
                .shown_tabs()
                .iter()
                .any(|(t, _)| t == k)
        });
        self.code.update(cx, |c, _| {
            let code = &mut c.code;
            code.tabs
                .retain(|t| Some(t) == keep.as_ref() || code.pins.contains(t));
            if code.preview != keep {
                code.preview = None;
            }
        });
        self.after_closing(keep, cx);
    }

    /// Close to the right: the tabs after `key` close, the pinned ones staying.
    pub(super) fn close_code_tabs_right(&mut self, key: String, cx: &mut Context<Self>) {
        let found = self.code.update(cx, |c, _| {
            let code = &mut c.code;
            let tabs = code.shown_tabs();
            let Some(at) = tabs.iter().position(|(t, _)| *t == key) else {
                return false;
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
            true
        });
        if found {
            self.after_closing(Some(key), cx);
        }
    }

    /// Tabs closed: `keep` shows if the one on screen went, the one on screen stays if it did not,
    /// else the project's page.
    fn after_closing(&mut self, keep: Option<String>, cx: &mut Context<Self>) {
        let shown = self.shown_tab();
        let left = shown.as_ref().is_some_and(|s| {
            self.code
                .read(cx)
                .code
                .shown_tabs()
                .iter()
                .any(|(t, _)| t == s)
        });
        match keep {
            _ if left => cx.notify(),
            Some(key) => self.open_code_tab(&key, cx),
            None => {
                self.review.update(cx, |rv, _| rv.file_view = None);
                self.show(Space::Code, None, cx);
            }
        }
    }

    /// ⌃⇥ and ⌃⇧⇥ in Code: the next or the previous tab, going round.
    pub(super) fn step_code_tab(&mut self, forward: bool, cx: &mut Context<Self>) {
        let shown = self.code.read(cx).code.shown_tabs();
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
        self.open_code_tab(&key, cx);
    }

    /// Code: the tree beside the folder page or the file's tabs, the tool window after them.
    pub(super) fn render_code_space(&self, cx: &mut Context<Self>) -> Div {
        let (width, shown) = (self.layout.sidebar, self.nav_page().cloned());
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .flex()
            // It slides and fades in and out with ⌘J, as the side panel does: with its handle,
            // so no gap is left to vanish at the end (the developer, 2026-10-23); the handle
            // resizes it as it does the side panel.
            .children(
                (!self.compact() && (self.layout.show_sidebar || self.motion.recent("side"))).then(
                    || {
                        let room = self.layout.sidebar + super::frame::handle_width();
                        let tree = self
                            .code
                            .update(cx, |c, cx| c.render_tree(width, shown.clone(), cx));
                        super::motion::toward(
                            div()
                                .flex()
                                .flex_none()
                                .overflow_hidden()
                                .child(tree)
                                .child(self.handle(super::Split::Sidebar, cx)),
                            "code-tree-pane",
                            self.layout.show_sidebar,
                            self.motion.recent("side"),
                            super::motion::BASE,
                            move |pane, v| pane.max_w(px(room * v)).opacity(v),
                        )
                    },
                ),
            )
            .child(self.code.clone())
            .children(self.render_tool(cx))
            // Narrow, the tree opens over the page (#80).
            .children(self.side_drawer().then(|| {
                let tree = self
                    .code
                    .update(cx, |c, cx| c.render_tree(width, shown.clone(), cx));
                self.drawer(tree)
            }))
    }

    /// The Outline tool window: the declarations of the file read in Code; a click goes to one.
    pub(super) fn outline_tool(&self, cx: &mut Context<Self>) -> super::tools::ToolView {
        let outline = self.review.read(cx).file_outline();
        let n = outline.len();
        let rows = outline.into_iter().map(|(line, depth, kind, name)| {
            super::row(SharedString::from(format!("outline-{line}")), false)
                .h(px(26.))
                .pl(px(10. + depth as f32 * 14.))
                .gap(px(8.))
                .text_size(px(12.))
                .child(
                    div()
                        .flex_none()
                        .font_family(theme::code_font())
                        .text_color(theme::faint())
                        .child(kind),
                )
                .child(super::hover::cut("name", name).flex_1())
                .child(
                    div()
                        .flex_none()
                        .text_size(px(11.))
                        .text_color(theme::faint())
                        .child(line.to_string()),
                )
                .on_click(
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.reveal_file_line(line, cx)),
                )
        });
        let empty = match &self.review.read(cx).file_view {
            None => "Open a file to see what it declares.",
            Some(_) => "Nothing declared here that Artoo reads.",
        };
        super::tools::ToolView {
            title: "Outline".into(),
            count: (n > 0).then(|| n.to_string()),
            actions: Vec::new(),
            body: div()
                .id("outline-list")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .p(px(6.))
                .children(rows)
                .children((n == 0).then(|| {
                    div()
                        .px(px(8.))
                        .py(px(6.))
                        .text_size(px(12.))
                        .text_color(theme::muted())
                        .child(empty)
                }))
                .into_any_element(),
            foot: None,
        }
    }

    /// The menu's choice: Code reads there.
    pub(super) fn set_code_revision(&mut self, rev: Option<(String, Oid)>, cx: &mut Context<Self>) {
        self.read_code_at(rev, cx);
    }
}

/// The tab as its Code space's host.
pub(super) struct CodeHost(pub(super) WeakEntity<Workspace>);

impl CodeHost {
    /// `change` made to the tab once the update under way is over.
    fn later(
        &self,
        cx: &mut App,
        change: impl FnOnce(&mut Workspace, &mut Context<Workspace>) + 'static,
    ) {
        let tab = self.0.clone();
        cx.defer(move |cx| {
            tab.update(cx, change).ok();
        });
    }
}

impl Host for CodeHost {
    fn shown(&self, cx: &App) -> Option<Page> {
        let tab = self.0.upgrade()?;
        tab.read(cx)
            .nav_page()
            .filter(|p| matches!(p, Page::File(_) | Page::Folder(_)))
            .cloned()
    }

    fn file(&self, path: &str, cx: &mut App) -> FilePage {
        self.0
            .update(cx, |tab, cx| tab.file_page(path, cx))
            .unwrap_or(FilePage::Empty)
    }

    fn folder_notes(&self, folder: &str, cx: &App) -> (usize, Vec<AnyElement>) {
        let Some(tab) = self.0.upgrade() else {
            return (0, Vec::new());
        };
        let tab = tab.read(cx);
        let prefix = format!("{folder}/");
        let notes: Vec<u64> = tab
            .comments
            .iter()
            .filter(|c| c.path == prefix && !c.archived)
            .map(|c| c.id)
            .collect();
        let mut drawn: Vec<AnyElement> = notes
            .iter()
            .map(|&id| tab.render_thread(id, 0., self.0.clone(), cx))
            .collect();
        if tab.folder_compose() == Some(folder) {
            drawn.push(tab.render_composer(0., self.0.clone(), cx));
        }
        (notes.len(), drawn)
    }

    fn note_waits(&self, folder: &str, cx: &App) -> bool {
        self.0
            .upgrade()
            .is_some_and(|tab| tab.read(cx).folder_note_waits(folder))
    }

    fn recent_files(&self, cx: &App) -> Vec<String> {
        let Some(tab) = self.0.upgrade() else {
            return Vec::new();
        };
        tab.read(cx)
            .recents
            .iter()
            .filter_map(|e| match &e.key {
                super::recents::Key::File(path) => Some(path.clone()),
                _ => None,
            })
            .take(8)
            .collect()
    }

    fn selecting(&self, cx: &App) -> Option<markdown::Selecting> {
        let tab = self.0.upgrade()?;
        Some(tab.read(cx).selecting(self.0.clone()))
    }

    fn tree_row(&self, path: &str, row: AnyElement, cx: &App) -> AnyElement {
        match self.0.upgrade() {
            Some(tab) => tab.read(cx).tree_row(path, row),
            None => row,
        }
    }

    fn branches(&self, cx: &App) -> Vec<BranchRef> {
        self.0
            .upgrade()
            .map(|tab| tab.read(cx).branches.clone())
            .unwrap_or_default()
    }

    fn open_app(&self, path: &Path, cx: &App) -> Option<super::open_in::App> {
        self.0.upgrade()?.read(cx).open_app(path)
    }

    fn open_file(&self, path: String, preview: bool, cx: &mut App) {
        self.later(cx, move |tab, cx| tab.open_code_file(path, preview, cx));
    }

    fn show_file(&self, path: String, cx: &mut App) {
        self.later(cx, move |tab, cx| tab.open_file_view(path, cx));
    }

    fn open_folder(&self, path: String, cx: &mut App) {
        self.later(cx, move |tab, cx| tab.open_folder_page(path, cx));
    }

    fn open_tab(&self, key: String, cx: &mut App) {
        self.later(cx, move |tab, cx| tab.open_code_tab(&key, cx));
    }

    fn close_tab(&self, key: String, cx: &mut App) {
        self.later(cx, move |tab, cx| tab.close_code_tab(&key, cx));
    }

    fn note_folder(&self, folder: String, cx: &mut App) {
        self.later(cx, move |tab, cx| tab.start_folder_comment(&folder, cx));
    }

    fn menu(&self, at: gpui::Point<gpui::Pixels>, groups: Vec<Vec<Entry>>, cx: &mut App) {
        self.later(cx, move |tab, cx| tab.open_menu(at, groups, cx));
    }

    fn go_to_file(&self, cx: &mut App) {
        self.later(cx, |tab, cx| tab.search(super::palette::Scope::Files, cx));
    }

    fn close_drawer(&self, cx: &mut App) {
        self.later(cx, |tab, cx| tab.close_drawer(cx));
    }

    fn tree_opening(&self, folder: String, cx: &mut App) {
        self.later(cx, move |tab, cx| tab.tree_opening(&folder, cx));
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
