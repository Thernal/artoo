//! The diff the review shows, read from the repository (`read`): its types are the contract's
//! (`artoo_review_api::diff`); and a change to it the reviewer did not ask for (`Notice`).

use artoo_core::git::{DiffSettings, Repo};
pub use artoo_review_api::Notice;
use artoo_review_api::Source;
pub use artoo_review_api::diff::{Diff, FileTags, Header};
use artoo_ui_kit::format;
use std::path::Path;
use std::sync::Arc;

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
