//! The requests list (#72, #81, #189): grouped by whose turn each request is (or its author, or not at all, as
//! `Display ▾` says), a group's band folding its rows; the folds, the display and when you last read each request
//! kept per project; every row's pipeline, approvals, discussions, pushes and size read in the background, the
//! picked one's merge state and what changed since your review for its preview. Its views draw the rows under their
//! bands, the preview, why none is listed and the banner while the list cannot be read; the tab puts them in the
//! root's frame (`ListLook`, `Body`).

use crate::group::{Band, Group, GroupBy, SortBy};
use crate::requests::{Act, Acts, Requests, Scope};
use crate::turn::{self, Turn};
use artoo_core::git::{DiffSettings, Repo};
use artoo_core::{markdown, rank, storage};
use artoo_hosts_api::{Approvals, Host, Mr, MrState, Pipeline, Thread};
use artoo_review_api::Seen;
use artoo_ui_kit::controls::{empty_state, outline_button, primary_button, unread_dot};
use artoo_ui_kit::format::{self, plural};
use artoo_ui_kit::hover::{self, Hover, RowTip};
use artoo_ui_kit::room::Widths;
use artoo_ui_kit::{ISLAND_RADIUS, ROW_RADIUS, discussion, flat, icons, motion, px, theme, words};
use gpui::{AnyElement, ClickEvent, Context, Div, FontWeight, SharedString, Task, div, prelude::*};
use std::cell::Cell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// What the requests list keeps per project (`<repo>\t<folded groups>\t<requests read>\t<folds>\t<display>`, the
/// first two comma-separated, a request read as `<iid>@<its update then>`; `folds` once a group was folded or
/// unfolded by hand; the display as `group::display_key` writes it) (#81, #189). Every group starts open (the
/// developer, 2026-10-23); a line written before the folds were yours carries the old default, read as nothing
/// folded.
const LIST_FILE: &str = "requests-list.txt";

/// The height of a group's band in the requests list.
pub const BAND: f32 = 28.;

pub use artoo_hosts_api::Size;

/// What a request's row says, read for every request listed (#189): its pipeline, approvals, discussions, when it
/// was pushed and how large it is, with its `updated` then. Kept between starts (`FACTS_FILE`).
#[derive(Default, serde::Serialize, serde::Deserialize)]
pub struct Facts {
    updated: i64,
    pub pipeline: Option<Pipeline>,
    pub approvals: Option<Approvals>,
    pub threads: Vec<Thread>,
    pub pushes: Vec<i64>,
    pub size: Option<Size>,
    /// When this run read them; none for those kept from the last start, which count as fresh at their update.
    #[serde(skip)]
    at: Option<Instant>,
}

/// A row's pipeline and approvals change without the request's `updated` moving: they are read again after this.
const FACTS_TTL: Duration = Duration::from_secs(300);

impl Facts {
    /// Whether these facts still stand for a request at `updated`: read at it, and (when read in this run) not
    /// older than `FACTS_TTL`.
    fn stands(&self, updated: i64) -> bool {
        self.updated == updated && self.at.is_none_or(|at| at.elapsed() < FACTS_TTL)
    }
}

/// The picked request's preview, beyond its row: what changed since your review, with
/// its `updated` then.
pub struct Preview {
    updated: i64,
    /// Pushed since your review: the version you viewed, and how many files changed since then
    /// (X22).
    pub since: Option<(usize, usize)>,
}

/// The list's rows: their facts and previews, its scroll, the groups folded, how they are displayed and when each
/// request was read.
#[derive(Default)]
pub struct Rows {
    facts: HashMap<u64, Facts>,
    reading: HashSet<u64>,
    /// The requests whose facts are still to read, the next first: the picked one, then as the list shows them.
    queue: VecDeque<u64>,
    /// How many reads go on at once, `LANES` at most.
    workers: usize,
    /// The project's default branch: a row names its target only when it is another (#189).
    pub main_line: Option<String>,
    previews: HashMap<u64, Preview>,
    preview_task: Option<Task<()>>,
    /// The requests list's scroll, and whether the next frame brings the picked row into view
    /// (after ↑ ↓ or esc back to the list).
    pub scroll: gpui::ScrollHandle,
    pub reveal: Cell<bool>,
    /// The request picked, previewed: by a click, by ↑ ↓, or the one open as you came back.
    pub picked: Option<u64>,
    /// Project `kept_of`'s requests list: the groups folded, the requests read and their update then — a row's dot
    /// says something new since (#189).
    folded: HashSet<Group>,
    read: HashMap<u64, i64>,
    /// A group was folded or unfolded by hand: the folds are kept.
    chosen: bool,
    /// `Display ▾`: how the rows are grouped and ordered.
    pub display: (GroupBy, SortBy),
    kept_of: Option<String>,
}

/// A request read before its update was kept: nothing it has is new.
const READ_BEFORE: i64 = i64::MAX;
/// How many requests' facts are read at once: each asks the host about four things (#189).
const LANES: usize = 3;
/// Every project's open requests' facts, by request, kept between starts: a start reads only what was updated since.
const FACTS_FILE: &str = "requests-facts.json";

impl Rows {
    /// Project `repo`'s folds, display and requests read, unless they are the ones held.
    pub fn load(&mut self, repo: &str) {
        if self.kept_of.as_deref() == Some(repo) {
            return;
        }
        let kept = kept_list(&storage::read(LIST_FILE), repo).unwrap_or_default();
        (self.folded, self.read, self.chosen, self.display) =
            (kept.folded, kept.read, kept.chosen, kept.display);
        // Another project: its own requests' facts, as they were last read.
        self.facts = kept_facts(&storage::read(FACTS_FILE), repo);
        self.reading.clear();
        self.queue.clear();
        self.previews.clear();
        self.kept_of = Some(repo.to_owned());
    }

    /// The project whose list this is (`load`'s key): where the open list is kept on disk.
    pub fn kept_key(&self) -> Option<&str> {
        self.kept_of.as_deref()
    }

    /// The facts of the requests in `open`, kept for the next start; those of requests no longer open go, other
    /// projects' stay as they were.
    fn keep_facts(&self, open: &HashSet<u64>) {
        if let Some(repo) = &self.kept_of {
            let mine = (self.facts.iter()).filter(|(iid, _)| open.contains(iid));
            storage::save(
                FACTS_FILE,
                with_kept_facts(&storage::read(FACTS_FILE), repo, mine),
            );
        }
    }

    /// The next request whose facts are wanted, taken off the queue: open, not read at its update, not being read.
    fn next_to_read(&mut self, list: &[Mr]) -> Option<Mr> {
        while let Some(iid) = self.queue.pop_front() {
            let Some(m) = list
                .iter()
                .find(|m| m.iid == iid && m.state == MrState::Opened)
            else {
                continue;
            };
            let fresh = self.facts.get(&iid).is_some_and(|f| f.stands(m.updated));
            if fresh || !self.reading.insert(iid) {
                continue;
            }
            return Some(m.clone());
        }
        None
    }

    fn keep(&self) {
        if let Some(repo) = &self.kept_of {
            let text = storage::read(LIST_FILE);
            storage::save(
                LIST_FILE,
                with_kept_list(
                    &text,
                    repo,
                    &Kept {
                        folded: self.folded.clone(),
                        read: self.read.clone(),
                        chosen: self.chosen,
                        display: self.display,
                    },
                ),
            );
        }
    }

    /// Whether group `g`'s rows are folded away; a filter typed (`filtering`) opens every group.
    pub fn folds(&self, g: &Group, filtering: bool) -> bool {
        !filtering && self.folded.contains(g)
    }

    /// Group `g` folded or unfolded by hand.
    pub fn toggle(&mut self, g: Group) {
        self.chosen = true;
        if !self.folded.remove(&g) {
            self.folded.insert(g);
        }
        self.keep();
    }

    /// `Display ▾` changed.
    pub fn set_display(&mut self, group: Option<GroupBy>, sort: Option<SortBy>) {
        self.display = (
            group.unwrap_or(self.display.0),
            sort.unwrap_or(self.display.1),
        );
        self.keep();
    }

    /// Whether request `iid` was opened here (#81).
    pub fn opened(&self, iid: u64) -> bool {
        self.read.contains_key(&iid)
    }

    /// Something new on request `m` since you last read it: it was updated after (#189).
    pub fn news(&self, m: &Mr) -> bool {
        self.read.get(&m.iid).is_some_and(|&at| m.updated > at)
    }

    /// Request `iid`'s facts, once read.
    pub fn facts(&self, iid: u64) -> Option<&Facts> {
        self.facts.get(&iid)
    }

    /// Request `iid`'s merge state and what changed since your review, when they were read.
    pub fn preview(&self, iid: u64) -> Option<&Preview> {
        self.previews.get(&iid)
    }
}

/// The order requests `a` and `b` come in under a band, by `sort`: a draft after the others (#189).
fn order(sort: SortBy, size: &dyn Fn(&Mr) -> usize) -> impl Fn(&Mr) -> (bool, i64) + '_ {
    move |m| {
        let key = match sort {
            SortBy::Updated => -m.updated,
            // The one waiting longest first: when it was opened, not its last note (L17).
            SortBy::Oldest => m.created,
            SortBy::Size => -(size(m) as i64),
        };
        (m.draft, key)
    }
}

impl Requests {
    /// The requests the scope and `filter` let through, in the order the list shows them: the
    /// merged or the closed as read, newest first, the host having picked the scope's (#114,
    /// #169); the open under their groups (`group_of`), ordered as `Display ▾` says, a draft last in its group.
    /// Copied: a row is drawn with the tab's context.
    pub fn shown(&self, filter: &str) -> Vec<Mr> {
        let filter = filter.to_lowercase();
        let terms: Vec<&str> = filter.split_whitespace().collect();
        let past = self.view != MrState::Opened;
        let list = if past {
            &self.past.of_scope(self.scope).list
        } else {
            &self.list
        };
        let mut shown: Vec<&Mr> = list
            .iter()
            .filter(|m| past || self.in_scope(m, self.scope))
            .filter(|m| filter_lets(m, &terms))
            .collect();
        if !past {
            let size = |m: &Mr| self.size_of(m.iid).map_or(0, |(_, a, r, _)| a + r);
            let order = order(self.rows.display.1, &size);
            shown.sort_by_key(|m| (self.group_of(m), order(m)));
        }
        shown.into_iter().cloned().collect()
    }

    /// The next request to review below the open one, going round the list once: the first whose turn is yours or
    /// someone else's, not one you approved, not yours (#81, #189). Counted from where the open one was when it was
    /// opened. `filter` the list's.
    pub fn next_request(&self, filter: &str) -> Option<Mr> {
        let mut shown = self.shown(filter);
        let walk = self
            .walk
            .clone()
            .filter(|(iid, _)| Some(*iid) == self.current);
        if let Some((walked, was)) = walk
            && self.view == MrState::Opened
        {
            let size = |m: &Mr| self.size_of(m.iid).map_or(0, |(_, a, r, _)| a + r);
            let order = order(self.rows.display.1, &size);
            shown.sort_by_key(|m| {
                let group = if m.iid == walked {
                    was.clone()
                } else {
                    self.group_of(m)
                };
                (group, order(m))
            });
        }
        let at = shown.iter().position(|m| Some(m.iid) == self.current);
        let open: Vec<bool> = shown
            .iter()
            .map(|m| matches!(self.turn_group(m), Group::You | Group::Others))
            .collect();
        next_to_review(&open, at).map(|ix| shown[ix].clone())
    }

    /// Request `m`'s group in the list, as `Display ▾` groups it.
    pub fn group_of(&self, m: &Mr) -> Group {
        match self.rows.display.0 {
            GroupBy::Turn => self.turn_group(m),
            GroupBy::Author => Group::Author(m.author.clone()),
            GroupBy::None => Group::All,
        }
    }

    /// Request `m`'s group by whose turn it is: yours, else as its facts say; until they are read, yours when you
    /// are asked to review it, else someone else's.
    pub fn turn_group(&self, m: &Mr) -> Group {
        let me = self.login.as_deref().unwrap_or_default();
        let mine = !me.is_empty() && m.author_login == me;
        match self.turn_of(m) {
            Some(t) => t.group(mine),
            None if mine => Group::Yours,
            None if !me.is_empty() && m.reviewers.iter().any(|r| r == me) => Group::You,
            None => Group::Others,
        }
    }

    /// Whose turn request `m` is, once its facts are read.
    pub fn turn_of(&self, m: &Mr) -> Option<Turn> {
        let me = self.login.as_deref().unwrap_or_default();
        let f = self.rows.facts(m.iid)?;
        Some(turn::turn(
            me,
            m,
            f.approvals.as_ref(),
            &f.threads,
            &f.pushes,
        ))
    }

    /// Request `iid`'s size, once read.
    pub fn size_of(&self, iid: u64) -> Option<Size> {
        self.rows.facts(iid).and_then(|f| f.size)
    }

    /// Of a request's `threads`, those waiting for you, its author (`waiting`).
    pub fn waiting_for_me<'a>(&self, threads: &'a [Thread]) -> Vec<&'a Thread> {
        turn::waiting_on(self.login.as_deref().unwrap_or_default(), threads)
    }

    /// Request `iid` opened: read as it is now; its row's dot goes until it is updated again (#189). Requests no
    /// longer open are forgotten.
    pub fn mark_opened(&mut self, iid: u64) {
        let updated = self.find(iid).map_or(0, |m| m.updated);
        if self.rows.read.get(&iid).is_some_and(|&at| at >= updated) {
            return;
        }
        if !self.list.is_empty() {
            let list = &self.list;
            self.rows
                .read
                .retain(|i, _| list.iter().any(|m| m.iid == *i));
        }
        self.rows.read.insert(iid, updated);
        self.rows.keep();
    }

    /// Every open request's facts not read at its update yet, read in the background a few at a time, each shown as
    /// it comes (#189): its pipeline, approvals, discussions and pushes from `host`, its size from the clone. The
    /// picked request first, then as the list shows them; those kept from the last start at their update are not
    /// read again.
    pub fn read_facts(&mut self, host: Arc<dyn Host>, cx: &mut Context<Self>) {
        let mut queue: VecDeque<u64> = {
            let size = |m: &Mr| self.size_of(m.iid).map_or(0, |(_, a, r, _)| a + r);
            let order = order(self.rows.display.1, &size);
            let mut open: Vec<&Mr> = (self.list.iter())
                .filter(|m| m.state == MrState::Opened)
                .filter(|m| !(self.rows.facts.get(&m.iid)).is_some_and(|f| f.stands(m.updated)))
                .collect();
            open.sort_by_key(|m| (self.group_of(m), order(m)));
            open.iter().map(|m| m.iid).collect()
        };
        if let Some(picked) = self.rows.picked
            && let Some(at) = queue.iter().position(|iid| *iid == picked)
        {
            queue.remove(at);
            queue.push_front(picked);
        }
        self.rows.queue = queue;
        if self.rows.main_line.is_none()
            && let Some(root) = self.root.clone()
        {
            cx.spawn(async move |this, cx| {
                let main = cx
                    .background_executor()
                    .spawn(async move { Repo::open(&root).ok().and_then(|r| r.default_branch()) })
                    .await;
                this.update(cx, |rq, cx| {
                    rq.rows.main_line = main;
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }
        self.pump_facts(host, cx);
    }

    /// Request `iid`, picked: its facts next, if they are still to read.
    pub fn read_facts_first(&mut self, iid: u64, host: Arc<dyn Host>, cx: &mut Context<Self>) {
        if let Some(at) = self.rows.queue.iter().position(|q| *q == iid) {
            self.rows.queue.remove(at);
            self.rows.queue.push_front(iid);
            self.pump_facts(host, cx);
        }
    }

    /// Reads started until `LANES` go on at once, each taking the queue's next until it is empty.
    fn pump_facts(&mut self, host: Arc<dyn Host>, cx: &mut Context<Self>) {
        while self.rows.workers < LANES && !self.rows.queue.is_empty() {
            self.rows.workers += 1;
            let (host, root) = (host.clone(), self.root.clone());
            cx.spawn(async move |this, cx| {
                loop {
                    let next = this
                        .update(cx, |rq, _| {
                            let list = std::mem::take(&mut rq.list);
                            let next = rq.rows.next_to_read(&list);
                            rq.list = list;
                            next
                        })
                        .ok()
                        .flatten();
                    let Some(m) = next else {
                        break;
                    };
                    let iid = m.iid;
                    let (host, root) = (host.clone(), root.clone());
                    let facts = cx
                        .background_executor()
                        .spawn(async move { facts_of(&*host, root.as_deref(), &m) })
                        .await;
                    let kept = this.update(cx, |rq, cx| {
                        rq.rows.reading.remove(&iid);
                        rq.rows.facts.insert(iid, facts);
                        cx.notify();
                    });
                    if kept.is_err() {
                        return;
                    }
                }
                this.update(cx, |rq, _| {
                    rq.rows.workers -= 1;
                    if rq.rows.workers == 0 {
                        let open = rq.list.iter().map(|m| m.iid).collect();
                        rq.rows.keep_facts(&open);
                    }
                })
                .ok();
            })
            .detach();
        }
    }

    /// Request `m`, picked in the list: where merging it stands, once per update of it; with `viewed` (its head now,
    /// and the one you reviewed it at), how many files changed since.
    pub fn read_preview(
        &mut self,
        m: &Mr,
        viewed: Option<(git2::Oid, git2::Oid)>,
        host: Arc<dyn Host>,
        root: PathBuf,
        settings: DiffSettings,
        cx: &mut Context<Self>,
    ) {
        let (iid, updated) = (m.iid, m.updated);
        self.read_facts_first(iid, host.clone(), cx);
        if self
            .rows
            .previews
            .get(&iid)
            .is_some_and(|p| p.updated == updated)
        {
            return;
        }
        self.rows.preview_task = Some(cx.spawn(async move |this, cx| {
            let since = cx
                .background_executor()
                .spawn(async move {
                    viewed.and_then(|(head, viewed)| {
                        let pushes: Vec<_> = host
                            .versions(iid)
                            .ok()?
                            .iter()
                            .filter_map(|p| {
                                Some((
                                    git2::Oid::from_str(&p.head).ok()?,
                                    git2::Oid::from_str(&p.base).ok(),
                                    p.created,
                                ))
                            })
                            .collect();
                        let repo = Repo::open(&root).ok()?;
                        let versions = repo.versions_from_pushes(&pushes);
                        let from = versions.iter().find(|v| v.tip == viewed)?;
                        let to = versions.iter().find(|v| v.tip == head)?;
                        let diff = repo.version_diff(from, to, settings).ok()?;
                        Some((from.number, diff.files.len()))
                    })
                })
                .await;
            this.update(cx, |rq, cx| {
                rq.rows.previews.insert(iid, Preview { updated, since });
                cx.notify();
            })
            .ok();
        }));
    }
}

/// Request `m`'s facts, read now: from `host`, and its size from the clone at `root` when its head is there, else as
/// the host counts it (#189).
fn facts_of(host: &dyn Host, root: Option<&Path>, m: &Mr) -> Facts {
    let fetched = root.and_then(|root| {
        let repo = Repo::open(root).ok()?;
        let head = git2::Oid::from_str(&m.head)
            .ok()
            .filter(|h| repo.has_commit(*h))?;
        let base = repo
            .resolve(&format!("origin/{}", m.target))
            .or_else(|_| repo.resolve(&m.target))
            .ok()?;
        repo.range_stat(base, head)
            .ok()
            .filter(|(files, ..)| *files > 0)
    });
    let size = fetched.or_else(|| host.size(m.iid).ok().flatten());
    Facts {
        updated: m.updated,
        pipeline: host.pipeline(m.iid).ok().flatten(),
        approvals: host.approvals(m.iid).ok(),
        threads: host.threads(m.iid).unwrap_or_default(),
        pushes: host
            .versions(m.iid)
            .unwrap_or_default()
            .iter()
            .map(|p| p.created)
            .collect(),
        size,
        at: Some(Instant::now()),
    }
}

/// `text` (`FACTS_FILE`) with project `repo`'s facts replaced by `facts`; the other projects' as they were.
fn with_kept_facts<'a>(
    text: &str,
    repo: &str,
    facts: impl Iterator<Item = (&'a u64, &'a Facts)>,
) -> String {
    let mut all: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(text).unwrap_or_default();
    let mine: HashMap<u64, &Facts> = facts.map(|(iid, f)| (*iid, f)).collect();
    if let Ok(v) = serde_json::to_value(&mine) {
        all.insert(repo.to_owned(), v);
    }
    serde_json::Value::Object(all).to_string()
}

/// Project `repo`'s requests' facts in `text` (`FACTS_FILE`), as last read.
fn kept_facts(text: &str, repo: &str) -> HashMap<u64, Facts> {
    serde_json::from_str::<HashMap<String, serde_json::Value>>(text)
        .ok()
        .and_then(|mut all| all.remove(repo))
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default()
}

/// What the list keeps of a project.
#[derive(Default, Debug, PartialEq)]
struct Kept {
    folded: HashSet<Group>,
    read: HashMap<u64, i64>,
    chosen: bool,
    display: (GroupBy, SortBy),
}

/// Repository `repo`'s line of `requests-list.txt`: its folded groups (none unless folded by hand), the requests
/// read, whether the folds are yours and the display; none when it has no line.
fn kept_list(text: &str, repo: &str) -> Option<Kept> {
    let line = text.lines().find(|l| l.split('\t').next() == Some(repo))?;
    let parts: Vec<&str> = line.split('\t').skip(1).collect();
    let field = |n: usize| {
        parts
            .get(n)
            .copied()
            .unwrap_or_default()
            .split(',')
            .filter(|f| !f.is_empty())
    };
    let chosen = parts.get(2) == Some(&"folds");
    let folded = field(0)
        .filter_map(crate::group::parse)
        .filter(|_| chosen)
        .collect();
    // Read before its update was kept: nothing it has is new.
    let read = field(1)
        .filter_map(|r| match r.split_once('@') {
            Some((iid, at)) => Some((iid.parse().ok()?, at.parse().ok()?)),
            None => Some((r.parse().ok()?, READ_BEFORE)),
        })
        .collect();
    let display = parts
        .get(3)
        .map_or_else(Default::default, |d| crate::group::parse_display(d));
    Some(Kept {
        folded,
        read,
        chosen,
        display,
    })
}

/// `text` with repository `repo`'s line written anew; other repositories' kept.
fn with_kept_list(text: &str, repo: &str, kept: &Kept) -> String {
    let mut folded: Vec<String> = kept.folded.iter().map(Band::key).collect();
    folded.sort();
    let mut read: Vec<(u64, i64)> = kept.read.iter().map(|(&i, &a)| (i, a)).collect();
    read.sort();
    let line = format!(
        "{repo}\t{}\t{}\t{}\t{}",
        folded.join(","),
        read.iter()
            .map(|(iid, at)| if *at == READ_BEFORE {
                iid.to_string()
            } else {
                format!("{iid}@{at}")
            })
            .collect::<Vec<_>>()
            .join(","),
        if kept.chosen { "folds" } else { "" },
        crate::group::display_key(kept.display.0, kept.display.1),
    );
    let mut lines: Vec<String> = text
        .lines()
        .filter(|l| !l.is_empty() && l.split('\t').next() != Some(repo))
        .map(str::to_owned)
        .chain([line])
        .collect();
    lines.sort_unstable();
    lines.join("\n") + "\n"
}

/// The band held at the top of the requests list while its rows scroll under it, by the bands'
/// tops and the list's top: the last band above it, pushed up as far as the next one comes into
/// its room (#81).
pub fn held_band(tops: &[f32], top: f32) -> Option<(usize, f32)> {
    let at = tops.iter().rposition(|&t| t < top)?;
    let push = tops
        .get(at + 1)
        .map_or(0., |&next| (next - top - BAND).min(0.));
    Some((at, push))
}

// ---- the views ------------------------------------------------------------------------------

/// What the list draws that the tab knows.
pub struct ListLook<'a> {
    /// The requests listed, in the list's order: the open by what they need, the merged or the
    /// closed newest first.
    pub shown: &'a [Mr],
    /// The row picked (a click, ↑ ↓), and the filter typed in the page bar.
    pub picked: Option<u64>,
    pub filter: &'a str,
    /// Narrow: no preview beside the list, and a click opens a request (#80).
    pub compact: bool,
    /// The lists' widths: a row shows what its list has room for (#79).
    pub widths: &'a Widths,
    /// How far your review of each request listed got, and how many of its files are reviewed
    /// at its head (done, total).
    pub seen: HashMap<u64, Seen>,
    pub parts: HashMap<u64, (u32, u32)>,
    /// A row folding with its group (#110): by its key, its group's motion and whether it is
    /// open; `None` once it is shut.
    pub fold: &'a dyn Fn(String, &str, bool, AnyElement) -> Option<AnyElement>,
}

impl ListLook<'_> {
    fn seen(&self, iid: u64) -> Seen {
        self.seen.get(&iid).copied().unwrap_or(Seen::Never)
    }

    /// Whether the band's chevrons fold: not while a filter is typed.
    fn filtering(&self) -> bool {
        !self.filter.trim().is_empty()
    }
}

/// The list as it is drawn.
pub enum Body {
    /// Nothing read yet.
    Loading,
    /// No request to list, and why.
    Empty(Div),
    /// The rows under their bands, one held at the top as they scroll; the picked one's preview
    /// beside them unless the tab is narrow; whether the list's end, a past page's shimmer, is in
    /// sight (`Some(true)`: the next page is read) or not laid out yet (`Some(false)`: looked at
    /// again once it is).
    List {
        rows: Div,
        preview: Option<AnyElement>,
        more: Option<bool>,
    },
}

/// Whether you have been inside request `iid`: opened here, or files of it marked (#81).
pub fn opened_before(rq: &Requests, iid: u64, seen: Seen) -> bool {
    rq.rows.opened(iid) || !matches!(seen, Seen::Never)
}

/// Merge requests' root: the list grouped, the picked one previewed; or why there is none.
pub fn list(rq: &Requests, look: &ListLook, acts: &Acts) -> Body {
    let shown = look.shown;
    // The merged or the closed (#114): a flat list, newest first, read as it is scrolled.
    let past = (rq.view != MrState::Opened).then(|| rq.past.of_scope(rq.scope));
    let loading = match past {
        Some(p) => p.loading && p.list.is_empty(),
        None => rq.loading && rq.list.is_empty(),
    };
    if loading {
        return Body::Loading;
    }
    if shown.is_empty() && past.is_none_or(|p| !p.more) {
        return Body::Empty(empty(rq, look, acts));
    }
    let groups: Vec<Group> = match past {
        Some(_) => vec![Group::All; shown.len()],
        None => shown.iter().map(|m| rq.group_of(m)).collect(),
    };
    // Not grouped (`Display ▾`): one list, no band, nothing to fold.
    let folds = |g: &Group| *g != Group::All && rq.rows.folds(g, look.filtering());
    let picked = look
        .picked
        .and_then(|iid| shown.iter().find(|m| m.iid == iid))
        .or_else(|| {
            shown
                .iter()
                .zip(&groups)
                .find(|(_, g)| past.is_some() || !folds(g))
                .map(|(m, _)| m)
        });
    // Each group a band, its rows under it unless it is folded (#81).
    let mut rows: Vec<AnyElement> = Vec::new();
    let mut bands: Vec<(usize, Group, usize)> = Vec::new();
    let mut picked_row = None;
    for (n, (m, g)) in shown.iter().zip(&groups).enumerate() {
        if past.is_some() || *g == Group::All {
            if picked.is_some_and(|p| p.iid == m.iid) {
                picked_row = Some(rows.len());
            }
            rows.push(row(m, rq, look, acts).into_any_element());
            continue;
        }
        if n == 0 || groups[n - 1] != *g {
            let count = groups.iter().filter(|o| *o == g).count();
            bands.push((rows.len(), g.clone(), count));
            let top = rows.is_empty();
            rows.push(band(g.clone(), count, false, top, rq, look, acts).into_any_element());
        }
        let is_picked = picked.is_some_and(|p| p.iid == m.iid);
        // Each row folds with its group, in size and alpha (#110): the group opens and shuts as
        // one, and every row stays a child of its own for the held band.
        let Some(row) = (look.fold)(
            format!("request-row-{}", m.iid),
            &format!("group-{}", g.key()),
            !folds(g) || is_picked,
            row(m, rq, look, acts).into_any_element(),
        ) else {
            continue;
        };
        if is_picked {
            picked_row = Some(rows.len());
        }
        rows.push(row);
    }
    // More of them while there may be: a shimmer row at the end, read when it comes into sight
    // (#114).
    if let Some(error) = past.and_then(|p| p.error.as_ref()) {
        let acts = acts.clone();
        rows.push(
            div()
                .px(px(16.))
                .py(px(12.))
                .flex()
                .items_center()
                .gap(px(10.))
                .text_size(px(12.))
                .text_color(theme::muted())
                .child(hover::cut("past-error", error.clone()).flex_1())
                .child(
                    div()
                        .id("past-retry")
                        .child(outline_button("Retry"))
                        .on_click(move |_: &ClickEvent, _, cx| acts(Act::RetryPast, cx)),
                )
                .into_any_element(),
        );
    } else if past.is_some_and(|p| p.more || p.loading) {
        rows.push(
            div()
                .id("past-more")
                .px(px(16.))
                .py(px(12.))
                .child(motion::shimmer("past-more-bars", 2))
                .into_any_element(),
        );
    }
    // The rows as the last frame laid them out, when they are these rows: where the list's top
    // is, so the band above it is held there.
    let scroll = &rq.rows.scroll;
    let laid = scroll.children_count() == rows.len();
    let mut more = None;
    if past.is_some_and(|p| p.more && !p.loading) {
        // Not laid out yet: looked at again once it is. A row's bounds are where it is in the
        // unscrolled list.
        let near = laid
            && scroll.bounds_for_item(rows.len() - 1).is_some_and(|b| {
                b.top() + scroll.offset().y < scroll.bounds().bottom() + px(BAND * 4.)
            });
        if near || !laid {
            more = Some(near);
        }
    }
    let top = f32::from(scroll.bounds().top() - scroll.offset().y);
    let top_of = |ix: usize| scroll.bounds_for_item(ix).map(|b| f32::from(b.top()));
    if rq.rows.reveal.take()
        && let Some(ix) = picked_row
    {
        // A row going up out of sight would stop under the held band: the one above it comes
        // into view instead, and with it the row.
        let under_band = laid && ix > 0 && top_of(ix).is_some_and(|t| t < top + BAND);
        scroll.scroll_to_item(if under_band { ix - 1 } else { ix });
    }
    let tops: Vec<f32> = bands.iter().filter_map(|&(ix, ..)| top_of(ix)).collect();
    let held = (laid && tops.len() == bands.len())
        .then(|| held_band(&tops, top))
        .flatten()
        .map(|(at, push)| {
            let (_, g, count) = bands[at].clone();
            div()
                .absolute()
                .top(px(push))
                .left_0()
                .right_0()
                .child(band(g, count, true, true, rq, look, acts))
        });
    let rows = div()
        .relative()
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .overflow_hidden()
        .child(look.widths.measure("requests"))
        .child(
            div()
                .id("requests")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .track_scroll(scroll)
                .pb(px(6.))
                .children(rows),
        )
        .children(held);
    Body::List {
        rows,
        preview: picked
            .filter(|_| !look.compact)
            .map(|m| preview(m, rq, look, acts)),
        more,
    }
}

/// No request to list: why, per filter, and the way to more.
fn empty(rq: &Requests, look: &ListLook, acts: &Acts) -> Div {
    let all = rq.list.len();
    let filter = look.filter.trim();
    let view = rq.view;
    let show_all = |label: &'static str| {
        let acts = acts.clone();
        div()
            .id("empty-show-all")
            .child(outline_button(label))
            .on_click(move |_: &ClickEvent, _, cx| acts(Act::ShowAll, cx))
            .into_any_element()
    };
    if !filter.is_empty() {
        return empty_state(
            "search",
            format!("Nothing matches “{filter}”"),
            "Looks in number, title, author and branch.".to_owned(),
            Some(show_all("Clear the filter")),
        );
    }
    // The merged or the closed (#114): none read, or none of this filter's.
    if view != MrState::Opened {
        let word = view.word().to_lowercase();
        let past = rq.past.of_scope(rq.scope);
        if let Some(error) = &past.error {
            let acts = acts.clone();
            return empty_state(
                "pull-request",
                words::say(&format!("Couldn't read the {word} merge requests")).into_owned(),
                error.clone(),
                Some(
                    div()
                        .id("past-retry")
                        .child(outline_button("Retry"))
                        .on_click(move |_: &ClickEvent, _, cx| acts(Act::RetryPast, cx))
                        .into_any_element(),
                ),
            );
        }
        let read = !rq.past.of_scope(Scope::All).list.is_empty();
        let (title, more) = match rq.scope {
            Scope::All => (
                words::say(&format!("No {word} merge requests")).into_owned(),
                false,
            ),
            Scope::ReviewMe => (format!("None {word} asked for your review"), read),
            Scope::Mine => (format!("None of yours is {word}"), read),
        };
        return empty_state(
            "pull-request",
            title,
            String::new(),
            more.then(|| show_all("Show all")),
        );
    }
    // A read that failed is never "none" (#185): the banner over the list says why, with Retry.
    if rq.error.is_some() && all == 0 {
        return empty_state(
            "pull-request",
            words::say("Couldn't read the merge requests").into_owned(),
            String::new(),
            None,
        );
    }
    // A state word and its way on, no sentence (#185).
    let (title, more) = match rq.scope {
        Scope::ReviewMe => ("Nothing to review".to_owned(), all > 0),
        Scope::Mine => ("None of yours is open".to_owned(), all > 0),
        Scope::All => (words::say("No open merge requests").into_owned(), false),
    };
    empty_state(
        "pull-request",
        title,
        String::new(),
        more.then(|| show_all("Show all")),
    )
}

/// A group's band in the requests list: its icon, name and count, and the chevron that folds it
/// (a click anywhere on it does); `held` is its copy held at the top of the list (#81).
fn band(
    g: Group,
    count: usize,
    held: bool,
    top: bool,
    rq: &Requests,
    look: &ListLook,
    acts: &Acts,
) -> impl IntoElement + use<> {
    let (icon, tone) = g.icon();
    // At the list's top its fill follows the island's round corners, and the island's own border
    // is its top edge.
    let corner = if flat() { 0. } else { ISLAND_RADIUS - 1. };
    let foldable = !look.filtering();
    let folded = rq.rows.folds(&g, look.filtering());
    let acts = acts.clone();
    div()
        .id(SharedString::from(format!(
            "{}-{}",
            if held { "group-held" } else { "group-band" },
            g.key()
        )))
        .h(px(BAND))
        .flex_none()
        .flex()
        .items_center()
        .gap(px(8.))
        .pl(px(16.))
        .pr(px(10.))
        .bg(theme::base())
        .when(top, |s| s.rounded_t(px(corner)))
        .when(!top, |s| s.border_t_1())
        .border_b_1()
        .border_color(theme::island_border())
        .text_size(px(12.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(if folded {
            theme::muted()
        } else {
            theme::text()
        })
        .child(icons::icon(icon).size(px(14.)).text_color(tone))
        .child(hover::cut("heading", g.heading()).flex_1())
        .child(
            div()
                .flex_none()
                .px(px(6.))
                .rounded_full()
                .bg(theme::hover())
                .text_size(px(11.))
                .font_weight(FontWeight::NORMAL)
                .text_color(theme::muted())
                .child(count.to_string()),
        )
        .when(foldable, |s| {
            s.cursor_pointer()
                // The chevron turns as the group's rows fold (#56, 1).
                .child(motion::toward(
                    icons::icon("chevron")
                        .size(px(14.))
                        .text_color(theme::faint()),
                    "group-chevron",
                    !folded,
                    false,
                    motion::FAST,
                    |chevron, v| {
                        chevron.with_transformation(gpui::Transformation::rotate(gpui::radians(
                            -std::f32::consts::FRAC_PI_2 * (1. - v),
                        )))
                    },
                ))
                .on_click(
                    // A double click folds or opens it once, as one click does.
                    move |e: &ClickEvent, _, cx| {
                        if e.click_count() < 2 {
                            acts(Act::ToggleGroup(g.clone()), cx)
                        }
                    },
                )
        })
}

/// A small outlined tag after a title: `New`, `Draft`.
fn tag(text: &'static str, color: gpui::Rgba) -> Div {
    div()
        .flex_none()
        .px(px(6.))
        .h(px(18.))
        .flex()
        .items_center()
        .rounded_full()
        .border_1()
        .border_color(color)
        .text_size(px(11.))
        .text_color(color)
        .child(text)
}

/// Whether a request passes the list's filter, typed as `terms` (lower case): each matches its number, title,
/// author (name or username) or source branch (#191).
pub(crate) fn filter_lets(m: &Mr, terms: &[&str]) -> bool {
    let hay = format!(
        "{} {} {} {} {}",
        words::number(m.iid),
        m.title,
        m.author,
        m.author_login,
        m.source
    )
    .to_lowercase();
    terms.iter().all(|w| rank::word_matches(w, &hay))
}

/// Where the filter's words are found in `text`, in order and apart: the stretches a row marks (#191), a word
/// matched by its initials at the starts it matched (`juha`: **Ju**niper **Ha**le).
fn hits(text: &str, filter: &str) -> Vec<std::ops::Range<usize>> {
    let mut found: Vec<std::ops::Range<usize>> = (filter.split_whitespace())
        .flat_map(|w| rank::word_spans(w, text))
        .collect();
    found.sort_by_key(|r| r.start);
    let mut merged: Vec<std::ops::Range<usize>> = Vec::new();
    for r in found {
        match merged.last_mut() {
            Some(last) if r.start <= last.end => last.end = last.end.max(r.end),
            _ => merged.push(r),
        }
    }
    merged
}

/// `text` with the filter's hits marked as find marks them.
fn marked(text: String, filter: &str) -> gpui::StyledText {
    let ranges = hits(&text, filter);
    let mark = gpui::HighlightStyle {
        color: Some(theme::text().into()),
        background_color: Some(theme::warning_bg().into()),
        ..Default::default()
    };
    gpui::StyledText::new(text).with_highlights(ranges.into_iter().map(|r| (r, mark)))
}

/// Approvals as a row and the preview say them: `1 of 2`, or how many approved where no rule asks.
fn approvals_words(a: &Approvals) -> Option<String> {
    if a.required > 0 {
        Some(format!(
            "{} of {}",
            a.required - a.left.min(a.required),
            a.required
        ))
    } else {
        (!a.by.is_empty()).then(|| format!("{} approved", a.by.len()))
    }
}

/// Of a request's `threads`, those open (resolvable, not resolved) and those resolved.
fn discussions(threads: &[Thread]) -> (usize, usize) {
    let resolvable = threads.iter().filter(|t| t.resolvable);
    let open = resolvable.clone().filter(|t| !t.resolved).count();
    (open, resolvable.count() - open)
}

/// A request's row (#189, Design L1): line one, its number, its title — muted for a draft, tagged
/// `Draft` — and when, short, in the column every row shares; line two, its author, and at its end glyphs with
/// their numbers: a conflict, the open discussions, the pipeline's state, a tooltip each. Whose turn it is, the
/// size, the approvals and the target are in the row's tooltip. The dot: never opened, or news since you read it.
fn row(m: &Mr, rq: &Requests, look: &ListLook, acts: &Acts) -> impl IntoElement + use<> {
    let iid = m.iid;
    let picked = look.picked == Some(iid);
    let open = m.state == MrState::Opened;
    let facts = rq.rows.facts(iid);
    let me = rq.login.as_deref().unwrap_or_default();
    let mine = !me.is_empty() && m.author_login == me;
    let new_to_you = open && !opened_before(rq, iid, look.seen(iid));
    let unread = open && (new_to_you || rq.rows.news(m));
    let turn = if open { rq.turn_of(m) } else { None };
    // The time shows while the list has room for it (#79); the tip holds the rest.
    let room = look.widths.room("requests");
    // The row's date says what is sorted (L17): an open one's opening under Oldest first.
    let when_time = if open && rq.rows.display.1 == SortBy::Oldest {
        m.created
    } else {
        m.updated
    };
    let pipeline = facts.and_then(|f| f.pipeline.as_ref());
    let (open_threads, _) = facts.map(|f| discussions(&f.threads)).unwrap_or_default();
    // The tip holds what the row does not say (#202): who and where from, the pipeline, approvals, size, whose
    // turn, and when exactly; the title only when the row cut it.
    let author = if mine { "You" } else { m.author.as_str() };
    let mut tip = vec![format!("{} · {} → {}", m.author, m.source, m.target)];
    tip.extend(pipeline.map(|p| format!("Pipeline {}", crate::bar::pipeline_word(p).1)));
    tip.extend(
        facts
            .and_then(|f| f.approvals.as_ref())
            .and_then(approvals_words)
            .map(|said| format!("Approvals {said}")),
    );
    if let Some((files, added, removed, _)) = facts.and_then(|f| f.size) {
        tip.push(format!("{} +{added} −{removed}", plural(files, "file")));
    }
    if open_threads > 0 {
        tip.push(thread_words(open_threads));
    }
    if m.conflicts {
        tip.push("Conflicts with its target".to_owned());
    }
    tip.extend(turn.as_ref().map(|t| t.words(mine)));
    tip.push(format!("updated {}", hover::full_date(m.updated)));
    let title_cut = hover::WasCut::default();
    let mut lines: Vec<(Option<hover::WasCut>, SharedString)> =
        vec![(Some(title_cut.clone()), m.title.clone().into())];
    lines.extend(tip.into_iter().map(|line| (None, line.into())));
    let glyph = |id: &'static str, name: &'static str, color, said: String| {
        div()
            .id((id, iid))
            .flex_none()
            .child(icons::icon(name).size(px(14.)).text_color(color))
            .tip(said)
    };
    let compact = look.compact;
    let (context, acts) = (acts.clone(), acts.clone());
    div().px(px(6.)).py(px(1.)).child(
        div()
            .id(("mr-row", iid))
            .on_mouse_down(
                gpui::MouseButton::Right,
                move |e: &gpui::MouseDownEvent, _, cx| {
                    context(
                        Act::RequestContext {
                            iid,
                            at: e.position,
                        },
                        cx,
                    )
                },
            )
            .px(px(10.))
            .py(px(8.))
            .flex()
            .items_start()
            .gap(px(10.))
            .rounded(px(ROW_RADIUS))
            .cursor_pointer()
            .when(picked, |s| s.bg(theme::selected()))
            .when(!picked, |s| s.hover(|s| s.bg(theme::hover())))
            .child(unread_dot(unread))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(
                                div()
                                    .flex_none()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(marked(words::number(iid), look.filter)),
                            )
                            .child(
                                hover::cut_in_row(
                                    m.title.clone(),
                                    marked(m.title.clone(), look.filter),
                                    &title_cut,
                                )
                                .when(m.draft, |s| s.text_color(theme::muted())),
                            )
                            .when(m.draft, |s| s.child(tag("Draft", theme::muted())))
                            .child(div().flex_1())
                            .children(room.second().then(|| {
                                format::time_cell(when_time)
                                    .text_size(px(11.))
                                    .text_color(theme::faint())
                            })),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .h(px(16.))
                            .text_size(px(12.))
                            .text_color(theme::muted())
                            .child(
                                hover::cut_with(
                                    ("mr-author", iid),
                                    author.to_owned(),
                                    marked(author.to_owned(), look.filter),
                                )
                                .flex_1(),
                            )
                            .when(m.conflicts, |s| {
                                s.child(glyph(
                                    "mr-conflicts",
                                    "triangle-alert",
                                    theme::warning(),
                                    "Conflicts with its target".to_owned(),
                                ))
                            })
                            .when(open_threads > 0, |s| {
                                s.child(
                                    div()
                                        .id(("mr-threads", iid))
                                        .flex_none()
                                        .flex()
                                        .items_center()
                                        .gap(px(3.))
                                        .child(
                                            icons::icon("message")
                                                .size(px(14.))
                                                .text_color(theme::muted()),
                                        )
                                        .child(open_threads.to_string())
                                        .tip(thread_words(open_threads)),
                                )
                            })
                            .children(pipeline.map(|p| {
                                let (name, color) = crate::bar::pipeline_icon(p);
                                glyph(
                                    "mr-pipeline",
                                    name,
                                    color,
                                    format!("Pipeline {}", crate::bar::pipeline_word(p).1),
                                )
                            })),
                    ),
            )
            .row_tip_cut(picked, lines)
            .on_click(move |event: &ClickEvent, _, cx| {
                // A second click on the picked one opens it, as ↵ does; with no preview beside the
                // list, the first (#80).
                if (picked && event.click_count() > 1) || compact {
                    acts(Act::OpenRequest { iid, note: None }, cx)
                } else {
                    acts(Act::PickRequest(iid), cx)
                }
            }),
    )
}

/// `1 discussion open`, `2 discussions open`.
fn thread_words(open: usize) -> String {
    format!("{} open", plural(open, "discussion"))
}

/// A count in a preview's actions (Design M1): its glyph and its number in a 28 px button, what it counts in its
/// tooltip. It keeps its place whatever the text; it opens the request, as Open does.
fn count_chip(
    id: &'static str,
    icon: &'static str,
    count: String,
    tip: String,
    iid: u64,
    acts: &Acts,
) -> impl IntoElement + use<> {
    let acts = acts.clone();
    div()
        .id((id, iid))
        .flex_none()
        .h(px(28.))
        .px(px(10.))
        .flex()
        .items_center()
        .gap(px(5.))
        .rounded(px(ROW_RADIUS))
        .border_1()
        .border_color(theme::island_border())
        .text_size(px(13.))
        .text_color(theme::muted())
        .cursor_pointer()
        .hover(|s| s.bg(theme::hover()))
        .child(icons::icon(icon).size(px(14.)).text_color(theme::muted()))
        .child(count)
        .tip(tip)
        .on_click(move |_: &ClickEvent, _, cx| acts(Act::OpenRequest { iid, note: None }, cx))
}

/// The request picked (#189, Design L3, M1): its head — number, pipeline, approvals, the author's face and name on
/// one line —; its title; the description; its branches on a line of their own; Open and the counts that keep
/// their place, a tooltip each. What waits for you on one of yours is its Overview's, not the preview's (W).
fn preview(m: &Mr, rq: &Requests, look: &ListLook, acts: &Acts) -> AnyElement {
    let iid = m.iid;
    let me = rq.login.as_deref().unwrap_or_default();
    let mine = !me.is_empty() && m.author_login == me;
    let open = m.state == MrState::Opened;
    let facts = rq.rows.facts(iid);
    let threads: &[Thread] = facts.map_or(&[], |f| f.threads.as_slice());
    let approvals = facts.and_then(|f| f.approvals.as_ref());
    let pipeline = facts.and_then(|f| f.pipeline.as_ref());
    let preview = rq.rows.preview(iid);
    let author = if mine { "You" } else { m.author.as_str() };
    let mut head = div()
        .flex()
        .items_center()
        .gap(px(10.))
        .text_size(px(12.))
        .text_color(theme::muted())
        .child(
            div()
                .id(("preview-number", iid))
                .flex_none()
                .font_family(theme::code_font())
                .text_color(theme::accent())
                .child(words::number(iid))
                .tip(format!("{} → {}", m.source, m.target)),
        );
    if let Some(p) = pipeline {
        let (name, color) = crate::bar::pipeline_icon(p);
        let (_, said, _) = crate::bar::pipeline_word(p);
        let (_, short, _) = crate::bar::pipeline_short(p);
        head = head.child(
            div()
                .id(("preview-pipeline", iid))
                .flex_none()
                .h(px(22.))
                .px(px(8.))
                .flex()
                .items_center()
                .gap(px(4.))
                .rounded_full()
                .bg(theme::hover())
                .text_color(color)
                .child(icons::icon(name).size(px(13.)).text_color(color))
                .child(short)
                .tip(format!("Pipeline {said}")),
        );
    }
    if let Some(a) = approvals {
        let count = if a.required > 0 {
            format!("{}/{}", a.required - a.left.min(a.required), a.required)
        } else {
            a.by.len().to_string()
        };
        if a.required > 0 || !a.by.is_empty() {
            let mut said = approvals_words(a)
                .map(|w| format!("{w} approvals"))
                .unwrap_or_default();
            if !a.by.is_empty() {
                said = format!("{said} · {}", a.by.join(", "));
            }
            head = head.child(
                div()
                    .id(("preview-approvals", iid))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .child(
                        icons::icon("circle-check")
                            .size(px(14.))
                            .text_color(theme::muted()),
                    )
                    .child(count)
                    .tip(said),
            );
        }
    }
    let head = head
        .child(discussion::avatar(author, mine, 16.))
        .child(hover::cut(("preview-author", iid), author.to_owned()).min_w_0())
        .child(div().flex_1())
        // A merged or closed one: what became of it, and when (#114).
        .children((!open).then(|| {
            div()
                .flex_none()
                .child(format!("{} {}", m.state.word(), format::ago(m.updated)))
        }));
    let title = div()
        .text_size(px(19.))
        .line_height(px(26.))
        .font_weight(FontWeight::SEMIBOLD)
        .when(m.draft, |s| s.text_color(theme::muted()))
        .child(m.title.clone());
    // The branches, a line of their own: a long one is the subject (the checklist's seventh).
    let branches = div()
        .id(("preview-branches-line", iid))
        .flex()
        .items_center()
        .gap(px(6.))
        .min_w_0()
        .text_size(px(12.))
        .text_color(theme::muted())
        .child(
            icons::icon("branch")
                .size(px(14.))
                .text_color(theme::muted()),
        )
        .child(
            hover::branches("preview-branches", &m.source, &m.target)
                .min_w_0()
                .font_family(theme::code_font()),
        )
        .tip(format!("{} into {}", m.source, m.target));
    // The counts: files changed since you viewed (else the diff's files), the open discussions, the files reviewed.
    let (open_threads, _) = discussions(threads);
    let size = facts.and_then(|f| f.size);
    let mut chips: Vec<AnyElement> = Vec::new();
    match (look.seen(iid), preview.and_then(|p| p.since)) {
        (Seen::Earlier, Some((viewed, n))) if n > 0 => chips.push(
            count_chip(
                "preview-files",
                "files",
                n.to_string(),
                format!(
                    "{} changed since you viewed v{viewed} · show them",
                    plural(n, "file")
                ),
                iid,
                acts,
            )
            .into_any_element(),
        ),
        _ => {
            if let Some((files, added, removed, commits)) = size {
                chips.push(
                    count_chip(
                        "preview-files",
                        "files",
                        files.to_string(),
                        format!(
                            "{} +{added} −{removed} in {}",
                            plural(files, "file"),
                            plural(commits, "commit")
                        ),
                        iid,
                        acts,
                    )
                    .into_any_element(),
                );
            }
        }
    }
    if open_threads > 0 {
        chips.push(
            count_chip(
                "preview-threads",
                "message",
                open_threads.to_string(),
                format!("{} open · show them", plural(open_threads, "discussion")),
                iid,
                acts,
            )
            .into_any_element(),
        );
    }
    let reviewed = match (look.seen(iid), look.parts.get(&iid).copied(), size) {
        (_, Some((done, total)), _) => Some((done as usize, total as usize)),
        (Seen::Done, _, Some((files, ..))) => Some((files, files)),
        (_, _, Some((files, ..))) => Some((0, files)),
        _ => None,
    };
    if let Some((done, total)) = reviewed.filter(|_| open && !mine) {
        chips.push(
            count_chip(
                "preview-reviewed",
                "eye",
                format!("{done}/{total}"),
                format!(
                    "{done} of {} reviewed · the next one",
                    plural(total, "file")
                ),
                iid,
                acts,
            )
            .into_any_element(),
        );
    }
    let description = markdown::plain(&m.description);
    let description = description.trim();
    let acts = acts.clone();
    div()
        .flex()
        .flex_col()
        .gap(px(14.))
        .px(px(28.))
        .py(px(22.))
        .max_w(px(820.))
        .child(head)
        .child(title)
        .children((!description.is_empty()).then(|| {
            hover::cut_to(
                SharedString::from(format!("mr-description-{iid}")),
                description.to_owned(),
                5,
            )
            .text_size(px(13.))
            .line_height(px(20.))
            .text_color(theme::muted())
        }))
        .child(branches)
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(
                    primary_button("request-open", "Open", "↵")
                        .tip("opens on its changes, at the first file not reviewed · ⌥↓ the next not reviewed")
                        .on_click(move |_: &ClickEvent, _, cx| {
                            acts(Act::OpenRequest { iid, note: None }, cx)
                        }),
                )
                .children(chips),
        )
        .into_any_element()
}

/// Over a list whose host cannot be read now: offline (the VPN), or the token refused; the last
/// list stays under it.
pub fn banner(rq: &Requests, acts: &Acts) -> Option<AnyElement> {
    let Some(error) = rq.error.as_ref() else {
        return kept_note(rq);
    };
    let offline = error.starts_with("Can't reach");
    // One fact and the question the VPN guess asks (Design S2); when the list is from, a clock and its time at the
    // end, the whole date and what happens next in its tip (#185).
    let fact = error.trim_end_matches('.').to_owned();
    let tip = rq.listed_at.map(|t| {
        let mut tip = format!("The list is from {}", hover::full_date(t));
        if offline {
            tip.push_str("\nIt refreshes when the host is back");
        }
        tip
    });
    let token = error.contains("token");
    let color = if offline {
        theme::accent()
    } else {
        theme::removed()
    };
    let acts = acts.clone();
    Some(
        div()
            .id("requests-banner")
            .flex_none()
            .flex()
            .items_center()
            .gap(px(10.))
            .px(px(14.))
            .py(px(8.))
            .rounded(px(if flat() { 0. } else { ISLAND_RADIUS }))
            .border_1()
            .border_color(color)
            .bg(theme::panel())
            .text_size(px(13.))
            .child(
                icons::icon(if offline { "info" } else { "alert" })
                    .size(px(16.))
                    .text_color(color),
            )
            .child(div().flex_1().min_w_0().child(fact))
            .children(rq.listed_at.map(|t| {
                div()
                    .id("banner-listed")
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(5.))
                    .text_size(px(12.))
                    .text_color(theme::muted())
                    .child(
                        icons::icon("clock")
                            .size(px(14.))
                            .text_color(theme::muted()),
                    )
                    .child(format::clock(t))
                    .tip(tip.clone().unwrap_or_default())
            }))
            .child(
                div()
                    .id("banner-action")
                    .child(outline_button(if token {
                        "Renew token…"
                    } else {
                        "Retry"
                    }))
                    .on_click(move |_: &ClickEvent, _, cx| {
                        acts(if token { Act::RenewToken } else { Act::Relist }, cx)
                    }),
            )
            .into_any_element(),
    )
}

/// The list as the last run kept it, while the host is asked again: when it is from.
fn kept_note(rq: &Requests) -> Option<AnyElement> {
    let at = rq.listed_at.filter(|_| rq.from_disk)?;
    Some(
        div()
            .id("requests-kept")
            .flex_none()
            .flex()
            .items_center()
            .gap(px(5.))
            .px(px(14.))
            .text_size(px(12.))
            .text_color(theme::muted())
            .child(
                icons::icon("clock")
                    .size(px(14.))
                    .text_color(theme::muted()),
            )
            .child(format!("From {}", format::clock(at)))
            .tip(format!(
                "The list from {}\nIt refreshes now",
                hover::full_date(at)
            ))
            .into_any_element(),
    )
}

/// The row to review after row `at`, going round: the nearest of the lowest tier; a row of no
/// tier is never one.
fn next_to_review(open: &[bool], at: Option<usize>) -> Option<usize> {
    let n = open.len();
    let start = at.map_or(0, |a| a + 1);
    (0..n)
        .map(|step| (start + step) % n)
        .find(|&ix| Some(ix) != at && open[ix])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_projects_folds_reads_and_display_are_kept_on_its_own_line() {
        let kept = Kept {
            folded: HashSet::from([Group::Others, Group::Done]),
            read: HashMap::from([(415, 1_700), (398, READ_BEFORE)]),
            chosen: true,
            display: (GroupBy::Author, SortBy::Oldest),
        };
        let text = with_kept_list("b\tyou\t7\tfolds\n", "a", &kept);
        assert_eq!(
            text,
            "a\tdone,others\t398,415@1700\tfolds\tauthor/oldest\nb\tyou\t7\tfolds\n"
        );
        assert_eq!(kept_list(&text, "a"), Some(kept));
        // A line from before the display: the defaults; a request read before its update was kept is not new.
        let b = kept_list(&text, "b").unwrap();
        assert_eq!(b.folded, HashSet::from([Group::You]));
        assert_eq!(b.read, HashMap::from([(7, READ_BEFORE)]));
        assert_eq!(b.display, (GroupBy::Turn, SortBy::Updated));
        assert_eq!(kept_list(&text, "c"), None);
        // A line from before the folds were yours: its fold was the old default, so none.
        assert_eq!(
            kept_list("a\treviewed\t398\n", "a").map(|k| k.folded),
            Some(HashSet::new())
        );
    }

    #[test]
    fn facts_stand_at_their_update_and_for_five_minutes_when_read_this_run() {
        let kept = Facts {
            updated: 5,
            ..Default::default()
        };
        // Kept from the last start: fresh while the update is the same.
        assert!(kept.stands(5) && !kept.stands(6));
        let now = Facts {
            at: Some(Instant::now()),
            ..kept
        };
        assert!(now.stands(5));
        let old = Facts {
            updated: 5,
            at: Instant::now().checked_sub(FACTS_TTL + Duration::from_secs(1)),
            ..Default::default()
        };
        assert!(!old.stands(5));
    }

    #[test]
    fn facts_are_kept_per_project_and_only_the_wanted_are_read() {
        let facts = |updated: i64, pushes: Vec<i64>| Facts {
            updated,
            pushes,
            ..Default::default()
        };
        let a = HashMap::from([(7, facts(70, vec![1, 2])), (8, facts(80, vec![]))]);
        let text = with_kept_facts("", "a", a.iter());
        let text = with_kept_facts(&text, "b", [(&9, &facts(90, vec![3]))].into_iter());
        // Project a's again, 8 no longer open: b's stay.
        let text = with_kept_facts(&text, "a", a.iter().filter(|(iid, _)| **iid == 7));
        let kept = kept_facts(&text, "a");
        assert_eq!(kept.keys().collect::<Vec<_>>(), [&7]);
        assert_eq!(
            (kept[&7].updated, kept[&7].pushes.clone()),
            (70, vec![1, 2])
        );
        assert_eq!(kept_facts(&text, "b")[&9].updated, 90);
        assert!(kept_facts(&text, "c").is_empty() && kept_facts("not json", "a").is_empty());

        // The queue's next: open, not read at its update, not being read.
        let m = |iid: u64, updated: i64, state: MrState| Mr {
            iid,
            updated,
            state,
            ..Default::default()
        };
        let list = [
            m(7, 70, MrState::Opened),
            m(8, 81, MrState::Opened),
            m(9, 90, MrState::Merged),
            m(10, 100, MrState::Opened),
            m(11, 110, MrState::Opened),
        ];
        let mut rows = Rows {
            facts: HashMap::from([(7, facts(70, vec![])), (8, facts(80, vec![]))]),
            queue: VecDeque::from([7, 9, 11, 8, 10, 12]),
            ..Default::default()
        };
        rows.reading.insert(11);
        let read: Vec<u64> =
            std::iter::from_fn(|| rows.next_to_read(&list).map(|m| m.iid)).collect();
        assert_eq!(read, [8, 10]);
        assert!(rows.reading.contains(&8) && rows.reading.contains(&10));
    }

    #[test]
    fn a_draft_comes_last_and_the_sort_orders_the_rest() {
        let m = |iid: u64, updated: i64, draft: bool| Mr {
            iid,
            updated,
            // Opened in the other order than last updated: Oldest first goes by the opening.
            created: 100 - updated,
            draft,
            ..Default::default()
        };
        let mut list = [m(1, 10, false), m(2, 30, true), m(3, 20, false)];
        let size = |m: &Mr| m.iid as usize;
        list.sort_by_key(order(SortBy::Updated, &size));
        assert_eq!(list.iter().map(|m| m.iid).collect::<Vec<_>>(), [3, 1, 2]);
        list.sort_by_key(order(SortBy::Oldest, &size));
        assert_eq!(list.iter().map(|m| m.iid).collect::<Vec<_>>(), [3, 1, 2]);
        list.sort_by_key(order(SortBy::Size, &size));
        assert_eq!(list.iter().map(|m| m.iid).collect::<Vec<_>>(), [3, 1, 2]);
    }

    #[test]
    fn a_plain_comment_waits_for_the_author_until_they_write_again() {
        use artoo_hosts_api::Note;
        let note = |login: &str, created: i64| Note {
            id: created as u64,
            author: login.into(),
            login: login.into(),
            body: String::new(),
            created,
            edited: false,
        };
        let thread = |notes: Vec<Note>, resolvable: bool, resolved: bool| Thread {
            id: String::new(),
            notes,
            path: None,
            new_line: None,
            old_line: None,
            resolved,
            resolvable,
            resolved_by: None,
            resolved_at: 0,
            commit: None,
            outdated: false,
        };
        let threads = [
            // An open discussion, its last word theirs: waits.
            thread(vec![note("me", 1), note("ember", 2)], true, false),
            // Resolved: does not.
            thread(vec![note("ember", 3)], true, true),
            // A plain comment after my last word: waits.
            thread(vec![note("kestrel", 9)], false, false),
            // A plain comment before it: I have written since.
            thread(vec![note("kestrel", 4)], false, false),
            // My own last word.
            thread(vec![note("ember", 5), note("me", 8)], true, false),
        ];
        let waits: Vec<i64> = crate::turn::waiting_on("me", &threads)
            .iter()
            .map(|t| t.notes.last().unwrap().created)
            .collect();
        assert_eq!(waits, [2, 9]);
        assert!(crate::turn::waiting_on("", &threads).is_empty());
    }

    #[test]
    fn the_band_above_the_lists_top_is_held_until_the_next_pushes_it_up() {
        // Bands at 0, 200 and 300; the list's top at 0: nothing is above it.
        assert_eq!(held_band(&[0., 200., 300.], 0.), None);
        assert_eq!(held_band(&[0., 200., 300.], 100.), Some((0, 0.)));
        // The next band 10 px below the top: the held one goes up by what it covers.
        assert_eq!(held_band(&[0., 200., 300.], 190.), Some((0, -18.)));
        assert_eq!(held_band(&[0., 200., 300.], 250.), Some((1, 0.)));
        assert_eq!(held_band(&[0., 200., 300.], 900.), Some((2, 0.)));
    }

    #[test]
    fn the_next_request_to_review_goes_round_to_the_top() {
        // 0 to review, 1 reviewed, 2 current, 3 reviewed: past the end, back to 0.
        assert_eq!(
            next_to_review(&[true, false, true, false], Some(2)),
            Some(0)
        );
        assert_eq!(next_to_review(&[false, true, false], Some(0)), Some(1));
        assert_eq!(next_to_review(&[false, true], None), Some(1));
        // Only the current one is left: nowhere to go.
        assert_eq!(next_to_review(&[false, true, false], Some(1)), None);
        assert_eq!(next_to_review(&[false, false, false], Some(1)), None);
    }

    #[test]
    fn the_one_below_comes_before_the_top() {
        // 0 needs review, 1 current, 2 reviewed, 3 new to you: on down to 3, not back up to 0
        // (#136).
        assert_eq!(next_to_review(&[true, true, false, true], Some(1)), Some(3));
    }

    #[test]
    fn the_filter_finds_the_author_by_username_and_marks_where_it_matched() {
        let m = Mr {
            iid: 398,
            title: "Refactor the login screen".into(),
            author: "Juniper Hale".into(),
            author_login: "jhale".into(),
            source: "feature/login-state".into(),
            ..Default::default()
        };
        assert!(filter_lets(&m, &["jhale"]));
        assert!(filter_lets(&m, &["juniper", "login"]));
        assert!(!filter_lets(&m, &["ember"]));
        // Each word's hits, in order, those that touch made one.
        assert_eq!(
            hits("Refactor the login screen", "LOG the"),
            vec![9..12, 13..16]
        );
        assert_eq!(hits("aaa", "aa a"), vec![0..3]);
        assert!(hits("Juniper Hale", "").is_empty());
        assert_eq!(hits("Juniper Hale", "juha"), vec![0..2, 8..10]);
    }
}
