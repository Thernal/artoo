//! Search everywhere (#52, Design `docs/specs/command-palette.md`): one window with a tab per
//! scope — All ⌘K, Files ⌘P, Commits ⌘⇧C, Branches ⌘⇧B, Requests ⌘⇧R, Actions ⌘⇧P —, each
//! named with what it found; ⇥ ⇧⇥ move between them keeping the text, `>` typed moves to Actions.
//! The same overlay shows a list alone: the tab's own (a recent place, a bookmark, a face…) and those a feature
//! offers (`Parts::lists`: the refs of a comparison, ⌥⌘C, the branches'; the themes to try, the appearance's). What it
//! lists and does is here; the window is drawn by the shell (`artoo_shell_api::palette`).

use super::input::{self, Edit};
use super::{Workspace, format, theme};
use artoo_shell_api::palette;
use artoo_shell_api::search::Opens;
use git2::Oid;
use gpui::{Action, App, Context, KeyDownEvent, WeakEntity, Window, prelude::*};
use std::rc::Rc;

/// A tab of search everywhere: the tab's own, or one a feature offers (`Parts::searches`), by its id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    All,
    Actions,
    Offered(&'static str),
}

impl Scope {
    /// An own tab's place among the tabs, the offered ones between them by theirs (`Search::order`).
    fn order(self) -> u16 {
        match self {
            Scope::All => 0,
            Scope::Actions => 90,
            Scope::Offered(_) => u16::MAX,
        }
    }
}

/// A section of the All tab: the tab's own, or an offered scope's few.
enum Section<'a> {
    Commands,
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
    /// A list a feature offers (`Parts::lists`), by its id: a comparison's refs, the themes to try.
    List(&'static str),
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
    Branch(usize),
    Request(u64),
    Link(crate::links::Link),
    /// A command offered (`Workspace::contributed`), by its place there.
    Command(usize),
    /// A find of a scope or a list offered: where it goes, the file it is, the face its row is drawn in, what it
    /// tries as the selection comes to it, whether it is the one in force.
    Found {
        open: artoo_shell_api::search::Opens,
        file: Option<String>,
        face: Option<gpui::SharedString>,
        tried: Option<artoo_shell_api::search::Doing>,
        current: bool,
    },
    /// Browse… for a folder: Open's.
    Browse,
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
        (_, Pick::Found { face, .. }) => face.clone(),
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
    /// The faces installed, read when a face's list opened.
    faces: Vec<String>,
    query: String,
    at: usize,
    /// ⌘B: only what is not reviewed yet (⇥ switches).
    open_only: bool,
    items: Vec<Item>,
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

/// An offered scope's or list's finds as rows.
fn found_items(finds: Vec<artoo_shell_api::search::Find>) -> Vec<Item> {
    finds
        .into_iter()
        .map(|f| Item {
            group: f.group,
            label: f.label,
            detail: f.detail,
            key: "",
            pick: Pick::Found {
                open: f.open,
                file: f.file,
                face: f.face,
                tried: f.tried,
                current: f.current,
            },
        })
        .collect()
}

/// A scope's shipped key as the foot writes it, the command key first and the letter in capitals (`⌘⇧R`), as the
/// app's written keys are; a key the user changed shows as the keymap writes it (`⇧⌘r`).
fn foot_key(keys: &str) -> String {
    let written = super::keymap::symbols(keys);
    let rest: String = written.chars().filter(|&c| c != '⌘').collect();
    let cmd = if written.contains('⌘') { "⌘" } else { "" };
    format!("{cmd}{}", rest.to_uppercase())
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

    fn offered_list(&self, id: &str) -> Option<&artoo_shell_api::search::List> {
        self.parts
            .iter()
            .flat_map(|p| &p.lists)
            .find(|l| l.id == id)
    }

    /// ⌘K opened on scope `id`, or the window on list `id` (`Frame::search`).
    pub(super) fn search_offered(&mut self, id: &'static str, cx: &mut Context<Self>) {
        if self.offered_list(id).is_some() {
            self.open_palette(Kind::List(id), cx);
        } else if self.offered_search(id).is_some() {
            self.search(Scope::Offered(id), cx);
        }
    }

    /// Search everywhere's tabs, in their order: the tab's own, the offered ones among them.
    pub(super) fn scopes(&self) -> Vec<Scope> {
        let mut scopes: Vec<(u16, Scope)> = [Scope::All, Scope::Actions]
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
            Scope::Actions => "Actions",
            Scope::Offered(id) => self.offered_search(id).map_or(id, |s| s.label),
        }
    }

    /// The action that opens ⌘K on `scope`, by its id: the foot says its key in force.
    fn scope_go(&self, scope: Scope) -> &'static str {
        match scope {
            Scope::All => "shell.search",
            Scope::Actions => "shell.search-actions",
            Scope::Offered(id) => self.offered_search(id).map_or("", |s| s.go),
        }
    }

    fn scope_placeholder(&self, scope: Scope) -> &'static str {
        match scope {
            Scope::All => "Search everywhere — files, commits, branches, requests, actions",
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
        found_items((search.find)(ask, cx))
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
        self.field = None;
        self.rv().clear_find(cx);
        if let Some(branches) = self.branches() {
            branches.drop_find(cx);
        }
        self.ctx_menu.update(cx, |m, cx| m.shut(cx));
        self.reset_field_edit();
        let face = match kind {
            Kind::Face { code } => Some(code),
            _ => None,
        };
        let faces = match face {
            Some(_) => super::fonts::installed(cx),
            None => Vec::new(),
        };
        // What the offered scopes read once for their finds (Code's: the project's files), or the list its own.
        let opened: Vec<_> = match kind {
            Kind::Search(_) => (self.offered_searches())
                .filter_map(|s| s.opened.clone())
                .collect(),
            Kind::List(id) => (self.offered_list(id).and_then(|l| l.opened.clone()))
                .into_iter()
                .collect(),
            _ => Vec::new(),
        };
        for opened in opened {
            opened(cx);
        }
        self.palette = Some(Palette {
            kind,
            from: None,
            counts: None,
            faces,
            query: String::new(),
            at: 0,
            open_only: false,
            items: Vec::new(),
        });
        self.refresh_palette(cx);
        // A list opens on the one in force (the theme on the window).
        if let Some(p) = &mut self.palette
            && matches!(p.kind, Kind::List(_))
        {
            p.at = (p.items.iter())
                .position(|i| matches!(i.pick, Pick::Found { current: true, .. }))
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
            // What a list tried is put back (a theme, the chosen one).
            if let Kind::List(id) = p.kind
                && let Some(closed) = self.offered_list(id).and_then(|l| l.closed.clone())
            {
                closed(cx);
            }
            cx.notify();
        }
    }

    /// What the find under the selection tries (a theme on the whole window).
    fn try_found(&mut self, cx: &mut Context<Self>) {
        let tried = (self.palette.as_ref()).and_then(|p| match &p.items.get(p.at)?.pick {
            Pick::Found { tried, .. } => tried.clone(),
            _ => None,
        });
        if let Some(tried) = tried {
            tried(cx);
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

    /// A file Code's search found, picked (`artoo_code_api::Host::pick_file`): one of the diff on screen picked there,
    /// its page coming on screen (#71) — but in Code, where it opens as a tab, changed or not (#75) —; else, not changed
    /// here, read in Code (D18, #71).
    pub(super) fn pick_file(&mut self, path: String, in_diff: bool, cx: &mut Context<Self>) {
        if !in_diff || self.nav.space == super::spaces::CODE {
            return self.open_file_view(path, cx);
        }
        let shown = self.shown_diff(cx);
        if let Some(ix) = shown.and_then(|d| d.files.iter().position(|f| f.path() == path)) {
            self.select_file(ix, cx);
            self.follow_loaded(cx);
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
            Scope::All => self.all_items(query, cx),
            Scope::Actions => self.command_items("Actions", &words, cx),
            Scope::Offered(id) => self.offered_search(id).map_or_else(Vec::new, |search| {
                let ask = artoo_shell_api::search::Ask {
                    text: query,
                    all: false,
                };
                self.offered_items(search, ask, cx)
            }),
        }
    }

    fn palette_items(&self, p: &Palette, cx: &App) -> Vec<Item> {
        let query = p.query.trim_start();
        if matches!(p.kind, Kind::Projects) {
            return project_items(query, &crate::storage::recent(), self.root.as_deref());
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
        match p.kind {
            Kind::List(id) => self.offered_list(id).map_or_else(Vec::new, |list| {
                let ask = artoo_shell_api::search::Ask {
                    text: query,
                    all: false,
                };
                found_items((list.find)(ask, cx))
            }),
            _ => Vec::new(),
        }
    }

    /// The All tab: a few of every kind, a pasted link as one place, `#` `@` narrowing.
    fn all_items(&self, query: &str, cx: &App) -> Vec<Item> {
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
        // `>` narrows it to the commands, an offered scope's own character to it (`@` the files, `#` the branches).
        let narrows =
            |c: char| c == '>' || self.offered_searches().any(|s| s.all.prefix == Some(c));
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
        let mut sections: Vec<(u16, Section)> = vec![(0, Section::Commands)];
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
                self.try_found(cx);
            }
            Edit::Enter { .. } => {
                let at = palette.at;
                self.run_palette_item(at, window, cx);
                return;
            }
            Edit::Escape => {
                // A second pick goes back to the first.
                let back = match palette.kind {
                    Kind::List(id) => self.offered_list(id).and_then(|l| l.back),
                    _ => None,
                };
                match (back, &mut self.palette) {
                    (Some(back), Some(palette)) => {
                        palette.kind = Kind::List(back);
                        palette.query.clear();
                        self.refresh_palette(cx);
                    }
                    _ => return self.close_palette(cx),
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
            self.try_found(cx);
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
            Pick::Branch(ix) => {
                self.palette = None;
                self.open_branch(ix, None, cx);
            }
            Pick::Request(iid) => {
                self.palette = None;
                self.open_request(iid, cx);
            }
            // Opened as a part's route is (`Frame::open`).
            Pick::Found {
                open: Opens::Route(route),
                ..
            } => {
                self.palette = None;
                if self.drawn().has(&route) {
                    self.go_page(route, cx);
                } else {
                    self.toast_info(format!("Nothing here opens {}", route.kind).into(), cx);
                }
            }
            // The owner's doing, once the tab is no longer being changed: it may ask the tab.
            Pick::Found {
                open: Opens::Run(run),
                ..
            } => {
                self.palette = None;
                cx.notify();
                cx.defer(move |cx| run(cx));
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
        // What a list offered says as it stands.
        let said = match p.kind {
            Kind::List(id) => self.offered_list(id).map(|l| (l.says)(cx)),
            _ => None,
        };
        let placeholder = match &p.kind {
            Kind::Search(scope) => self.scope_placeholder(*scope),
            Kind::Recent => "Recent places — files, commits, merge requests",
            Kind::Structure => "Functions and types this file changes",
            Kind::Bookmarks => "Bookmarks — lines marked with F3",
            Kind::Switch if p.open_only => "Switch to — only what is not reviewed yet   ⇥ all",
            Kind::Switch => "Switch to a branch or a merge request   ⇥ only not reviewed",
            Kind::List(_) => said.as_ref().map_or("", |s| s.placeholder),
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
            | Kind::Face { .. } => None,
            Kind::Projects => Some("Open a repository".to_owned()),
            Kind::List(_) => said.as_ref().and_then(|s| s.title.clone()),
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
                        Pick::Found { file, .. } => file.clone(),
                        _ => None,
                    },
                    file: matches!(
                        item.pick,
                        Pick::Found { file: Some(_), .. } | Pick::Project(_)
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
            Kind::List(_) => said.as_ref().map_or("", |s| s.empty),
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
