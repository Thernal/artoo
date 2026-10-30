//! The requests list (#72, #81): grouped by what each needs from you, a group's band folding its rows; the folds and
//! the requests you have opened kept per project; the picked one's pipeline, approvals and what changed since your
//! review read for its preview.

use crate::group::Group;
use crate::requests::Requests;
use artoo_core::git::{DiffSettings, Repo};
use artoo_core::storage;
use artoo_hosts_api::{Approvals, Host, Mr, Pipeline, Thread};
use artoo_review_api::Seen;
use gpui::{Context, Task};
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

/// What the requests list keeps per project (`<repo>\t<folded groups>\t<requests opened>`, both
/// comma-separated, then `folds` once a group was folded or unfolded by hand) (#81). Every group
/// starts open (the developer, 2026-10-23); a line written before the folds were yours carries the
/// old default (Reviewed folded), and is read as nothing folded.
const LIST_FILE: &str = "requests-list.txt";

/// The height of a group's band in the requests list.
pub const BAND: f32 = 28.;

/// Where request `seen` sits: yours first of all, then by how far your review got.
pub fn group(mine: bool, asks: bool, seen: Seen) -> Group {
    match seen {
        _ if mine => Group::Yours,
        Seen::Done => Group::Reviewed,
        Seen::Earlier => Group::Updated,
        Seen::Started => Group::Asks,
        Seen::Never if asks => Group::Asks,
        Seen::Never => Group::New,
    }
}

/// A request's pipeline and approvals, read for its preview, with its `updated` then.
pub struct Preview {
    updated: i64,
    pub pipeline: Option<Pipeline>,
    pub approvals: Option<Approvals>,
    /// Pushed since your review: the version you viewed, and how many files changed since then
    /// (X22).
    pub since: Option<(usize, usize)>,
}

/// The list's rows: their previews, its scroll, the groups folded and the requests opened.
#[derive(Default)]
pub struct Rows {
    previews: HashMap<u64, Preview>,
    preview_task: Option<Task<()>>,
    /// The requests list's scroll, and whether the next frame brings the picked row into view
    /// (after ↑ ↓ or esc back to the list).
    pub scroll: gpui::ScrollHandle,
    pub reveal: Cell<bool>,
    /// Project `kept_of`'s requests list: the groups folded, and the requests you have opened —
    /// a row's dot is gone once you have (#81).
    folded: HashSet<Group>,
    opened: HashSet<u64>,
    /// A group was folded or unfolded by hand: the folds are kept.
    chosen: bool,
    kept_of: Option<String>,
}

impl Rows {
    /// Project `repo`'s folds and opened requests, unless they are the ones held.
    pub fn load(&mut self, repo: &str) {
        if self.kept_of.as_deref() == Some(repo) {
            return;
        }
        (self.folded, self.opened, self.chosen) =
            kept_list(&storage::read(LIST_FILE), repo).unwrap_or_default();
        self.kept_of = Some(repo.to_owned());
    }

    fn keep(&self) {
        if let Some(repo) = &self.kept_of {
            let text = storage::read(LIST_FILE);
            storage::save(
                LIST_FILE,
                with_kept_list(&text, repo, &self.folded, &self.opened, self.chosen),
            );
        }
    }

    /// Whether group `g`'s rows are folded away; a filter typed (`filtering`) opens every group.
    pub fn folds(&self, g: Group, filtering: bool) -> bool {
        !filtering && self.folded.contains(&g)
    }

    /// Group `g` folded or unfolded by hand.
    pub fn toggle(&mut self, g: Group) {
        self.chosen = true;
        if !self.folded.remove(&g) {
            self.folded.insert(g);
        }
        self.keep();
    }

    /// Whether request `iid` was opened here (#81).
    pub fn opened(&self, iid: u64) -> bool {
        self.opened.contains(&iid)
    }

    /// Request `iid`'s pipeline and approvals, when they were read.
    pub fn preview(&self, iid: u64) -> Option<&Preview> {
        self.previews.get(&iid)
    }
}

impl Requests {
    /// Request `m`'s group in the list, `seen` how far your review of it got.
    pub fn group_of(&self, m: &Mr, seen: Seen) -> Group {
        let me = self.login.as_deref().unwrap_or_default();
        group(
            !me.is_empty() && m.author_login == me,
            !me.is_empty() && m.reviewers.iter().any(|r| r == me),
            seen,
        )
    }

    /// Of a request's `threads`, those waiting for you, its author (`waiting`).
    pub fn waiting_for_me<'a>(&self, threads: &'a [Thread]) -> Vec<&'a Thread> {
        waiting(self.login.as_deref().unwrap_or_default(), threads)
    }

    /// Request `iid` opened: its row's dot goes for good. Requests no longer open are forgotten.
    pub fn mark_opened(&mut self, iid: u64) {
        if self.rows.opened.contains(&iid) {
            return;
        }
        if !self.list.is_empty() {
            let list = &self.list;
            self.rows
                .opened
                .retain(|i| list.iter().any(|m| m.iid == *i));
        }
        self.rows.opened.insert(iid);
        self.rows.keep();
    }

    /// Request `m`, picked in the list: its pipeline and approvals, once per update of it; with
    /// `viewed` (its head now, and the one you reviewed it at), how many files changed since.
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
        if self
            .rows
            .previews
            .get(&iid)
            .is_some_and(|p| p.updated == updated)
        {
            return;
        }
        self.rows.preview_task = Some(cx.spawn(async move |this, cx| {
            let (pipeline, approvals, since) = cx
                .background_executor()
                .spawn(async move {
                    let since = viewed.and_then(|(head, viewed)| {
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
                    });
                    (
                        host.pipeline(iid).ok().flatten(),
                        host.approvals(iid).ok(),
                        since,
                    )
                })
                .await;
            this.update(cx, |rq, cx| {
                rq.rows.previews.insert(
                    iid,
                    Preview {
                        updated,
                        pipeline,
                        approvals,
                        since,
                    },
                );
                cx.notify();
            })
            .ok();
        }));
    }
}

/// Repository `repo`'s line of `requests-list.txt`: its folded groups (none unless folded by
/// hand), the requests opened, and whether the folds are yours; none when it has no line.
fn kept_list(text: &str, repo: &str) -> Option<(HashSet<Group>, HashSet<u64>, bool)> {
    let line = text.lines().find(|l| l.split('\t').next() == Some(repo))?;
    let parts: Vec<&str> = line.split('\t').skip(1).collect();
    let field = |n: usize| parts.get(n).copied().unwrap_or_default().split(',');
    let chosen = parts.get(2) == Some(&"folds");
    let folded = field(0)
        .filter_map(Group::parse)
        .filter(|_| chosen)
        .collect();
    let opened = field(1).filter_map(|iid| iid.parse().ok()).collect();
    Some((folded, opened, chosen))
}

/// `text` with repository `repo`'s line written anew; other repositories' kept.
fn with_kept_list(
    text: &str,
    repo: &str,
    folded: &HashSet<Group>,
    opened: &HashSet<u64>,
    chosen: bool,
) -> String {
    let mut folded: Vec<Group> = folded.iter().copied().collect();
    folded.sort();
    let mut opened: Vec<u64> = opened.iter().copied().collect();
    opened.sort();
    let line = format!(
        "{repo}\t{}\t{}{}",
        folded.iter().map(|g| g.key()).collect::<Vec<_>>().join(","),
        opened
            .iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join(","),
        if chosen { "\tfolds" } else { "" }
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

/// Of a request's `threads`, those waiting for `me`, its author: the last word not mine, and —
/// a discussion — still open, or — a plain comment, which cannot be resolved (X21, the developer,
/// 2026-10-23) — written after anything I wrote on the request.
fn waiting<'a>(me: &str, threads: &'a [Thread]) -> Vec<&'a Thread> {
    if me.is_empty() {
        return Vec::new();
    }
    let my_last = threads
        .iter()
        .flat_map(|t| &t.notes)
        .filter(|n| n.login == me)
        .map(|n| n.created)
        .max()
        .unwrap_or(0);
    threads
        .iter()
        .filter(|t| match t.notes.last() {
            Some(last) if last.login != me => {
                if t.resolvable {
                    !t.resolved
                } else {
                    last.created > my_last
                }
            }
            _ => false,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_is_grouped_by_what_it_needs_from_you() {
        assert_eq!(group(true, true, Seen::Done), Group::Yours);
        assert_eq!(group(false, true, Seen::Never), Group::Asks);
        assert_eq!(group(false, false, Seen::Never), Group::New);
        assert_eq!(group(false, false, Seen::Started), Group::Asks);
        assert_eq!(group(false, true, Seen::Earlier), Group::Updated);
        assert_eq!(group(false, true, Seen::Done), Group::Reviewed);
        // What changed, then what is new, then what is reviewed; yours last.
        assert!(Group::Asks < Group::Updated && Group::Updated < Group::New);
        assert!(Group::New < Group::Reviewed && Group::Reviewed < Group::Yours);
    }

    #[test]
    fn a_projects_folds_and_opened_requests_are_kept_on_its_own_line() {
        let folded = HashSet::from([Group::New, Group::Reviewed]);
        let opened = HashSet::from([415, 398]);
        let text = with_kept_list("b\tasks\t7\tfolds\n", "a", &folded, &opened, true);
        assert_eq!(
            text,
            "a\tothers,reviewed\t398,415\tfolds\nb\tasks\t7\tfolds\n"
        );
        assert_eq!(kept_list(&text, "a"), Some((folded, opened, true)));
        assert_eq!(
            kept_list(&text, "b"),
            Some((HashSet::from([Group::Asks]), HashSet::from([7]), true))
        );
        // Every group unfolded by hand, nothing opened: a line still.
        let none = with_kept_list(&text, "a", &HashSet::new(), &HashSet::new(), true);
        assert_eq!(
            kept_list(&none, "a"),
            Some((HashSet::new(), HashSet::new(), true))
        );
        assert_eq!(kept_list(&none, "c"), None);
        // A line from before the folds were yours: its fold was the old default, so none.
        assert_eq!(
            kept_list("a\treviewed\t398\n", "a"),
            Some((HashSet::new(), HashSet::from([398]), false))
        );
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
        let waits: Vec<i64> = waiting("me", &threads)
            .iter()
            .map(|t| t.notes.last().unwrap().created)
            .collect();
        assert_eq!(waits, [2, 9]);
        assert!(waiting("", &threads).is_empty());
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
}
