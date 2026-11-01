//! What goes over the agents' folder (#165: the agents' model, which the tab reads on its beat): the chats that
//! listen, the windows of Artoo there, what an agent asks the app to show. The agents write and read the folder.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// How long a watcher may go quiet and still count as listening.
pub const FRESH: i64 = 12;

/// The same for a Codex session the app watches for (the agents' `codex`): the app writes it on its
/// own beat, which slows to 8 s when nothing changes and more in the background, and takes it off
/// when the session closes — at 12 s it came and went (the developer, 2026-10-23).
const CODEX_FRESH: i64 = 90;

/// A chat whose watcher runs (or ran).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chat {
    /// The agent's session id.
    pub session: String,
    /// "Claude Code", "Codex", …
    pub agent: String,
    /// The folder it works in: the project's main working tree, or a worktree of it.
    pub repo: PathBuf,
    /// The project's main working tree, where its notes are kept (#77); empty from an older
    /// watcher, which worked in the main one.
    #[serde(default)]
    pub project: PathBuf,
    /// The branch checked out in its folder: the notes of that branch go to it (#77).
    #[serde(default)]
    pub branch: String,
    /// Its first message, to tell chats apart.
    pub title: String,
    /// The agent's process, and the watcher's.
    pub pid: u32,
    pub watcher: u32,
    /// When the watcher last said it is there, seconds since the epoch.
    pub beat: i64,
}

/// Seconds since the epoch, as everything on the bus is stamped.
pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// A window of Artoo while it runs (#153): its process and the projects open in its tabs. The
/// MCP server reads them to know whether what an agent does is seen, and when `open_artoo` is
/// done.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Presence {
    pub pid: u32,
    /// Each tab's project: its main working tree.
    pub projects: Vec<PathBuf>,
    /// When the window last said it is there.
    pub beat: i64,
}

/// What an agent asks the app to show (#153): written by the MCP server, taken once by the app.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub uid: String,
    /// The project it is about: its main working tree.
    pub project: PathBuf,
    /// Who: "Claude Code", "Codex", "An agent".
    pub agent: String,
    pub at: i64,
    pub what: What,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum What {
    /// The project in a tab (`open_artoo`).
    Open,
    /// Pending comments written on request `iid` (`add_pending_comment`, #150).
    Pending { iid: u64, ids: Vec<u64> },
    /// Requests put under Merge requests, in this order (`gather_requests`, #148).
    Gathered { iids: Vec<u64> },
    /// Two refs compared, since their merge base unless `direct`, by their names and the commits
    /// they were at (`open_changes`, #149).
    Compare {
        base: String,
        base_id: String,
        head: String,
        head_id: String,
        direct: bool,
    },
    /// The working tree, or what is not staged in it (`open_changes`, #149).
    WorkingTree { unstaged: bool },
    /// Notes on lines of `changes` (a `Compare` or a `WorkingTree`) and their summary
    /// (`annotate_changes`, #151); with `open`, the changes are opened under Branches as well
    /// (`open_changes` with notes).
    Notes {
        changes: Box<What>,
        summary: String,
        notes: Vec<crate::notes::Note>,
        open: bool,
    },
    /// A merge request drafted for the reviewer to check and create (`draft_request`, #152).
    NewRequest(RequestDraft),
    /// One place the agent points the reviewer at (`show`, #154).
    Point { place: Place },
}

/// A place an agent points at (#154): the toast says it, and its Show goes there.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "at", rename_all = "snake_case")]
pub enum Place {
    /// A file of the project's working tree, at a line.
    File {
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        line: Option<u32>,
    },
    /// A commit, by its whole id.
    Commit { id: String },
    /// Request `iid`, at a comment of one of its discussions or at a pending comment.
    Request {
        iid: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        note: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        draft: Option<u64>,
    },
}

impl Place {
    /// How the toast says it: `nav.rs:785`, `commit a1b2c3d`, `!12, a comment`.
    pub fn said(&self, github: bool) -> String {
        match self {
            Place::File { path, line } => {
                let name = path.rsplit('/').next().unwrap_or(path);
                match line {
                    Some(line) => format!("{name}:{line}"),
                    None => name.to_owned(),
                }
            }
            Place::Commit { id } => format!("commit {}", &id[..id.len().min(7)]),
            Place::Request { iid, note, draft } => {
                let number = format!("{}{iid}", if github { '#' } else { '!' });
                match (note, draft) {
                    (Some(_), _) => format!("{number}, a comment"),
                    (_, Some(_)) => format!("{number}, a pending comment"),
                    _ => number,
                }
            }
        }
    }
}

/// An agent's draft of a merge request (#152): the New merge request card opens with it filled in;
/// nothing is created until the reviewer creates it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestDraft {
    /// The branch it merges, as the host names it: `feature-x`, not `origin/feature-x`.
    pub source: String,
    pub target: String,
    pub title: String,
    pub description: String,
    /// Usernames, without `@`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reviewers: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,
}

/// Something an agent left on a request that the app tells apart from the reviewer's own: a
/// pending comment it wrote (#150), or its line on the request when it gathered it (#148).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mark {
    pub agent: String,
    pub iid: u64,
    /// The pending comment's id, as the host lists it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draft: Option<u64>,
    /// What it did there, as it said when it gathered the request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub at: i64,
}

impl Chat {
    pub fn listening(&self) -> bool {
        let fresh = if self.limited() { CODEX_FRESH } else { FRESH };
        now() - self.beat <= fresh
    }

    /// A Codex session the app watches for (the agents' `codex`), limited: said so wherever it is picked.
    pub fn limited(&self) -> bool {
        self.watcher == 0 && self.agent == "Codex"
    }
}
