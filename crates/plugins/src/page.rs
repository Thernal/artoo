//! The plugins' page (#176, #181; Design's handoff "plugins, a page of their own", P1–P7), reached from the row at the
//! foot of the spaces' menu, the same from every project. Its bar has the tabs Discover · Installed · Local with their
//! counts and the sort; a search heads the list and filters all three tabs at once; the selected plugin's page is
//! beside it: its facts in one row, its one action, its README. One bar over the page says when Artoo must restart.
//!
//! Words (P9): facts stay — stars, downloads, version, licence, who wrote it, a row's state in a word —; a sentence
//! that warns comes only at the decision, the trust question Install opens.

use crate::load::{Origin, Plugin, State};
use crate::market::{self, Build, Detail, Done, Found, Market, Offer, Read};
use crate::state::{self, Loaded};
use artoo_core::git::WebRemote;
use artoo_core::route::Route;
use artoo_shell_api::bar::BarAction;
use artoo_shell_api::search::{AllOf, Ask, Find, Opens, Search};
use artoo_shell_api::settings::Rows;
use artoo_shell_api::{
    BarParts, Frame, Input, Link, ListKeys, MenuItem, Pages, Parts, Screen, Snap, Typed, input_box,
};
use artoo_ui_kit::controls::{
    bar_button, chip, empty_state, group, outline_button, skeleton, switch, tag,
};
use artoo_ui_kit::hover::{self, Hover};
use artoo_ui_kit::input::{self, Area, Edit};
use artoo_ui_kit::monogram::{hue, monogram};
use artoo_ui_kit::{ROW_RADIUS, icons, keys, markdown, px, theme};
use gpui::{
    AnyElement, App, AppContext, ClickEvent, Context, Entity, FontWeight, IntoElement, MouseButton,
    MouseDownEvent, Render, Rgba, ScrollHandle, SharedString, Window, div, prelude::*,
};
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// The page's space, by its id: the row at the menu's foot (`artoo_shell_api::spaces::Foot`).
pub(crate) const SPACE: &str = "plugins";
/// The action that goes to it: `Browse plugins` in ⌘K.
pub(crate) const GO: &str = "plugins.open";
/// The search's field, as a part's input.
const SEARCH: &str = "plugins.search";
/// The trust question's words (P5): at the decision, never over the list.
const RIGHTS: &str = "Runs with all of Artoo's rights, over your files and accounts.";
/// This Artoo's version, what a build must be for.
const ARTOO: &str = env!("CARGO_PKG_VERSION");

gpui::actions!(plugins, [OpenPlugins, FindPlugins]);

/// The page's tabs.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum Tab {
    #[default]
    Discover,
    Installed,
    Local,
}

impl Tab {
    const ALL: [Tab; 3] = [Tab::Discover, Tab::Installed, Tab::Local];

    fn label(self) -> &'static str {
        match self {
            Tab::Discover => "Discover",
            Tab::Installed => "Installed",
            Tab::Local => "Local",
        }
    }
}

/// How the list is ordered.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum Sort {
    #[default]
    Stars,
    Updated,
    Name,
}

impl Sort {
    const ALL: [Sort; 3] = [Sort::Stars, Sort::Updated, Sort::Name];

    fn label(self) -> &'static str {
        match self {
            Sort::Stars => "Most stars",
            Sort::Updated => "Recently updated",
            Sort::Name => "Name",
        }
    }
}

/// A row of the list, by what it is.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum Key {
    /// A repository the search found, `owner/name`.
    Offer(String),
    /// A plugin found as Artoo started, by its file.
    Plugin(PathBuf),
    /// An entry of the local list added since Artoo started.
    Added(PathBuf),
}

/// A row: what it shows and what it stands for.
#[derive(Clone)]
pub(crate) struct Item {
    pub key: Key,
    pub tab: Tab,
    /// What its mark's hue is taken from.
    pub id: String,
    pub name: String,
    /// Who wrote it, quiet beside its name.
    pub author: String,
    /// What it does; a local plugin's folder.
    pub about: String,
    pub folder: bool,
    pub stars: Option<u64>,
    pub offer: Option<Offer>,
    pub plugin: Option<Plugin>,
}

/// `path` with the home folder as `~`.
fn tilde(path: &Path) -> String {
    let text = path.display().to_string();
    match std::env::var("HOME") {
        Ok(home) if !home.is_empty() && text.starts_with(&home) => {
            format!("~{}", &text[home.len()..])
        }
        _ => text,
    }
}

/// The id of `repo`'s build for this Artoo, once its releases are read.
fn built_id(market: &Market, repo: &str) -> Option<String> {
    match market.details.get(repo) {
        Some(Read::Got(Detail {
            build: Build::Ready { id, .. },
            ..
        })) => Some(id.clone()),
        _ => None,
    }
}

/// The plugin installed from `offer`'s repository, as Artoo started.
fn installed_from<'a>(loaded: &'a Loaded, offer: &Offer) -> Option<&'a Plugin> {
    (loaded.plugins.iter()).filter(|p| !p.dev()).find(|p| {
        (p.stamp.as_ref()).is_some_and(|s| market::same_repository(&s.repository, &offer.url))
    })
}

/// Every row of the three tabs.
pub(crate) fn items(cx: &App) -> Vec<Item> {
    let (loaded, market) = (cx.global::<Loaded>(), cx.global::<Market>());
    let offers: &[Offer] = match &market.found {
        Found::Got(offers) => offers,
        _ => &[],
    };
    let offer_of = |repository: &str| {
        let repo = market::github_repo(repository)?;
        offers
            .iter()
            .find(|o| o.repo.eq_ignore_ascii_case(&repo))
            .cloned()
    };
    let mut out: Vec<Item> = (offers.iter())
        .map(|o| Item {
            key: Key::Offer(o.repo.clone()),
            tab: Tab::Discover,
            id: o.repo.clone(),
            name: o.name.clone(),
            author: o.owner.clone(),
            about: o.about.clone(),
            folder: false,
            stars: Some(o.stars),
            offer: Some(o.clone()),
            plugin: None,
        })
        .collect();
    for p in &loaded.plugins {
        let stamp = p.stamp.as_ref();
        let repository = stamp.map(|s| s.repository.as_str()).unwrap_or("");
        let offer = offer_of(repository);
        let local = p.dev();
        let author = (market::github_repo(repository))
            .and_then(|r| r.split('/').next().map(str::to_owned))
            .unwrap_or_default();
        let folder = match &p.origin {
            Origin::Local(entry) => tilde(entry),
            _ => tilde(&p.path),
        };
        out.push(Item {
            key: Key::Plugin(p.path.clone()),
            tab: if local { Tab::Local } else { Tab::Installed },
            id: p.id(),
            name: stamp.map_or_else(|| p.id(), |s| s.name.clone()),
            author,
            about: match (&offer, local) {
                (_, true) => folder,
                (Some(o), false) => o.about.clone(),
                (None, false) => stamp.map(|s| s.version.clone()).unwrap_or_default(),
            },
            folder: local,
            stars: offer.as_ref().map(|o| o.stars),
            offer,
            plugin: Some(p.clone()),
        });
    }
    // Installed this run, loaded at the next start: on Installed already, `Restart to finish`.
    for o in offers {
        if let Some(id) = built_id(market, &o.repo)
            && matches!(
                market.done.get(&id),
                Some(Done::Installed | Done::Installing)
            )
            && installed_from(loaded, o).is_none()
        {
            out.push(Item {
                tab: Tab::Installed,
                ..out
                    .iter()
                    .find(|i| i.key == Key::Offer(o.repo.clone()))
                    .cloned()
                    .expect("offered")
            });
        }
    }
    // Added to the local list since Artoo started.
    for entry in &loaded.local {
        let loaded_now =
            (loaded.plugins.iter()).any(|p| matches!(&p.origin, Origin::Local(e) if e == entry));
        if !loaded_now {
            let name = (entry.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            out.push(Item {
                key: Key::Added(entry.clone()),
                tab: Tab::Local,
                id: name.clone(),
                name,
                author: String::new(),
                about: tilde(entry),
                folder: true,
                stars: None,
                offer: None,
                plugin: None,
            });
        }
    }
    out
}

/// Whether every word typed is in the row's name, what it does or who wrote it.
fn matches(item: &Item, typed: &str) -> bool {
    let hay = format!("{} {} {}", item.name, item.about, item.author).to_lowercase();
    typed
        .split_whitespace()
        .all(|w| hay.contains(&w.to_lowercase()))
}

/// What a row's right edge holds (P3).
#[derive(Clone, PartialEq, Debug)]
enum Act {
    Install,
    /// Installing, or its build being read first.
    Progress,
    /// It failed, and why.
    TryAgain(String),
    Restart,
    Update(String),
    NoBuild,
    Installed,
    /// The Installed tab's switch, with the state a word says beside it.
    Switch(bool, Option<&'static str>),
    /// Not loaded, the reason on its page.
    NotLoaded,
    Nothing,
}

fn act_of(item: &Item, cx: &App) -> Act {
    let (loaded, market) = (cx.global::<Loaded>(), cx.global::<Market>());
    let done_act = |id: &str| match market.done.get(id) {
        Some(Done::Installing) => Some(Act::Progress),
        Some(Done::Installed | Done::Removed) => Some(Act::Restart),
        Some(Done::Failed(why)) => Some(Act::TryAgain(why.clone())),
        None => None,
    };
    if let Some(p) = &item.plugin {
        if item.tab == Tab::Local {
            return Act::Nothing;
        }
        let id = p.id();
        if let Some(act) = done_act(&id) {
            return act;
        }
        if let Some(update) = market.updates.get(&id) {
            return Act::Update(update.version.clone());
        }
        let on = !loaded.off.contains(&id);
        return match &p.state {
            State::Refused(_) => Act::NotLoaded,
            State::Struck if !on => Act::Switch(false, Some("Ended Artoo twice")),
            _ => Act::Switch(on, None),
        };
    }
    let Some(offer) = &item.offer else {
        return Act::Nothing;
    };
    if let Some(p) = installed_from(loaded, offer) {
        let id = p.id();
        return done_act(&id).unwrap_or_else(|| match market.updates.get(&id) {
            Some(update) => Act::Update(update.version.clone()),
            None => Act::Installed,
        });
    }
    if market.starting.contains(&offer.repo) {
        return Act::Progress;
    }
    match market.details.get(&offer.repo) {
        Some(Read::Got(d)) => match &d.build {
            Build::Ready { id, .. } => done_act(id).unwrap_or(Act::Install),
            _ => Act::NoBuild,
        },
        _ => Act::Install,
    }
}

/// The page: what is typed, the tab, the sort, the row picked.
pub(crate) struct Page {
    frame: Rc<dyn Frame>,
    pub(crate) tab: Tab,
    sort: Sort,
    text: String,
    area: Area,
    picked: Option<Key>,
    /// Shown once this run: the search is open from then on, and the tab was chosen.
    shown: Rc<std::cell::Cell<bool>>,
    /// The trust question floats over the list, under its row, rather than on the plugin's page.
    from_row: bool,
    scroll: ScrollHandle,
}

impl Page {
    fn new(frame: Rc<dyn Frame>, cx: &mut Context<Self>) -> Self {
        cx.observe_global::<Loaded>(|_, cx| cx.notify()).detach();
        cx.observe_global::<Market>(|_, cx| cx.notify()).detach();
        Self {
            frame,
            tab: Tab::default(),
            sort: Sort::default(),
            text: String::new(),
            area: Area::default(),
            picked: None,
            shown: Rc::default(),
            from_row: false,
            scroll: ScrollHandle::new(),
        }
    }

    fn typed(&self) -> bool {
        !self.text.trim().is_empty()
    }

    /// The rows listed now: the tab's, or every tab's that match what is typed; in the sort's order.
    fn listed(&self, cx: &App) -> Vec<Item> {
        let all = items(cx);
        let mut list: Vec<Item> = if self.typed() {
            // One installed from the market is its Installed row, not its Discover one too.
            let covered: Vec<String> = (all.iter())
                .filter(|i| i.tab != Tab::Discover)
                .filter_map(|i| i.offer.as_ref().map(|o| o.repo.clone()))
                .collect();
            let mut seen: Vec<Key> = Vec::new();
            (all.into_iter())
                .filter(|i| matches(i, &self.text))
                .filter(|i| {
                    i.tab != Tab::Discover
                        || !i.offer.as_ref().is_some_and(|o| covered.contains(&o.repo))
                })
                .filter(|i| {
                    let fresh = !seen.contains(&i.key);
                    seen.push(i.key.clone());
                    fresh
                })
                .collect()
        } else {
            all.into_iter().filter(|i| i.tab == self.tab).collect()
        };
        if self.tab == Tab::Local && !self.typed() {
            return list;
        }
        match self.sort {
            Sort::Stars => list.sort_by_key(|i| std::cmp::Reverse(i.stars.unwrap_or(0))),
            Sort::Updated => {
                list.sort_by_key(|i| std::cmp::Reverse(i.offer.as_ref().map_or(0, |o| o.pushed)))
            }
            Sort::Name => list.sort_by_key(|i| i.name.to_lowercase()),
        }
        // Updates first on Installed (P10).
        if self.tab == Tab::Installed && !self.typed() {
            list.sort_by_key(|i| !matches!(act_of(i, cx), Act::Update(_)));
        }
        list
    }

    /// Each tab's count: its rows, or its matches while words are typed.
    fn counts(&self, cx: &App) -> [usize; 3] {
        let all = items(cx);
        let typed = self.typed();
        Tab::ALL.map(|tab| {
            let mut keys: Vec<&Key> = Vec::new();
            for i in all
                .iter()
                .filter(|i| i.tab == tab && (!typed || matches(i, &self.text)))
            {
                if !keys.contains(&&i.key) {
                    keys.push(&i.key);
                }
            }
            keys.len()
        })
    }

    /// The row picked, the first when none is or it is gone.
    fn picked_of<'a>(&self, list: &'a [Item]) -> Option<&'a Item> {
        (self.picked.as_ref())
            .and_then(|k| list.iter().find(|i| i.key == *k))
            .or_else(|| list.first())
    }

    /// The page on screen; `fresh` when it was just gone to. The first time, it opens on Installed when updates
    /// wait (P10), and GitHub is searched.
    fn show(&mut self, fresh: bool, cx: &mut Context<Self>) {
        if !self.shown.get() {
            self.shown.set(true);
            if market::waiting(cx) > 0 {
                self.tab = Tab::Installed;
            }
        }
        market::opened(cx);
        if fresh {
            self.frame.take_keyboard(SEARCH, cx);
        }
        self.read_picked(cx);
        cx.notify();
    }

    /// The picked row's releases and README read, once.
    fn read_picked(&mut self, cx: &mut Context<Self>) {
        let list = self.listed(cx);
        let repo = self.picked_of(&list).and_then(|i| {
            (i.offer.as_ref().map(|o| o.repo.clone())).or_else(|| {
                (i.plugin.as_ref()?.stamp.as_ref()).and_then(|s| market::github_repo(&s.repository))
            })
        });
        if let Some(repo) = repo {
            market::read(&repo, cx);
        }
    }

    fn pick(&mut self, key: Key, cx: &mut Context<Self>) {
        if self.picked.as_ref() != Some(&key) {
            let market = cx.global_mut::<Market>();
            market.question = None;
        }
        self.picked = Some(key);
        self.read_picked(cx);
        cx.notify();
    }

    fn set_tab(&mut self, tab: Tab, cx: &mut Context<Self>) {
        self.tab = tab;
        self.text.clear();
        self.area = Area::default();
        self.picked = None;
        self.read_picked(cx);
        cx.notify();
    }

    fn set_text(&mut self, text: String, cx: &mut Context<Self>) {
        self.text = text;
        self.picked = None;
        self.read_picked(cx);
        self.scroll.scroll_to_item(0);
        cx.notify();
    }

    /// ↑ (`false`) or ↓: the next row picked, kept in view.
    fn step(&mut self, down: bool, cx: &mut Context<Self>) {
        let list = self.listed(cx);
        if list.is_empty() {
            return;
        }
        let at = (self.picked.as_ref())
            .and_then(|k| list.iter().position(|i| i.key == *k))
            .unwrap_or(0);
        let next = if down {
            (at + 1).min(list.len() - 1)
        } else {
            at.saturating_sub(1)
        };
        self.scroll.scroll_to_item(next);
        self.pick(list[next].key.clone(), cx);
    }

    /// ↵: the question's Install while it is up, else the picked row's action.
    fn enter(&mut self, cx: &mut Context<Self>) {
        let list = self.listed(cx);
        let Some(item) = self.picked_of(&list).cloned() else {
            return;
        };
        let asked = cx.global::<Market>().question.clone();
        if let (Some(repo), Some(offer)) = (asked, &item.offer)
            && repo == offer.repo
        {
            market::install_offer(offer, cx);
            return;
        }
        self.act(&item, false, cx);
    }

    /// Esc in the search: the question goes, then what is typed; the field lets the keyboard go once both are.
    fn escape(&mut self, cx: &mut Context<Self>) -> Typed {
        if cx.global::<Market>().question.is_some() {
            cx.global_mut::<Market>().question = None;
            return Typed::Kept;
        }
        if !self.text.is_empty() {
            self.set_text(String::new(), cx);
            self.area = Area::default();
            return Typed::Kept;
        }
        Typed::Leave
    }

    fn key(&mut self, event: &gpui::KeyDownEvent, cx: &mut Context<Self>) -> Typed {
        let mut text = self.text.clone();
        match input::edit(&mut text, &mut self.area.edit, &event.keystroke, cx) {
            Edit::Changed => {
                self.set_text(text, cx);
                Typed::Kept
            }
            Edit::Selected => {
                cx.notify();
                Typed::Kept
            }
            Edit::Enter { .. } => {
                self.enter(cx);
                Typed::Kept
            }
            Edit::Escape => self.escape(cx),
            Edit::Ignored => Typed::Kept,
        }
    }

    /// What a row's action does (P5): Install asks the first time from an author; an update never asks.
    fn act(&mut self, item: &Item, from_row: bool, cx: &mut Context<Self>) {
        match act_of(item, cx) {
            Act::Install | Act::TryAgain(_) => {
                let Some(offer) = &item.offer else {
                    return;
                };
                if cx
                    .global::<Market>()
                    .trusted
                    .contains(&offer.owner.to_lowercase())
                {
                    market::install_offer(offer, cx);
                } else {
                    self.from_row = from_row;
                    self.picked = Some(item.key.clone());
                    market::read(&offer.repo, cx);
                    cx.global_mut::<Market>().question = Some(offer.repo.clone());
                }
            }
            Act::Update(_) => update(item, cx),
            Act::Switch(..) => {
                if let Some(p) = &item.plugin {
                    state::flip(&p.id(), cx);
                }
            }
            _ => {}
        }
        cx.notify();
    }

    /// A row's right click: Install or Update, Turn off, Remove, Open repository.
    fn row_menu(&mut self, item: &Item, at: gpui::Point<gpui::Pixels>, cx: &mut Context<Self>) {
        let this = cx.weak_entity();
        let act = act_of(item, cx);
        let mut first = Vec::new();
        let on = |item: &Item, f: fn(&mut Page, &Item, &mut Context<Page>)| {
            let (this, item) = (this.clone(), item.clone());
            move |_: &mut Window, cx: &mut App| {
                this.update(cx, |p, cx| f(p, &item, cx)).ok();
            }
        };
        match act {
            Act::Install | Act::TryAgain(_) => first.push(MenuItem::new(
                "Install",
                on(item, |p, i, cx| p.act(i, true, cx)),
            )),
            Act::Update(_) => first.push(MenuItem::new(
                "Update",
                on(item, |p, i, cx| p.act(i, true, cx)),
            )),
            _ => {}
        }
        if let Some(p) = &item.plugin
            && p.stamp.is_some()
            && item.tab == Tab::Installed
        {
            let off = cx.global::<Loaded>().off.contains(&p.id());
            first.push(MenuItem::new(
                if off { "Turn on" } else { "Turn off" },
                on(item, |_, i, cx| {
                    if let Some(p) = &i.plugin {
                        state::flip(&p.id(), cx);
                    }
                }),
            ));
            first.push(MenuItem::new("Remove", on(item, |_, i, cx| remove(i, cx))));
        }
        let mut groups = vec![first];
        if let Some(url) = repository_url(item) {
            groups.push(vec![MenuItem::new("Open repository", move |_, cx| {
                cx.open_url(&url)
            })]);
        }
        groups.retain(|g| !g.is_empty());
        if !groups.is_empty() {
            self.frame.menu(at, groups, cx);
        }
    }

    fn sort_menu(&mut self, at: gpui::Point<gpui::Pixels>, cx: &mut Context<Self>) {
        let this = cx.weak_entity();
        let items = Sort::ALL
            .into_iter()
            .map(|sort| {
                let this = this.clone();
                MenuItem::new(sort.label(), move |_, cx| {
                    this.update(cx, |p, cx| {
                        p.sort = sort;
                        cx.notify();
                    })
                    .ok();
                })
                .checked(self.sort == sort)
            })
            .collect();
        self.frame.menu(at, vec![items], cx);
    }
}

/// An installed plugin's update installed.
fn update(item: &Item, cx: &mut App) {
    let market = cx.global::<Market>();
    let id = match (&item.plugin, &item.offer) {
        (Some(p), _) => Some(p.id()),
        (None, Some(o)) => installed_from(cx.global::<Loaded>(), o).map(Plugin::id),
        _ => None,
    };
    let Some((id, zip)) = id.and_then(|id| market.updates.get(&id).map(|u| (id, u.zip.clone())))
    else {
        return;
    };
    market::install(id, zip, cx);
}

/// An installed plugin removed at the next start; a local one off the list.
fn remove(item: &Item, cx: &mut App) {
    let Some(p) = &item.plugin else {
        if let Key::Added(entry) = &item.key {
            state::remove_local(entry, cx);
        }
        return;
    };
    match &p.origin {
        Origin::Local(entry) => state::remove_local(entry, cx),
        Origin::Installed => {
            let folder = (p.path.parent())
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned());
            if let Some(id) = folder {
                market::remove(&id, cx);
            }
        }
        Origin::Env => {}
    }
}

/// Where a row's repository is on GitHub.
fn repository_url(item: &Item) -> Option<String> {
    (item.offer.as_ref().map(|o| o.url.clone())).or_else(|| {
        let s = item.plugin.as_ref()?.stamp.as_ref()?;
        (!s.repository.is_empty()).then(|| s.repository.clone())
    })
}

/// `1.2k` for 1204 stars; the number itself under a thousand.
fn short_count(n: u64) -> String {
    match n {
        0..1000 => n.to_string(),
        1000..10_000 => {
            let tenths = (n + 50) / 100;
            if tenths.is_multiple_of(10) {
                format!("{}k", tenths / 10)
            } else {
                format!("{}.{}k", tenths / 10, tenths % 10)
            }
        }
        _ => format!("{}k", (n + 500) / 1000),
    }
}

/// `1,204`.
fn grouped(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// How long ago, as a version's age says it: `today`, `yesterday`, `3 days ago`, `2 weeks ago`, `5 months ago`.
fn age(time: i64) -> String {
    let days = (artoo_ui_kit::format::now() - time).max(0) / 86_400;
    let plural = |n: i64, unit: &str| format!("{n} {unit}{} ago", if n == 1 { "" } else { "s" });
    match days {
        0 => "today".to_owned(),
        1 => "yesterday".to_owned(),
        2..14 => plural(days, "day"),
        14..60 => plural(days / 7, "week"),
        60..730 => plural(days / 30, "month"),
        _ => plural(days / 365, "year"),
    }
}

/// A row's mark: the first letter of its name on a hue taken from its id (P10).
fn mark(item: &Item, size: f32) -> gpui::Div {
    let letter: String = item
        .name
        .chars()
        .next()
        .map(|c| c.to_uppercase().collect())
        .unwrap_or_default();
    monogram(&letter, hue(&item.id), size)
}

fn faint(text: impl Into<SharedString>) -> gpui::Div {
    div()
        .text_size(px(12.))
        .text_color(theme::faint())
        .child(text.into())
}

/// A word in a colour, a row's state (`Installed`, `No build yet`).
fn word(text: &'static str, color: Rgba) -> gpui::Div {
    div().text_size(px(11.)).text_color(color).child(text)
}

/// A thin bar in a button's place while it installs.
fn progress() -> AnyElement {
    artoo_ui_kit::motion::breathe(
        "plugin-progress",
        div()
            .w(px(56.))
            .h(px(4.))
            .rounded_full()
            .bg(theme::hover())
            .child(div().w(px(28.)).h_full().rounded_full().bg(theme::accent())),
    )
}

/// The small button of a row's action, 72 px wide.
fn small_button(
    id: impl Into<gpui::ElementId>,
    label: &str,
    accent: bool,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .w(px(72.))
        .h(px(22.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(ROW_RADIUS))
        .border_1()
        .border_color(theme::island_border())
        .text_size(px(12.))
        .text_color(if accent {
            theme::accent()
        } else {
            theme::text()
        })
        .cursor_pointer()
        .hover(|s| s.bg(theme::hover()))
        .child(label.to_owned())
}

impl Page {
    /// A row (P3): mark, name and author, what it does; at the right, the stars over the action.
    fn row(&self, n: usize, item: &Item, picked: bool, cx: &mut Context<Self>) -> AnyElement {
        let act = act_of(item, cx);
        let dim = matches!(act, Act::Switch(false, _));
        let (clicked, menu, acted) = (item.clone(), item.clone(), item.clone());
        let action: Option<AnyElement> = match &act {
            Act::Install => Some(
                small_button(("plugin-install", n), "Install", false)
                    .on_click(cx.listener(move |p, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        p.act(&acted, true, cx)
                    }))
                    .into_any_element(),
            ),
            Act::TryAgain(why) => Some(
                small_button(("plugin-again", n), "Try again", false)
                    .tip(format!("Not installed: {why}"))
                    .on_click(cx.listener(move |p, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        p.act(&acted, true, cx)
                    }))
                    .into_any_element(),
            ),
            Act::Update(version) => Some(
                small_button(("plugin-update", n), "Update", true)
                    .tip(format!("Update to {version}"))
                    .on_click(cx.listener(move |p, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        p.act(&acted, true, cx)
                    }))
                    .into_any_element(),
            ),
            Act::Progress => Some(progress()),
            Act::Restart => Some(word("Restart to finish", theme::added()).into_any_element()),
            Act::NoBuild => Some(word("No build yet", theme::warning()).into_any_element()),
            Act::Installed => Some(word("Installed", theme::added()).into_any_element()),
            Act::NotLoaded => Some(word("Not loaded", theme::warning()).into_any_element()),
            Act::Switch(on, said) => Some(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .children(said.map(|w| word(w, theme::warning())))
                    .child(
                        div()
                            .id(("plugin-switch", n))
                            .child(switch("plugin-on", *on))
                            .on_click(cx.listener(move |p, _: &ClickEvent, _, cx| {
                                cx.stop_propagation();
                                p.act(&acted, true, cx)
                            })),
                    )
                    .into_any_element(),
            ),
            Act::Nothing => None,
        };
        let stars = item.stars.map(|n| {
            div()
                .id(("plugin-stars", n as usize))
                .flex()
                .items_center()
                .gap(px(3.))
                .text_size(px(12.))
                .text_color(theme::muted())
                .child(icons::icon("star").size(px(12.)).text_color(theme::muted()))
                .child(short_count(n))
                .tip(format!("{} stars on GitHub", grouped(n)))
        });
        let question = (self.from_row)
            .then(|| cx.global::<Market>().question.clone())
            .flatten()
            .filter(|repo| item.offer.as_ref().is_some_and(|o| o.repo == *repo) && picked)
            .map(|_| self.question(item, cx));
        let tip = if item.author.is_empty() {
            item.name.clone()
        } else {
            format!("{} by {}", item.name, item.author)
        };
        div()
            .relative()
            .child(
                div()
                    .id(("plugin-row", n))
                    .mx(px(6.))
                    .px(px(8.))
                    .py(px(9.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .rounded(px(ROW_RADIUS))
                    .cursor_pointer()
                    .when(picked, |s| s.bg(theme::selected()))
                    .when(!picked, |s| s.hover(|s| s.bg(theme::hover())))
                    .when(dim, |s| s.opacity(0.55))
                    .child(mark(item, 32.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(
                                div()
                                    .flex()
                                    .items_baseline()
                                    .gap(px(6.))
                                    .min_w_0()
                                    .child(
                                        div()
                                            .flex_none()
                                            .text_size(px(13.))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(theme::text())
                                            .child(item.name.clone()),
                                    )
                                    .when(!item.author.is_empty(), |s| {
                                        s.child(
                                            hover::cut(
                                                ("plugin-row-author", n),
                                                item.author.clone(),
                                            )
                                            .min_w_0()
                                            .text_size(px(11.))
                                            .text_color(theme::faint()),
                                        )
                                    })
                                    .when(
                                        matches!(&item.plugin, Some(p) if p.origin == Origin::Env),
                                        |s| s.child(tag("dev", theme::agent())),
                                    ),
                            )
                            .when(!item.about.is_empty(), |s| {
                                s.child(
                                    hover::cut(("plugin-row-about", n), item.about.clone())
                                        .text_size(px(12.))
                                        .text_color(theme::muted())
                                        .when(item.folder, |s| s.font_family(theme::code_font())),
                                )
                            }),
                    )
                    .when(stars.is_some() || action.is_some(), |s| {
                        s.child(
                            div()
                                .flex_none()
                                .w(px(72.))
                                .flex()
                                .flex_col()
                                .items_end()
                                .gap(px(4.))
                                .children(stars)
                                .children(action),
                        )
                    })
                    .tip(tip)
                    .on_click(
                        cx.listener(move |p, _: &ClickEvent, _, cx| {
                            p.pick(clicked.key.clone(), cx)
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |p, e: &MouseDownEvent, _, cx| {
                            p.row_menu(&menu, e.position, cx)
                        }),
                    ),
            )
            // The question floats over the list, under its row (P5).
            // Deferred: the rows after it are drawn later and would cover it.
            .children(question.map(|q| {
                gpui::deferred(
                    div()
                        .absolute()
                        .top_full()
                        .right(px(14.))
                        .w(px(300.))
                        .child(q),
                )
                .with_priority(1)
            }))
            .into_any_element()
    }

    /// The trust question (P5): asked once an author, beside the Install that opened it.
    fn question(&self, item: &Item, cx: &mut Context<Self>) -> AnyElement {
        let offer = item.offer.clone();
        div()
            .id("plugin-question")
            .occlude()
            .p(px(12.))
            .flex()
            .flex_col()
            .gap(px(10.))
            .rounded(px(8.))
            .border_1()
            .border_color(theme::island_border())
            .bg(theme::panel())
            .shadow_lg()
            .child(
                div()
                    .text_size(px(13.))
                    .text_color(theme::text())
                    .child(RIGHTS),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(
                        bar_button("plugin-question-cancel", "Cancel", "esc", false).on_click(
                            cx.listener(|_, _: &ClickEvent, _, cx| {
                                cx.global_mut::<Market>().question = None;
                                cx.notify();
                            }),
                        ),
                    )
                    .child(
                        bar_button("plugin-question-install", "Install", "↵", true).on_click(
                            cx.listener(move |_, _: &ClickEvent, _, cx| {
                                if let Some(offer) = &offer {
                                    market::install_offer(offer, cx);
                                }
                                cx.notify();
                            }),
                        ),
                    ),
            )
            .into_any_element()
    }

    /// The search's field, heading the list (P2).
    fn field(&self, cx: &mut Context<Self>) -> AnyElement {
        let active = self.frame.typing(cx) == Some(SEARCH);
        let this = cx.weak_entity();
        let field =
            div()
                .id("plugins-search")
                .flex_none()
                .flex()
                .items_center()
                .gap_2()
                .mx(px(6.))
                .mt(px(6.))
                .mb(px(4.))
                .px_2()
                .h(px(32.))
                .rounded(px(ROW_RADIUS))
                .border_2()
                .border_color(if active {
                    theme::focus()
                } else {
                    theme::island_border()
                })
                .bg(theme::base())
                .cursor_text()
                .child(
                    icons::icon("search")
                        .size(px(14.))
                        .text_color(theme::faint()),
                )
                .child(self.area.field_of(
                    &self.text,
                    active,
                    "Search plugins",
                    false,
                    |p: &mut Page| Some(&mut p.area),
                    this,
                ))
                .child(div().text_size(px(11.)).text_color(theme::faint()).child(
                    keys::text(if self.text.is_empty() { "⌘F" } else { "esc" }).into_owned(),
                ));
        input_box(field, SEARCH, &self.frame).into_any_element()
    }

    /// The list under the search, or what stands in its place (P2's states).
    fn list(&self, list: &[Item], cx: &mut Context<Self>) -> AnyElement {
        let market = cx.global::<Market>();
        let discover = !self.typed() && self.tab == Tab::Discover;
        let body: AnyElement = match &market.found {
            Found::NotAsked | Found::Asking if discover => div()
                .px(px(16.))
                .pt(px(10.))
                .child(skeleton("plugins-loading", 6))
                .into_any_element(),
            Found::Failed(why) if discover => empty_state(
                "alert",
                "GitHub can't be reached".to_owned(),
                String::new(),
                Some(
                    outline_button("Try again")
                        .id("plugins-again")
                        .tip(why.clone())
                        .on_click(|_: &ClickEvent, _, cx| market::ask(cx))
                        .into_any_element(),
                ),
            )
            .into_any_element(),
            _ if list.is_empty() && self.typed() => empty_state(
                "search",
                format!("No plugin matches \u{201c}{}\u{201d}", self.text.trim()),
                String::new(),
                Some(
                    outline_button("Clear search")
                        .id("plugins-clear")
                        .on_click(cx.listener(|p, _: &ClickEvent, _, cx| {
                            p.area = Area::default();
                            p.set_text(String::new(), cx)
                        }))
                        .into_any_element(),
                ),
            )
            .into_any_element(),
            _ if list.is_empty() => {
                empty_state("puzzle", "Nothing here".to_owned(), String::new(), None)
                    .into_any_element()
            }
            _ => {
                let picked = self.picked_of(list).map(|i| i.key.clone());
                let rows: Vec<AnyElement> = (list.iter().enumerate())
                    .map(|(n, item)| self.row(n, item, picked.as_ref() == Some(&item.key), cx))
                    .collect();
                div()
                    .id("plugins-rows")
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .pb(px(6.))
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .children(rows)
                    .into_any_element()
            }
        };
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(self.field(cx))
            .child(body)
            .into_any_element()
    }

    /// One labelled fact of a plugin's page.
    fn fact(label: &'static str, value: AnyElement, last: bool) -> AnyElement {
        div()
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(4.))
            .pr(px(16.))
            .when(!last, |s| {
                s.mr(px(16.))
                    .border_r_1()
                    .border_color(theme::island_border())
            })
            .child(
                div()
                    .text_size(px(10.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme::muted())
                    .child(label.to_uppercase()),
            )
            .child(
                div()
                    .text_size(px(13.))
                    .text_color(theme::text())
                    .child(value),
            )
            .into_any_element()
    }

    /// The selected plugin's page (P4): its head with the one action, its facts, its README.
    fn detail(&self, item: &Item, cx: &mut Context<Self>) -> AnyElement {
        let market = cx.global::<Market>();
        let repo = (item.offer.as_ref().map(|o| o.repo.clone())).or_else(|| {
            (item.plugin.as_ref()?.stamp.as_ref()).and_then(|s| market::github_repo(&s.repository))
        });
        let read = repo.as_ref().and_then(|r| market.details.get(r)).cloned();
        let question = market.question.clone();
        let got = match &read {
            Some(Read::Got(d)) => Some(d.clone()),
            _ => None,
        };
        let act = act_of(item, cx);
        let local = item.tab == Tab::Local;
        // The head's one action, at its right.
        let primary: AnyElement = self.primary(item, &act, got.as_ref(), cx);
        let byline = (!item.author.is_empty()).then(|| {
            let url = repository_url(item).unwrap_or_default();
            let shown = url.trim_start_matches("https://").to_owned();
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .text_size(px(12.))
                .text_color(theme::muted())
                .child(format!("by {}", item.author))
                .child("·")
                .child(
                    div()
                        .id("plugin-repository")
                        .flex()
                        .items_center()
                        .gap(px(3.))
                        .cursor_pointer()
                        .text_color(theme::accent())
                        .hover(|s| s.underline())
                        .child(shown)
                        .child(
                            icons::icon("external")
                                .size(px(11.))
                                .text_color(theme::accent()),
                        )
                        .on_click(move |_: &ClickEvent, _, cx| cx.open_url(&url)),
                )
        });
        let head = div()
            .flex()
            .items_start()
            .gap(px(14.))
            .child(mark(item, 48.))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(
                                div()
                                    .text_size(px(18.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme::text())
                                    .child(item.name.clone()),
                            )
                            .when(
                                matches!(&item.plugin, Some(p) if p.origin == Origin::Env),
                                |s| s.child(tag("dev", theme::agent())),
                            ),
                    )
                    .children(byline),
            )
            .child(primary);
        let asked = (!self.from_row)
            .then(|| question.clone())
            .flatten()
            .filter(|r| item.offer.as_ref().is_some_and(|o| o.repo == *r))
            .map(|_| {
                div()
                    .flex()
                    .justify_end()
                    .child(div().w(px(320.)).child(self.question(item, cx)))
            });
        let facts = if local {
            self.local_facts(item, cx)
        } else {
            self.facts(item, got.as_ref(), matches!(read, Some(Read::Reading)), cx)
        };
        let readme: Option<AnyElement> = match &read {
            _ if local => None,
            Some(Read::Reading) => Some(skeleton("plugin-readme", 4)),
            Some(Read::Got(Detail {
                readme: Some(text), ..
            })) => Some(markdown::body(
                text,
                true,
                format!("plugin-readme-{}", item.id),
                None,
                self.frame.selecting(cx),
                cx,
            )),
            Some(Read::Failed(why)) => {
                Some(faint(format!("The README can't be read: {why}")).into_any_element())
            }
            _ => None,
        };
        div()
            .id("plugin-page")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px(px(24.))
            .py(px(22.))
            .flex()
            .flex_col()
            .gap(px(16.))
            .child(head)
            .children(asked)
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_y(px(10.))
                    .py(px(14.))
                    .border_t_1()
                    .border_b_1()
                    .border_color(theme::island_border())
                    .children(facts),
            )
            .children(readme)
            .into_any_element()
    }

    /// The head's action (P4): `Install`, `Update to 1.3.0`, `No build for Artoo 2.0.0`, or an installed plugin's
    /// `Turn off` and `Remove`; a local one's `Remove from this list`.
    fn primary(
        &self,
        item: &Item,
        act: &Act,
        got: Option<&Detail>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (i1, i2) = (item.clone(), item.clone());
        let loaded = cx.global::<Loaded>();
        if item.tab == Tab::Local {
            let removable = match &item.plugin {
                Some(p) => matches!(&p.origin, Origin::Local(e) if loaded.local.contains(e)),
                None => true,
            };
            return match removable {
                true => outline_button("Remove from this list")
                    .id("plugin-unlist")
                    .on_click(move |_: &ClickEvent, _, cx| remove(&i1, cx))
                    .into_any_element(),
                false if matches!(&item.plugin, Some(p) if p.origin == Origin::Env) => {
                    div().into_any_element()
                }
                false => word("Restart to finish", theme::added()).into_any_element(),
            };
        }
        match act {
            Act::Install | Act::TryAgain(_) => {
                let no_build = got.is_some_and(|d| !matches!(d.build, Build::Ready { .. }));
                if no_build {
                    return outline_button(&format!("No build for Artoo {ARTOO}"))
                        .opacity(0.6)
                        .cursor_default()
                        .into_any_element();
                }
                let words = if matches!(act, Act::TryAgain(_)) {
                    "Try again"
                } else {
                    "Install"
                };
                let tip = match act {
                    Act::TryAgain(why) => format!("Not installed: {why}"),
                    _ => words.to_owned(),
                };
                bar_button("plugin-install-page", words, "", true)
                    .tip(tip)
                    .on_click(cx.listener(move |p, _: &ClickEvent, _, cx| p.act(&i1, false, cx)))
                    .into_any_element()
            }
            Act::NoBuild => outline_button(&format!("No build for Artoo {ARTOO}"))
                .opacity(0.6)
                .cursor_default()
                .into_any_element(),
            Act::Update(version) => bar_button(
                "plugin-update-page",
                &format!("Update to {version}"),
                "",
                true,
            )
            .on_click(cx.listener(move |p, _: &ClickEvent, _, cx| p.act(&i1, false, cx)))
            .into_any_element(),
            Act::Progress => div().pt(px(10.)).child(progress()).into_any_element(),
            Act::Restart => word("Restart to finish", theme::added()).into_any_element(),
            Act::Installed | Act::Switch(..) | Act::NotLoaded => {
                let plugin = item.plugin.clone().or_else(|| {
                    let offer = item.offer.as_ref()?;
                    installed_from(loaded, offer).cloned()
                });
                let Some(plugin) = plugin else {
                    return div().into_any_element();
                };
                let id = plugin.id();
                let off = loaded.off.contains(&id);
                let item = Item {
                    plugin: Some(plugin.clone()),
                    ..i2
                };
                div()
                    .flex()
                    .gap(px(8.))
                    .when(
                        plugin.stamp.is_some() && !matches!(plugin.state, State::Refused(_)),
                        |s| {
                            s.child(
                                outline_button(if off { "Turn on" } else { "Turn off" })
                                    .id("plugin-flip")
                                    .on_click(move |_: &ClickEvent, _, cx| state::flip(&id, cx)),
                            )
                        },
                    )
                    .child(
                        outline_button("Remove")
                            .id("plugin-remove")
                            .on_click(move |_: &ClickEvent, _, cx| remove(&item, cx)),
                    )
                    .into_any_element()
            }
            Act::Nothing => div().into_any_element(),
        }
    }

    /// A market plugin's facts (P4): Stars, Downloads, Version with its age, License, For this Artoo; Loaded when it
    /// was not.
    fn facts(&self, item: &Item, got: Option<&Detail>, reading: bool, cx: &App) -> Vec<AnyElement> {
        let mut facts: Vec<(&'static str, AnyElement)> = Vec::new();
        let stat = |icon: &'static str, n: u64| {
            div()
                .flex()
                .items_center()
                .gap(px(4.))
                .child(icons::icon(icon).size(px(12.)).text_color(theme::muted()))
                .child(grouped(n))
                .into_any_element()
        };
        if let Some(stars) = item.stars {
            facts.push(("Stars", stat("star", stars)));
        }
        if let Some(d) = got {
            facts.push(("Downloads", stat("download", d.downloads)));
        }
        let installed = item
            .plugin
            .as_ref()
            .and_then(|p| p.stamp.as_ref())
            .map(|s| s.version.clone());
        let version = match (installed, got) {
            (Some(v), _) => Some(v),
            (
                None,
                Some(Detail {
                    build: Build::Ready { version, .. },
                    published,
                    ..
                }),
            ) if *published > 0 => Some(format!("{version} · {}", age(*published))),
            (
                None,
                Some(Detail {
                    build: Build::Ready { version, .. },
                    ..
                }),
            ) => Some(version.clone()),
            _ => None,
        };
        if let Some(v) = version {
            facts.push(("Version", div().child(v).into_any_element()));
        }
        if let Some(o) = item.offer.as_ref().filter(|o| !o.license.is_empty()) {
            facts.push(("License", div().child(o.license.clone()).into_any_element()));
        }
        let built = match got.map(|d| &d.build) {
            _ if item.plugin.is_some() => None,
            Some(Build::Ready { .. }) => Some(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .text_color(theme::added())
                    .child(
                        icons::icon("check")
                            .size(px(12.))
                            .text_color(theme::added()),
                    )
                    .child("Has a build")
                    .into_any_element(),
            ),
            Some(_) => Some(
                div()
                    .text_color(theme::removed())
                    .child(format!("No build for {ARTOO}"))
                    .into_any_element(),
            ),
            None if reading => Some(
                div()
                    .text_color(theme::faint())
                    .child("…")
                    .into_any_element(),
            ),
            None => None,
        };
        if let Some(b) = built {
            facts.push(("For this Artoo", b));
        }
        if let Some(p) = &item.plugin {
            let off = cx.global::<Loaded>().off.contains(&p.id());
            let loaded_now = match &p.state {
                State::On => None,
                State::Off => Some(("Off".to_owned(), false)),
                State::Struck => Some(("Off · ended Artoo twice".to_owned(), true)),
                State::Refused(why) => Some((format!("No · {why}"), true)),
            };
            if let Some((text, bad)) = loaded_now {
                facts.push((
                    "Loaded",
                    div()
                        .text_color(if bad {
                            theme::warning()
                        } else {
                            theme::muted()
                        })
                        .child(text)
                        .into_any_element(),
                ));
            } else if off {
                facts.push((
                    "Loaded",
                    div().child("Yes, until Artoo restarts").into_any_element(),
                ));
            }
        }
        let n = facts.len();
        (facts.into_iter().enumerate())
            .map(|(i, (label, value))| Self::fact(label, value, i + 1 == n))
            .collect()
    }

    /// A local plugin's facts (P6): Folder, Built, For this Artoo, this Artoo's SDK build in its tooltip.
    fn local_facts(&self, item: &Item, cx: &App) -> Vec<AnyElement> {
        let loaded = cx.global::<Loaded>();
        let folder = div()
            .font_family(theme::code_font())
            .text_size(px(12.))
            .child(item.about.clone())
            .into_any_element();
        let library = item
            .plugin
            .as_ref()
            .filter(|p| p.stamp.is_some())
            .map(|p| p.path.clone());
        let built = (library.as_ref())
            .and_then(|l| std::fs::metadata(l).ok())
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| artoo_ui_kit::format::ago(d.as_secs() as i64));
        let sdk = loaded.sdk.clone().unwrap_or_default();
        let (text, color) = match item.plugin.as_ref().map(|p| &p.state) {
            Some(State::Refused(why))
                if item.plugin.as_ref().is_some_and(|p| p.stamp.is_none()) =>
            {
                (why.clone(), theme::warning())
            }
            Some(State::Refused(_)) => ("Built for another Artoo".to_owned(), theme::warning()),
            Some(_) => ("✓ Stamp matches".to_owned(), theme::added()),
            None => ("Loads when Artoo restarts".to_owned(), theme::muted()),
        };
        let matches = div()
            .id("plugin-stamp")
            .text_color(color)
            .child(text)
            .when(!sdk.is_empty(), |s| {
                s.tip(format!("This Artoo's SDK build: {sdk}"))
            })
            .into_any_element();
        let mut facts = vec![("Folder", folder)];
        if let Some(built) = built {
            facts.push(("Built", div().child(built).into_any_element()));
        }
        facts.push(("For this Artoo", matches));
        let n = facts.len();
        (facts.into_iter().enumerate())
            .map(|(i, (label, value))| Self::fact(label, value, i + 1 == n))
            .collect()
    }

    /// The bar over the page when Artoo must restart (P5): `Jira is ready. Restart Artoo to turn it on.`, or after a
    /// switch `Restart Artoo to apply this.`; the page's only Restart button.
    fn restart_bar(&self, cx: &App) -> Option<AnyElement> {
        let (loaded, market) = (cx.global::<Loaded>(), cx.global::<Market>());
        let name_of = |id: &str| -> String {
            (loaded
                .plugins
                .iter()
                .filter_map(|p| p.stamp.as_ref())
                .find(|s| s.id == id))
            .map(|s| s.name.clone())
            .or_else(|| {
                let Found::Got(offers) = &market.found else {
                    return None;
                };
                (offers.iter())
                    .find(|o| built_id(market, &o.repo).as_deref() == Some(id))
                    .map(|o| o.name.clone())
            })
            .unwrap_or_else(|| id.to_owned())
        };
        let ready: Vec<String> = (market.done.iter())
            .filter(|(_, d)| **d == Done::Installed)
            .map(|(id, _)| name_of(id))
            .collect();
        let words = match ready.len() {
            0 if loaded.changed() || market.done.values().any(|d| *d == Done::Removed) => {
                "Restart Artoo to apply this.".to_owned()
            }
            0 => return None,
            1 => format!("{} is ready. Restart Artoo to turn it on.", ready[0]),
            _ => format!(
                "{} are ready. Restart Artoo to turn them on.",
                ready.join(", ")
            ),
        };
        Some(
            div()
                .flex_none()
                .mb(px(artoo_ui_kit::gap()))
                .px(px(14.))
                .py(px(8.))
                .flex()
                .items_center()
                .gap(px(12.))
                .rounded(px(8.))
                .bg(theme::info_bg())
                .child(
                    div()
                        .flex_1()
                        .text_size(px(13.))
                        .text_color(theme::text())
                        .child(words),
                )
                .child(
                    bar_button("plugins-restart-now", "Restart Artoo", "", true)
                        .on_click(|_: &ClickEvent, _, cx| cx.restart()),
                )
                .into_any_element(),
        )
    }
}

impl Render for Page {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let list = self.listed(cx);
        let picked = self.picked_of(&list).cloned();
        let rows = self.list(&list, cx);
        let detail = match &picked {
            Some(item) => self.detail(item, cx),
            None => div().into_any_element(),
        };
        let restart = self.restart_bar(cx);
        let page = self.frame.list_page(rows, detail, cx);
        div()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .children(restart)
            .child(page)
    }
}

/// The page bar (P2): the tabs with their counts, then the sort; Installed's `Update all` once two updates wait;
/// Local's own buttons.
fn bar(page: &Entity<Page>, cx: &mut App) -> BarParts {
    let p = page.read(cx);
    let counts = p.counts(cx);
    let typed = p.typed();
    let tab = p.tab;
    let sort = p.sort;
    let tabs = Tab::ALL.into_iter().zip(counts).map(|(t, n)| {
        let page = page.clone();
        chip(
            SharedString::from(format!("plugins-tab-{}", t.label())),
            format!("{} {n}", t.label()),
            !typed && t == tab,
        )
        .on_click(move |_: &ClickEvent, _, cx| page.update(cx, |p, cx| p.set_tab(t, cx)))
    });
    let mut actions = Vec::new();
    if tab == Tab::Local && !typed {
        actions.push(
            BarAction::new(
                "plugins-add",
                bar_button("plugins-add", "Add…", "", false)
                    .child(icons::icon("plus").size(px(12.)).text_color(theme::muted()))
                    .on_click(|_: &ClickEvent, _, cx| state::add_local(cx)),
            )
            .folds(MenuItem::new("Add…", |_, cx| state::add_local(cx))),
        );
        actions.push(
            BarAction::new(
                "plugins-folder",
                bar_button("plugins-folder", "Open plugins folder", "", false)
                    .on_click(|_: &ClickEvent, _, cx| state::open_folder(cx)),
            )
            .folds(MenuItem::new("Open plugins folder", |_, cx| {
                state::open_folder(cx)
            }))
            .fold_order(1),
        );
        actions.push(
            BarAction::new(
                "plugins-restart",
                bar_button("plugins-restart", "Restart Artoo", "", false)
                    .on_click(|_: &ClickEvent, _, cx| cx.restart()),
            )
            .folds(MenuItem::new("Restart Artoo", |_, cx| cx.restart())),
        );
    } else {
        if tab == Tab::Installed && !typed && market::waiting(cx) >= 2 {
            actions.push(
                BarAction::new(
                    "plugins-update-all",
                    bar_button("plugins-update-all", "Update all", "", false)
                        .on_click(|_: &ClickEvent, _, cx| market::update_all(cx)),
                )
                .folds(MenuItem::new("Update all", |_, cx| market::update_all(cx))),
            );
        }
        let at = page.clone();
        actions.push(BarAction::new(
            "plugins-sort",
            div()
                .id("plugins-sort")
                .h(px(28.))
                .px(px(8.))
                .flex()
                .items_center()
                .gap(px(4.))
                .rounded(px(ROW_RADIUS))
                .cursor_pointer()
                .text_size(px(13.))
                .text_color(theme::muted())
                .hover(|s| s.bg(theme::hover()).text_color(theme::text()))
                .child(sort.label())
                .child(
                    icons::icon("chevron")
                        .size(px(12.))
                        .text_color(theme::faint()),
                )
                .on_click(move |e: &ClickEvent, _, cx| {
                    let position = e.position();
                    at.update(cx, |p, cx| p.sort_menu(position, cx))
                }),
        ));
    }
    BarParts {
        view: Some(group().children(tabs).into_any_element()),
        actions,
        ..Default::default()
    }
}

/// ⌘K's Plugins (P7): the market's plugins and the installed ones, by name, what they do, who wrote them. ↵ installs
/// one from a trusted author; any other opens its page, the question up under Install.
fn scope(page: Entity<Page>, frame: Rc<dyn Frame>) -> Search {
    let find_page = page.clone();
    Search {
        id: "plugins",
        label: "Plugins",
        placeholder: "Search plugins",
        go: GO,
        order: 70,
        all: AllOf {
            few: 3,
            order: 70,
            prefix: None,
            alone: false,
        },
        find: Rc::new(move |ask: Ask, cx: &App| {
            let typed = ask.text.trim();
            let mut found: Vec<Find> = Vec::new();
            let mut seen: Vec<String> = Vec::new();
            for item in items(cx) {
                if item.tab == Tab::Local || (!typed.is_empty() && !matches(&item, typed)) {
                    continue;
                }
                let name = item.name.clone();
                if seen.contains(&name) {
                    continue;
                }
                seen.push(name.clone());
                let mut detail = item.about.clone();
                if !item.author.is_empty() {
                    detail = format!("{detail} · {}", item.author);
                }
                if let Some(stars) = item.stars {
                    detail = format!("{detail} · ★ {}", short_count(stars));
                }
                let (page, frame) = (find_page.clone(), frame.clone());
                let open = Opens::Run(Rc::new(move |cx: &mut App| {
                    frame.go_space(SPACE, cx);
                    let item = item.clone();
                    page.update(cx, |p, cx| {
                        p.tab = item.tab;
                        p.text.clear();
                        p.area = Area::default();
                        p.picked = Some(item.key.clone());
                        p.from_row = false;
                        if act_of(&item, cx) == Act::Install {
                            p.act(&item, false, cx);
                        }
                        p.read_picked(cx);
                        cx.notify();
                    });
                }));
                found.push(
                    Find::new("Plugins", name, detail, open)
                        .glyph("puzzle")
                        .key("↵".to_owned()),
                );
            }
            // Browse plugins, what is typed carried over.
            let (page, frame) = (find_page.clone(), frame.clone());
            let text = typed.to_owned();
            let label = if text.is_empty() {
                "Browse plugins".to_owned()
            } else {
                format!("Browse plugins for \u{201c}{text}\u{201d}")
            };
            found.push(
                Find::new(
                    "Actions",
                    label,
                    String::new(),
                    Opens::Run(Rc::new(move |cx: &mut App| {
                        frame.go_space(SPACE, cx);
                        let text = text.clone();
                        page.update(cx, |p, cx| {
                            p.area = Area::default();
                            p.set_text(text, cx)
                        });
                    })),
                )
                .glyph("puzzle"),
            );
            if ask.all {
                found.truncate(4);
            }
            found
        }),
        opened: Some(Rc::new(market::opened)),
    }
}

/// The first row offered by an author not trusted yet, to install: the trust question's sample.
fn untrusted(p: &Page, cx: &App) -> Option<Item> {
    let trusted = &cx.global::<Market>().trusted;
    (p.listed(cx).into_iter()).find(|i| {
        i.offer.as_ref().is_some_and(|o| !trusted.contains(&o.owner)) && act_of(i, cx) == Act::Install
    })
}

/// The page's parts in a tab: its pages, its search's field, ⌘K's Plugins, the way from Settings' search, its
/// snapshots.
pub(crate) fn parts(frame: Rc<dyn Frame>, _: &mut Window, cx: &mut App) -> Parts {
    let page = cx.new(|cx| Page::new(frame.clone(), cx));
    let (go, find) = (frame.clone(), frame.clone());
    let (shows, steps, opens, bars) = (page.clone(), page.clone(), page.clone(), page.clone());
    let (k, l, sel) = (page.clone(), page.clone(), page.clone());
    let shown = page.read(cx).shown.clone();
    let input = Input {
        id: SEARCH,
        // Read while the page draws (its field asks who is typing): a cell, not the page.
        open: Rc::new(move |_| shown.get()),
        key: Rc::new(move |event, _, cx| k.update(cx, |p, cx| p.key(event, cx))),
        left: Rc::new(move |cx| l.update(cx, |_, cx| cx.notify())),
        selected: Rc::new(move |cx| sel.read(cx).area.edit.selected_text().map(str::to_owned)),
        caret: Rc::new(|_| false),
    };
    // Settings keeps no list of plugins: its search finds this row, which opens the page (P10).
    let door = artoo_shell_api::settings::row_view(
        page.clone(),
        |page: &mut Page, _| {
            let frame = page.frame.clone();
            artoo_ui_kit::controls::setting_row(
                "plugins-door",
                "Plugins",
                "Discover · Installed · Local",
                outline_button("Open")
                    .id("plugins-door-open")
                    .on_click(move |_: &ClickEvent, _, cx| frame.go_space(SPACE, cx)),
                None,
            )
            .into_any_element()
        },
        cx,
    );
    let nothing = cx.new(|_| Nothing);
    let snap_frame = frame.clone();
    let snap_page = page.clone();
    let sample: Snap = Rc::new(move |arg, cx| {
        cx.set_global(state::sample());
        cx.set_global(market::sample());
        snap_frame.open_space(SPACE, cx);
        let arg = arg.to_owned();
        snap_page.update(cx, |p, cx| {
            p.shown.set(true);
            for part in arg.split('+').filter(|a| !a.is_empty()) {
                match part {
                    "installed" => p.tab = Tab::Installed,
                    "local" => p.tab = Tab::Local,
                    "ask" => {
                        if let Some(item) = untrusted(p, cx) {
                            p.act(&item, true, cx);
                        }
                    }
                    "ask-page" => {
                        if let Some(item) = untrusted(p, cx) {
                            p.picked = Some(item.key.clone());
                            p.act(&item, false, cx);
                        }
                    }
                    "pending" => {
                        let market = cx.global_mut::<Market>();
                        market.done.insert("jira".to_owned(), Done::Installed);
                        market.done.insert("sprint".to_owned(), Done::Installing);
                    }
                    "loading" => cx.global_mut::<Market>().found = Found::Asking,
                    "offline" => {
                        cx.global_mut::<Market>().found =
                            Found::Failed("GitHub can't be reached".to_owned())
                    }
                    other if other.starts_with("pick=") => {
                        let n: usize = other["pick=".len()..].parse().unwrap_or(0);
                        let item = p.listed(cx).into_iter().nth(n);
                        if let Some(item) = item {
                            p.picked = Some(item.key.clone());
                        }
                    }
                    other if other.starts_with("search=") => {
                        p.text = other["search=".len()..].to_owned();
                    }
                    _ => {}
                }
            }
            cx.notify();
        });
    });
    Parts {
        actions: vec![
            (
                Box::new(OpenPlugins),
                Rc::new(move |_, cx| go.go_space(SPACE, cx)),
            ),
            (
                Box::new(FindPlugins),
                Rc::new(move |_, cx| find.take_keyboard(SEARCH, cx)),
            ),
        ],
        inputs: vec![input],
        searches: vec![scope(page.clone(), frame.clone())],
        settings: vec![
            Rows::new(artoo_shell_api::settings::ABOUT, 90, nothing).hit("Plugins", door),
        ],
        snapshots: vec![
            ("plugins:", sample),
            ("plugin-install:", Rc::new(market::install_found)),
        ],
        pages: Some(Pages {
            space: SPACE,
            kinds: &[],
            parent: |_| None,
            item: |_| None,
            path: Rc::new(|_: &Route, _: &artoo_shell_api::Drawing, _: &App| None),
            screen: |_| Screen::List,
            row: Rc::new(|_: &Route, _: &Route, _: &App| None),
            called: |_| "the plugins",
            link: Rc::new(
                |_: Option<&Route>, _: &WebRemote, _: bool, _: &App| -> Result<Link, String> {
                    Err("The plugins' page has no link".to_owned())
                },
            ),
            view: page.clone().into(),
            side: None,
            show: Rc::new(move |_: Option<&Route>, fresh: bool, cx: &mut App| {
                shows.update(cx, |p, cx| p.show(fresh, cx))
            }),
            keys: Some(ListKeys {
                step: Rc::new(move |down, cx| steps.update(cx, |p, cx| p.step(down, cx))),
                open: Rc::new(move |cx| opens.update(cx, |p, cx| p.enter(cx))),
            }),
            here: None,
            bar: Some(Rc::new(move |_: Option<&Route>, cx: &mut App| {
                Some(bar(&bars, cx))
            })),
        }),
        ..Default::default()
    }
}

/// A view that draws nothing: Settings' About gets the page's door only as a search's hit.
struct Nothing;

impl Render for Nothing {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_are_short_and_grouped() {
        assert_eq!(short_count(96), "96");
        assert_eq!(short_count(1204), "1.2k");
        assert_eq!(short_count(2000), "2k");
        assert_eq!(short_count(15_400), "15k");
        assert_eq!(grouped(1204), "1,204");
        assert_eq!(grouped(3_412_000), "3,412,000");
    }

    #[test]
    fn a_versions_age_is_said_in_words() {
        let now = artoo_ui_kit::format::now();
        assert_eq!(age(now), "today");
        assert_eq!(age(now - 86_400), "yesterday");
        assert_eq!(age(now - 3 * 86_400), "3 days ago");
        assert_eq!(age(now - 21 * 86_400), "3 weeks ago");
    }
}
