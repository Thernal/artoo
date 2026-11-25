//! How current the Inbox is (round 41, Design `docs/specs/loading-stability.md`, `inbox.html?s=…`): its slot, and the
//! new notices a read brings while the list is on screen, held behind `1 new ↑` so no row moves under the pointer.
//! They land on a click, `⌘R` or when the Inbox is left, and fade in where they land.

use super::{Notice, Notices};
use crate::freshness::Freshness;
use artoo_ui_kit::format;
use artoo_ui_kit::fresh::{Held, Slot};
use artoo_ui_kit::motion;
use std::collections::HashSet;
use std::time::Instant;

/// What the Inbox keeps about its reads.
#[derive(Default)]
pub struct Current {
    /// The list as the last read left it, while some of it waits to be shown.
    pub(super) held: Option<Vec<Notice>>,
    /// The Inbox is on screen: only then does a new notice wait.
    watched: bool,
    /// The rows just done, drawn on their way out, and when each was done.
    leaving: Vec<(Notice, Instant)>,
    /// The keys that just landed, and when.
    landed: Option<(HashSet<String>, Instant)>,
    /// When what is listed was read.
    read_at: Option<i64>,
    /// What is listed is the copy the last run kept.
    from_disk: bool,
    /// The read of everything, and the one of the to-dos alone, under way.
    reading_all: bool,
    reading_todos: bool,
    /// The slot's and the hairline's timing, and the first read's tints.
    pub(crate) clock: Freshness,
}

/// How long a row done is kept to be drawn: its fade, its gap closing, and the frames after.
const LEAVES: std::time::Duration = std::time::Duration::from_millis(500);

impl Current {
    /// Row `n` done at `at`: drawn until it has gone.
    pub(super) fn leave(&mut self, n: Notice, at: Instant) {
        self.leaving.retain(|(_, since)| at < *since + LEAVES);
        self.leaving.push((n, at));
    }

    /// Row `key` is back.
    pub(super) fn stay(&mut self, key: &str) {
        self.leaving.retain(|(n, _)| n.key != key);
    }

    pub(super) fn leaving(&self, at: Instant) -> Vec<&Notice> {
        let mut rows: Vec<&Notice> = (self.leaving.iter())
            .filter(|(_, since)| at < *since + LEAVES)
            .map(|(n, _)| n)
            .collect();
        rows.sort_by_key(|n| std::cmp::Reverse(n.created));
        rows
    }
}

impl Notices {
    pub fn reading(&self) -> bool {
        self.current.reading_all || self.current.reading_todos
    }

    /// A read of everything (`all`) or of the to-dos alone begins.
    pub(super) fn reading_begins(&mut self, all: bool) {
        let nothing = all && self.list.is_empty() && !self.loaded;
        match all {
            true => self.current.reading_all = true,
            false => self.current.reading_todos = true,
        }
        self.current.clock.begin(nothing, Instant::now());
    }

    /// That read answered, `ok` or not. A to-do read alone does not date the list: only a read of everything does.
    pub(super) fn reading_ends(&mut self, all: bool, ok: bool) {
        match all {
            true => self.current.reading_all = false,
            false => self.current.reading_todos = false,
        }
        self.failed = !ok;
        if ok && all {
            self.current.read_at = Some(format::now());
            self.current.from_disk = false;
        }
        if !self.reading() {
            self.current.clock.end(Instant::now());
        }
    }

    /// What is listed came from the last run's copy, read at `at`.
    pub(super) fn date_kept(&mut self, at: i64) {
        self.current.read_at = Some(at);
        self.current.from_disk = true;
    }

    /// Another project's, or none to read: nothing waits and nothing is dated.
    pub(super) fn forget_reads(&mut self) {
        let watched = self.current.watched;
        self.current = Current {
            watched,
            ..Current::default()
        };
    }

    /// The Inbox came on screen, or was left: leaving lands what waits. Whether rows changed.
    pub fn watch(&mut self, on: bool) -> bool {
        if self.current.watched == on {
            return false;
        }
        self.current.watched = on;
        if on {
            return false;
        }
        let landed = self.apply_held();
        self.current.landed = None;
        landed
    }

    /// Lists what a read found. While the Inbox shows a list, one with a new or a gone notice waits: the rows shown
    /// change their fields where they are, none comes or goes.
    pub(super) fn take(&mut self, read: Vec<Notice>) {
        let shows = self.current.watched && !self.list.is_empty();
        let listed = |list: &[Notice], key: &str| list.iter().any(|n| n.key == key);
        let moves = shows
            && (read.iter().any(|n| !listed(&self.list, &n.key))
                || self.list.iter().any(|n| !listed(&read, &n.key)));
        if !moves {
            self.current.held = None;
            return self.show_list(read);
        }
        let shown = (self.list.iter())
            .map(|old| {
                let new = read.iter().find(|n| n.key == old.key);
                new.unwrap_or(old).clone()
            })
            .collect();
        self.current.held = Some(read);
        self.show_list(shown);
    }

    /// `list` on screen; the selected notice stays selected where it is now.
    fn show_list(&mut self, list: Vec<Notice>) {
        let selected = self.list.get(self.at).map(|n| n.key.clone());
        self.list = list;
        self.at = selected
            .and_then(|key| self.list.iter().position(|n| n.key == key))
            .unwrap_or(self.at)
            .min(self.list.len().saturating_sub(1));
    }

    /// The notices waiting to be shown.
    pub fn waiting_new(&self) -> impl Iterator<Item = &Notice> {
        (self.current.held.iter().flatten())
            .filter(|n| !self.is_pending(n) && !self.list.iter().any(|m| m.key == n.key))
    }

    /// Shows what waits. Whether anything did.
    pub fn apply_held(&mut self) -> bool {
        let new: HashSet<String> = self.waiting_new().map(|n| n.key.clone()).collect();
        let Some(mut list) = self.current.held.take() else {
            return false;
        };
        list.retain(|n| !self.is_pending(n));
        self.show_list(list);
        self.current.landed = Some((new, Instant::now()));
        true
    }

    /// Whether notice `key` landed a moment before `at`: its row fades in.
    pub fn landed(&self, key: &str, at: Instant) -> bool {
        (self.current.landed.as_ref())
            .is_some_and(|(keys, since)| at < *since + motion::SLOT_FADE && keys.contains(key))
    }

    /// The Inbox's slot, with `waiting` new notices held; `error` when the host did not answer, `refused` when it did
    /// not take the token.
    pub fn slot(&self, waiting: usize, error: bool, refused: bool) -> Slot {
        let at = self.current.read_at.map(format::clock);
        if self.reading() {
            return match at {
                None if self.list.is_empty() => Slot::Reading,
                _ => Slot::Refreshing,
            };
        }
        if error || self.failed {
            return match (refused, at) {
                (true, _) => Slot::TokenExpired,
                (false, Some(at)) => Slot::CantReach(at),
                (false, None) => Slot::CantRead,
            };
        }
        let Some(at) = at else {
            return Slot::Reading;
        };
        if self.current.from_disk {
            Slot::Kept(at)
        } else if waiting > 0 {
            Slot::Waiting(at, waiting, Held::NewAbove)
        } else {
            Slot::Updated(at)
        }
    }

    /// The Inbox as a state of `inbox.html?s=` for a snapshot, as if the host had answered so. Needs a list read
    /// before it (a fixture's).
    pub(crate) fn force_fresh(&mut self, state: &str) {
        let now = Instant::now();
        // Long enough ago that the hairline and the tints have come.
        let long = now - std::time::Duration::from_secs(2);
        self.task = None;
        self.poll = None;
        self.failed = false;
        self.loaded = true;
        self.forget_reads();
        self.current.watched = true;
        self.current.read_at = Some(format::now());
        let new = self.list.first().map(|n| Notice {
            key: "note:forced".to_owned(),
            iid: 418,
            title: "Cache refund quotes for ten minutes".to_owned(),
            who: "Ember Lind".to_owned(),
            created: format::now() - 120,
            note: None,
            todo: None,
            resolved: None,
            ..n.clone()
        });
        let with_new = new.map(|new| {
            let mut list = self.list.clone();
            list.insert(0, new);
            list
        });
        match state {
            "loading" => {
                self.list.clear();
                self.loaded = false;
                self.current.read_at = None;
                self.current.reading_all = true;
                self.current.clock.begin(true, long);
            }
            "kept" => self.current.from_disk = true,
            "refreshing" => {
                self.current.reading_all = true;
                self.current.clock.begin(false, long);
            }
            "new" => self.current.held = with_new,
            "applied" => {
                if let Some(list) = with_new {
                    self.show_list(list);
                }
            }
            "error" => self.failed = true,
            "failed" => {
                self.list.clear();
                self.failed = true;
                self.loaded = false;
                self.current.read_at = None;
            }
            // `loaded`, and anything else.
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notices::Arrived;

    fn notice(key: &str, created: i64) -> Notice {
        Notice {
            key: key.to_owned(),
            created,
            todo: None,
            ..super::super::tests::todo_notice(1)
        }
    }

    fn read(list: Vec<Notice>) -> Arrived {
        Arrived {
            news: Vec::new(),
            list,
            complete: false,
        }
    }

    fn keys(n: &Notices) -> Vec<&str> {
        n.list.iter().map(|n| n.key.as_str()).collect()
    }

    #[test]
    fn a_new_notice_is_held_and_lands_on_apply() {
        let mut n = Notices::default();
        n.watch(true);
        n.put(read(vec![notice("b", 2), notice("a", 1)]));
        assert_eq!(keys(&n), ["b", "a"], "the first list shows at once");
        n.at = 1;
        let mut changed = notice("a", 1);
        changed.title = "Renamed".to_owned();
        n.put(read(vec![notice("c", 3), notice("b", 2), changed]));
        assert_eq!(keys(&n), ["b", "a"], "the new one waits");
        assert_eq!(n.list[1].title, "Renamed", "a field changes in place");
        assert_eq!(n.waiting_new().count(), 1);
        n.loaded = true;
        n.reading_ends(true, true);
        assert!(matches!(
            n.slot(1, false, false),
            Slot::Waiting(_, 1, Held::NewAbove)
        ));
        assert!(n.apply_held());
        assert_eq!(keys(&n), ["c", "b", "a"]);
        assert_eq!(n.at, 2, "the selected one stays selected");
        assert!(n.landed("c", Instant::now()));
        assert!(!n.landed("b", Instant::now()));
        assert!(!n.apply_held());
    }

    #[test]
    fn leaving_the_inbox_lands_what_waits_and_away_nothing_does() {
        let mut n = Notices::default();
        n.watch(true);
        n.put(read(vec![notice("a", 1)]));
        n.put(read(vec![notice("b", 2), notice("a", 1)]));
        assert_eq!(keys(&n), ["a"]);
        assert!(n.watch(false));
        assert_eq!(keys(&n), ["b", "a"]);
        n.put(read(vec![notice("c", 3), notice("b", 2)]));
        assert_eq!(keys(&n), ["c", "b"], "not on screen: shown at once");
    }

    #[test]
    fn a_notice_done_here_does_not_come_back_with_what_waited() {
        let mut n = Notices::default();
        n.watch(true);
        n.put(read(vec![notice("a", 1), notice("b", 0)]));
        n.put(read(vec![notice("c", 2), notice("a", 1), notice("b", 0)]));
        let done = n.dismiss(0).unwrap();
        assert_eq!(done.key, "a");
        n.apply_held();
        assert_eq!(keys(&n), ["c", "b"]);
    }

    #[test]
    fn the_slot_says_how_old_the_list_is() {
        let mut n = Notices::default();
        n.reading_begins(true);
        assert_eq!(n.slot(0, false, false), Slot::Reading);
        n.reading_ends(true, false);
        assert_eq!(n.slot(0, false, false), Slot::CantRead);
        n.list = vec![notice("a", 1)];
        n.date_kept(0);
        n.failed = false;
        assert!(matches!(n.slot(0, false, false), Slot::Kept(_)));
        n.reading_begins(true);
        assert_eq!(n.slot(0, false, false), Slot::Refreshing);
        n.reading_ends(true, false);
        assert!(matches!(n.slot(0, false, false), Slot::CantReach(_)));
        assert_eq!(n.slot(0, true, true), Slot::TokenExpired);
        n.loaded = true;
        n.reading_ends(true, true);
        assert!(matches!(n.slot(0, false, false), Slot::Updated(_)));
    }
}
