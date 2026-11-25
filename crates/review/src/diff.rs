//! The diff the review shows, read from the repository (`read`): its types are the contract's
//! (`artoo_review_api::diff`); and a change to it the reviewer did not ask for (`Notice`).

use artoo_core::git::{DiffSettings, Repo};
pub use artoo_review_api::Notice;
use artoo_review_api::Source;
pub use artoo_review_api::diff::{Diff, FileTags, Header};
use artoo_ui_kit::format;
use std::path::Path;
use std::sync::Arc;

/// The whole work's range as the repository is now, not as it was remembered: the main line at its
/// tip today, and only while the branch it was for is still the one checked out. HEAD moved outside Artoo — a
/// checkout, a rebase — leaves the remembered base and branch behind, and a base older than the new HEAD's fork
/// would count everything the main line did since.
fn fresh_since(
    repo: &Repo,
    since: Option<artoo_review_api::diff::Since>,
) -> Option<artoo_review_api::diff::Since> {
    let mut since = since?;
    if repo.head_branch().as_deref() != Some(since.branch.0.as_str()) {
        return None;
    }
    if let Ok(base) = repo.resolve(&since.base.0) {
        since.base.1 = base;
    }
    if let Ok(tip) = repo.resolve(&since.branch.0) {
        since.branch.1 = tip;
    }
    Some(since)
}

/// What `source` is, read from the repository at `root` with `settings`.
pub fn read(root: &Path, source: Source, settings: DiffSettings) -> anyhow::Result<Diff> {
    let repo = Repo::open(root)?;
    let plain = |header, files| Diff {
        header,
        files: Arc::new(files),
        pairs: Vec::new(),
        tags: Default::default(),
    };
    Ok(match source {
        Source::Commit(commit) => {
            let files = repo.commit_diff(commit.id, settings)?;
            plain(Header::Commit(commit), files)
        }
        Source::Commits { newest, oldest } => {
            // A first commit has no parent: its diff is all it adds, a run is then from it.
            let base = repo.parent_of(oldest).unwrap_or(oldest);
            let (files, start, commits) = if newest == oldest {
                (repo.commit_diff(newest, settings)?, base, 1)
            } else {
                let c = repo.compare(base, newest, false, settings)?;
                (c.files, c.start, c.commits)
            };
            let header = Header::Compare {
                base: (format!("{}^", format::short(oldest)), base),
                head: (format::short(newest), newest),
                since_merge_base: false,
                start,
                commits,
                request: None,
                stale: None,
            };
            plain(header, files)
        }
        Source::Versions { from, to } => {
            let diff = repo.version_diff(&from, &to, settings)?;
            let pairs = repo.range_pairs(&from, &to).unwrap_or_default();
            Diff {
                header: Header::Versions {
                    from,
                    to,
                    rebased: diff.rebased,
                    conflicts: diff.conflicts.len(),
                },
                files: Arc::new(diff.files),
                pairs,
                tags: Default::default(),
            }
        }
        Source::Interdiff { from, to, old, new } => {
            let diff = repo.interdiff(old.id, new.id, settings)?;
            plain(Header::Interdiff { from, to, old, new }, diff.files)
        }
        Source::WorkingTree {
            only_unstaged,
            since,
        } => {
            // All changes as `git status` groups them (#22); Not staged only is that group alone; the checked-out
            // branch's whole work against its main line, one list (#209).
            let since = fresh_since(&repo, since);
            let only_unstaged = only_unstaged && since.is_none();
            let tree = match (&since, only_unstaged) {
                (Some(since), _) => repo.working_since(since.base.1, settings)?,
                (None, true) => repo.working_tree(settings, true)?,
                (None, false) => repo.working_groups(settings)?,
            };
            Diff {
                header: Header::WorkingTree {
                    only_unstaged,
                    since,
                },
                files: Arc::new(tree.files),
                pairs: Vec::new(),
                tags: FileTags {
                    words: tree.tags,
                    staged: tree.staged,
                },
            }
        }
        Source::Compare {
            base,
            head,
            since,
            request,
        } => {
            let c = repo.compare(base.1, head.1, since, settings)?;
            // Only a local branch that still points where it was picked can be stale.
            let stale = repo
                .stale(&base.0)
                .filter(|_| repo.resolve(&base.0).ok() == Some(base.1));
            let header = Header::Compare {
                base,
                head,
                since_merge_base: since,
                start: c.start,
                commits: c.commits,
                request,
                stale,
            };
            plain(header, c.files)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use artoo_review_api::diff::Since;

    fn commit(
        repo: &git2::Repository,
        refname: &str,
        parents: &[git2::Oid],
        file: (&str, &str),
    ) -> git2::Oid {
        let sig = git2::Signature::now("t", "t@example.com").unwrap();
        let mut b = repo.treebuilder(None).unwrap();
        let blob = repo.blob(file.1.as_bytes()).unwrap();
        b.insert(file.0, blob, 0o100644).unwrap();
        let tree = repo.find_tree(b.write().unwrap()).unwrap();
        let parents: Vec<_> = parents
            .iter()
            .map(|p| repo.find_commit(*p).unwrap())
            .collect();
        let parents: Vec<&git2::Commit> = parents.iter().collect();
        repo.commit(Some(refname), &sig, &sig, "c", &tree, &parents)
            .unwrap()
    }

    #[test]
    fn the_whole_work_is_read_from_the_main_line_and_branch_as_they_are_now() {
        let dir = tempfile::tempdir().unwrap();
        let git = git2::Repository::init(dir.path()).unwrap();
        let old_main = commit(&git, "refs/heads/main", &[], ("m.txt", "1"));
        // The main line moved on, and the branch forked from its new tip.
        let main = commit(&git, "refs/heads/main", &[old_main], ("m.txt", "2"));
        let new = commit(&git, "refs/heads/new", &[main], ("n.txt", "n"));
        git.set_head("refs/heads/new").unwrap();
        let repo = Repo::open(dir.path()).unwrap();
        // Remembered from an older reading: the main line before it moved, another branch.
        let remembered = Since {
            base: ("main".into(), old_main),
            branch: ("new".into(), main),
        };
        let fresh = fresh_since(&repo, Some(remembered.clone())).unwrap();
        assert_eq!(fresh.base.1, main);
        assert_eq!(fresh.branch.1, new);
        // HEAD on another branch than the one remembered: no whole work, the working tree alone.
        let other = Since {
            branch: ("old".into(), old_main),
            ..remembered
        };
        assert!(fresh_since(&repo, Some(other)).is_none());
    }
}
