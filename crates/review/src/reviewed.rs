//! The marks of files read: which files of a diff a reviewer read, kept per commit, version range or merge request on
//! this machine (`Store`); a file that changes in a newer version loses its mark (`fingerprint`).

use crate::diff::Header;
use crate::review::Review;
use artoo_core::git::FileDiff;
use artoo_core::storage;
use artoo_review_api::{Host, Seen};
use gpui::App;
use std::collections::HashMap;
use std::rc::Rc;

pub use artoo_review_api::ReviewState as State;

/// A file marked reviewed: its fingerprint then, when (0: not known, an older mark), and the head
/// the diff was at — its version says "at v2" (X59).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Mark {
    fp: u64,
    time: i64,
    head: Option<git2::Oid>,
}

impl Mark {
    fn now(fp: u64, head: Option<git2::Oid>) -> Self {
        let time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        Self { fp, time, head }
    }
}

/// Past `limit` entries, the one written longest ago (`age`, smallest first) goes, never `keep`,
/// the one just written: a `HashMap`'s first key is none of the two, and was often the new one
/// (#163).
fn cap<V, A: Ord>(map: &mut HashMap<String, V>, limit: usize, keep: &str, age: impl Fn(&V) -> A) {
    while map.len() > limit {
        let Some(oldest) = map
            .iter()
            .filter(|(key, _)| *key != keep)
            .min_by_key(|(_, v)| age(v))
            .map(|(key, _)| key.clone())
        else {
            return;
        };
        map.remove(&oldest);
    }
}

/// Between the open paths of a head's progress (`reviewed-progress.txt`'s fourth column).
const OPEN_SEP: &str = "\u{1f}";

/// Every mark ever made: per scope, per path, the file when it was marked.
#[derive(Default)]
pub struct Store {
    marks: HashMap<String, HashMap<String, Mark>>,
    /// `root|commit` of every head whose whole diff was reviewed: a branch or merge request whose tip
    /// is one of them shows ✓, and a new push (a new tip) takes it away. The value is the order it
    /// was written in: the file keeps them oldest first.
    heads: HashMap<String, u64>,
    /// `root|commit` → (files reviewed, files, order written) of the last whole diff read up to that
    /// head: how far a branch or request got, for the lists. Kept oldest first too.
    progress: HashMap<String, (u32, u32, u64)>,
    /// `root|commit` → the paths of that whole diff left open: a mark made in another reading of the same
    /// head (Since v2) moves its count without the whole diff open (#182).
    open: HashMap<String, Vec<String>>,
    /// The order the next head or progress written takes.
    next: u64,
}

impl Store {
    pub fn load() -> Self {
        let mut store = Self::parse(&storage::read("reviewed.txt"));
        store.heads = storage::read("reviewed-heads.txt")
            .lines()
            .zip(0..)
            .map(|(key, order)| (key.to_owned(), order))
            .collect();
        store.progress = storage::read("reviewed-progress.txt")
            .lines()
            .zip(0..)
            .filter_map(|(line, order)| {
                let mut parts = line.split('\t');
                let key = parts.next()?.to_owned();
                let done = parts.next()?.parse().ok()?;
                let total = parts.next()?.parse().ok()?;
                Some((key, (done, total, order), parts.next().map(str::to_owned)))
            })
            .map(|(key, progress, open)| {
                if let Some(open) = open.filter(|o| !o.is_empty()) {
                    let paths = open.split(OPEN_SEP).map(str::to_owned).collect();
                    store.open.insert(key.clone(), paths);
                }
                (key, progress)
            })
            .collect();
        store.next = store.heads.len().max(store.progress.len()) as u64;
        store
    }

    fn tick(&mut self) -> u64 {
        self.next += 1;
        self.next
    }

    /// How far the whole diff up to `head` got: (files reviewed, files).
    pub fn head_progress(&self, root: &str, head: git2::Oid) -> Option<(u32, u32)> {
        self.progress
            .get(&Self::head_key(root, head))
            .map(|&(done, total, _)| (done, total))
    }

    /// How far the whole diff up to `head` got, and its files left `open`.
    pub fn set_progress(
        &mut self,
        root: &str,
        head: git2::Oid,
        done: u32,
        total: u32,
        open: Vec<String>,
    ) {
        let key = Self::head_key(root, head);
        if self.head_progress(root, head) == Some((done, total))
            && self.open.get(&key).is_some_and(|o| *o == open)
        {
            return;
        }
        if done == 0 {
            self.progress.remove(&key);
            self.open.remove(&key);
        } else {
            let order = self.tick();
            self.progress.insert(key.clone(), (done, total, order));
            self.open.insert(key.clone(), open);
        }
        cap(&mut self.progress, 500, &key, |&(_, _, order)| order);
        self.open.retain(|k, _| self.progress.contains_key(k));
        self.save_progress();
    }

    fn save_progress(&self) {
        let mut rows: Vec<_> = self.progress.iter().collect();
        rows.sort_unstable_by_key(|(_, (_, _, order))| *order);
        let lines: Vec<String> = rows
            .into_iter()
            .map(|(k, (d, t, _))| {
                let open = self
                    .open
                    .get(k)
                    .map(|o| o.join(OPEN_SEP))
                    .unwrap_or_default();
                format!("{k}\t{d}\t{t}\t{open}")
            })
            .collect();
        storage::save("reviewed-progress.txt", lines.join("\n") + "\n");
    }

    /// File `path` read in another reading of the whole diff up to `head` (Since v2, #182): no longer open
    /// there, its count one more, and the head reviewed once none is open. Whether it moved anything.
    pub fn read_at(&mut self, root: &str, head: git2::Oid, path: &str) -> bool {
        let key = Self::head_key(root, head);
        let Some(open) = self.open.get_mut(&key) else {
            return false;
        };
        let Some(at) = open.iter().position(|p| p == path) else {
            return false;
        };
        open.remove(at);
        let left = open.is_empty();
        if let Some((done, total, _)) = self.progress.get(&key).copied() {
            let order = self.tick();
            self.progress
                .insert(key, ((done + 1).min(total), total, order));
        }
        self.save_progress();
        if left {
            self.set_head(root, head, true);
        }
        true
    }

    /// Whether a reading of `root` marked `path` with fingerprint `fp`: the same change read anywhere — a
    /// comparison of the same branch, a commit that is its only change (#182).
    pub fn read_anywhere(&self, root: &str, path: &str, fp: u64) -> bool {
        let prefix = format!("{root}|");
        (self.marks.iter())
            .filter(|(scope, _)| scope.starts_with(&prefix))
            .any(|(_, files)| files.get(path).is_some_and(|m| m.fp == fp))
    }

    /// Whether `path` was read in a since view up to `head` (`Since v2`): what changed since a version read,
    /// read to this head (#182).
    pub fn read_since(&self, root: &str, path: &str, head: git2::Oid) -> bool {
        let (prefix, suffix) = (format!("{root}|ver:"), format!("..{head}"));
        (self.marks.iter())
            .filter(|(scope, _)| scope.starts_with(&prefix) && scope.ends_with(&suffix))
            .any(|(_, files)| files.contains_key(path))
    }

    /// Whether `path` was marked in commit `id`'s own diff.
    pub fn read_in_commit(&self, root: &str, path: &str, id: git2::Oid) -> bool {
        (self.marks.get(&format!("{root}|commit:{id}"))).is_some_and(|f| f.contains_key(path))
    }

    /// Whether `path` was marked in any commit of `root`: only then are a branch's commits asked.
    pub fn read_in_commits(&self, root: &str, path: &str) -> bool {
        let prefix = format!("{root}|commit:");
        (self.marks.iter())
            .filter(|(scope, _)| scope.starts_with(&prefix))
            .any(|(_, files)| files.contains_key(path))
    }

    fn save_heads(&self) {
        let mut rows: Vec<_> = self.heads.iter().collect();
        rows.sort_unstable_by_key(|(_, order)| **order);
        let lines: Vec<&str> = rows.into_iter().map(|(k, _)| k.as_str()).collect();
        storage::save("reviewed-heads.txt", lines.join("\n") + "\n");
    }

    fn head_key(root: &str, head: git2::Oid) -> String {
        format!("{root}|{head}")
    }

    pub fn head_done(&self, root: &str, head: git2::Oid) -> bool {
        self.heads.contains_key(&Self::head_key(root, head))
    }

    pub fn set_head(&mut self, root: &str, head: git2::Oid, done: bool) {
        let key = Self::head_key(root, head);
        let changed = if done {
            !self.heads.contains_key(&key) && {
                let order = self.tick();
                self.heads.insert(key.clone(), order);
                true
            }
        } else {
            self.heads.remove(&key).is_some()
        };
        if changed {
            cap(&mut self.heads, 500, &key, |&order| order);
            self.save_heads();
        }
    }

    /// Marks or unmarks many files of one scope with a single write.
    pub fn set_many(
        &mut self,
        scope: &str,
        files: &[(String, u64)],
        done: bool,
        head: Option<git2::Oid>,
    ) {
        let entry = self.marks.entry(scope.to_owned()).or_default();
        for (path, fp) in files {
            if done {
                entry.insert(path.clone(), Mark::now(*fp, head));
            } else {
                entry.remove(path);
            }
        }
        if entry.is_empty() {
            self.marks.remove(scope);
        }
        self.prune(scope);
        self.save();
    }

    /// `scope path fingerprint [time head]`, tab apart; older lines have no time and head.
    fn parse(text: &str) -> Self {
        let mut marks: HashMap<String, HashMap<String, Mark>> = HashMap::new();
        for line in text.lines() {
            let mut parts = line.splitn(5, '\t');
            let (Some(scope), Some(path), Some(fp)) = (parts.next(), parts.next(), parts.next())
            else {
                continue;
            };
            let time = parts.next().and_then(|t| t.parse().ok()).unwrap_or(0);
            let head = parts.next().and_then(|h| git2::Oid::from_str(h).ok());
            if let Ok(fp) = u64::from_str_radix(fp, 16) {
                marks
                    .entry(scope.to_owned())
                    .or_default()
                    .insert(path.to_owned(), Mark { fp, time, head });
            }
        }
        Self {
            marks,
            ..Self::default()
        }
    }

    fn render(&self) -> String {
        let mut out = String::new();
        for (scope, files) in &self.marks {
            for (path, m) in files {
                let head = m.head.map(|h| h.to_string()).unwrap_or_default();
                out.push_str(&format!(
                    "{scope}\t{path}\t{:x}\t{}\t{head}\n",
                    m.fp, m.time
                ));
            }
        }
        out
    }

    fn save(&self) {
        storage::save("reviewed.txt", self.render());
    }

    pub fn get(&self, scope: &str, path: &str) -> Option<u64> {
        self.marks.get(scope)?.get(path).map(|m| m.fp)
    }

    /// The heads the files of `scope` were marked at, one per file marked.
    pub fn marked_heads(&self, scope: &str) -> Vec<Option<git2::Oid>> {
        self.marks
            .get(scope)
            .map(|m| m.values().map(|m| m.head).collect())
            .unwrap_or_default()
    }

    /// The head the newest mark of `scope` was made at, when it is not `now`: the push last
    /// viewed (X22).
    pub fn last_head(&self, scope: &str, now: Option<git2::Oid>) -> Option<git2::Oid> {
        self.marks
            .get(scope)?
            .values()
            .filter(|m| m.head.is_some() && m.head != now)
            .max_by_key(|m| m.time)?
            .head
    }

    /// When the file was marked, and the head the diff was at then.
    pub fn when(&self, scope: &str, path: &str) -> Option<(i64, Option<git2::Oid>)> {
        let m = self.marks.get(scope)?.get(path)?;
        (m.time > 0).then_some((m.time, m.head))
    }

    pub fn set(&mut self, scope: &str, path: &str, fp: u64, head: Option<git2::Oid>) {
        self.marks
            .entry(scope.to_owned())
            .or_default()
            .insert(path.to_owned(), Mark::now(fp, head));
        self.prune(scope);
        self.save();
    }

    pub fn clear(&mut self, scope: &str, path: &str) {
        if let Some(files) = self.marks.get_mut(scope) {
            files.remove(path);
            if files.is_empty() {
                self.marks.remove(scope);
            }
        }
        self.save();
    }

    /// The file does not grow without end: past 300 scopes, the one whose newest mark is oldest
    /// goes, never `scope`, the one just marked.
    fn prune(&mut self, scope: &str) {
        cap(&mut self.marks, 300, scope, |files| {
            files.values().map(|m| m.time).max().unwrap_or(0)
        });
    }
}

/// The file after `now` in `order`, going round, that is not `done`; none when it is the only one
/// left or there is none.
pub fn next_open(order: &[usize], now: usize, done: impl Fn(usize) -> bool) -> Option<usize> {
    let at = order.iter().position(|&ix| ix == now).unwrap_or(0);
    (1..=order.len())
        .map(|step| order[(at + step) % order.len()])
        .find(|&ix| !done(ix))
        .filter(|&ix| ix != now)
}

fn feed(h: &mut u64, bytes: &[u8]) {
    for b in bytes {
        *h ^= u64::from(*b);
        *h = h.wrapping_mul(0x100_0000_01b3);
    }
}

/// What a file's change is, for the purpose of "is it the change I read" (#182, the spec's patch-id): its kind and the
/// text of its added and removed lines, each with its whitespace squeezed, in order — no line numbers, nothing of the
/// file around them, so a rebase that leaves the file's own change alone keeps its mark. An image's sides by their
/// bytes. Hashed with FNV-1a, which does not change between builds.
pub fn fingerprint(file: &FileDiff) -> u64 {
    use artoo_core::git::LineKind;
    fn lines(text: Option<&str>) -> Vec<&str> {
        text.map(|t| t.lines().collect()).unwrap_or_default()
    }
    let (old, new) = (
        lines(file.old_text.as_deref()),
        lines(file.new_text.as_deref()),
    );
    let at = |side: &[&'_ str], n: Option<u32>| -> String {
        let line = n
            .and_then(|n| side.get((n as usize).checked_sub(1)?))
            .copied();
        line.unwrap_or_default()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    };
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    feed(&mut h, format!("{:?}", file.change).as_bytes());
    for line in file.hunks.iter().flat_map(|hunk| &hunk.lines) {
        let (sign, text) = match line.kind {
            LineKind::Added => (b'+', at(&new, line.new_line)),
            LineKind::Removed => (b'-', at(&old, line.old_line)),
            _ => continue,
        };
        feed(&mut h, &[sign]);
        feed(&mut h, text.as_bytes());
        feed(&mut h, b"\n");
    }
    for side in [&file.old_image, &file.new_image].into_iter().flatten() {
        feed(&mut h, &side.bytes);
    }
    h
}

/// The fingerprint marks were made with before #182 (the whole file, line numbers and all): a mark made then still
/// counts while its file is as it was.
pub fn legacy_fingerprint(file: &FileDiff) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    feed(
        &mut h,
        format!("{:?}{}{}", file.change, file.added, file.removed).as_bytes(),
    );
    for hunk in &file.hunks {
        for line in &hunk.lines {
            feed(
                &mut h,
                format!("{:?}{:?}{:?}", line.kind, line.old_line, line.new_line).as_bytes(),
            );
        }
    }
    feed(
        &mut h,
        file.new_text.as_deref().unwrap_or_default().as_bytes(),
    );
    feed(
        &mut h,
        file.old_text.as_deref().unwrap_or_default().as_bytes(),
    );
    h
}

impl Review {
    pub fn review_state(&self, ix: usize) -> Option<State> {
        let scope = self.review_scope.as_deref()?;
        let path = self.diff.as_ref()?.files.get(ix)?.path();
        let same =
            |was: u64| self.fps.get(ix) == Some(&was) || self.legacy_fps.get(ix) == Some(&was);
        Some(match self.reviewed.get(scope, path) {
            Some(was) if same(was) => State::Done,
            // Read in another reading of the same change (#182).
            _ if self.crossed.get(ix).copied().unwrap_or(false) => State::Done,
            Some(_) => State::Stale,
            None => State::Open,
        })
    }

    /// Which files of the open diff were read in another reading (#182): the same change marked anywhere; on a
    /// branch or request, what changed since a version read up to its head (`Since v2`), or every commit of it
    /// that touches the file marked.
    fn crossings(&self) -> Vec<bool> {
        let (Some(root), Some(diff)) = (self.root.as_ref(), self.diff.as_ref()) else {
            return Vec::new();
        };
        let root_key = root.display().to_string();
        let whole = match &diff.header {
            Header::Compare {
                head,
                start,
                commits,
                ..
            } => Some((head.1, *start, *commits)),
            _ => None,
        };
        // The branch's commits and what each touched, asked of git once and only when a commit was marked.
        let mut touched: Option<Vec<(git2::Oid, Vec<String>)>> = None;
        let mut commits_of = |path: &str| -> Option<Vec<git2::Oid>> {
            let (head, start, count) = whole?;
            let touched = touched.get_or_insert_with(|| {
                let Ok(repo) = artoo_core::git::Repo::open(root) else {
                    return Vec::new();
                };
                let ids: Vec<git2::Oid> = (repo.log(head, 0, count.max(1), Some(start)))
                    .map(|log| {
                        log.into_iter()
                            .map(|c| c.id)
                            .filter(|id| *id != start)
                            .collect()
                    })
                    .unwrap_or_default();
                repo.changed_paths(&ids)
            });
            let ids: Vec<git2::Oid> = (touched.iter())
                .filter(|(_, paths)| paths.iter().any(|p| p == path))
                .map(|(id, _)| *id)
                .collect();
            (!ids.is_empty()).then_some(ids)
        };
        (diff.files.iter().zip(&self.fps))
            .map(|(file, &fp)| {
                let path = file.path();
                self.reviewed.read_anywhere(&root_key, path, fp)
                    || whole
                        .is_some_and(|(head, ..)| self.reviewed.read_since(&root_key, path, head))
                    || (whole.is_some() && self.reviewed.read_in_commits(&root_key, path))
                        && commits_of(path).is_some_and(|ids| {
                            ids.iter()
                                .all(|id| self.reviewed.read_in_commit(&root_key, path, *id))
                        })
            })
            .collect()
    }

    /// What a file's chip says when its change moved since it was marked (#182, Design A1): `↻ v2`, the version
    /// it was read at — the icon and the version, no word; `↻` alone where the version is not known.
    pub fn stale_chip(&self, ix: usize, host: &Rc<dyn Host>, cx: &App) -> String {
        let version = (self.review_scope.as_deref())
            .zip(self.diff.as_ref().and_then(|d| d.files.get(ix)))
            .and_then(|(scope, f)| self.reviewed.when(scope, f.path()))
            .and_then(|(_, head)| host.version_of(head?, cx));
        match version {
            Some(n) => format!("↻ v{n}"),
            None => "↻".to_owned(),
        }
    }

    /// `↻ v2` beside file `ix`'s name while the diff is what changed since the version you read (#187, Design A1):
    /// the icon and the version, no word; it goes once the file is read again.
    pub fn since_chip(&self, ix: usize) -> Option<gpui::Stateful<gpui::Div>> {
        use artoo_ui_kit::hover::Hover;
        use artoo_ui_kit::{px, theme};
        use gpui::{ParentElement, Styled, div, prelude::*};
        let n = self.since?.version;
        (self.review_state(ix) != Some(State::Done)).then(|| {
            div()
                .id(("since-chip", ix))
                .flex_none()
                .px(px(6.))
                .h(px(18.))
                .flex()
                .items_center()
                .rounded_full()
                .bg(theme::warning_bg())
                .text_size(px(11.))
                .text_color(theme::warning())
                .child(format!("↻ v{n}"))
                .tip(format!("Changed since v{n}, the version you read"))
        })
    }

    /// How many files are marked read, of how many.
    pub fn review_progress(&self) -> Option<(usize, usize)> {
        self.review_scope.as_ref()?;
        let total = self.diff.as_ref()?.files.len();
        let done = (0..total)
            .filter(|&ix| self.review_state(ix) == Some(State::Done))
            .count();
        Some((done, total))
    }

    /// Every file of the open diff is reviewed.
    pub fn all_reviewed(&self) -> bool {
        self.review_progress()
            .is_some_and(|(done, total)| total > 0 && done == total)
    }

    /// The files under folder `path` of the tree.
    pub fn files_under(&self, path: &str) -> Vec<usize> {
        let prefix = format!("{path}/");
        self.diff
            .as_ref()
            .map(|d| {
                d.files
                    .iter()
                    .enumerate()
                    .filter(|(_, f)| f.path().starts_with(&prefix))
                    .map(|(ix, _)| ix)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The key the marks of the open diff are kept under; none for the working tree, which has
    /// no stable identity. A branch's pages keep them too: your own work is reviewed there before
    /// it goes out (the developer, 2026-10-23; #74 had none).
    fn review_scope(&self) -> Option<String> {
        let root = self.root.as_ref()?.display().to_string();
        let key = match &self.diff.as_ref()?.header {
            Header::Commit(c) => format!("commit:{}", c.id),
            Header::Compare {
                request: Some(iid), ..
            } => format!("mr:{iid}"),
            Header::Compare { base, head, .. } => format!("range:{}..{}", base.1, head.1),
            Header::Versions { from, to, .. } => format!("ver:{}..{}", from.tip, to.tip),
            Header::Interdiff { .. } | Header::WorkingTree { .. } => return None,
        };
        Some(format!("{root}|{key}"))
    }

    /// Records whether the head of the open diff is fully reviewed; the card offering what is next
    /// comes up the moment it becomes so.
    fn sync_complete(&mut self) {
        let (Some(root), Some(head)) = (self.root.clone(), self.viewed_rev()) else {
            return;
        };
        let root = root.display().to_string();
        let done = self.all_reviewed();
        // Only a whole branch or request (a compare) makes its head count as reviewed; one commit read is not its
        // branch read, nor what changed since a version (#182: its marks reach the whole diff through `read_at`).
        let whole = matches!(
            self.diff.as_ref().map(|d| &d.header),
            Some(Header::Compare { .. })
        );
        if done && !(whole && self.reviewed.head_done(&root, head)) {
            self.done_card = true;
        }
        if !done {
            self.done_card = false;
        }
        if whole {
            self.reviewed.set_head(&root, head, done);
            if let Some((reviewed, total)) = self.review_progress() {
                let open: Vec<String> = (0..total)
                    .filter(|&ix| self.review_state(ix) != Some(State::Done))
                    .filter_map(|ix| Some(self.diff.as_ref()?.files.get(ix)?.path().to_owned()))
                    .collect();
                self.reviewed
                    .set_progress(&root, head, reviewed as u32, total as u32, open);
            }
        }
    }

    /// How far the review of the branch or request ending at `head` got, when it started and is
    /// not done: (files reviewed, files).
    pub fn head_part(&self, head: git2::Oid) -> Option<(u32, u32)> {
        let root = self.root.as_ref()?.display().to_string();
        if self.reviewed.head_done(&root, head) {
            return None;
        }
        self.reviewed
            .head_progress(&root, head)
            .filter(|&(done, total)| done > 0 && done < total)
    }

    /// How far your review of request `iid` got, against `head`, its head now (#72).
    pub fn request_seen(&self, iid: u64, head: Option<git2::Oid>) -> Seen {
        let Some(root) = self.root.as_ref().map(|r| r.display().to_string()) else {
            return Seen::Never;
        };
        if head.is_some_and(|h| self.reviewed.head_done(&root, h)) {
            return Seen::Done;
        }
        let heads = self.reviewed.marked_heads(&format!("{root}|mr:{iid}"));
        if heads.is_empty() {
            Seen::Never
        } else if head.is_some() && heads.iter().all(|h| *h == head) {
            Seen::Started
        } else {
            Seen::Earlier
        }
    }

    /// The head of request `iid` you last reviewed at, when it was pushed since (`Seen::Earlier`).
    pub fn reviewed_head(&self, iid: u64, head: Option<git2::Oid>) -> Option<git2::Oid> {
        let root = self.root.as_ref()?.display().to_string();
        self.reviewed.last_head(&format!("{root}|mr:{iid}"), head)
    }

    /// Whether the whole diff of a branch or request ending at `head` was reviewed.
    pub fn head_reviewed(&self, head: git2::Oid) -> bool {
        self.root
            .as_ref()
            .is_some_and(|r| self.reviewed.head_done(&r.display().to_string(), head))
    }

    /// Called when a diff arrives: its scope and the fingerprint of each file.
    pub fn index(&mut self) {
        self.review_scope = self.review_scope();
        (self.fps, self.legacy_fps) = match (&self.review_scope, &self.diff) {
            (Some(_), Some(d)) => (
                d.files.iter().map(fingerprint).collect(),
                d.files.iter().map(legacy_fingerprint).collect(),
            ),
            _ => (Vec::new(), Vec::new()),
        };
        self.crossed = self.crossings();
        self.done_card = false;
        // Another diff, another range on the toolbar's left: measured anew (#187).
        self.tools.left.set(0.);
    }

    /// Marks file `ix` read (`done`) or unmarks it; false when the diff keeps no marks.
    pub fn set_reviewed(&mut self, ix: usize, done: bool) -> bool {
        let (Some(scope), Some(fp)) = (self.review_scope.clone(), self.fps.get(ix).copied()) else {
            return false;
        };
        let Some(path) = self
            .diff
            .as_ref()
            .and_then(|d| d.files.get(ix))
            .map(|f| f.path().to_owned())
        else {
            return false;
        };
        if done {
            let head = self.viewed_rev();
            self.reviewed.set(&scope, &path, fp, head);
            self.read_through(&[path]);
        } else {
            self.reviewed.clear(&scope, &path);
        }
        self.review_changed(&[ix]);
        self.sync_complete();
        true
    }

    /// Files `paths` marked in a since view (`Since v2`): read in the whole diff up to the same head too, its
    /// count moving without it open (#182).
    fn read_through(&mut self, paths: &[String]) {
        let (Some(root), Some(Header::Versions { to, .. })) = (
            self.root.as_ref().map(|r| r.display().to_string()),
            self.diff.as_ref().map(|d| &d.header),
        ) else {
            return;
        };
        let head = to.tip;
        for path in paths {
            self.reviewed.read_at(&root, head, path);
        }
    }

    /// Marks or unmarks files `ixs` at once; false when the diff keeps no marks.
    pub fn set_reviewed_many(&mut self, ixs: &[usize], done: bool) -> bool {
        let (Some(scope), Some(diff)) = (self.review_scope.clone(), self.diff.as_ref()) else {
            return false;
        };
        let files: Vec<(String, u64)> = ixs
            .iter()
            .filter_map(|&ix| Some((diff.files.get(ix)?.path().to_owned(), *self.fps.get(ix)?)))
            .collect();
        let head = self.viewed_rev();
        self.reviewed.set_many(&scope, &files, done, head);
        if done {
            let paths: Vec<String> = files.iter().map(|(p, _)| p.clone()).collect();
            self.read_through(&paths);
        }
        self.review_changed(ixs);
        self.sync_complete();
        true
    }

    /// The next file, in the tree's order, that is not marked read: its collapsed folders open, and a
    /// generated file folded opens too, as its "Show it anyway" (#144, #167).
    pub fn next_unreviewed(&mut self) -> Option<usize> {
        let order = self.file_order_all();
        let ix = next_open(&order, self.file, |ix| {
            self.review_state(ix) == Some(State::Done)
        })?;
        self.open_folders_of(ix);
        if let Some(path) = self
            .diff
            .as_ref()
            .and_then(|d| d.files.get(ix))
            .filter(|f| self.generated_folded(f))
            .map(|f| f.path().to_owned())
        {
            self.unfolded.insert(path);
            self.sync_stack();
        }
        Some(ix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use artoo_core::git::{ChangeKind, DiffLine, Hunk, LineKind};

    /// `path` whose line `at` (1-based) changes from `was` to `is` among `around`'s other lines.
    fn changed(path: &str, around: &[&str], at: usize, was: &str, is: &str) -> FileDiff {
        let side = |line: &str| {
            let mut lines: Vec<&str> = around.to_vec();
            lines.insert(at - 1, line);
            lines.join("\n")
        };
        let line = |kind, old: Option<usize>, new: Option<usize>| DiffLine {
            kind,
            old_line: old.map(|n| n as u32),
            new_line: new.map(|n| n as u32),
        };
        FileDiff {
            old_path: Some(path.to_owned()),
            new_path: Some(path.to_owned()),
            change: ChangeKind::Modified,
            added: 1,
            removed: 1,
            hunks: vec![Hunk {
                lines: vec![
                    line(LineKind::Removed, Some(at), None),
                    line(LineKind::Added, None, Some(at)),
                ],
            }],
            old_text: Some(side(was).into()),
            new_text: Some(side(is).into()),
            note: None,
            old_image: None,
            new_image: None,
            pictures: Vec::new(),
        }
    }

    #[test]
    fn a_rebase_that_keeps_a_files_change_keeps_its_fingerprint() {
        let read = changed(
            "a.rs",
            &["fn a() {}", "fn b() {}"],
            2,
            "let x = 1;",
            "let x = 2;",
        );
        // main grew the file above the change, elsewhere: the lines move, the change is the same.
        let rebased = changed(
            "a.rs",
            &["// new header", "use std::fmt;", "fn a() {}", "fn b() {}"],
            4,
            "let x = 1;",
            "let  x = 2;",
        );
        assert_eq!(fingerprint(&read), fingerprint(&rebased));
        assert_ne!(legacy_fingerprint(&read), legacy_fingerprint(&rebased));
        // A push that changes the hunk itself.
        let pushed = changed(
            "a.rs",
            &["fn a() {}", "fn b() {}"],
            2,
            "let x = 1;",
            "let x = 3;",
        );
        assert_ne!(fingerprint(&read), fingerprint(&pushed));
    }

    #[test]
    fn a_mark_is_found_in_any_reading_of_the_same_change() {
        let mut store = Store::default();
        let (v2, v3, commit) = (
            git2::Oid::from_bytes(&[2; 20]).unwrap(),
            git2::Oid::from_bytes(&[3; 20]).unwrap(),
            git2::Oid::from_bytes(&[9; 20]).unwrap(),
        );
        // In a comparison: the same change's fingerprint, read anywhere in the repository.
        store.set("/r|range:a..b", "a.rs", 42, Some(v3));
        assert!(store.read_anywhere("/r", "a.rs", 42));
        assert!(!store.read_anywhere("/r", "a.rs", 43));
        assert!(!store.read_anywhere("/other", "a.rs", 42));
        // In Since v2, up to v3.
        store.set(&format!("/r|ver:{v2}..{v3}"), "b.rs", 7, Some(v3));
        assert!(store.read_since("/r", "b.rs", v3));
        assert!(!store.read_since("/r", "b.rs", v2));
        assert!(!store.read_since("/r", "a.rs", v3));
        // In a commit.
        assert!(!store.read_in_commits("/r", "c.rs"));
        store.set(&format!("/r|commit:{commit}"), "c.rs", 1, Some(commit));
        assert!(store.read_in_commits("/r", "c.rs"));
        assert!(store.read_in_commit("/r", "c.rs", commit));
        assert!(!store.read_in_commit("/r", "c.rs", v2));
    }

    #[test]
    fn a_file_read_since_a_version_moves_the_whole_count_and_the_last_one_finishes_it() {
        let mut store = Store::default();
        let head = git2::Oid::from_bytes(&[3; 20]).unwrap();
        let open = vec!["a.rs".to_owned(), "b.rs".to_owned()];
        store.set_progress("/r", head, 6, 8, open);
        // Read in the request already: nothing moves.
        assert!(!store.read_at("/r", head, "c.rs"));
        assert_eq!(store.head_progress("/r", head), Some((6, 8)));
        assert!(store.read_at("/r", head, "a.rs"));
        assert_eq!(store.head_progress("/r", head), Some((7, 8)));
        assert!(!store.head_done("/r", head));
        assert!(store.read_at("/r", head, "b.rs"));
        assert_eq!(store.head_progress("/r", head), Some((8, 8)));
        assert!(store.head_done("/r", head));
        // Another head: its own count, untouched.
        let other = git2::Oid::from_bytes(&[4; 20]).unwrap();
        assert!(!store.read_at("/r", other, "a.rs"));
    }

    #[test]
    fn marks_survive_a_round_trip() {
        let mut store = Store::default();
        let head = git2::Oid::from_bytes(&[7; 20]).unwrap();
        store.marks.insert(
            "/r|mr:412".into(),
            HashMap::from([(
                "src/a.rs".to_owned(),
                Mark {
                    fp: 0xdead_beef_u64,
                    time: 1_792_149_840,
                    head: Some(head),
                },
            )]),
        );
        let again = Store::parse(&store.render());
        assert_eq!(again.get("/r|mr:412", "src/a.rs"), Some(0xdead_beef));
        assert_eq!(
            again.when("/r|mr:412", "src/a.rs"),
            Some((1_792_149_840, Some(head)))
        );
        assert_eq!(again.get("/r|mr:413", "src/a.rs"), None);
        // A line from before marks had a time still reads.
        let old = Store::parse("/r|mr:412\tsrc/a.rs\tdeadbeef\n");
        assert_eq!(old.get("/r|mr:412", "src/a.rs"), Some(0xdead_beef));
        assert_eq!(old.when("/r|mr:412", "src/a.rs"), None);
    }

    #[test]
    fn a_folder_is_marked_and_unmarked_at_once() {
        let mut store = Store::default();
        let files = [("ui/a.rs".to_owned(), 1), ("ui/b.rs".to_owned(), 2)];
        store.set_many("s", &files, true, None);
        assert_eq!(
            (store.get("s", "ui/a.rs"), store.get("s", "ui/b.rs")),
            (Some(1), Some(2))
        );
        store.set_many("s", &files[..1], false, None);
        assert_eq!(
            (store.get("s", "ui/a.rs"), store.get("s", "ui/b.rs")),
            (None, Some(2))
        );
    }

    #[test]
    fn a_reviewed_head_stops_counting_when_the_branch_moves() {
        let mut store = Store::default();
        let (old, new) = (
            git2::Oid::from_bytes(&[1; 20]).unwrap(),
            git2::Oid::from_bytes(&[2; 20]).unwrap(),
        );
        store.set_head("/r", old, true);
        assert!(store.head_done("/r", old));
        assert!(!store.head_done("/r", new));
        assert!(!store.head_done("/other", old));
        store.set_head("/r", old, false);
        assert!(!store.head_done("/r", old));
    }

    #[test]
    fn past_the_limit_the_oldest_goes_and_the_new_one_stays() {
        let mut store = Store::default();
        let mark = |time| Mark {
            fp: 1,
            time,
            head: None,
        };
        for i in 0..300 {
            store.marks.insert(
                format!("/r|mr:{i}"),
                HashMap::from([("a".to_owned(), mark(1_000 + i))]),
            );
        }
        // A scope is as old as its newest mark: mr:0's second file makes it the newest of them.
        store
            .marks
            .get_mut("/r|mr:0")
            .unwrap()
            .insert("b".into(), mark(2_000));
        store.set("/r|mr:300", "a", 1, None);
        assert!(store.marks.contains_key("/r|mr:0") && !store.marks.contains_key("/r|mr:1"));
        for i in 301..600 {
            let scope = format!("/r|mr:{i}");
            store.set(&scope, "a", 1, None);
            assert_eq!(store.get(&scope, "a"), Some(1), "{scope} lost its mark");
            assert_eq!(store.marks.len(), 300);
        }
        assert!(
            store
                .marks
                .keys()
                .all(|s| s["/r|mr:".len()..].parse::<u32>().unwrap() >= 300)
        );

        let head =
            |i: u32| git2::Oid::from_bytes(&[&[0; 16][..], &i.to_be_bytes()].concat()).unwrap();
        for i in 0..600 {
            store.set_head("/r", head(i), true);
            store.set_progress("/r", head(i), 1, 2, Vec::new());
            assert!(store.head_done("/r", head(i)));
            assert_eq!(store.head_progress("/r", head(i)), Some((1, 2)));
        }
        assert_eq!((store.heads.len(), store.progress.len()), (500, 500));
        assert!(!store.head_done("/r", head(99)) && store.head_done("/r", head(100)));
        assert_eq!(store.head_progress("/r", head(99)), None);
    }

    #[test]
    fn v_walks_every_file_and_goes_round() {
        // Tree order with every folder open: A/a.kt, B/b.kt, C/d.kt, c.kt, e.kt (#167).
        let order = [0, 1, 3, 2, 4];
        let done = |set: &'static [usize]| move |ix| set.contains(&ix);
        // From c.kt: e.kt; from e.kt round to a.kt; b.kt, reviewed, is passed.
        assert_eq!(next_open(&order, 2, done(&[])), Some(4));
        assert_eq!(next_open(&order, 4, done(&[0])), Some(1));
        assert_eq!(next_open(&order, 4, done(&[0, 1])), Some(3));
        // Everything else reviewed: nowhere to go.
        assert_eq!(next_open(&order, 2, done(&[0, 1, 3, 4])), None);
        assert_eq!(next_open(&[], 0, done(&[])), None);
    }

    #[test]
    fn junk_lines_are_skipped() {
        let store = Store::parse("nonsense\nscope\tpath\tzz\nscope\tp\t1f\n");
        assert_eq!(store.get("scope", "p"), Some(0x1f));
        assert_eq!(store.get("scope", "path"), None);
    }
}
