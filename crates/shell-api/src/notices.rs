//! Notices (#196, Design `docs/specs/notifications.md`): what nobody at the screen just asked for — someone else did
//! it, or work running on its own reached a point. Each source offers its kinds as it installs (merge requests,
//! agents, each plugin), a kind with what it does by default on three channels: it waits in the Inbox, it shows a
//! toast while Artoo is in front, it goes to the system's notifications while no window is. Settings → Notifications
//! switches each, and what differs from the default is kept (`notices.txt`).
//!
//! What you just did (feedback), what stays true until it changes (a state) and a card are not notices and have no
//! kind. A notice is raised as a toast with its kind (`Toast::kind`).
//!
//! A plugin's source is known by the loader, never by what it says (`guest`): its id is the plugin's, three kinds at
//! most, and its System is off until the person turns it on.

pub use artoo_core::notice::Why;
use artoo_core::{applog, storage};
use gpui::{App, Global};
use std::collections::HashMap;

/// Where the choices that differ from the defaults are kept: `requests.resolved system on`, a line each.
pub const FILE: &str = "notices.txt";

/// The most kinds a plugin offers.
pub const GUEST_KINDS: usize = 3;

/// Where a notice goes.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Channel {
    /// It waits in the Inbox until opened or done, and is counted there.
    Inbox,
    /// A toast while a window of Artoo's is in front and its tab on screen.
    Toast,
    /// The system's notifications while no window of Artoo's is in front, System notifications on.
    System,
}

impl Channel {
    pub const ALL: [Channel; 3] = [Channel::Inbox, Channel::Toast, Channel::System];

    fn word(self) -> &'static str {
        match self {
            Channel::Inbox => "inbox",
            Channel::Toast => "toast",
            Channel::System => "system",
        }
    }

    fn read(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.word() == word)
    }
}

/// A kind of notice and what it does by default.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Kind {
    /// `<source>.<what>`: `requests.review`, `agents.asks`, `jira.assigned`. Kept in `notices.txt`, so renamed only
    /// with what was kept under it.
    pub id: &'static str,
    /// Its row in Settings: `Review requests`.
    pub name: &'static str,
    /// Whether it waits in the Inbox; `None`: it has a home of its own and is never listed there.
    pub inbox: Option<bool>,
    pub toast: bool,
    pub system: bool,
    /// What happened, the Inbox's filter it is listed under (#197); none: under All only.
    pub why: Option<Why>,
}

impl Kind {
    /// One that waits in the Inbox, nothing else by default.
    pub const fn listed(id: &'static str, name: &'static str) -> Self {
        Self {
            id,
            name,
            inbox: Some(true),
            toast: false,
            system: false,
            why: None,
        }
    }

    /// One with a home of its own: never in the Inbox.
    pub const fn homed(id: &'static str, name: &'static str) -> Self {
        Self {
            id,
            name,
            inbox: None,
            toast: false,
            system: false,
            why: None,
        }
    }

    /// A toast by default.
    pub const fn toast(self) -> Self {
        Self {
            toast: true,
            ..self
        }
    }

    /// To the system by default.
    pub const fn system(self) -> Self {
        Self {
            system: true,
            ..self
        }
    }

    /// Listed in the Inbox under `why` (#197).
    pub const fn why(self, why: Why) -> Self {
        Self {
            why: Some(why),
            ..self
        }
    }

    /// What it does on `channel` unless the person said otherwise.
    pub fn default_on(&self, channel: Channel) -> bool {
        match channel {
            Channel::Inbox => self.inbox.unwrap_or(false),
            Channel::Toast => self.toast,
            Channel::System => self.system,
        }
    }
}

/// Who raises notices, and its kinds in the order Settings lists them.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Source {
    /// `requests`, `agents`; a plugin's is its id.
    pub id: &'static str,
    /// Its group in Settings: `Merge requests`; a toast of a plugin's starts with it (`Jira · Assigned to you`).
    pub name: &'static str,
    pub kinds: Vec<Kind>,
    /// Its place in Settings among the app's, the lowest first; a plugin's come after them all, as installed.
    pub order: u16,
    /// A plugin's: set by the registry, from the loader.
    pub guest: bool,
}

impl Source {
    pub fn new(id: &'static str, name: &'static str, kinds: Vec<Kind>) -> Self {
        Self {
            id,
            name,
            kinds,
            order: 100,
            guest: false,
        }
    }

    pub fn order(self, order: u16) -> Self {
        Self { order, ..self }
    }
}

#[derive(Default)]
struct Registry {
    sources: Vec<Source>,
    /// The plugin installing now (its id), said by the loader.
    guest: Option<String>,
    /// The plugins have installed: no source is offered after.
    closed: bool,
    /// The choices that differ from the defaults, read once.
    kept: Option<HashMap<(String, Channel), bool>>,
}

impl Global for Registry {}

/// A row a part lists in the Inbox (#196): what waits for the person until they open it or are done with it. It is
/// listed and counted while its kind's Inbox is on, read once looked at (a new `key` is new again). Its preview says
/// the item's id and where it is beside its source, its facts labelled, and what Open opens (`tag`, `place`,
/// `facts`, `open`; empty, they are left out).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Listed {
    pub kind: &'static str,
    /// This one: `PAY-412:assigned`; the same key is the same row.
    pub key: String,
    /// What happened, who included: `Assigned to you`, `Ember Lind mentioned you`.
    pub says: String,
    /// What it is about: `PAY-412 Refund rounds down on partial captures`.
    pub about: String,
    /// The start of what was written; empty: none.
    pub text: String,
    /// When, Unix seconds: the Inbox's order.
    pub at: i64,
    /// The item's id, first in the preview: `PAY-412`.
    pub tag: String,
    /// Where it is, after the source's name: `Payments` (`Jira · Payments`).
    pub place: String,
    /// The item's facts, labelled: `State` `In progress`, `Assignee` `You · was Juniper Hale`.
    pub facts: Vec<(String, String)>,
    /// What Open says: `Open PAY-412`; empty, `Open`.
    pub open: String,
}

/// A row's place among a tab's Inbox rows, kept while its key is the same.
pub fn row_id(key: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    key.hash(&mut hash);
    hash.finish()
}

/// What a part lists now (`Lister::rows`).
pub type Rows = std::rc::Rc<dyn Fn(&App) -> Vec<Listed>>;

/// What is done to a row, by its key (`Lister::open`, `Lister::done`).
pub type OnRow = std::rc::Rc<dyn Fn(&str, &mut App)>;

/// What a part lists in the Inbox, and what opening a row and being done with it do (by the row's key).
#[derive(Clone)]
pub struct Lister {
    pub rows: Rows,
    pub open: OnRow,
    /// `E`: done with it at its source too (a plugin's issue marked seen); none, read is enough.
    pub done: Option<OnRow>,
}

/// The parts' listers in one tab, a service of the tab (`Listers::of`): each part adds its own as it is made.
#[derive(Clone, Default)]
pub struct Listers(std::rc::Rc<std::cell::RefCell<Vec<Lister>>>);

impl Listers {
    /// The tab's listers, made the first time a part asks.
    pub fn of(services: &crate::Services) -> Listers {
        services.get::<Listers>().unwrap_or_else(|| {
            let listers = Listers::default();
            services.set(listers.clone());
            listers
        })
    }

    pub fn add(&self, lister: Lister) {
        self.0.borrow_mut().push(lister);
    }

    /// Every part's rows the Inbox lists now: of a kind offered whose Inbox is on.
    pub fn rows(&self, cx: &App) -> Vec<Listed> {
        let listers = self.0.borrow().clone();
        (listers.iter())
            .flat_map(|l| (l.rows)(cx))
            .filter(|r| is_on(cx, r.kind, Channel::Inbox))
            .collect()
    }

    /// Row `id`'s lister and the row, while it is listed.
    pub fn find(&self, id: u64, cx: &App) -> Option<(Lister, Listed)> {
        let listers = self.0.borrow().clone();
        listers.into_iter().find_map(|l| {
            let row = (l.rows)(cx)
                .into_iter()
                .find(|r| row_id(&r.key) == id && is_on(cx, r.kind, Channel::Inbox))?;
            Some((l, row))
        })
    }
}

/// The plugin installing now, or none once it has (the plugins' loader, around each `install`).
pub fn guest(cx: &mut App, id: Option<String>) {
    cx.default_global::<Registry>().guest = id;
}

/// The plugins have installed: a source offered from now on is refused.
pub fn close(cx: &mut App) {
    let registry = cx.default_global::<Registry>();
    registry.guest = None;
    registry.closed = true;
}

/// Offers `source` and its kinds (a feature's or a plugin's `install`). Refused, and logged: an id taken, a kind not
/// named `<source>.<what>` or taken, and from a plugin a source not of its id or a kind past the third; any after the
/// plugins installed.
pub fn add_source(cx: &mut App, mut source: Source) {
    let refusal = {
        let registry = cx.default_global::<Registry>();
        source.guest = registry.guest.is_some();
        why_refused(registry, &source)
    };
    if let Some(why) = refusal {
        let what = format!("notices {} ({why})", source.id);
        applog::line(&format!("{what}: refused"));
        crate::note_refused(cx, what);
        return;
    }
    if source.guest {
        // The system's notifications reach outside Artoo on Artoo's permission: a plugin's wait for the person.
        for kind in &mut source.kinds {
            kind.system = false;
        }
    }
    cx.default_global::<Registry>().sources.push(source);
    // What was kept, read now: a view drawing reads it (`is_on`) and cannot.
    kept(cx);
}

fn why_refused(registry: &Registry, source: &Source) -> Option<String> {
    if registry.closed {
        return Some("offered after the plugins installed".to_owned());
    }
    if let Some(guest) = &registry.guest {
        if source.id != guest {
            return Some(format!("a plugin offers its own source, {guest}"));
        }
        if source.kinds.len() > GUEST_KINDS {
            return Some(format!("a plugin offers {GUEST_KINDS} kinds at most"));
        }
    }
    if registry.sources.iter().any(|s| s.id == source.id) {
        return Some("its id is taken".to_owned());
    }
    let prefix = format!("{}.", source.id);
    for (i, kind) in source.kinds.iter().enumerate() {
        if !kind.id.starts_with(&prefix) || kind.id.len() == prefix.len() {
            return Some(format!("kind {} is not {prefix}<what>", kind.id));
        }
        let taken = (registry.sources.iter().flat_map(|s| &s.kinds))
            .chain(&source.kinds[..i])
            .any(|k| k.id == kind.id);
        if taken {
            return Some(format!("kind {} is taken", kind.id));
        }
    }
    None
}

/// The sources offered, the app's first, each as offered.
pub fn sources(cx: &App) -> Vec<Source> {
    let mut sources = (cx.try_global::<Registry>())
        .map(|r| r.sources.clone())
        .unwrap_or_default();
    sources.sort_by_key(|s| (s.guest, if s.guest { 0 } else { s.order }));
    sources
}

/// Kind `id` and its source, when offered.
pub fn kind(cx: &App, id: &str) -> Option<(Kind, Source)> {
    let registry = cx.try_global::<Registry>()?;
    registry.sources.iter().find_map(|s| {
        let kind = s.kinds.iter().find(|k| k.id == id)?;
        Some((*kind, s.clone()))
    })
}

/// Whether kind `id` goes to `channel`: as the person said, else its default. A kind not offered goes nowhere; one
/// with a home of its own is never in the Inbox.
pub fn on(cx: &mut App, id: &str, channel: Channel) -> bool {
    let Some((kind, _)) = kind(cx, id) else {
        return false;
    };
    if channel == Channel::Inbox && kind.inbox.is_none() {
        return false;
    }
    let kept = kept(cx);
    kept.get(&(id.to_owned(), channel))
        .copied()
        .unwrap_or_else(|| kind.default_on(channel))
}

/// `on` where only a read is at hand (a view drawing): the choices as they were last read, the default before.
pub fn is_on(cx: &App, id: &str, channel: Channel) -> bool {
    let Some(registry) = cx.try_global::<Registry>() else {
        return false;
    };
    let Some(kind) = registry
        .sources
        .iter()
        .flat_map(|s| &s.kinds)
        .find(|k| k.id == id)
    else {
        return false;
    };
    if channel == Channel::Inbox && kind.inbox.is_none() {
        return false;
    }
    (registry.kept.as_ref())
        .and_then(|k| k.get(&(id.to_owned(), channel)).copied())
        .unwrap_or_else(|| kind.default_on(channel))
}

/// Kind `id` turned on or off on `channel` by the person, and kept.
pub fn set(cx: &mut App, id: &str, channel: Channel, on: bool) {
    kept(cx);
    let registry = cx.default_global::<Registry>();
    if let Some(kept) = registry.kept.as_mut() {
        kept.insert((id.to_owned(), channel), on);
    }
    storage::save(FILE, written(registry));
}

/// The choices kept, read from `notices.txt` once.
fn kept(cx: &mut App) -> HashMap<(String, Channel), bool> {
    let registry = cx.default_global::<Registry>();
    registry
        .kept
        // A test reads none of the user's.
        .get_or_insert_with(|| {
            let text = if artoo_core::platform::sandboxed() {
                String::new()
            } else {
                storage::read(FILE)
            };
            read(&text)
        })
        .clone()
}

/// `notices.txt`'s lines: `<kind> <channel> on|off`.
fn read(text: &str) -> HashMap<(String, Channel), bool> {
    text.lines()
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            let (kind, channel, on) = (words.next()?, words.next()?, words.next()?);
            let on = match on {
                "on" => true,
                "off" => false,
                _ => return None,
            };
            Some(((kind.to_owned(), Channel::read(channel)?), on))
        })
        .collect()
}

/// What is kept: a choice that differs from its kind's default, and every choice of a kind not offered this run (a
/// plugin removed or off keeps its own, as it comes back).
fn written(registry: &Registry) -> String {
    let Some(kept) = &registry.kept else {
        return String::new();
    };
    let default = |id: &str, channel: Channel| {
        (registry.sources.iter().flat_map(|s| &s.kinds))
            .find(|k| k.id == id)
            .map(|k| k.default_on(channel))
    };
    let mut lines: Vec<String> = kept
        .iter()
        .filter(|((id, channel), on)| default(id, *channel) != Some(**on))
        .map(|((id, channel), on)| {
            format!("{id} {} {}", channel.word(), if *on { "on" } else { "off" })
        })
        .collect();
    lines.sort();
    lines.iter().map(|l| format!("{l}\n")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn requests() -> Source {
        Source::new(
            "requests",
            "Merge requests",
            vec![
                Kind::listed("requests.review", "Review requests").system(),
                Kind::listed("requests.resolved", "Discussions resolved"),
            ],
        )
    }

    #[test]
    fn a_kind_goes_where_its_default_says_until_the_person_says_otherwise() {
        gpui::TestAppContext::single().update(|cx| {
            add_source(cx, requests());
            add_source(
                cx,
                Source::new(
                    "agents",
                    "Agents",
                    vec![Kind::homed("agents.shows", "An agent shows you something").toast()],
                ),
            );
            assert!(on(cx, "requests.review", Channel::System));
            assert!(!on(cx, "requests.resolved", Channel::System));
            assert!(!on(cx, "agents.shows", Channel::Inbox));
            assert!(on(cx, "agents.shows", Channel::Toast));
            assert!(!on(cx, "nobody.offered", Channel::Toast));
            set(cx, "requests.review", Channel::System, false);
            assert!(!on(cx, "requests.review", Channel::System));
            assert!(!is_on(cx, "requests.review", Channel::System));
        });
    }

    #[test]
    fn what_is_kept_is_what_differs() {
        gpui::TestAppContext::single().update(|cx| {
            add_source(cx, requests());
            let registry = cx.default_global::<Registry>();
            registry.kept = Some(read(
                "requests.review system off\nrequests.resolved inbox on\njira.assigned toast off\nbroken line\n",
            ));
            assert_eq!(
                written(cx.global::<Registry>()),
                "jira.assigned toast off\nrequests.review system off\n"
            );
        });
    }

    #[test]
    fn a_plugins_source_is_its_own_and_small() {
        gpui::TestAppContext::single().update(|cx| {
            add_source(cx, requests());
            guest(cx, Some("jira".to_owned()));
            // Not its id.
            add_source(cx, Source::new("slack", "Slack", vec![]));
            // Four kinds.
            let four = (0..4)
                .map(|_| Kind::listed("jira.a", "A"))
                .collect::<Vec<_>>();
            add_source(cx, Source::new("jira", "Jira", four));
            add_source(
                cx,
                Source::new(
                    "jira",
                    "Jira",
                    vec![
                        Kind::listed("jira.assigned", "Assigned to you")
                            .toast()
                            .system(),
                    ],
                ),
            );
            guest(cx, None);
            close(cx);
            // After the plugins.
            add_source(cx, Source::new("late", "Late", vec![]));

            let sources = sources(cx);
            let ids: Vec<_> = sources.iter().map(|s| (s.id, s.guest)).collect();
            assert_eq!(ids, [("requests", false), ("jira", true)]);
            let (kind, _) = kind(cx, "jira.assigned").unwrap();
            assert!(kind.toast && !kind.system, "a plugin's System starts off");
            assert_eq!(crate::refused(cx).len(), 3);
        });
    }

    #[test]
    fn a_kind_is_named_by_its_source_and_once() {
        gpui::TestAppContext::single().update(|cx| {
            add_source(cx, requests());
            add_source(
                cx,
                Source::new(
                    "agents",
                    "Agents",
                    vec![Kind::listed("requests.review", "Again")],
                ),
            );
            add_source(
                cx,
                Source::new(
                    "other",
                    "Other",
                    vec![Kind::listed("other.a", "A"), Kind::listed("other.a", "A")],
                ),
            );
            add_source(cx, requests());
            assert_eq!(sources(cx).len(), 1);
            assert_eq!(crate::refused(cx).len(), 3);
        });
    }
}
