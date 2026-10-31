//! Notifications for the open repository's project (#72): the user's to-dos (mentions, review requests, being
//! addressed), new comments where they take part — in discussions they wrote in, on any open merge request, and on
//! their own merge requests — and their discussions resolved by someone else. The tab's Inbox lists them and reads
//! them again (`read`, in the background); the menu counts the unread.
//!
//! A request seen for the first time sets its comments so far as read, and its resolved discussions as known: only
//! what comes after is news. Kept per project in `seen-notes.txt` and `seen-resolved.txt`; what was opened or marked
//! read in `seen-read.txt`. A request's discussions are read again only when it changed (`updated_at`), or when the
//! list is opened.

use crate::waiting::{Me, Waiting};
use artoo_core::links::{self, Target};
use artoo_core::storage;
use artoo_hosts_api::{Host, Mr, Thread, Todo};
use gpui::Task;
use std::collections::{HashMap, HashSet};
use std::time::Duration;

/// How often the list is read again while the app runs.
pub const EVERY: Duration = Duration::from_secs(180);
/// Open requests whose discussions are read, at most.
const MOST_REQUESTS: usize = 40;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Mention,
    Review,
    /// Another to-do (assigned, approval required, …).
    Todo,
    /// A comment in a discussion the user wrote in.
    Reply,
    /// A comment on the user's own merge request.
    OnYours,
    /// A discussion the user took part in, resolved by someone else.
    Resolved,
}

impl Kind {
    fn verb(self) -> &'static str {
        match self {
            Kind::Mention => "mentioned you",
            Kind::Review => "asked for your review",
            Kind::Todo => "has a to-do for you",
            Kind::Reply => "replied",
            Kind::OnYours => "commented on your request",
            Kind::Resolved => "resolved your discussion",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    /// `todo:<id>` or `note:<id>`.
    pub key: String,
    pub kind: Kind,
    pub iid: u64,
    pub title: String,
    pub who: String,
    pub text: String,
    pub created: i64,
    pub note: Option<u64>,
    pub todo: Option<u64>,
    /// `base|iid|discussion` of a resolved discussion.
    pub resolved: Option<String>,
}

#[derive(Default)]
pub struct Notices {
    pub list: Vec<Notice>,
    pub at: usize,
    /// Opened or looked at: no longer bold, not counted. Kept for the project `read_base`.
    read: HashSet<String>,
    read_base: Option<String>,
    /// `base|iid` → the newest comment id already known.
    seen: HashMap<String, u64>,
    /// `base|iid|discussion` of resolved discussions already known.
    known_resolved: HashSet<String>,
    seen_loaded: bool,
    /// A request's discussions as last read, with its `updated_at` then.
    cache: HashMap<u64, (i64, Vec<Thread>)>,
    /// The tick the read of each began at, and what shows of those with changes waiting on them
    /// (#139).
    from: HashMap<u64, u64>,
    shown: HashMap<u64, Vec<Thread>>,
    /// The read under way, and the one every `EVERY`.
    pub task: Option<Task<()>>,
    pub poll: Option<Task<()>>,
    /// A first read finished, whatever it found: before it the Inbox shows rows loading (#72).
    pub loaded: bool,
    /// Notices already there or already said to the system's notifications: each is said once,
    /// and none of those listed at the first read (#26).
    told: HashSet<String>,
}

impl Notice {
    /// "mentioned you", "asked for your review", …
    pub fn kind_verb(&self) -> &'static str {
        self.kind.verb()
    }
}

impl Notices {
    /// Request `iid`'s discussions as last read, with the changes waiting on them, when it is one
    /// of those read.
    pub fn threads_of(&self, iid: u64) -> Option<&[Thread]> {
        match self.shown.get(&iid) {
            Some(threads) => Some(threads),
            None => self.cache.get(&iid).map(|(_, threads)| threads.as_slice()),
        }
    }

    /// The discussion of request `iid` holding note `note`, as it shows.
    pub fn thread_of(&self, iid: u64, note: u64) -> Option<&Thread> {
        self.threads_of(iid)?
            .iter()
            .find(|t| t.notes.iter().any(|n| n.id == note))
    }

    /// Request `iid`'s discussions are read again on the next load (after a change to it); until
    /// then they show as they are.
    pub fn stale(&mut self, iid: u64) {
        if let Some((updated, _)) = self.cache.get_mut(&iid) {
            *updated = i64::MIN;
        }
    }

    /// What shows of request `iid`'s discussions made again with the changes waiting on them.
    pub fn show(&mut self, iid: u64, waiting: &Waiting, me: Me) {
        let from = self.from.get(&iid).copied().unwrap_or(0);
        match self.cache.get(&iid) {
            Some((_, threads)) if waiting.any(iid, from) => {
                let mut threads = threads.clone();
                waiting.apply(iid, from, me, &mut threads, &mut Vec::new(), &mut None);
                self.shown.insert(iid, threads);
            }
            _ => {
                self.shown.remove(&iid);
            }
        }
    }

    /// Whether what shows of request `iid` was read after the answer at tick `done`, or is not
    /// read at all.
    pub fn holds(&self, iid: u64, done: u64) -> bool {
        !self.cache.contains_key(&iid) || self.from.get(&iid).is_some_and(|&from| from > done)
    }

    /// Whether `key` (a notice's, or an agent's answer's in the Inbox) was looked at.
    /// The notices of `list` not listed before, and not read: each once, none at the `first`
    /// read (#26).
    fn news(&mut self, list: &[Notice], first: bool) -> Vec<Notice> {
        list.iter()
            .filter(|n| self.told.insert(n.key.clone()))
            .filter(|n| !first && !self.read.contains(&n.key))
            .cloned()
            .collect()
    }

    pub fn is_read(&self, key: &str) -> bool {
        self.read.contains(key)
    }

    /// `key` looked at: no longer bold, not counted.
    pub fn mark_read(&mut self, key: String) {
        if self.read.insert(key) {
            self.save_read();
        }
    }

    pub fn unread(&self) -> usize {
        self.list
            .iter()
            .filter(|n| !self.read.contains(&n.key))
            .count()
    }

    /// Something about request `iid` is not read yet: the menu's blue dot on it (#71).
    pub fn unread_of(&self, iid: u64) -> bool {
        self.list
            .iter()
            .any(|n| n.iid == iid && !self.read.contains(&n.key))
    }

    /// Notice `ix` is not read yet.
    pub fn unread_at(&self, ix: usize) -> bool {
        self.list
            .get(ix)
            .is_some_and(|n| !self.read.contains(&n.key))
    }

    /// The requests whose discussions were read.
    pub fn iids(&self) -> Vec<u64> {
        self.cache.keys().copied().collect()
    }

    /// What a read of project `base`'s notices starts from; what was seen and read loaded first.
    pub fn begin(&mut self, base: &str) -> Reading {
        self.load_seen();
        self.load_read(base);
        Reading {
            base: base.to_owned(),
            seen: self.seen.clone(),
            known: self.known_resolved.clone(),
            cache: self.cache.clone(),
        }
    }

    /// A read arrived, begun at tick `tick`: what it learned is kept and its discussions show as read (the changes
    /// waiting go on them after, `show`). None when it failed. Its notices go in with `put`.
    pub fn arrive(&mut self, found: anyhow::Result<Found>, tick: u64) -> Option<Arrived> {
        let first = !self.loaded;
        self.loaded = true;
        let found = found.ok()?;
        // What came since the last read, not read yet: to the system too (#26).
        let news = self.news(&found.list, first);
        let changed = !found.floors.is_empty()
            || !found.first_resolved.is_empty()
            || !found.reopened.is_empty();
        self.seen.extend(found.floors);
        self.known_resolved.extend(found.first_resolved);
        for key in found.reopened {
            self.known_resolved.remove(&key);
        }
        if changed {
            self.save_seen();
        }
        self.cache = found.fresh;
        // What was read again holds the changes answered before it began (#139).
        for iid in found.read_again {
            self.from.insert(iid, tick);
        }
        let cache = &self.cache;
        self.from.retain(|iid, _| cache.contains_key(iid));
        self.shown.clear();
        Some(Arrived {
            news,
            list: found.list,
            complete: found.complete,
        })
    }

    /// A read's notices listed; the selected one stays selected where it moved to.
    pub fn put(&mut self, arrived: Arrived) {
        let list = arrived.list;
        // Everything read: only then may read keys no longer listed be forgotten.
        if arrived.complete {
            let before = self.read.len();
            self.read.retain(|k| list.iter().any(|n| &n.key == k));
            if self.read.len() != before {
                self.save_read();
            }
        }
        let selected = self.list.get(self.at).map(|n| n.key.clone());
        self.list = list;
        self.at = selected
            .and_then(|key| self.list.iter().position(|n| n.key == key))
            .unwrap_or(self.at)
            .min(self.list.len().saturating_sub(1));
    }

    /// Everything is read afresh, not only what reports a change.
    pub fn forget(&mut self) {
        self.at = 0;
        self.cache.clear();
    }

    /// Notice `ix` opened: read from now on.
    pub fn open(&mut self, ix: usize) -> Option<Notice> {
        let n = self.list.get(ix).cloned()?;
        self.mark_read(n.key.clone());
        Some(n)
    }

    /// Done with notice `ix` of project `base`: gone from the list; a discussion resolved is known, a comment
    /// counts as read from now on. The notice, to put back if its to-do is not marked done.
    pub fn dismiss(&mut self, ix: usize, base: Option<&str>) -> Option<Notice> {
        if ix >= self.list.len() {
            return None;
        }
        let n = self.list.remove(ix);
        self.at = self.at.min(self.list.len().saturating_sub(1));
        if let Some(key) = n.resolved.clone() {
            self.known_resolved.insert(key);
            self.save_seen();
        } else if let (Some(id), Some(base)) = (n.note.filter(|_| n.todo.is_none()), base) {
            let key = format!("{base}|{}", n.iid);
            let floor = self.seen.entry(key).or_default();
            *floor = (*floor).max(id);
            self.save_seen();
        }
        Some(n)
    }

    /// Notice `n`, dismissed at `ix`, back where it was: its to-do was not marked done.
    pub fn put_back(&mut self, ix: usize, n: Notice) {
        if !self.list.iter().any(|m| m.key == n.key) {
            let at = ix.min(self.list.len());
            self.list.insert(at, n);
        }
    }

    /// Everything listed, and the `others` (an agent's answers in the Inbox), counts as read.
    pub fn read_all(&mut self, others: impl IntoIterator<Item = String>) {
        let keys: Vec<String> = self.list.iter().map(|n| n.key.clone()).collect();
        self.read.extend(keys.into_iter().chain(others));
        self.save_read();
    }

    fn load_seen(&mut self) {
        if self.seen_loaded {
            return;
        }
        self.seen_loaded = true;
        self.seen = storage::read("seen-notes.txt")
            .lines()
            .filter_map(|l| {
                let (key, id) = l.split_once('\t')?;
                Some((key.to_owned(), id.parse().ok()?))
            })
            .collect();
        self.known_resolved = storage::read("seen-resolved.txt")
            .lines()
            .filter(|l| !l.is_empty())
            .map(str::to_owned)
            .collect();
    }

    /// The read notices of project `base`, once per project.
    fn load_read(&mut self, base: &str) {
        if self.read_base.as_deref() != Some(base) {
            self.read = read_of(&storage::read(READ_FILE), base);
            self.read_base = Some(base.to_owned());
        }
    }

    /// Writes this project's read notices; other projects' lines (other tabs) are kept as on disk.
    fn save_read(&self) {
        if let Some(base) = &self.read_base {
            storage::save(
                READ_FILE,
                with_read(&storage::read(READ_FILE), base, &self.read),
            );
        }
    }

    fn save_seen(&self) {
        let mut lines: Vec<String> = self.seen.iter().map(|(k, v)| format!("{k}\t{v}")).collect();
        lines.sort_unstable();
        storage::save("seen-notes.txt", lines.join("\n") + "\n");
        let mut known: Vec<&String> = self.known_resolved.iter().collect();
        known.sort_unstable();
        let known: Vec<&str> = known.into_iter().map(String::as_str).collect();
        storage::save("seen-resolved.txt", known.join("\n") + "\n");
    }
}

const READ_FILE: &str = "seen-read.txt";

/// The read notice keys of project `base` in `text`: lines `base<TAB>key`.
fn read_of(text: &str, base: &str) -> HashSet<String> {
    text.lines()
        .filter_map(|l| l.split_once('\t'))
        .filter(|(b, _)| *b == base)
        .map(|(_, key)| key.to_owned())
        .collect()
}

/// `text` with project `base`'s lines replaced by `read`.
fn with_read(text: &str, base: &str, read: &HashSet<String>) -> String {
    let mut keys: Vec<&String> = read.iter().collect();
    keys.sort_unstable();
    let mut lines: Vec<String> = text
        .lines()
        .filter(|l| l.split_once('\t').is_some_and(|(b, _)| b != base))
        .map(str::to_owned)
        .chain(keys.into_iter().map(|k| format!("{base}\t{k}")))
        .collect();
    lines.sort_unstable();
    lines.join("\n") + "\n"
}

/// The new comments of request `mr` for user `me`: after `floor` (the newest already known), by
/// someone else — in a discussion `me` wrote in only those after `me`'s last note there (what came
/// before was answered), and in any discussion of `me`'s own request.
pub fn replies(me: &str, mr: &Mr, threads: &[Thread], floor: u64) -> Vec<Notice> {
    let mine = mr.author_login == me;
    let mut out = Vec::new();
    for t in threads {
        let last_mine = t.notes.iter().filter(|n| n.login == me).map(|n| n.id).max();
        let took_part = last_mine.is_some();
        if !(took_part || mine) {
            continue;
        }
        let after = floor.max(last_mine.unwrap_or(0));
        for n in t.notes.iter().filter(|n| n.id > after && n.login != me) {
            out.push(Notice {
                key: format!("note:{}", n.id),
                kind: if took_part {
                    Kind::Reply
                } else {
                    Kind::OnYours
                },
                iid: mr.iid,
                title: mr.title.clone(),
                who: n.author.clone(),
                text: n.body.clone(),
                created: n.created,
                note: Some(n.id),
                todo: None,
                resolved: None,
            });
        }
    }
    out
}

/// The discussions of request `mr` that `me` took part in and someone else resolved, not yet in
/// `known` (keys `base|iid|discussion`).
pub fn resolutions(
    me: &str,
    base: &str,
    mr: &Mr,
    threads: &[Thread],
    known: &HashSet<String>,
) -> Vec<Notice> {
    threads
        .iter()
        .filter(|t| t.resolved && t.notes.iter().any(|n| n.login == me))
        .filter_map(|t| {
            let (who, login) = t.resolved_by.clone()?;
            let key = format!("{base}|{}|{}", mr.iid, t.id);
            (login != me && !known.contains(&key)).then(|| Notice {
                key: format!("resolved:{key}"),
                kind: Kind::Resolved,
                iid: mr.iid,
                title: mr.title.clone(),
                who,
                text: t.notes.first().map(|n| n.body.clone()).unwrap_or_default(),
                created: t.resolved_at,
                note: t.notes.first().map(|n| n.id),
                todo: None,
                resolved: Some(key),
            })
        })
        .collect()
}

fn from_todo(t: &Todo) -> Notice {
    let note = match links::parse(&t.url).map(|l| l.target) {
        Some(Target::Request { note, .. }) => note,
        _ => None,
    };
    Notice {
        key: format!("todo:{}", t.id),
        kind: match t.action.as_str() {
            "mentioned" | "directly_addressed" => Kind::Mention,
            "review_requested" => Kind::Review,
            _ => Kind::Todo,
        },
        iid: t.iid,
        title: t.title.clone(),
        who: t.author.clone(),
        text: t.body.clone(),
        created: t.created,
        note,
        todo: Some(t.id),
        resolved: None,
    }
}

/// What a read of the notices takes from what is kept: the project, the newest comment known of each request, the
/// resolved discussions known, the discussions as last read.
pub struct Reading {
    base: String,
    seen: HashMap<String, u64>,
    known: HashSet<String>,
    cache: HashMap<u64, (i64, Vec<Thread>)>,
}

/// What a read found.
pub struct Found {
    list: Vec<Notice>,
    floors: HashMap<String, u64>,
    first_resolved: Vec<String>,
    reopened: Vec<String>,
    fresh: HashMap<u64, (i64, Vec<Thread>)>,
    read_again: Vec<u64>,
    complete: bool,
}

/// A read taken in (`Notices::arrive`): the notices news to the user, and the list for `Notices::put`.
pub struct Arrived {
    pub news: Vec<Notice>,
    list: Vec<Notice>,
    complete: bool,
}

/// Reads the to-dos and the discussions of `requests` from `host`: what changed since `reading`, in the
/// background.
pub fn read(host: &dyn Host, requests: &[Mr], reading: Reading) -> anyhow::Result<Found> {
    let Reading {
        base,
        seen,
        known,
        cache,
    } = reading;
    let me = host.username()?;
    let todos = host.todos();
    // Everything read: only then may read keys no longer listed be forgotten.
    let mut complete = todos.is_ok();
    let mut notices: Vec<Notice> = todos.unwrap_or_default().iter().map(from_todo).collect();
    let mut floors = HashMap::new();
    let mut first_resolved = Vec::new();
    let mut reopened = Vec::new();
    let mut fresh = HashMap::new();
    let mut read_again = Vec::new();
    for m in requests.iter().take(MOST_REQUESTS) {
        // Read again only what changed since the last look.
        let threads = match cache.get(&m.iid) {
            Some((updated, threads)) if *updated == m.updated => threads.clone(),
            _ => match host.threads(m.iid) {
                Ok(threads) => {
                    read_again.push(m.iid);
                    threads
                }
                Err(_) => {
                    complete = false;
                    continue;
                }
            },
        };
        let key = format!("{base}|{}", m.iid);
        let resolved_keys: Vec<String> = threads
            .iter()
            .filter(|t| t.resolved)
            .map(|t| format!("{key}|{}", t.id))
            .collect();
        match seen.get(&key) {
            Some(&floor) => {
                notices.extend(replies(&me, m, &threads, floor));
                notices.extend(resolutions(&me, &base, m, &threads, &known));
                // Opened again: a later resolve is news again.
                reopened.extend(
                    threads
                        .iter()
                        .filter(|t| !t.resolved)
                        .map(|t| format!("{key}|{}", t.id))
                        .filter(|k| known.contains(k)),
                );
            }
            // A request seen for the first time: what is there now is not news.
            None => {
                let top = threads
                    .iter()
                    .flat_map(|t| t.notes.iter().map(|n| n.id))
                    .max()
                    .unwrap_or(0);
                floors.insert(key, top);
                first_resolved.extend(resolved_keys);
            }
        }
        fresh.insert(m.iid, (m.updated, threads));
    }
    notices.sort_by_key(|n| std::cmp::Reverse(n.created));
    Ok(Found {
        list: notices,
        floors,
        first_resolved,
        reopened,
        fresh,
        read_again,
        complete,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use artoo_hosts_api::Note;

    fn note(id: u64, login: &str) -> Note {
        Note {
            id,
            author: login.to_uppercase(),
            login: login.into(),
            body: format!("note {id}"),
            created: id as i64,
        }
    }

    fn thread(notes: Vec<Note>) -> Thread {
        Thread {
            id: "t".into(),
            notes,
            path: Some("a.rs".into()),
            new_line: Some(1),
            old_line: None,
            resolved: false,
            resolvable: true,
            resolved_by: None,
            resolved_at: 0,
            commit: None,
            outdated: false,
        }
    }

    fn request(author: &str) -> Mr {
        Mr {
            iid: 7,
            title: "Clamp".into(),
            author: author.into(),
            source: "f".into(),
            target: "main".into(),
            url: String::new(),
            updated: 0,
            created: 0,
            head: String::new(),
            comments: 0,
            draft: false,
            author_login: author.into(),
            reviewers: vec!["you".into()],
            description: String::new(),
            ..Default::default()
        }
    }

    #[test]
    fn replies_come_from_discussions_one_wrote_in_after_the_floor() {
        let threads = [
            thread(vec![note(1, "you"), note(2, "anna"), note(5, "boris")]),
            thread(vec![note(3, "anna"), note(6, "boris")]),
        ];
        let got = replies("you", &request("anna"), &threads, 2);
        let ids: Vec<_> = got.iter().map(|n| n.note).collect();
        assert_eq!(ids, vec![Some(5)]);
        assert_eq!(got[0].kind, Kind::Reply);
    }

    #[test]
    fn a_discussion_one_wrote_in_resolved_by_someone_else_is_news_once() {
        let mut done = thread(vec![note(1, "you"), note(2, "anna")]);
        done.resolved = true;
        done.resolved_by = Some(("Anna".into(), "anna".into()));
        let mut mine = thread(vec![note(3, "you")]);
        mine.id = "u".into();
        mine.resolved = true;
        mine.resolved_by = Some(("You".into(), "you".into()));
        let threads = [done, mine];
        let got = resolutions("you", "b", &request("anna"), &threads, &HashSet::new());
        assert_eq!(got.len(), 1);
        assert_eq!((got[0].kind, got[0].who.as_str()), (Kind::Resolved, "Anna"));
        let known: HashSet<String> = ["b|7|t".to_owned()].into();
        assert!(resolutions("you", "b", &request("anna"), &threads, &known).is_empty());
    }

    #[test]
    fn on_ones_own_request_every_new_comment_counts() {
        let threads = [thread(vec![
            note(3, "anna"),
            note(4, "you"),
            note(6, "boris"),
        ])];
        let got = replies("you", &request("you"), &threads, 3);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].note, Some(6));
    }

    #[test]
    fn a_comment_one_answered_is_not_a_reply() {
        // #2: boris comments on your line on anna's request; you answer. Boris did not reply to you.
        let answered = [thread(vec![note(5, "boris"), note(6, "you")])];
        assert!(replies("you", &request("anna"), &answered, 4).is_empty());
        // On your own request too: what you answered is not news.
        let own = [thread(vec![
            note(5, "boris"),
            note(6, "you"),
            note(8, "boris"),
        ])];
        let got = replies("you", &request("you"), &own, 4);
        assert_eq!(
            got.iter().map(|n| n.note).collect::<Vec<_>>(),
            vec![Some(8)]
        );
        assert_eq!(got[0].kind, Kind::Reply);
    }

    #[test]
    fn only_what_comes_after_the_first_read_is_news_and_once() {
        let notice = |key: &str| Notice {
            key: key.into(),
            kind: Kind::Review,
            iid: 1,
            title: String::new(),
            who: String::new(),
            text: String::new(),
            created: 0,
            note: None,
            todo: None,
            resolved: None,
        };
        let mut notices = Notices::default();
        let keys = |news: Vec<Notice>| news.into_iter().map(|n| n.key).collect::<Vec<_>>();
        assert!(notices.news(&[notice("todo:1")], true).is_empty());
        notices.read.insert("todo:3".into());
        let list = [notice("todo:1"), notice("todo:2"), notice("todo:3")];
        assert_eq!(keys(notices.news(&list, false)), ["todo:2"]);
        assert!(notices.news(&list, false).is_empty());
    }

    #[test]
    fn read_notices_are_kept_per_project() {
        let disk = "a\tnote:1\nb\tnote:2\nb\ttodo:3\n";
        let read = read_of(disk, "b");
        assert_eq!(read, ["note:2".to_owned(), "todo:3".to_owned()].into());
        // b's lines replaced, a's (another tab) kept.
        let now: HashSet<String> = ["todo:3".to_owned(), "note:9".to_owned()].into();
        let saved = with_read(disk, "b", &now);
        assert_eq!(saved, "a\tnote:1\nb\tnote:9\nb\ttodo:3\n");
        assert_eq!(read_of(&saved, "b"), now);
        assert_eq!(read_of(&saved, "a"), ["note:1".to_owned()].into());
    }
}
