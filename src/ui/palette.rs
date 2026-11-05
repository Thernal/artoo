//! Search everywhere (#52, Design `docs/specs/command-palette.md`): one window with a tab per
//! scope — All ⌘K, Files ⌘P, Commits ⌘⇧C, Branches ⌘⇧B, Requests ⌘⇧R, Actions ⌘⇧P —, each
//! named with what it found; ⇥ ⇧⇥ move between them keeping the text, `>` typed moves to Actions.
//! The same overlay shows a list alone, every one offered (`Parts::lists`): the tab's own (`palette_lists`: a recent
//! place, a bookmark, a face…) and the features' (the refs of a comparison, ⌥⌘C, the branches'; the themes to try, the
//! appearance's). What a scope or a list finds and does runs once the tab is no longer being changed, since it may
//! read the tab. What it lists and does is here; the window is drawn by the shell (`artoo_shell_api::palette`).

use super::Workspace;
use super::input::{self, Edit};
use artoo_shell_api::palette;
use artoo_shell_api::search::{Doing, Opens};
use git2::Oid;
use gpui::{Action, App, Context, KeyDownEvent, SharedString, WeakEntity, Window, prelude::*};
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

#[derive(Clone, Copy)]
pub enum Kind {
    /// Search everywhere, on one tab.
    Search(Scope),
    /// A list offered (`Parts::lists`), by its id: the tab's places opened lately (`palette_lists`), a comparison's
    /// refs, the themes to try.
    List(&'static str),
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
    Link(crate::links::Link),
    /// A command offered (`Workspace::contributed`), by its place there.
    Command(usize),
    /// A find of a scope or a list offered: where it goes, what it tries as the selection comes to it, whether it is
    /// the one in force.
    Found {
        open: Opens,
        tried: Option<Doing>,
        current: bool,
    },
}

#[derive(Clone)]
struct Item {
    group: SharedString,
    label: String,
    detail: String,
    key: String,
    /// A file's path: its icon before it (#95).
    icon: Option<String>,
    /// Its words a name and its folder (a file, a repository).
    in_folder: bool,
    /// The face its row is drawn in (#24).
    face: Option<SharedString>,
    pick: Pick,
}

pub struct Palette {
    kind: Kind,
    /// The tab `>` moved away from: ⌫ in the empty field goes back to it.
    from: Option<Scope>,
    /// What each tab found for the text, in the tabs' order (with text typed).
    counts: Option<Vec<usize>>,
    query: String,
    at: usize,
    items: Vec<Item>,
}

/// An offered scope's or list's finds as rows.
fn found_items(finds: Vec<artoo_shell_api::search::Find>) -> Vec<Item> {
    finds
        .into_iter()
        .map(|f| Item {
            group: f.group,
            label: f.label,
            detail: f.detail,
            key: f.key,
            in_folder: f.in_folder || f.file.is_some(),
            icon: f.file,
            face: f.face,
            pick: Pick::Found {
                open: f.open,
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

    pub(super) fn open_palette(&mut self, kind: Kind, cx: &mut Context<Self>) {
        self.field = None;
        self.rv().clear_find(cx);
        if let Some(branches) = self.branches() {
            branches.drop_find(cx);
        }
        self.ctx_menu.update(cx, |m, cx| m.shut(cx));
        self.reset_field_edit();
        // What the offered scopes read once for their finds (Code's: the project's files), or the list its own (the
        // faces installed), before the first finds.
        let opened: Vec<Doing> = match kind {
            Kind::Search(_) => (self.offered_searches())
                .filter_map(|s| s.opened.clone())
                .collect(),
            Kind::List(id) => (self.offered_list(id).and_then(|l| l.opened.clone()))
                .into_iter()
                .collect(),
        };
        cx.defer(move |cx| opened.iter().for_each(|opened| opened(cx)));
        self.palette = Some(Palette {
            kind,
            from: None,
            counts: None,
            query: String::new(),
            at: 0,
            items: Vec::new(),
        });
        // A list opens on the one in force (the theme on the window, the face in use).
        self.refresh_then(cx, |this, _| {
            if let Some(p) = &mut this.palette
                && matches!(p.kind, Kind::List(_))
            {
                p.at = (p.items.iter())
                    .position(|i| matches!(i.pick, Pick::Found { current: true, .. }))
                    .unwrap_or(0);
            }
        });
        cx.notify();
    }

    pub(super) fn close_palette(&mut self, cx: &mut Context<Self>) {
        if let Some(p) = self.palette.take() {
            // What a list tried is put back (a theme, the chosen one).
            if let Kind::List(id) = p.kind
                && let Some(closed) = self.offered_list(id).and_then(|l| l.closed.clone())
            {
                cx.defer(move |cx| closed(cx));
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
            cx.defer(move |cx| tried(cx));
        }
    }

    /// Finds again what the window lists for its text.
    fn refresh_palette(&mut self, cx: &mut Context<Self>) {
        self.refresh_then(cx, |_, _| {});
    }

    /// Finds again what the window lists for its text, then `then` (the list starting on the one in force, what the
    /// selection tries) — once the tab is no longer being changed, since a find may read the tab (its own lists read
    /// its places).
    fn refresh_then(
        &mut self,
        cx: &mut Context<Self>,
        then: impl FnOnce(&mut Self, &mut Context<Self>) + 'static,
    ) {
        let tab = cx.weak_entity();
        cx.defer(move |cx| {
            let Some((items, counts)) = (tab.upgrade()).and_then(|t| t.read(cx).palette_found(cx))
            else {
                return;
            };
            tab.update(cx, |this, cx| {
                if let Some(p) = &mut this.palette {
                    p.at = p.at.min(items.len().saturating_sub(1));
                    p.items = items;
                    p.counts = counts;
                }
                then(this, cx);
                cx.notify();
            })
            .ok();
        });
    }

    /// What the window lists for its text, with every tab's count while something is typed; None while it is closed.
    fn palette_found(&self, cx: &App) -> Option<(Vec<Item>, Option<Vec<usize>>)> {
        let palette = self.palette.as_ref()?;
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
        Some((items, counts))
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
                group: group.into(),
                label: self.command_label(o, cx),
                detail: String::new(),
                key: String::new(),
                icon: None,
                in_folder: false,
                face: None,
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
        match p.kind {
            Kind::Search(scope) => self.scope_items(scope, p, cx),
            Kind::List(id) => self.offered_list(id).map_or_else(Vec::new, |list| {
                let ask = artoo_shell_api::search::Ask {
                    text: p.query.trim_start(),
                    all: false,
                };
                found_items((list.find)(ask, cx))
            }),
        }
    }

    /// The All tab: a few of every kind, a pasted link as one place, `#` `@` narrowing.
    fn all_items(&self, query: &str, cx: &App) -> Vec<Item> {
        // A pasted link of a host's is one place to go.
        if let Some(link) = artoo_hosts_api::get(cx).link(query) {
            return vec![Item {
                group: "Link".into(),
                label: format!("Open {}", link.describe()),
                detail: link.project().to_owned(),
                key: "↵".to_owned(),
                icon: None,
                in_folder: false,
                face: None,
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
            items.sort_by_key(|i| (i.group.as_ref() == "Commands") as u8);
        }
        items
    }

    pub(super) fn palette_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(kind) = self.palette.as_ref().map(|p| p.kind) else {
            return;
        };
        if event.keystroke.key == "tab" {
            match kind {
                Kind::Search(_) => return self.step_scope(event.keystroke.modifiers.shift, cx),
                // What ⇥ does in a list (Switch to: only what is not reviewed), found again from the first.
                Kind::List(id) => {
                    if let Some(tab) = self.offered_list(id).and_then(|l| l.tab.clone()) {
                        if let Some(p) = &mut self.palette {
                            p.at = 0;
                        }
                        cx.defer(move |cx| tab(cx));
                        self.refresh_palette(cx);
                        return cx.notify();
                    }
                }
            }
        }
        let Some(palette) = &mut self.palette else {
            return;
        };
        // ⌫ in the empty field goes back from Actions to the tab ">" left.
        if event.keystroke.key == "backspace"
            && palette.query.is_empty()
            && let Some(from) = palette.from.take()
        {
            palette.kind = Kind::Search(from);
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
                self.refresh_then(cx, |this, cx| this.try_found(cx));
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
    pub(super) fn type_in_palette(&mut self, text: &str, cx: &mut Context<Self>) {
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
        let placeholder = match p.kind {
            Kind::Search(scope) => self.scope_placeholder(scope),
            Kind::List(_) => said.as_ref().map_or("", |s| s.placeholder),
        };
        let title = said.as_ref().and_then(|s| s.title.clone());
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
                    _ => item.key.clone(),
                };
                palette::Row {
                    group: item.group.clone(),
                    label: item.label.clone(),
                    detail: item.detail.clone(),
                    key,
                    icon: item.icon.clone(),
                    file: item.in_folder,
                    face: item.face.clone(),
                }
            })
            .collect();
        let empty = match p.kind {
            Kind::Search(_) if p.query.is_empty() => "Type to search.",
            Kind::Search(_) => "Nothing matches.",
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
}
