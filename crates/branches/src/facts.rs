//! What a branch's page says of it (#74), read when the branch loads: the main line it forks from, how far it is
//! from it, where it is pushed.

use crate::history::NamedTip;
use artoo_core::git::Repo;
use git2::Oid;

/// What a branch's page says of it, read when the branch loads.
#[derive(Clone, Debug, Default)]
pub struct Facts {
    /// The branch it is a page of.
    pub refname: String,
    /// The main line it forks from, at its tip; none for the main line itself.
    pub base: Option<NamedTip>,
    /// Commits it has that the main line has not, and the other way round.
    pub ahead: usize,
    pub behind: usize,
    /// Its upstream, with how far it is ahead of it and behind it; none when it was never pushed.
    pub upstream: Option<(String, usize, usize)>,
    /// The files of its Changes, once read.
    pub files: Option<usize>,
}

impl Facts {
    /// Reads them for branch `name` (`refname`) at `tip`: its base is the main line's local branch,
    /// else its remote one.
    pub fn read(repo: &Repo, refname: &str, name: &str, tip: Oid) -> Facts {
        let main = repo.default_branch();
        let base = main
            .as_deref()
            .filter(|m| name != *m && name != format!("origin/{m}"))
            .and_then(|m| {
                [m.to_owned(), format!("origin/{m}")]
                    .into_iter()
                    .find_map(|n| repo.resolve(&n).ok().map(|id| (n, id)))
            });
        let (ahead, behind) = base
            .as_ref()
            .and_then(|(_, b)| repo.ahead_behind(tip, *b))
            .unwrap_or_default();
        Facts {
            refname: refname.to_owned(),
            base,
            ahead,
            behind,
            upstream: repo.tracking(name),
            files: None,
        }
    }
}
