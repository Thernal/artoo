//! Search everywhere (#52, Design `docs/specs/command-palette.md`): one window with a tab per scope — All ⌘K, Files
//! ⌘P, Commits ⌘⇧C, Branches ⌘⇧B, Requests ⌘⇧R, Actions ⌘⇧P —, each named with what it found; ⇥ ⇧⇥ move between them
//! keeping the text, `>` typed moves to Actions. The same window shows a list alone (`search::List`): a comparison's
//! refs, the themes to try, the tab's places opened lately.
//!
//! One in each tab, its handle in the tab's services (`search::SearchWindow`, D20 of #171): All and Actions are its
//! own; every other scope and every list is offered (`Parts::searches`, `Parts::lists`) — a feature's or the tab's —
//! and handed to it by the tab. It never asks the tab while an entity is being changed: what a scope or a list finds
//! and does runs deferred, so it may read the tab.

use artoo_core::rank::word_matches;
use artoo_core::textedit::TextEdit;
use artoo_shell_api::palette::{self, PaletteHost, PaletteView, Row, Tab};
use artoo_shell_api::search::{self, Ask, Doing, Find, List, Opens, Search, SearchWindow};
use artoo_shell_api::{Frame, Parts};
use artoo_ui_kit::input::{self, Edit, Look};
use artoo_ui_kit::{keys, words};
use gpui::{
    Action, AnyView, App, Bounds, Context, Entity, KeyDownEvent, Pixels, ScrollHandle,
    SharedString, WeakEntity, Window, div, prelude::*,
};
use std::cell::Cell;
use std::rc::{Rc, Weak};

/// The group ⌘K lists the page's commands under, nothing typed (#192).
const HERE: &str = "Here";

/// A tab of search everywhere: the window's own, or one offered (`Parts::searches`), by its id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scope {
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

/// A section of the All tab: the window's own, or an offered scope's few.
enum Section<'a> {
    Commands,
    Offered(&'a Search),
}

#[derive(Clone, Copy)]
enum Kind {
    /// Search everywhere, on one tab.
    Search(Scope),
    /// A list offered (`Parts::lists`), by its id.
    List(&'static str),
}

/// A command in ⌘K: one offered with words (`artoo_shell_api::Action::words`), in ⌘K's order.
struct Command {
    id: &'static str,
    words: &'static str,
    /// What `Here` says, on the page it acts on.
    here: Option<&'static str>,
    icon: Option<&'static str>,
    act: Box<dyn Action>,
}

/// The commands offered with words, the window root's and the features', in ⌘K's order (`Action::order`).
fn commands(cx: &App) -> Vec<Command> {
    let mut offered: Vec<&artoo_shell_api::Action> = artoo_shell_api::actions(cx)
        .iter()
        .filter(|a| a.words.is_some())
        .collect();
    offered.sort_by_key(|a| a.order);
    offered
        .into_iter()
        .map(|a| Command {
            id: a.id,
            words: a.words.unwrap_or_default(),
            here: a.here,
            icon: a.icon,
            act: a.act.boxed_clone(),
        })
        .collect()
}

#[derive(Clone)]
enum Pick {
    /// A command offered, by its place among them.
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
    /// An icon by the kit's name before it (#192).
    glyph: Option<&'static str>,
    dots: Vec<palette::Dot>,
    /// Its words a name and its folder (a file, a repository).
    in_folder: bool,
    /// The face its row is drawn in (#24).
    face: Option<SharedString>,
    /// A swatch before it (a theme's, #204).
    swatch: Option<[u32; 3]>,
    /// A word on it while picked, and what it does (a theme's Edit, #204).
    aside: Option<(&'static str, Doing)>,
    pick: Pick,
}

/// The window while it is open.
struct Open {
    kind: Kind,
    /// The tab `>` moved away from: ⌫ in the empty field goes back to it.
    from: Option<Scope>,
    /// What each tab found for the text, in the tabs' order (with text typed).
    counts: Option<Vec<usize>>,
    query: String,
    at: usize,
    items: Vec<Item>,
    /// The button a list dropped from, in the window (#204); None over the window's middle.
    under: Option<Bounds<Pixels>>,
}

/// An offered scope's or list's finds as rows.
fn found_items(finds: Vec<Find>) -> Vec<Item> {
    finds
        .into_iter()
        .map(|f| Item {
            group: f.group,
            label: f.label,
            detail: f.detail,
            key: f.key,
            in_folder: f.in_folder || f.file.is_some(),
            icon: f.file,
            glyph: f.glyph,
            dots: f.dots,
            face: f.face,
            swatch: f.swatch,
            aside: f.aside,
            pick: Pick::Found {
                open: f.open,
                tried: f.tried,
                current: f.current,
            },
        })
        .collect()
}

/// The words of `query` all occur in `hay`, ignoring case — or as the starts of its words.
fn matches(words: &[String], hay: &str) -> bool {
    words.iter().all(|w| word_matches(w, hay))
}

/// The window in a tab: what the tab's parts offer, the commands, the window while open with its field's edit.
pub(crate) struct Machine {
    /// The tab's frame, which the tab keeps: a route picked opens through it, a command's words are its.
    frame: Weak<dyn Frame>,
    searches: Vec<Search>,
    lists: Vec<List>,
    commands: Vec<Command>,
    open: Option<Open>,
    edit: TextEdit,
    scroll: Rc<Cell<f32>>,
    /// The list's scroll, and whether it goes to the selection as it is drawn next: it moved by the keys or the
    /// window opened, never as the wheel scrolls it.
    list: ScrollHandle,
    follow: Cell<bool>,
    /// The tabs' sideways scroll, and the tab on as last drawn: a tab come to is scrolled into sight once.
    tabs: ScrollHandle,
    tab_on: Cell<Option<usize>>,
}

/// The window in a new tab, its handle in the tab's services; it draws nothing of its own as a part.
pub(crate) fn parts(frame: Rc<dyn Frame>, _: &mut Window, cx: &mut App) -> Parts {
    let commands = commands(cx);
    let machine = cx.new(|_| Machine {
        frame: Rc::downgrade(&frame),
        searches: Vec::new(),
        lists: Vec::new(),
        commands,
        open: None,
        edit: TextEdit::default(),
        scroll: Rc::default(),
        list: ScrollHandle::new(),
        follow: Cell::new(false),
        tabs: ScrollHandle::new(),
        tab_on: Cell::new(None),
    });
    frame
        .services()
        .set::<Rc<dyn SearchWindow>>(Rc::new(Handle(machine)));
    Parts::default()
}

/// The handle the tab asks.
struct Handle(Entity<Machine>);

impl SearchWindow for Handle {
    fn offer(&self, searches: Vec<Search>, lists: Vec<List>, cx: &mut App) {
        self.0.update(cx, |m, _| {
            m.searches = searches;
            m.lists = lists;
        });
    }

    fn search(&self, id: &'static str, cx: &mut App) {
        self.0.update(cx, |m, cx| m.search(id, cx));
    }

    fn search_under(&self, id: &'static str, under: Bounds<Pixels>, cx: &mut App) {
        self.0.update(cx, |m, cx| {
            if m.offered_list(id).is_some() {
                m.open_on(Kind::List(id), Some(under), cx);
            }
        });
    }

    fn close(&self, cx: &mut App) {
        self.0.update(cx, |m, cx| m.close(cx));
    }

    fn is_open(&self, cx: &App) -> bool {
        self.0.read(cx).open.is_some()
    }

    fn key(&self, event: &KeyDownEvent, window: &mut Window, cx: &mut App) {
        self.0.update(cx, |m, cx| m.key(event, window, cx));
    }

    fn step(&self, down: bool, cx: &mut App) {
        self.0.update(cx, |m, cx| m.step(down, cx));
    }

    fn selected(&self, cx: &App) -> Option<String> {
        let m = self.0.read(cx);
        m.open.as_ref()?;
        m.edit.selected_text().map(str::to_owned)
    }

    fn view(&self, cx: &App) -> Option<AnyView> {
        self.0
            .read(cx)
            .open
            .is_some()
            .then(|| self.0.clone().into())
    }

    fn type_text(&self, text: &str, cx: &mut App) {
        self.0.update(cx, |m, cx| {
            if let Some(p) = &mut m.open {
                p.query = text.to_owned();
            }
            m.refresh(cx);
        });
    }
}

impl Machine {
    /// The scopes with a tab of their own: every one offered but those All shows alone.
    fn tabbed(&self) -> impl Iterator<Item = &Search> {
        self.searches.iter().filter(|s| !s.all.alone)
    }

    fn offered_search(&self, id: &str) -> Option<&Search> {
        self.tabbed().find(|s| s.id == id)
    }

    fn offered_list(&self, id: &str) -> Option<&List> {
        self.lists.iter().find(|l| l.id == id)
    }

    /// Search everywhere's tabs, in their order: the window's own, the offered ones among them.
    fn scopes(&self) -> Vec<Scope> {
        let mut scopes: Vec<(u16, Scope)> = [Scope::All, Scope::Actions]
            .into_iter()
            .map(|s| (s.order(), s))
            .chain(self.tabbed().map(|s| (s.order, Scope::Offered(s.id))))
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

    /// The field says `Search` on All and Actions (#192), the tab lit over it saying what is searched; a scope offered
    /// says how it finds.
    fn scope_placeholder(&self, scope: Scope) -> &'static str {
        match scope {
            Scope::All | Scope::Actions => "Search",
            Scope::Offered(id) => self.offered_search(id).map_or("", |s| s.placeholder),
        }
    }

    /// Opened on scope `id`, moved to its tab, closed when it is the one open; or opened on list `id`.
    fn search(&mut self, id: &'static str, cx: &mut Context<Self>) {
        if self.offered_list(id).is_some() {
            return self.open_on(Kind::List(id), None, cx);
        }
        let scope = match id {
            search::ALL => Scope::All,
            search::ACTIONS => Scope::Actions,
            id if self.offered_search(id).is_some() => Scope::Offered(id),
            _ => return,
        };
        match self.open.as_mut() {
            Some(p) if matches!(p.kind, Kind::Search(s) if s == scope) => self.close(cx),
            Some(p) if matches!(p.kind, Kind::Search(_)) => {
                p.kind = Kind::Search(scope);
                p.from = None;
                p.at = 0;
                self.refresh(cx);
                cx.notify();
            }
            _ => self.open_on(Kind::Search(scope), None, cx),
        }
    }

    fn open_on(&mut self, kind: Kind, under: Option<Bounds<Pixels>>, cx: &mut Context<Self>) {
        self.edit = TextEdit::default();
        self.scroll.set(0.);
        self.follow.set(true);
        // What the offered scopes read once for their finds (Code's: the project's files), or the list its own (the
        // faces installed), before the first finds.
        let opened: Vec<Doing> = match kind {
            Kind::Search(_) => (self.tabbed()).filter_map(|s| s.opened.clone()).collect(),
            Kind::List(id) => (self.offered_list(id).and_then(|l| l.opened.clone()))
                .into_iter()
                .collect(),
        };
        cx.defer(move |cx| opened.iter().for_each(|opened| opened(cx)));
        self.open = Some(Open {
            kind,
            from: None,
            counts: None,
            query: String::new(),
            at: 0,
            items: Vec::new(),
            under,
        });
        // A list opens on the one in force (the theme on the window, the face in use).
        self.refresh_then(cx, |this, _| {
            if let Some(p) = &mut this.open
                && matches!(p.kind, Kind::List(_))
            {
                p.at = (p.items.iter())
                    .position(|i| matches!(i.pick, Pick::Found { current: true, .. }))
                    .unwrap_or(0);
            }
        });
        self.redraw(cx);
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        if let Some(p) = self.open.take() {
            // What a list tried is put back (a theme, the chosen one).
            if let Kind::List(id) = p.kind
                && let Some(closed) = self.offered_list(id).and_then(|l| l.closed.clone())
            {
                cx.defer(move |cx| closed(cx));
            }
            self.redraw(cx);
        }
    }

    /// The window and the tab under it drawn again: it opened or closed.
    fn redraw(&self, cx: &mut Context<Self>) {
        if let Some(frame) = self.frame.upgrade() {
            frame.redraw(cx);
        }
        cx.notify();
    }

    /// What the find under the selection tries (a theme on the whole window).
    fn try_found(&mut self, cx: &mut Context<Self>) {
        let tried = (self.open.as_ref()).and_then(|p| match &p.items.get(p.at)?.pick {
            Pick::Found { tried, .. } => tried.clone(),
            _ => None,
        });
        if let Some(tried) = tried {
            cx.defer(move |cx| tried(cx));
        }
    }

    /// Finds again what the window lists for its text.
    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.refresh_then(cx, |_, _| {});
    }

    /// Finds again what the window lists for its text, then `then` (the list starting on the one in force, what the
    /// selection tries) — once no entity is being changed, since a find may read the tab (its own lists read its
    /// places).
    fn refresh_then(
        &mut self,
        cx: &mut Context<Self>,
        then: impl FnOnce(&mut Self, &mut Context<Self>) + 'static,
    ) {
        let this = cx.weak_entity();
        cx.defer(move |cx| {
            let Some((items, counts)) = (this.upgrade()).and_then(|m| m.read(cx).found(cx)) else {
                return;
            };
            this.update(cx, |m, cx| {
                if let Some(p) = &mut m.open {
                    p.at = p.at.min(items.len().saturating_sub(1));
                    p.items = items;
                    p.counts = counts;
                }
                then(m, cx);
                m.follow.set(true);
                cx.notify();
            })
            .ok();
        });
    }

    /// What the window lists for its text, with every tab's count while something is typed; None while it is closed.
    fn found(&self, cx: &App) -> Option<(Vec<Item>, Option<Vec<usize>>)> {
        let p = self.open.as_ref()?;
        let items = match p.kind {
            Kind::Search(scope) => self.scope_items(scope, p, cx),
            Kind::List(id) => self.offered_list(id).map_or_else(Vec::new, |list| {
                let ask = Ask {
                    text: p.query.trim_start(),
                    all: false,
                };
                found_items((list.find)(ask, cx))
            }),
        };
        // Every tab's count, while something is typed; All's is what the others found (X56).
        let counts = match p.kind {
            Kind::Search(_) if !p.query.trim().is_empty() => {
                let scopes = self.scopes();
                let mut counts: Vec<usize> = (scopes.iter())
                    .map(|&scope| match scope {
                        Scope::All => 0,
                        _ => self.scope_items(scope, p, cx).len(),
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

    /// The next tab (or the one before), the text kept.
    fn step_scope(&mut self, back: bool, cx: &mut Context<Self>) {
        let Some(Kind::Search(scope)) = self.open.as_ref().map(|p| p.kind) else {
            return;
        };
        let scopes = self.scopes();
        let Some(p) = &mut self.open else {
            return;
        };
        let at = scopes.iter().position(|s| *s == scope).unwrap_or(0);
        let n = scopes.len();
        p.kind = Kind::Search(scopes[if back { (at + n - 1) % n } else { (at + 1) % n }]);
        p.from = None;
        p.at = 0;
        self.refresh(cx);
        cx.notify();
    }

    /// The commands offered that the palette's words match, under `group`: each in the words the tab says it in
    /// now (Copy link naming the page), else its own; under `Here` in its short words, the page being the one it
    /// acts on (#192).
    fn command_items(&self, group: &'static str, words: &[String], cx: &App) -> Vec<Item> {
        let frame = self.frame.upgrade();
        let here = group == HERE;
        (self.commands.iter().enumerate())
            .filter(|(_, c)| matches(words, c.words))
            .map(|(ix, c)| Item {
                group: group.into(),
                label: (c.here.filter(|_| here).map(str::to_owned))
                    .or_else(|| frame.as_ref().and_then(|f| f.command_words(c.id, cx)))
                    .unwrap_or_else(|| c.words.to_owned()),
                detail: String::new(),
                key: String::new(),
                icon: None,
                glyph: c.icon,
                dots: Vec::new(),
                in_folder: false,
                face: None,
                swatch: None,
                aside: None,
                pick: Pick::Command(ix),
            })
            .collect()
    }

    /// What tab `scope` lists for the palette's text.
    fn scope_items(&self, scope: Scope, p: &Open, cx: &App) -> Vec<Item> {
        let query = p.query.trim();
        let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        match scope {
            Scope::All => self.all_items(query, cx),
            Scope::Actions => self.command_items("Actions", &words, cx),
            Scope::Offered(id) => self.offered_search(id).map_or_else(Vec::new, |search| {
                let ask = Ask {
                    text: query,
                    all: false,
                };
                found_items((search.find)(ask, cx))
            }),
        }
    }

    /// The All tab with nothing typed (#192): `Here`, the page's commands with their keys (`Frame::here`; else ⌘K's
    /// first five), then `Recent`, the places last opened, each with its owner.
    fn here_items(&self, cx: &App) -> Vec<Item> {
        let ids = (self.frame.upgrade()).map_or_else(Vec::new, |f| f.here(cx));
        let mut items: Vec<Item> = if ids.is_empty() {
            self.command_items(HERE, &[], cx)
                .into_iter()
                .take(5)
                .collect()
        } else {
            let all = self.command_items(HERE, &[], cx);
            ids.iter()
                .filter_map(|id| {
                    let ix = self.commands.iter().position(|c| c.id == *id)?;
                    all.iter()
                        .find(|i| matches!(i.pick, Pick::Command(c) if c == ix))
                        .cloned()
                })
                .collect()
        };
        if let Some(recent) = self.offered_list(search::RECENT) {
            let ask = Ask {
                text: "",
                all: true,
            };
            items.extend(found_items((recent.find)(ask, cx)).into_iter().take(5));
        }
        items
    }

    /// The All tab: a few of every kind, what a scope All shows alone (a pasted link, one place), `#` `@` narrowing;
    /// nothing typed, `Here` (`here_items`).
    fn all_items(&self, query: &str, cx: &App) -> Vec<Item> {
        if query.is_empty() {
            return self.here_items(cx);
        }
        let whole = Ask {
            text: query,
            all: true,
        };
        for alone in self.searches.iter().filter(|s| s.all.alone) {
            let found = (alone.find)(whole, cx);
            if !found.is_empty() {
                return found_items(found);
            }
        }
        // `>` narrows it to the commands, an offered scope's own character to it (`@` the files, `#` the branches).
        let narrows = |c: char| c == '>' || self.tabbed().any(|s| s.all.prefix == Some(c));
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
        sections.extend(self.tabbed().map(|s| (s.all.order, Section::Offered(s))));
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
                    let ask = Ask {
                        text: rest,
                        all: true,
                    };
                    let few = cap(search.all.few);
                    items.extend(found_items((search.find)(ask, cx)).into_iter().take(few));
                }
                _ => {}
            }
        }
        // A command whose words match ranks above what shares its word (#192): the commands come first.
        items
    }

    fn key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(kind) = self.open.as_ref().map(|p| p.kind) else {
            return;
        };
        if event.keystroke.key == "tab" {
            match kind {
                Kind::Search(_) => return self.step_scope(event.keystroke.modifiers.shift, cx),
                // What ⇥ does in a list (Switch to: only what is not reviewed), found again from the first.
                Kind::List(id) => {
                    if let Some(tab) = self.offered_list(id).and_then(|l| l.tab.clone()) {
                        if let Some(p) = &mut self.open {
                            p.at = 0;
                        }
                        cx.defer(move |cx| tab(cx));
                        self.refresh(cx);
                        return cx.notify();
                    }
                }
            }
        }
        // ⌫ in the empty field goes back from Actions to the tab ">" left.
        if event.keystroke.key == "backspace"
            && let Some(p) = &mut self.open
            && p.query.is_empty()
            && let Some(from) = p.from.take()
        {
            p.kind = Kind::Search(from);
            self.refresh(cx);
            return cx.notify();
        }
        let Machine {
            open: Some(p),
            edit,
            ..
        } = self
        else {
            return;
        };
        match input::edit(&mut p.query, edit, &event.keystroke, cx) {
            Edit::Changed => {
                p.at = 0;
                // ">" typed first moves to Actions, shown as a chip in the field.
                if let Kind::Search(scope) = p.kind
                    && scope != Scope::Actions
                    && p.query.starts_with('>')
                {
                    p.query = p.query[1..].trim_start().to_owned();
                    p.kind = Kind::Search(Scope::Actions);
                    p.from = Some(scope);
                    *edit = TextEdit::default();
                    self.scroll.set(0.);
                }
                self.refresh_then(cx, |this, cx| this.try_found(cx));
            }
            Edit::Enter { .. } => {
                let at = p.at;
                return self.run(at, window, cx);
            }
            Edit::Escape => {
                // A second pick goes back to the first.
                let back = match p.kind {
                    Kind::List(id) => self.offered_list(id).and_then(|l| l.back),
                    _ => None,
                };
                match (back, &mut self.open) {
                    (Some(back), Some(p)) => {
                        p.kind = Kind::List(back);
                        p.query.clear();
                        self.refresh(cx);
                    }
                    _ => return self.close(cx),
                }
            }
            Edit::Selected | Edit::Ignored => {}
        }
        cx.notify();
    }

    fn step(&mut self, down: bool, cx: &mut Context<Self>) {
        if let Some(p) = &mut self.open
            && !p.items.is_empty()
        {
            let n = p.items.len();
            p.at = if down {
                (p.at + 1) % n
            } else {
                (p.at + n - 1) % n
            };
            self.follow.set(true);
            self.try_found(cx);
            cx.notify();
        }
    }

    /// Row `ix` picked: a route opened as the tab opens one, the owner's doing once ⌘K has closed, a command run.
    fn run(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = self.open.as_ref().and_then(|p| p.items.get(ix)).cloned() else {
            return;
        };
        self.open = None;
        match item.pick {
            Pick::Found {
                open: Opens::Route(route),
                ..
            } => {
                if let Some(frame) = self.frame.upgrade() {
                    frame.open(route, cx);
                }
            }
            Pick::Found {
                open: Opens::Run(run),
                ..
            } => cx.defer(move |cx| run(cx)),
            Pick::Command(ix) => {
                if let Some(command) = self.commands.get(ix) {
                    window.dispatch_action(command.act.boxed_clone(), cx);
                }
            }
        }
        self.redraw(cx);
    }

    /// Row `ix`'s word (a theme's Edit): the window closes, what was tried put back, then the word's doing.
    fn aside(&mut self, ix: usize, cx: &mut Context<Self>) {
        let doing = (self.open.as_ref()).and_then(|p| Some(p.items.get(ix)?.aside.clone()?.1));
        if let Some(doing) = doing {
            self.close(cx);
            cx.defer(move |cx| doing(cx));
        }
    }

    /// The field: its text with the caret and selection, a click placing the caret.
    fn field(&self, text: &str, placeholder: &str, cx: &Context<Self>) -> gpui::Div {
        let (shown, this) = (text.to_owned(), cx.weak_entity());
        let place: input::Place = Rc::new(move |at, shift, clicks, drag, lower, cx| {
            this.update(cx, |m, cx| {
                input::sync(&shown, &mut m.edit);
                m.edit.click(at, clicks, shift || drag);
                if clicks == 1 && !shift && !drag {
                    m.edit.set_downstream(lower);
                }
                cx.notify();
            })
            .ok();
        });
        input::field(text, true, placeholder, &Look::default(), || {
            input::text_body(
                text,
                &self.edit,
                Look::default(),
                self.scroll.clone(),
                Some(place),
            )
        })
    }
}

impl Render for Machine {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(p) = self.open.as_ref() else {
            return div().into_any_element();
        };
        // What a list offered says as it stands.
        let said = match p.kind {
            Kind::List(id) => self.offered_list(id).map(|l| (l.says)(cx)),
            _ => None,
        };
        let placeholder = match p.kind {
            Kind::Search(scope) => self.scope_placeholder(scope),
            Kind::List(_) => said.as_ref().map_or("", |s| s.placeholder.as_ref()),
        };
        let title = said.as_ref().and_then(|s| s.title.clone());
        let keys_now = artoo_shell_api::keys::get(cx);
        let rows = (p.items.iter())
            .map(|item| {
                // A command's key in force: one changed in Settings → Keyboard shows the new one.
                let key = match item.pick {
                    Pick::Command(ix) => (self.commands.get(ix).zip(keys_now.as_ref()))
                        .map(|(c, keys)| keys.now(c.act.as_ref(), cx))
                        .unwrap_or_default(),
                    _ => item.key.clone(),
                };
                Row {
                    group: item.group.clone(),
                    label: item.label.clone(),
                    detail: item.detail.clone(),
                    key,
                    icon: item.icon.clone(),
                    glyph: item.glyph,
                    dots: item.dots.clone(),
                    file: item.in_folder,
                    face: item.face.clone(),
                    swatch: item.swatch,
                    checked: matches!(item.pick, Pick::Found { current: true, .. }),
                    aside: item.aside.as_ref().map(|(word, _)| *word),
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
                    .map(|(i, scope)| Tab {
                        label: self.scope_label(scope),
                        count: p.counts.as_ref().and_then(|c| c.get(i).copied()),
                        lit: scope == on,
                    })
                    .collect();
                let go = self.scope_go(on);
                let key = (artoo_shell_api::actions(cx).iter())
                    .find(|a| a.id == go)
                    .zip(keys_now.as_ref())
                    .map(|(a, keys)| keys.foot(a.act.as_ref(), cx))
                    .unwrap_or_default();
                let foot = format!(
                    "{} · {}",
                    words::say(self.scope_label(on)),
                    keys::text(&key)
                );
                (Some(tabs), Some(foot))
            }
            Kind::List(_) => (None, None),
        };
        // Dropped from a button: as tall as the window lets it under the button (#204).
        let under = p.under.map(|b| {
            let room = window.viewport_size().height - b.bottom() - gpui::px(12.);
            (b, room.max(gpui::px(160.)))
        });
        // Its keys: ↑↓ tries the selection where a find tries one (a theme).
        let tries = (p.items.get(p.at))
            .is_some_and(|i| matches!(i.pick, Pick::Found { tried: Some(_), .. }));
        let hint = under.map(|_| match tries {
            true => "↑↓ preview · ↵ · esc",
            false => "↑↓ · ↵ · esc",
        });
        let follow = self.follow.replace(false);
        // A tab come to (⇥, a click, the window opened on another) is scrolled into sight, once.
        let on = (tabs.as_ref()).and_then(|t: &Vec<Tab>| t.iter().position(|t| t.lit));
        let to_tab = on.is_some() && self.tab_on.replace(on) != on;
        if to_tab {
            // Its edges fade by where the row was scrolled to, known once it is laid out: drawn once more.
            window.request_animation_frame();
        }
        palette::palette(
            PaletteView {
                title,
                tabs,
                actions: p.from.is_some(),
                field: self.field(&p.query, placeholder, cx).into_any_element(),
                rows,
                at: p.at,
                empty,
                foot,
                under,
                hint,
                scroll: (self.list.clone(), follow),
                tabs_scroll: (self.tabs.clone(), to_tab),
            },
            Rc::new(Glue(cx.weak_entity())),
            cx,
        )
    }
}

/// What a click on the window does.
struct Glue(WeakEntity<Machine>);

impl PaletteHost for Glue {
    fn run(&self, ix: usize, window: &mut Window, cx: &mut App) {
        self.0.update(cx, |m, cx| m.run(ix, window, cx)).ok();
    }

    fn close(&self, _: &mut Window, cx: &mut App) {
        self.0.update(cx, |m, cx| m.close(cx)).ok();
    }

    fn aside(&self, ix: usize, _: &mut Window, cx: &mut App) {
        self.0.update(cx, |m, cx| m.aside(ix, cx)).ok();
    }

    fn scope(&self, ix: usize, _: &mut Window, cx: &mut App) {
        self.0
            .update(cx, |m, cx| {
                if let Some(scope) = m.scopes().get(ix).copied() {
                    let id = match scope {
                        Scope::All => search::ALL,
                        Scope::Actions => search::ACTIONS,
                        Scope::Offered(id) => id,
                    };
                    m.search(id, cx);
                }
            })
            .ok();
    }
}
