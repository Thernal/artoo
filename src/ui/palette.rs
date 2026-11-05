//! Search everywhere (#52, Design `docs/specs/command-palette.md`): one window with a tab per
//! scope — All ⌘K, Files ⌘P, Commits ⌘⇧C, Branches ⌘⇧B, Requests ⌘⇧R, Actions ⌘⇧P —, each
//! named with what it found; ⇥ ⇧⇥ move between them keeping the text, `>` typed moves to Actions.
//! The same overlay picks the refs of a comparison (⌥⌘C), a theme, a recent place, a bookmark. What it lists and
//! does is here; the window is drawn by the shell (`artoo_shell_api::palette`).

use super::input::{self, Edit};
use super::{Workspace, format, theme};
use crate::git::{NamedRef, Repo};
use artoo_shell_api::palette;
use git2::Oid;
use gpui::{Action, App, Context, KeyDownEvent, WeakEntity, Window, prelude::*};
use std::rc::Rc;
use std::sync::Arc;

/// A tab of search everywhere: the tab's own, or one a feature offers (`Parts::searches`), by its id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    All,
    Files,
    Commits,
    Actions,
    Offered(&'static str),
}

impl Scope {
    /// An own tab's place among the tabs, the offered ones between them by theirs (`Search::order`).
    fn order(self) -> u16 {
        match self {
            Scope::All => 0,
            Scope::Files => 10,
            Scope::Commits => 20,
            Scope::Actions => 90,
            Scope::Offered(_) => u16::MAX,
        }
    }
}

/// A section of the All tab: the tab's own, or an offered scope's few.
enum Section<'a> {
    Commands,
    Files,
    Commits,
    Offered(&'a artoo_shell_api::search::Search),
}

#[derive(Clone)]
pub enum Kind {
    /// Search everywhere, on one tab.
    Search(Scope),
    /// The places opened lately, newest first (⌘E).
    Recent,
    /// The functions and types the open file changes (⌘⇧O).
    Structure,
    /// The lines marked with F3 in the open diff (⌘F3).
    Bookmarks,
    /// Switching to a branch or a merge request (⌘B), with how far each one's review got.
    Switch,
    /// Picking the first side of a comparison.
    Base,
    /// Picking the second side, the first being `.0`.
    Head((String, Oid)),
    /// The themes (#12 B): ↑↓ tries each on the whole window, ↵ keeps, esc goes back.
    Theme,
    /// Open a repository (⌘T, the rail's +): the ones opened before, filtered as you type, a path
    /// typed, and Browse for the system's picker (the developer, 2026-10-23: not a new tab's
    /// welcome page, which had no way back).
    Projects,
    /// A face for the interface or (`code`) the code (#24, a face's Other…): every one installed,
    /// each drawn in itself; ↵ keeps it.
    Face { code: bool },
}

/// A command in ⌘K: one offered with words (`artoo_shell_api::Action::words`), in ⌘K's order.
pub(super) struct Offered {
    id: &'static str,
    words: &'static str,
    act: Box<dyn Action>,
}

/// The commands offered with words, the window root's and the features', in ⌘K's order (`Action::order`).
pub(super) fn offered(cx: &App) -> Vec<Offered> {
    let mut offered: Vec<&artoo_shell_api::Action> = artoo_shell_api::actions(cx)
        .iter()
        .filter(|a| a.words.is_some())
        .collect();
    offered.sort_by_key(|a| a.order);
    offered
        .into_iter()
        .map(|a| Offered {
            id: a.id,
            words: a.words.unwrap_or_default(),
            act: a.act.boxed_clone(),
        })
        .collect()
}

#[derive(Clone)]
enum Pick {
    Mark {
        path: String,
        line: u32,
    },
    Symbol {
        old: bool,
        line: u32,
    },
    Recent(super::recents::Key),
    Commit(Oid),
    File(usize),
    Branch(usize),
    Request(u64),
    /// A file of the repository the diff does not change.
    RepoFile(String),
    Link(crate::links::Link),
    /// A command offered (`Workspace::contributed`), by its place there.
    Command(usize),
    /// A find of a scope offered: where it goes.
    Open(artoo_core::route::Route),
    /// Browse… for a folder: Open's.
    Browse,
    Ref(String, Oid),
    Theme(String),
    /// A repository to open in a tab of its own, or go to when one has it.
    Project(std::path::PathBuf),
    /// A face (`Kind::Face`), None its default.
    Face(Option<String>),
}

/// The family a face's row is drawn in.
fn face_of(kind: &Kind, pick: &Pick) -> Option<gpui::SharedString> {
    match (kind, pick) {
        (Kind::Face { code }, Pick::Face(face)) => {
            Some(super::fonts::face_family(*code, face.as_deref()))
        }
        _ => None,
    }
}

#[derive(Clone)]
struct Item {
    group: &'static str,
    label: String,
    detail: String,
    key: &'static str,
    pick: Pick,
}

pub struct Palette {
    kind: Kind,
    /// The tab `>` moved away from: ⌫ in the empty field goes back to it.
    from: Option<Scope>,
    /// What each tab found for the text, in the tabs' order (with text typed).
    counts: Option<Vec<usize>>,
    /// The repository's files, read when search everywhere opened.
    files: Arc<Vec<String>>,
    /// The faces installed, read when a face's list opened.
    faces: Vec<String>,
    query: String,
    at: usize,
    /// ⌘B: only what is not reviewed yet (⇥ switches).
    open_only: bool,
    items: Vec<Item>,
    refs: Vec<NamedRef>,
}

/// Open a repository's list: a folder typed as a path, the ones opened before ranked by the
/// typed words (the one on screen marked), and Browse.
fn project_items(
    query: &str,
    recent: &[std::path::PathBuf],
    here: Option<&std::path::Path>,
) -> Vec<Item> {
    let mut items = Vec::new();
    let typed = query.trim();
    let expanded = match typed.strip_prefix('~') {
        Some(rest) => std::env::home_dir().map(|h| h.join(rest.trim_start_matches('/'))),
        None => Some(std::path::PathBuf::from(typed)),
    };
    if typed.starts_with(['/', '~'])
        && let Some(path) = expanded.filter(|p| p.is_dir())
    {
        items.push(Item {
            group: "Folder",
            label: path.display().to_string(),
            detail: String::new(),
            key: "↵",
            pick: Pick::Project(path),
        });
    }
    let named: Vec<(String, &std::path::PathBuf)> = recent
        .iter()
        .map(|p| (crate::labels::project_name(p), p))
        .collect();
    // By the name first, then by the whole path as Go to file does: `work ibam` finds
    // ~/Desktop/Work/ABB/ibam, `abb` every project under ABB (the developer, 2026-10-23).
    let ranked: Vec<&(String, &std::path::PathBuf)> = if typed.is_empty() {
        named.iter().collect()
    } else {
        let words: Vec<String> = typed.split_whitespace().map(str::to_lowercase).collect();
        crate::rank::rank(named.iter(), typed, |(name, _)| name.as_str())
            .into_iter()
            .chain(named.iter().filter(|(name, path)| {
                crate::rank::score(typed, name).is_none()
                    && matches(&words, &super::format::home_relative(path))
            }))
            .collect()
    };
    items.extend(ranked.into_iter().map(|(name, path)| Item {
        group: "Opened before",
        label: name.clone(),
        detail: if here == Some(path.as_path()) {
            "on screen".to_owned()
        } else {
            super::format::home_relative(path)
        },
        key: "",
        pick: Pick::Project((*path).clone()),
    }));
    items.push(Item {
        group: "On disk",
        label: "Browse…".to_owned(),
        detail: String::new(),
        key: "⌘O",
        pick: Pick::Browse,
    });
    items
}

/// A file's name, which leads, and its folder, which follows it fainter — as IDEs list files.
/// A scope's shipped key as the foot writes it, the command key first and the letter in capitals (`⌘⇧R`), as the
/// app's written keys are; a key the user changed shows as the keymap writes it (`⇧⌘r`).
fn foot_key(keys: &str) -> String {
    let written = super::keymap::symbols(keys);
    let rest: String = written.chars().filter(|&c| c != '⌘').collect();
    let cmd = if written.contains('⌘') { "⌘" } else { "" };
    format!("{cmd}{}", rest.to_uppercase())
}

fn name_and_folder(path: &str) -> (String, String) {
    match path.rsplit_once('/') {
        Some((folder, name)) => (name.to_owned(), folder.to_owned()),
        None => (path.to_owned(), String::new()),
    }
}

/// The words of `query` all occur in `hay`, ignoring case — or as the starts of its words.
fn matches(words: &[String], hay: &str) -> bool {
    words.iter().all(|w| crate::rank::word_matches(w, hay))
}

impl Workspace {
    /// The scopes the tab's features offer (`Parts::searches`).
    fn offered_searches(&self) -> impl Iterator<Item = &artoo_shell_api::search::Search> {
        self.parts.iter().flat_map(|p| &p.searches)
    }

    fn offered_search(&self, id: &str) -> Option<&artoo_shell_api::search::Search> {
        self.offered_searches().find(|s| s.id == id)
    }

    /// Search everywhere's tabs, in their order: the tab's own, the offered ones among them.
    pub(super) fn scopes(&self) -> Vec<Scope> {
        let mut scopes: Vec<(u16, Scope)> =
            [Scope::All, Scope::Files, Scope::Commits, Scope::Actions]
                .into_iter()
                .map(|s| (s.order(), s))
                .chain(
                    self.offered_searches()
                        .map(|s| (s.order, Scope::Offered(s.id))),
                )
                .collect();
        scopes.sort_by_key(|(order, _)| *order);
        scopes.into_iter().map(|(_, s)| s).collect()
    }

    fn scope_label(&self, scope: Scope) -> &'static str {
        match scope {
            Scope::All => "All",
            Scope::Files => "Files",
            Scope::Commits => "Commits",
            Scope::Actions => "Actions",
            Scope::Offered(id) => self.offered_search(id).map_or(id, |s| s.label),
        }
    }

    /// The action that opens ⌘K on `scope`, by its id: the foot says its key in force.
    fn scope_go(&self, scope: Scope) -> &'static str {
        match scope {
            Scope::All => "shell.search",
            Scope::Files => "shell.search-files",
            Scope::Commits => "shell.search-commits",
            Scope::Actions => "shell.search-actions",
            Scope::Offered(id) => self.offered_search(id).map_or("", |s| s.go),
        }
    }

    fn scope_placeholder(&self, scope: Scope) -> &'static str {
        match scope {
            Scope::All => "Search everywhere — files, commits, branches, requests, actions",
            Scope::Files => "Files — this diff's first, then the repository's",
            Scope::Commits => "Commits — by words of the message, the author or the id",
            Scope::Actions => "Actions — everything the app does, with its key",
            Scope::Offered(id) => self.offered_search(id).map_or("", |s| s.placeholder),
        }
    }

    /// What offered scope `search` finds for `ask`, as rows under its groups.
    fn offered_items(
        &self,
        search: &artoo_shell_api::search::Search,
        ask: artoo_shell_api::search::Ask,
        cx: &App,
    ) -> Vec<Item> {
        (search.find)(ask, cx)
            .into_iter()
            .map(|f| Item {
                group: f.group,
                label: f.label,
                detail: f.detail,
                key: "",
                pick: Pick::Open(f.open),
            })
            .collect()
    }

    /// ⌘B's list: the merge requests (when the project has them), then the local branches; each
    /// says how far its review got.
    fn switch_items(&self, query: &str, open_only: bool, cx: &App) -> Vec<Item> {
        let state = |head: Option<Oid>| match head {
            Some(h) if self.rv().head_reviewed(h, cx) => (true, "✓ reviewed".to_owned()),
            Some(h) => match self.rv().head_part(h, cx) {
                Some((done, total)) => (false, format!("{done}/{total} reviewed")),
                None => (false, "not reviewed".to_owned()),
            },
            None => (false, String::new()),
        };
        let list = self.request_list(cx);
        let requests = crate::rank::rank(list.iter(), query, |m| &m.title)
            .into_iter()
            .chain(
                // `!12` or a branch name finds a request too.
                list.iter().filter(|m| {
                    let q = query.trim().trim_start_matches('!');
                    !q.is_empty()
                        && (m.iid.to_string() == q || crate::rank::word_matches(q, &m.source))
                        && crate::rank::score(query, &m.title).is_none()
                }),
            )
            .filter_map(|m| {
                let (done, said) = state(Oid::from_str(&m.head).ok());
                (!(open_only && done)).then(|| Item {
                    group: "Merge requests",
                    label: format!("{} {}", super::words::number(m.iid), m.title),
                    detail: format!("{} · {said}", m.source),
                    key: "",
                    pick: Pick::Request(m.iid),
                })
            });
        let list = self.branch_list(cx);
        let branches = crate::rank::rank(
            list.iter()
                .enumerate()
                .filter(|(_, b)| b.kind == crate::git::RefKind::Local),
            query,
            |(_, b)| &b.name,
        )
        .into_iter()
        .filter_map(|(ix, b)| {
            let (done, said) = state(Some(b.tip));
            (!(open_only && done)).then(|| Item {
                group: "Branches",
                label: b.name.clone(),
                detail: format!("{} · {said}", format::short(b.tip)),
                key: "",
                pick: Pick::Branch(ix),
            })
        });
        requests.take(10).chain(branches.take(10)).collect()
    }

    pub(super) fn open_palette(&mut self, kind: Kind, cx: &mut Context<Self>) {
        let refs = match (&kind, &self.root) {
            (
                Kind::Search(_)
                | Kind::Recent
                | Kind::Structure
                | Kind::Bookmarks
                | Kind::Switch
                | Kind::Theme
                | Kind::Projects
                | Kind::Face { .. },
                _,
            )
            | (_, None) => Vec::new(),
            (_, Some(root)) => Repo::open(root)
                .and_then(|r| r.compare_refs())
                .unwrap_or_default(),
        };
        self.field = None;
        self.rv().clear_find(cx);
        if let Some(branches) = self.branches() {
            branches.drop_find(cx);
        }
        self.ctx_menu.update(cx, |m, cx| m.shut(cx));
        self.reset_field_edit();
        let theme = matches!(kind, Kind::Theme);
        let face = match kind {
            Kind::Face { code } => Some(code),
            _ => None,
        };
        let faces = match face {
            Some(_) => super::fonts::installed(cx),
            None => Vec::new(),
        };
        let code_files = self.code().map(|c| c.files(cx)).unwrap_or_default();
        let files = match (&kind, &self.root) {
            // Code's own: the tree it shows, a plain folder's files on disk (#75).
            (Kind::Search(_), Some(_)) if !code_files.is_empty() => code_files,
            (Kind::Search(_), Some(root)) if self.plain => {
                Arc::new(crate::files::on_disk(root, 200_000))
            }
            (Kind::Search(_), Some(root)) => Arc::new(
                Repo::open(root)
                    .map(|r| r.all_files(200_000))
                    .unwrap_or_default(),
            ),
            _ => Arc::default(),
        };
        self.palette = Some(Palette {
            kind,
            from: None,
            counts: None,
            files,
            faces,
            query: String::new(),
            at: 0,
            open_only: false,
            items: Vec::new(),
            refs,
        });
        self.refresh_palette(cx);
        // The themes open on the one in use.
        if theme && let Some(p) = &mut self.palette {
            let current = theme::choice().window;
            p.at = p
                .items
                .iter()
                .position(|i| matches!(&i.pick, Pick::Theme(id) if *id == current))
                .unwrap_or(0);
        }
        // A face's list opens on the one in use.
        if let Some(code) = face
            && let Some(p) = &mut self.palette
        {
            let faces = theme::faces();
            let current = if code { faces.code } else { faces.ui };
            p.at = p
                .items
                .iter()
                .position(|i| matches!(&i.pick, Pick::Face(f) if *f == current))
                .unwrap_or(0);
        }
        cx.notify();
    }

    pub(super) fn close_palette(&mut self, cx: &mut Context<Self>) {
        if let Some(p) = self.palette.take() {
            // A theme being tried goes back to the chosen one.
            if matches!(p.kind, Kind::Theme) {
                theme::restore();
                cx.refresh_windows();
            }
            cx.notify();
        }
    }

    /// The theme under the selection, drawn on the whole window for now.
    fn try_theme(&mut self, cx: &mut Context<Self>) {
        let Some(p) = &self.palette else {
            return;
        };
        if let Some(Pick::Theme(id)) = p.items.get(p.at).map(|i| &i.pick) {
            let mut choice = theme::choice();
            choice.window = id.clone();
            theme::preview(&choice);
            cx.refresh_windows();
        }
    }

    fn refresh_palette(&mut self, cx: &App) {
        let Some(palette) = &self.palette else {
            return;
        };
        let items = self.palette_items(palette, cx);
        // Every tab's count, while something is typed; All's is what the others found (X56).
        let counts = match palette.kind {
            Kind::Search(_) if !palette.query.trim().is_empty() => {
                let scopes = self.scopes();
                let mut counts: Vec<usize> = (scopes.iter())
                    .map(|&scope| match scope {
                        Scope::All => 0,
                        _ => self.scope_items(scope, palette, cx).len(),
                    })
                    .collect();
                let all = scopes.iter().position(|&s| s == Scope::All).unwrap_or(0);
                counts[all] = counts.iter().sum();
                Some(counts)
            }
            _ => None,
        };
        if let Some(p) = &mut self.palette {
            p.at = p.at.min(items.len().saturating_sub(1));
            p.items = items;
            p.counts = counts;
        }
    }

    /// Opens search everywhere on `scope`, or moves to that tab when it is open (the text stays);
    /// the same key again closes it.
    pub(super) fn search(&mut self, scope: Scope, cx: &mut Context<Self>) {
        match self.palette.as_mut() {
            Some(p) if matches!(p.kind, Kind::Search(s) if s == scope) => self.close_palette(cx),
            Some(p) if matches!(p.kind, Kind::Search(_)) => {
                p.kind = Kind::Search(scope);
                p.from = None;
                p.at = 0;
                self.refresh_palette(cx);
                cx.notify();
            }
            _ => self.open_palette(Kind::Search(scope), cx),
        }
    }

    /// The next tab (or the one before), the text kept.
    fn step_scope(&mut self, back: bool, cx: &mut Context<Self>) {
        let Some(p) = &mut self.palette else {
            return;
        };
        let Kind::Search(scope) = p.kind else {
            return;
        };
        let scopes = self.scopes();
        let Some(p) = &mut self.palette else {
            return;
        };
        let at = scopes.iter().position(|s| *s == scope).unwrap_or(0);
        let n = scopes.len();
        p.kind = Kind::Search(scopes[if back { (at + n - 1) % n } else { (at + 1) % n }]);
        p.from = None;
        p.at = 0;
        self.refresh_palette(cx);
        cx.notify();
    }

    /// A command's label: Copy link names what is on screen, Pin tab says Unpin on a pinned one.
    fn command_label(&self, offered: &Offered, cx: &App) -> String {
        match offered.id {
            "shell.copy-link" => format!("Copy link to {}", self.page_kind()),
            "shell.pin-tab" if self.shown_tab_pinned(cx) => "Unpin tab".to_owned(),
            _ => offered.words.to_owned(),
        }
    }

    /// The commands offered that the palette's words match, under `group`.
    fn command_items(&self, group: &'static str, words: &[String], cx: &App) -> Vec<Item> {
        self.contributed
            .iter()
            .enumerate()
            .filter(|(_, o)| matches(words, o.words))
            .map(|(ix, o)| Item {
                group,
                label: self.command_label(o, cx),
                detail: String::new(),
                key: "",
                pick: Pick::Command(ix),
            })
            .collect()
    }

    /// What tab `scope` lists for the palette's text.
    fn scope_items(&self, scope: Scope, p: &Palette, cx: &App) -> Vec<Item> {
        let query = p.query.trim();
        let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        match scope {
            Scope::All => self.all_items(query, &p.files, cx),
            Scope::Actions => self.command_items("Actions", &words, cx),
            Scope::Offered(id) => self.offered_search(id).map_or_else(Vec::new, |search| {
                let ask = artoo_shell_api::search::Ask {
                    text: query,
                    all: false,
                };
                self.offered_items(search, ask, cx)
            }),
            Scope::Commits => self
                .branches()
                .map(|b| {
                    b.commits_where(
                        &|c| {
                            words.is_empty()
                                || matches(&words, &c.summary)
                                || matches(&words, &c.author)
                                || c.id.to_string().starts_with(&words[0])
                        },
                        60,
                        cx,
                    )
                })
                .unwrap_or_default()
                .into_iter()
                .map(|c| Item {
                    group: "Commits",
                    label: c.summary.clone(),
                    detail: format!("{} · {}", format::short(c.id), c.author),
                    key: "",
                    pick: Pick::Commit(c.id),
                })
                .collect(),
            Scope::Files => self.file_items(query, &p.files, cx),
        }
    }

    fn palette_items(&self, p: &Palette, cx: &App) -> Vec<Item> {
        let query = p.query.trim_start();
        if matches!(p.kind, Kind::Projects) {
            return project_items(query, &crate::storage::recent(), self.root.as_deref());
        }
        if matches!(p.kind, Kind::Theme) {
            let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
            let current = theme::choice().window;
            return theme::all()
                .into_iter()
                .filter(|t| t.window && matches(&words, &t.label))
                .map(|t| Item {
                    group: if t.built_in { "Themes" } else { "Your themes" },
                    detail: if t.id == current {
                        "current".to_owned()
                    } else if !t.problems.is_empty() {
                        super::plural(t.problems.len(), "problem")
                    } else if t.light() {
                        "light".to_owned()
                    } else {
                        String::new()
                    },
                    label: t.label,
                    key: "",
                    pick: Pick::Theme(t.id),
                })
                .collect();
        }
        if let Kind::Face { code } = p.kind {
            let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
            let faces = theme::faces();
            let current = if code { faces.code } else { faces.ui };
            let item = |group, face: Option<String>| Item {
                group,
                label: super::fonts::face_label(code, face.as_deref()),
                detail: if face == current { "current" } else { "" }.to_owned(),
                key: "",
                pick: Pick::Face(face),
            };
            let default = super::fonts::face_label(code, None);
            return matches(&words, &default)
                .then(|| item("Default", None))
                .into_iter()
                .chain(
                    p.faces
                        .iter()
                        .filter(|f| matches(&words, f))
                        .map(|f| item("Installed", Some(f.clone()))),
                )
                .collect();
        }
        if matches!(p.kind, Kind::Bookmarks) {
            let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
            return self
                .bookmark_list(cx)
                .into_iter()
                .filter(|m| matches(&words, &format!("{} {}", m.path, m.text)))
                .map(|m| Item {
                    group: "Bookmarks",
                    label: format!(
                        "{}:{}",
                        m.path.rsplit('/').next().unwrap_or(&m.path),
                        m.line
                    ),
                    detail: m.text.clone(),
                    key: "",
                    pick: Pick::Mark {
                        path: m.path,
                        line: m.line,
                    },
                })
                .collect();
        }
        if matches!(p.kind, Kind::Structure) {
            let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
            let diff = self.shown_diff(cx);

            let Some(file) = diff.as_ref().and_then(|d| d.files.get(self.diff_file(cx))) else {
                return Vec::new();
            };
            let name = file
                .path()
                .rsplit('/')
                .next()
                .unwrap_or_default()
                .to_owned();
            let group: &'static str = Box::leak(format!("Changes in {name}").into_boxed_str());
            return crate::symbols::changed(file)
                .into_iter()
                .filter(|s| matches(&words, &s.name))
                .map(|s| Item {
                    group,
                    label: format!("{}  {} {}", s.mark, s.kind, s.name),
                    detail: format!("{}line {}", if s.old { "old " } else { "" }, s.line),
                    key: "",
                    pick: Pick::Symbol {
                        old: s.old,
                        line: s.line,
                    },
                })
                .collect();
        }
        if matches!(p.kind, Kind::Switch) {
            return self.switch_items(query, p.open_only, cx);
        }
        if matches!(p.kind, Kind::Recent) {
            let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
            return self
                .recents
                .iter()
                .filter(|r| matches(&words, &format!("{} {}", r.label, r.detail)))
                .take(14)
                .map(|r| Item {
                    group: "Recent",
                    label: r.label.clone(),
                    detail: r.detail.clone(),
                    key: "",
                    pick: Pick::Recent(r.key.clone()),
                })
                .collect();
        }
        if let Kind::Search(scope) = p.kind {
            return self.scope_items(scope, p, cx);
        }
        let group = if matches!(p.kind, Kind::Base) {
            "Base"
        } else {
            "Compare with"
        };
        {
            let mut ranked = crate::rank::rank(&p.refs, query, |r| &r.name);
            // A stale local branch comes after its upstream: that is the one meant.
            for i in 0..ranked.len() {
                if let Some(st) = &ranked[i].stale
                    && let Some(j) = ranked[i..].iter().position(|r| r.name == st.upstream)
                {
                    let upstream = ranked.remove(i + j);
                    ranked.insert(i, upstream);
                }
            }
            let mut items: Vec<Item> = ranked
                .into_iter()
                .take(14)
                .map(|r| Item {
                    group,
                    label: r.name.clone(),
                    detail: match &r.stale {
                        Some(st) => format!(
                            "branch {} · ↓{} behind {}",
                            format::short(r.tip),
                            st.behind,
                            st.upstream
                        ),
                        None => format!(
                            "{} {}",
                            if r.tag { "tag" } else { "branch" },
                            format::short(r.tip)
                        ),
                    },
                    key: "",
                    pick: Pick::Ref(r.name.clone(), r.tip),
                })
                .collect();
            // Anything git can name: a commit id, HEAD~3, a ref the list does not show.
            if !query.is_empty()
                && !p.refs.iter().any(|r| r.name == query.trim())
                && let Some(id) = self
                    .root
                    .as_ref()
                    .and_then(|root| Repo::open(root).ok()?.resolve(query).ok())
            {
                items.insert(
                    0,
                    Item {
                        group,
                        label: query.trim().to_owned(),
                        detail: format!("commit {}", format::short(id)),
                        key: "",
                        pick: Pick::Ref(query.trim().to_owned(), id),
                    },
                );
            }
            items
        }
    }

    /// The All tab: a few of every kind, a pasted link as one place, `#` `@` narrowing.
    /// The Files tab's finds: this diff's files first, then the repository's.
    fn file_items(&self, query: &str, files: &[String], cx: &App) -> Vec<Item> {
        let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        let name = |path: &str| path.rsplit('/').next().unwrap_or_default().to_owned();
        let mut items: Vec<Item> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        if let Some(diff) = &self.shown_diff(cx) {
            for (ix, f) in crate::rank::rank(diff.files.iter().enumerate(), query, |(_, f)| {
                f.path().rsplit('/').next().unwrap_or_default()
            })
            .into_iter()
            .chain(diff.files.iter().enumerate().filter(|(_, f)| {
                !words.is_empty()
                    && matches(&words, f.path())
                    && crate::rank::score(query, &name(f.path())).is_none()
            })) {
                if seen.insert(f.path().to_owned()) {
                    let (label, detail) = name_and_folder(f.path());
                    items.push(Item {
                        group: "In this diff",
                        label,
                        detail,
                        key: "",
                        pick: Pick::File(ix),
                    });
                }
            }
        }
        if !words.is_empty() {
            let ranked = crate::rank::rank(files.iter(), query, |f| {
                f.rsplit('/').next().unwrap_or_default()
            });
            items.extend(
                ranked
                    .into_iter()
                    .chain(files.iter().filter(|f| {
                        matches(&words, f) && crate::rank::score(query, &name(f)).is_none()
                    }))
                    .filter(|f| seen.insert((*f).clone()))
                    .take(60)
                    .map(|f| {
                        let (label, detail) = name_and_folder(f);
                        Item {
                            group: "In the repository",
                            label,
                            detail,
                            key: "",
                            pick: Pick::RepoFile(f.clone()),
                        }
                    }),
            );
        }
        items
    }

    fn all_items(&self, query: &str, files: &[String], cx: &App) -> Vec<Item> {
        // A pasted link of a host's is one place to go.
        if let Some(link) = artoo_hosts_api::get(cx).link(query) {
            return vec![Item {
                group: "Link",
                label: format!("Open {}", link.describe()),
                detail: link.project().to_owned(),
                key: "↵",
                pick: Pick::Link(link),
            }];
        }
        // `>` narrows it to the commands, `@` to the files, an offered scope's own character to it (`#`).
        let narrows = |c: char| {
            matches!(c, '>' | '@') || self.offered_searches().any(|s| s.all.prefix == Some(c))
        };
        let (scope, rest) = match query.chars().next() {
            Some(c) if narrows(c) => (Some(c), &query[c.len_utf8()..]),
            _ => (None, query),
        };
        let words: Vec<String> = rest.split_whitespace().map(str::to_lowercase).collect();
        let cap = |small: usize| if scope.is_some() { 14 } else { small };
        // A section with a character of its own: it alone when that is typed, else a few once words are.
        let shows = |own: Option<char>| match scope {
            Some(c) => own == Some(c),
            None => !words.is_empty(),
        };
        let mut sections: Vec<(u16, Section)> = vec![
            (0, Section::Commands),
            (20, Section::Files),
            (30, Section::Commits),
        ];
        sections.extend(
            self.offered_searches()
                .map(|s| (s.all.order, Section::Offered(s))),
        );
        sections.sort_by_key(|(order, _)| *order);
        let mut items = Vec::new();
        for (_, section) in sections {
            match section {
                Section::Commands if scope.is_none_or(|s| s == '>') => items.extend(
                    self.command_items("Commands", &words, cx)
                        .into_iter()
                        .take(cap(5)),
                ),
                // The Files tab's first finds: this diff's, then the repository's (X56).
                Section::Files if shows(Some('@')) => items.extend(
                    self.file_items(rest, files, cx)
                        .into_iter()
                        .take(cap(3))
                        .map(|i| Item {
                            group: "Files",
                            ..i
                        }),
                ),
                Section::Commits if shows(None) => items.extend(
                    self.branches()
                        .map(|b| {
                            b.commits_where(
                                &|c| {
                                    matches(&words, &c.summary)
                                        || matches(&words, &c.author)
                                        || c.id.to_string().starts_with(&words[0])
                                },
                                4,
                                cx,
                            )
                        })
                        .unwrap_or_default()
                        .into_iter()
                        .map(|c| Item {
                            group: "Commits",
                            label: c.summary.clone(),
                            detail: format!("{} · {}", format::short(c.id), c.author),
                            key: "",
                            pick: Pick::Commit(c.id),
                        }),
                ),
                Section::Offered(search) if shows(search.all.prefix) => {
                    let ask = artoo_shell_api::search::Ask {
                        text: rest,
                        all: true,
                    };
                    let few = cap(search.all.few);
                    items.extend(self.offered_items(search, ask, cx).into_iter().take(few));
                }
                _ => {}
            }
        }
        // Jumping to what was typed beats the commands when something real matched.
        if scope.is_none() && !words.is_empty() {
            items.sort_by_key(|i| (i.group == "Commands") as u8);
        }
        items
    }

    pub(super) fn palette_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(palette) = &mut self.palette else {
            return;
        };
        if event.keystroke.key == "tab" && matches!(palette.kind, Kind::Search(_)) {
            return self.step_scope(event.keystroke.modifiers.shift, cx);
        }
        // ⌫ in the empty field goes back from Actions to the tab ">" left.
        if event.keystroke.key == "backspace"
            && palette.query.is_empty()
            && let Some(from) = palette.from.take()
        {
            palette.kind = Kind::Search(from);
            self.refresh_palette(cx);
            return cx.notify();
        }
        if event.keystroke.key == "tab" && matches!(palette.kind, Kind::Switch) {
            palette.open_only = !palette.open_only;
            palette.at = 0;
            self.refresh_palette(cx);
            return cx.notify();
        }
        match input::edit(
            &mut palette.query,
            &mut self.field_edit,
            &event.keystroke,
            cx,
        ) {
            Edit::Changed => {
                palette.at = 0;
                // ">" typed first moves to Actions, shown as a chip in the field.
                if let Kind::Search(scope) = palette.kind
                    && scope != Scope::Actions
                    && palette.query.starts_with('>')
                {
                    palette.query = palette.query[1..].trim_start().to_owned();
                    palette.kind = Kind::Search(Scope::Actions);
                    palette.from = Some(scope);
                    self.reset_field_edit();
                }
                self.refresh_palette(cx);
                if matches!(self.palette.as_ref().map(|p| &p.kind), Some(Kind::Theme)) {
                    self.try_theme(cx);
                }
            }
            Edit::Enter { .. } => {
                let at = palette.at;
                self.run_palette_item(at, window, cx);
                return;
            }
            Edit::Escape => {
                // A second pick goes back to the first.
                if matches!(palette.kind, Kind::Head(_)) {
                    palette.kind = Kind::Base;
                    palette.query.clear();
                    self.refresh_palette(cx);
                } else {
                    return self.close_palette(cx);
                }
            }
            Edit::Selected | Edit::Ignored => {}
        }
        cx.notify();
    }

    /// Sets the field's text, as typing it would (UI checks).
    #[cfg(feature = "snapshot")]
    pub(super) fn type_in_palette(&mut self, text: &str, cx: &App) {
        if let Some(p) = &mut self.palette {
            p.query = text.to_owned();
        }
        self.refresh_palette(cx);
    }

    pub(super) fn palette_step(&mut self, down: bool, cx: &mut Context<Self>) {
        if let Some(p) = &mut self.palette
            && !p.items.is_empty()
        {
            let n = p.items.len();
            p.at = if down {
                (p.at + 1) % n
            } else {
                (p.at + n - 1) % n
            };
            if matches!(p.kind, Kind::Theme) {
                self.try_theme(cx);
            }
            cx.notify();
        }
    }

    pub(super) fn run_palette_item(
        &mut self,
        ix: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(item) = self.palette.as_ref().and_then(|p| p.items.get(ix)).cloned() else {
            return;
        };
        match item.pick {
            Pick::Ref(name, id) => {
                let kind = self.palette.as_ref().map(|p| p.kind.clone());
                match kind {
                    Some(Kind::Base) => {
                        if let Some(p) = &mut self.palette {
                            p.kind = Kind::Head((name, id));
                            p.query.clear();
                            p.at = 0;
                        }
                        self.refresh_palette(cx);
                    }
                    Some(Kind::Head(base)) => {
                        self.palette = None;
                        self.go_page(
                            super::nav::Page::Compare(base, (name, id), true).route(),
                            cx,
                        );
                    }
                    _ => self.palette = None,
                }
            }
            Pick::Recent(key) => {
                self.palette = None;
                self.open_recent(key, cx);
            }
            Pick::Mark { path, line } => {
                self.palette = None;
                self.jump_to_line(&path, line, false, cx);
            }
            Pick::Symbol { old, line } => {
                self.palette = None;
                if let Some(path) = self.current_path(cx) {
                    self.jump_to_line(&path, line, old, cx);
                }
            }
            // A result opens in its space (#71): a commit or a file of what is loaded on its page.
            Pick::Commit(id) => {
                self.palette = None;
                if let Some(ix) = self.commit_at(id, cx) {
                    self.open_commit(ix, cx);
                }
            }
            // In Code a file opens there, as a tab, changed or not (#75).
            Pick::File(ix) if self.nav.space == super::spaces::CODE => {
                self.palette = None;
                if let Some(path) = self
                    .shown_diff(cx)
                    .as_ref()
                    .and_then(|d| d.files.get(ix))
                    .map(|f| f.path().to_owned())
                {
                    self.open_file_view(path, cx);
                }
            }
            Pick::File(ix) => {
                self.palette = None;
                self.select_file(ix, cx);
                self.follow_loaded(cx);
            }
            Pick::Branch(ix) => {
                self.palette = None;
                self.open_branch(ix, None, cx);
            }
            Pick::Request(iid) => {
                self.palette = None;
                self.open_request(iid, cx);
            }
            // Opened as a part's route is (`Frame::open`).
            Pick::Open(route) => {
                self.palette = None;
                if self.drawn().has(&route) {
                    self.go_page(route, cx);
                } else {
                    self.toast_info(format!("Nothing here opens {}", route.kind).into(), cx);
                }
            }
            Pick::RepoFile(path) => {
                self.palette = None;
                // Not changed here: read in Code (D18, #71).
                self.open_file_view(path, cx);
            }
            Pick::Link(link) => {
                self.palette = None;
                self.open_link(link, cx);
            }
            Pick::Command(ix) => {
                self.palette = None;
                cx.notify();
                if let Some(offered) = self.contributed.get(ix) {
                    window.dispatch_action(offered.act.boxed_clone(), cx);
                }
            }
            Pick::Browse => {
                self.palette = None;
                cx.notify();
                window.dispatch_action(Box::new(super::Open), cx);
            }
            Pick::Theme(id) => {
                self.palette = None;
                let mut choice = theme::choice();
                choice.window = id;
                theme::choose(choice);
                cx.refresh_windows();
                cx.notify();
            }
            Pick::Face(face) => {
                let code = matches!(
                    self.palette.as_ref().map(|p| &p.kind),
                    Some(Kind::Face { code: true })
                );
                self.palette = None;
                self.choose_face(code, face, cx);
            }
            // A tab of its own, as + means; one that has it already comes on screen.
            Pick::Project(path) => {
                self.palette = None;
                match self.root {
                    None => self.request_open(path, None, cx),
                    Some(_) => self.open_in_new_tab(path, None, cx),
                }
            }
        }
        cx.notify();
    }

    /// Shows what `head` changed against `base`; `since_merge_base` for `base...head`.
    pub(super) fn run_compare(
        &mut self,
        base: (String, Oid),
        head: (String, Oid),
        since_merge_base: bool,
        request: Option<u64>,
        cx: &mut Context<Self>,
    ) {
        if self.root.is_none() {
            return;
        }
        if let Some(branches) = self.branches() {
            branches.compare(base.clone(), head.clone(), since_merge_base, request, cx);
        }
        // The whole request is its page; a commit or versions of it were (#71).
        if request.is_some() {
            self.record(cx);
        }
        // A comparison on screen is what it shows: Swap, Direct and Use <upstream> write it, so
        // coming back to it, ⌘[ ⌘] and nav.txt have it as it was left (#147).
        let page = super::nav::Page::Compare(base.clone(), head.clone(), since_merge_base);
        let route = page.route();
        if request.is_none()
            && matches!(self.shown_page(), Some(super::nav::Page::Compare(..)))
            && self.nav_page() != Some(&route)
        {
            self.arrive(route.clone(), cx);
        }
        // A comparison's own page lists its commits (#21); a request's or a branch's has its own.
        let own = request.is_none() && self.nav_page() == Some(&route);
        self.read_compare_commits(own.then_some(page), cx);
        cx.notify();
    }

    pub(super) fn render_palette(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let p = self.palette.as_ref()?;
        let placeholder = match &p.kind {
            Kind::Search(scope) => self.scope_placeholder(*scope),
            Kind::Recent => "Recent places — files, commits, merge requests",
            Kind::Structure => "Functions and types this file changes",
            Kind::Bookmarks => "Bookmarks — lines marked with F3",
            Kind::Switch if p.open_only => "Switch to — only what is not reviewed yet   ⇥ all",
            Kind::Switch => "Switch to a branch or a merge request   ⇥ only not reviewed",
            Kind::Base => "Compare — pick the base: a branch, a tag or a commit",
            Kind::Head(_) => "Compare — pick what to compare with it",
            Kind::Theme => "Theme — ↑↓ tries it on the whole window, ↵ keeps, esc goes back",
            Kind::Projects => "Open a repository — type to filter, or a path",
            Kind::Face { code: false } => "The interface's face — each in its own type",
            Kind::Face { code: true } => "The code's face — each in its own type",
        };
        let title = match &p.kind {
            Kind::Search(_)
            | Kind::Recent
            | Kind::Structure
            | Kind::Bookmarks
            | Kind::Switch
            | Kind::Theme
            | Kind::Face { .. } => None,
            Kind::Projects => Some("Open a repository".to_owned()),
            Kind::Base => Some("Compare · step 1 of 2".to_owned()),
            Kind::Head((name, _)) => Some(format!("Compare · {name} … ?")),
        };
        let keymap = cx.global::<super::keymap::Keymap>();
        let rows = p
            .items
            .iter()
            .map(|item| {
                // A command's key in force: one changed in Settings → Keyboard shows the new one.
                let key = match item.pick {
                    Pick::Command(ix) => self
                        .contributed
                        .get(ix)
                        .map(|o| keymap.key_now(o.act.as_ref()))
                        .unwrap_or_default(),
                    Pick::Browse => keymap
                        .shown(&super::Open)
                        .unwrap_or_else(|| item.key.to_owned()),
                    _ => item.key.to_owned(),
                };
                palette::Row {
                    group: item.group,
                    label: item.label.clone(),
                    detail: item.detail.clone(),
                    key,
                    // A file's icon before it (#95).
                    icon: match &item.pick {
                        Pick::File(f) => self
                            .shown_diff(cx)
                            .as_ref()
                            .and_then(|d| d.files.get(*f))
                            .map(|f| f.path().to_owned()),
                        Pick::RepoFile(path) => Some(path.clone()),
                        _ => None,
                    },
                    file: matches!(
                        item.pick,
                        Pick::File(_) | Pick::RepoFile(_) | Pick::Project(_)
                    ),
                    face: face_of(&p.kind, &item.pick),
                }
            })
            .collect();
        let empty = match &p.kind {
            Kind::Recent => "Nothing opened yet.",
            Kind::Projects => "None by that name. Browse… finds it on disk.",
            Kind::Bookmarks => "No bookmarks yet: click a line and press F3.",
            Kind::Structure => {
                "No function or type of this file is changed (or the language is not known)."
            }
            Kind::Search(_) if p.query.is_empty() => "Type to search.",
            Kind::Search(_) => "Nothing matches.",
            Kind::Switch if p.open_only => "Nothing left to review here.",
            Kind::Switch => "No branch or merge request by that name.",
            Kind::Face { .. } => "No face installed by that name.",
            _ => "No branch, tag or commit by that name.",
        };
        // Search everywhere: its tabs, what each found, and the one on with its key.
        let (tabs, foot) = match p.kind {
            Kind::Search(on) => {
                let tabs = (self.scopes().into_iter().enumerate())
                    .map(|(i, scope)| palette::Tab {
                        label: self.scope_label(scope),
                        count: p.counts.as_ref().and_then(|c| c.get(i).copied()),
                        lit: scope == on,
                    })
                    .collect();
                let go = self.scope_go(on);
                let key = (artoo_shell_api::actions(cx).iter())
                    .find(|a| a.id == go)
                    .map(|a| {
                        let act = a.act.as_ref();
                        let shipped = || keymap.shipped_key(act).map(|k| foot_key(&k));
                        keymap.shown(act).or_else(shipped).unwrap_or_default()
                    })
                    .unwrap_or_default();
                let foot = format!(
                    "{} · {}",
                    super::words::say(self.scope_label(on)),
                    super::keys::text(&key)
                );
                (Some(tabs), Some(foot))
            }
            _ => (None, None),
        };
        Some(palette::palette(
            palette::PaletteView {
                title,
                tabs,
                actions: p.from.is_some(),
                field: self
                    .field_text(&p.query, true, placeholder, cx)
                    .into_any_element(),
                rows,
                at: p.at,
                empty,
                foot,
            },
            Rc::new(Glue(cx.weak_entity())),
            cx,
        ))
    }
}

/// What a click on the palette does, answered by the tab.
struct Glue(WeakEntity<Workspace>);

impl palette::PaletteHost for Glue {
    fn run(&self, ix: usize, window: &mut Window, cx: &mut App) {
        self.0
            .update(cx, |this, cx| this.run_palette_item(ix, window, cx))
            .ok();
    }

    fn close(&self, _: &mut Window, cx: &mut App) {
        self.0.update(cx, |this, cx| this.close_palette(cx)).ok();
    }

    fn scope(&self, ix: usize, _: &mut Window, cx: &mut App) {
        self.0
            .update(cx, |this, cx| {
                if let Some(scope) = this.scopes().get(ix).copied() {
                    this.search(scope, cx)
                }
            })
            .ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_foot_writes_a_shipped_key_the_command_key_first() {
        assert_eq!(foot_key("cmd-k"), "⌘K");
        assert_eq!(foot_key("cmd-shift-r"), "⌘⇧R");
    }

    #[test]
    fn a_project_is_found_by_its_name_then_by_its_path() {
        let recent = [
            std::path::PathBuf::from("/w/ABB/ibam-android-app"),
            std::path::PathBuf::from("/w/Thernal/artoo"),
            std::path::PathBuf::from("/w/ABB/notes"),
        ];
        let labels = |q: &str| {
            project_items(q, &recent, None)
                .into_iter()
                .filter(|i| i.group == "Opened before")
                .map(|i| i.label)
                .collect::<Vec<_>>()
        };
        assert_eq!(labels("ibam"), ["ibam-android-app"]);
        // The folder above them: every project under it.
        assert_eq!(labels("abb"), ["ibam-android-app", "notes"]);
        // A name match comes before a path match.
        assert_eq!(labels("notes"), ["notes"]);
        assert_eq!(labels("abb notes"), ["notes"]);
    }
}
