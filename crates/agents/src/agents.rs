//! Artoo and the coding agents' chats on this Mac: how a review reaches a chat and the agent's
//! answers come back. Everything goes through files in the data folder's `agents/`, so the app,
//! the MCP server (`crate::mcp`) and a chat's watcher (`crate::watch`) need not run at the same
//! time:
//!
//! - `chats/<session>.json` — a chat that listens: its watcher writes it and keeps it fresh.
//! - `answers/<repo>.jsonl` — what agents did with the comments, appended by the MCP server and
//!   applied by the app (each answer once, by its uid). Only the app writes the comments.
//!
//! A comment sent to a chat says so itself (`review::Sent`); the watcher hands over the comments
//! of its chat that are not delivered yet, then exits so the agent wakes.

use crate::review::{self, Comment, Reply, State};
/// What goes over the folder, the contract's model.
pub use artoo_agents_api::bus::{Chat, Event, Mark, Place, Presence, RequestDraft, What, now};
use artoo_agents_api::provider::{Me, Provider};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// What an agent did with a comment.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Answer {
    pub uid: String,
    pub id: u64,
    /// The batch the comment was in when the agent answered.
    pub batch: String,
    pub kind: Kind,
    #[serde(default)]
    pub body: String,
    /// A reply asking with a form (#10).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub form: Option<review::Form>,
    pub at: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// The chat's watcher handed the comment over.
    Delivered,
    /// A question or remark; the comment stays open.
    Reply,
    /// The agent says it is done.
    Addressed,
}

/// A string no other answer or batch has.
pub use crate::platform::uid;

/// A problem an agent's chat hit and drafted a report of (`report_problem`, #118): the reviewer
/// checks it in the app and sends it, or not.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportDraft {
    pub title: String,
    pub description: String,
    /// Who drafted it: "Claude Code · payments-service".
    pub by: String,
    pub at: i64,
}

/// How long a window may go quiet and still count as there: it says so every few seconds.
const PRESENT: i64 = 60;

/// What the agent running this asks about `project`.
pub fn event(project: PathBuf, what: What) -> Event {
    Event {
        uid: uid(),
        project,
        agent: agent_name(),
        at: now(),
        what,
    }
}

/// The agent running this, by name, for what it leaves: "Claude Code", else "An agent".
pub fn agent_name() -> String {
    whoami().map_or_else(|| "An agent".to_owned(), |(p, _)| p.name().to_owned())
}

/// The folder all of this lives in.
#[derive(Clone, Debug)]
pub struct Bus {
    dir: PathBuf,
}

impl Bus {
    /// The data folder's `agents/`.
    pub fn new() -> Option<Self> {
        Some(Self::at(crate::platform::data_dir()?.join("agents")))
    }

    pub fn at(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// Keeps an agent's draft of a report for the app to open (#118).
    pub fn draft_report(&self, draft: &ReportDraft) -> std::io::Result<()> {
        let dir = self.dir.join("reports");
        std::fs::create_dir_all(&dir)?;
        let text = serde_json::to_string(draft).unwrap_or_default();
        std::fs::write(dir.join(format!("{}.json", uid())), text)
    }

    /// The agents' drafts of reports not taken yet, oldest first; taken, they are gone from here.
    pub fn take_reports(&self) -> Vec<ReportDraft> {
        let Ok(entries) = std::fs::read_dir(self.dir.join("reports")) else {
            return Vec::new();
        };
        let mut out: Vec<ReportDraft> = entries
            .flatten()
            .filter_map(|e| {
                let text = std::fs::read_to_string(e.path()).ok()?;
                std::fs::remove_file(e.path()).ok()?;
                serde_json::from_str(&text).ok()
            })
            .collect();
        out.sort_by_key(|d| d.at);
        out
    }

    fn chat_file(&self, session: &str) -> PathBuf {
        self.dir
            .join("chats")
            .join(format!("{}.json", safe(session)))
    }

    fn answers_file(&self, root: &Path) -> PathBuf {
        self.dir
            .join("answers")
            .join(format!("{}.jsonl", review::key(root)))
    }

    /// The comments file the app keeps, read as it is now.
    pub fn comments_file(&self, root: &Path) -> PathBuf {
        self.dir
            .parent()
            .unwrap_or(&self.dir)
            .join("comments")
            .join(format!("{}.json", review::key(root)))
    }

    /// Says the chat is listening (again).
    pub fn listen(&self, chat: &Chat) -> std::io::Result<()> {
        write_whole(&self.chat_file(&chat.session), chat)
    }

    fn presence_file(&self, pid: u32, window: u64) -> PathBuf {
        self.dir.join("apps").join(format!("{pid}-{window}.json"))
    }

    /// Says window `window` of the app is there with its projects (#153).
    pub fn present(&self, window: u64, presence: &Presence) -> std::io::Result<()> {
        write_whole(&self.presence_file(presence.pid, window), presence)
    }

    /// The window is gone.
    pub fn absent(&self, pid: u32, window: u64) {
        let _ = std::fs::remove_file(self.presence_file(pid, window));
    }

    /// The app's windows that are there: their process alive and their word fresh. The files of a
    /// process that is gone are cleared.
    pub fn running(&self) -> Vec<Presence> {
        let Ok(entries) = std::fs::read_dir(self.dir.join("apps")) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for entry in entries.flatten() {
            let Some(p) = std::fs::read_to_string(entry.path())
                .ok()
                .and_then(|t| serde_json::from_str::<Presence>(&t).ok())
            else {
                continue;
            };
            if !crate::platform::alive(p.pid) {
                let _ = std::fs::remove_file(entry.path());
            } else if now() - p.beat <= PRESENT {
                out.push(p);
            }
        }
        out
    }

    fn marks_file(&self, project: &Path) -> PathBuf {
        self.dir
            .join("marks")
            .join(format!("{}.jsonl", review::key(project)))
    }

    /// Notes that an agent left `mark` in `project` (#150).
    pub fn mark(&self, project: &Path, mark: &Mark) -> std::io::Result<()> {
        append_line(&self.marks_file(project), mark)
    }

    pub fn marks(&self, project: &Path) -> Vec<Mark> {
        std::fs::read_to_string(self.marks_file(project))
            .unwrap_or_default()
            .lines()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect()
    }

    /// When the marks last changed, to read them only then.
    pub fn marks_stamp(&self, project: &Path) -> Option<std::time::SystemTime> {
        std::fs::metadata(self.marks_file(project))
            .and_then(|m| m.modified())
            .ok()
    }

    /// Keeps an agent's event for the app to take (#153).
    pub fn event(&self, event: &Event) -> std::io::Result<()> {
        let file = self
            .dir
            .join("events")
            .join(format!("{}.json", safe(&event.uid)));
        write_whole(&file, event)
    }

    /// The events not taken yet, oldest first.
    pub fn events(&self) -> Vec<Event> {
        let Ok(entries) = std::fs::read_dir(self.dir.join("events")) else {
            return Vec::new();
        };
        let mut out: Vec<Event> = entries
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
            .filter_map(|e| serde_json::from_str(&std::fs::read_to_string(e.path()).ok()?).ok())
            .collect();
        out.sort_by_key(|e| e.at);
        out
    }

    /// Takes event `uid`: true for the one taker only, so two windows never both act on it.
    pub fn take_event(&self, uid: &str) -> bool {
        let file = self.dir.join("events").join(format!("{}.json", safe(uid)));
        std::fs::remove_file(file).is_ok()
    }

    pub fn stop(&self, session: &str) {
        let _ = std::fs::remove_file(self.chat_file(session));
    }

    /// `name` held while the claim lives, so two windows of a project do not both do one thing
    /// (Codex's hand-over, #18); one left over a minute by a crash is taken over.
    pub fn claim(&self, name: &str) -> Option<Claim> {
        let dir = self.dir.join("claims");
        std::fs::create_dir_all(&dir).ok()?;
        let file = dir.join(safe(name));
        let stale = std::fs::metadata(&file)
            .and_then(|m| m.modified())
            .is_ok_and(|at| at.elapsed().unwrap_or_default().as_secs() > 60);
        if stale {
            let _ = std::fs::remove_file(&file);
        }
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&file)
            .ok()?;
        Some(Claim(file))
    }

    /// The chats that listened in project `root` (its main working tree), in any of its
    /// worktrees, newest first; a chat whose watcher stopped saying so is still there until the
    /// watcher removes it, and `listening` tells them apart.
    pub fn chats(&self, root: &Path) -> Vec<Chat> {
        let Ok(dir) = std::fs::read_dir(self.dir.join("chats")) else {
            return Vec::new();
        };
        let mut chats: Vec<Chat> = dir
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
            .filter_map(|e| serde_json::from_str(&std::fs::read_to_string(e.path()).ok()?).ok())
            .filter(|c: &Chat| {
                c.project == root || (c.project.as_os_str().is_empty() && c.repo == root)
            })
            .collect();
        chats.sort_by_key(|c| std::cmp::Reverse(c.beat));
        chats
    }

    pub fn answer(&self, root: &Path, answer: &Answer) -> std::io::Result<()> {
        append_line(&self.answers_file(root), answer)
    }

    pub fn answers(&self, root: &Path) -> Vec<Answer> {
        std::fs::read_to_string(self.answers_file(root))
            .unwrap_or_default()
            .lines()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect()
    }

    /// When the answers last changed, to read them only then.
    pub fn answers_stamp(&self, root: &Path) -> Option<std::time::SystemTime> {
        std::fs::metadata(self.answers_file(root))
            .and_then(|m| m.modified())
            .ok()
    }

    /// The comments as the app saved them, with the agents' answers since applied.
    pub fn comments(&self, root: &Path) -> Vec<Comment> {
        let mut comments: Vec<Comment> = std::fs::read_to_string(self.comments_file(root))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        apply(&mut comments, &self.answers(root));
        comments
    }
}

/// Held by `Bus::claim` until dropped.
pub struct Claim(PathBuf);

impl Drop for Claim {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// `value` as one more line of `file`.
fn append_line(file: &Path, value: &impl Serialize) -> std::io::Result<()> {
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut line = serde_json::to_string(value).unwrap_or_default();
    line.push('\n');
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(file)?
        .write_all(line.as_bytes())
}

/// `value` as `file`, written beside and renamed, so a reader never sees half a file.
fn write_whole(file: &Path, value: &impl Serialize) -> std::io::Result<()> {
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = file.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string(value).unwrap_or_default())?;
    std::fs::rename(tmp, file)
}

fn safe(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Applies `answers` to `comments`; an answer already applied, or about an earlier sending of
/// its comment, changes nothing. Returns whether anything changed.
pub fn apply(comments: &mut [Comment], answers: &[Answer]) -> bool {
    let mut changed = false;
    for answer in answers {
        let Some(c) = comments.iter_mut().find(|c| c.id == answer.id) else {
            continue;
        };
        let Some(sent) = c.sent.as_mut().filter(|s| s.batch == answer.batch) else {
            continue;
        };
        match answer.kind {
            Kind::Delivered => {
                if !sent.delivered {
                    sent.delivered = true;
                    changed = true;
                }
            }
            Kind::Reply | Kind::Addressed => {
                if c.replies.iter().any(|r| r.uid == answer.uid) {
                    continue;
                }
                if !answer.body.trim().is_empty() {
                    // A new question: the last answer was to the one before, and stays with it.
                    if answer.kind == Kind::Reply
                        && let Some(asked) = c.replies.last_mut().filter(|r| r.form.is_some())
                    {
                        asked.answered = c.answer.take();
                    }
                    c.replies.push(Reply {
                        uid: answer.uid.clone(),
                        body: answer.body.trim().to_owned(),
                        form: answer.form.clone(),
                        at: answer.at,
                        answered: None,
                    });
                    if answer.kind == Kind::Reply {
                        c.answer = None;
                    }
                }
                if answer.kind == Kind::Addressed && c.state == State::Sent {
                    c.state = State::Addressed;
                }
                changed = true;
            }
        }
    }
    changed
}

/// Where a note on `branch` goes (#77): the chat picked for that branch (`chosen`) while it
/// listens, else the chat whose folder is on that branch, else the project's default — the newest
/// listening chat in the main working tree `main`, else the newest listening one. `chats` newest
/// first.
pub fn route<'a>(
    branch: Option<&str>,
    chosen: Option<&str>,
    chats: &'a [Chat],
    main: &Path,
) -> Option<&'a Chat> {
    let listening = || chats.iter().filter(|c| c.listening());
    chosen
        .and_then(|s| listening().find(|c| c.session == s))
        .or_else(|| branch.and_then(|b| listening().find(|c| c.branch == b)))
        .or_else(|| listening().find(|c| c.repo == main))
        .or_else(|| listening().next())
}

/// The agents' own kinds, in their order: Claude Code, then Codex. `install` offers them; a mode without a window
/// (the MCP server, a chat's watcher) asks them here.
pub fn providers() -> Vec<(u16, Arc<dyn Provider>)> {
    vec![
        (10, Arc::new(crate::claude::ClaudeCode)),
        (20, Arc::new(crate::codex::Codex)),
    ]
}

/// Who is running this: the agent and its session, from what it puts in a command's environment (the chat's watcher
/// and the MCP server, #77) — the first of the agents' own kinds whose session it is.
pub fn whoami() -> Option<(Arc<dyn Provider>, Me)> {
    providers()
        .into_iter()
        .find_map(|(_, p)| p.me().map(|me| (p, me)))
}

/// The comments waiting for `session`: sent to it and not handed over yet.
pub fn waiting<'a>(comments: &'a [Comment], session: &str) -> Vec<&'a Comment> {
    comments
        .iter()
        .filter(|c| c.state == State::Sent)
        .filter(|c| {
            c.sent
                .as_ref()
                .is_some_and(|s| s.session == session && !s.delivered)
        })
        .collect()
}

/// The command an agent runs for Artoo's MCP server: this program with `mcp`.
pub fn server_command() -> String {
    let exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "artoo".into());
    format!("\"{exe}\" mcp")
}

/// The name Artoo had before, GitLance: what an older connection is registered under.
pub const OLD_NAME: &str = "gitlance";

/// Whether an agent is still connected under GitLance's name only: a Claude Code profile with its
/// skill and not Artoo's, or Codex with its server and not Artoo's. Connecting again moves it over.
pub fn connected_as_gitlance() -> bool {
    providers().iter().any(|(_, p)| p.connected_as_gitlance())
}

/// The comments of `root` the agent should work on, as markdown with their ids: what it was sent,
/// placed on the lines of the working tree as it is now.
pub fn brief(root: &Path, comments: &[&Comment]) -> String {
    if comments.is_empty() {
        return "No review comments are waiting. The reviewer sends them from Artoo.".to_owned();
    }
    let mut out = format!(
        "{} review comment(s) from Artoo. Change the code for each, then call address_comment with its id and one line on what you did (or reply_comment to ask the reviewer). Line numbers are of the working tree now.\n",
        comments.len()
    );
    for c in comments {
        // A note on a plugin's place (#212): its words, the context it gave, the tool that reads more of it.
        if let Some(on) = &c.on {
            out.push_str(&format!(
                "\n## Comment {} — {}\n\n{}\n\n{}\n",
                c.id,
                on.reads,
                artoo_agents_api::comment::quote(&on.digest),
                c.body.trim()
            ));
            if let Some(tool) = &on.tool {
                out.push_str(&format!(
                    "\nThis is not a line of code: it is on `{}`. Call the tool `{tool}` for more of it.\n",
                    on.kind
                ));
            }
            for r in &c.replies {
                out.push_str(&format!("\n> Your earlier answer: {}\n", r.body));
            }
            if let Some(a) = &c.answer {
                let form = c.replies.last().and_then(|r| r.form.as_ref());
                out.push_str(&format!("\nThe reviewer answered:\n{}\n", a.words(form)));
            }
            continue;
        }
        // A note on a whole folder: no line, no code (#97).
        if c.is_folder() {
            let gone = !root.join(&c.path).is_dir();
            out.push_str(&format!(
                "\n## Comment {} — {} ({})\n\n{}\n",
                c.id,
                c.path,
                if gone {
                    "a folder that is gone since"
                } else {
                    "the folder and everything in it"
                },
                c.body.trim()
            ));
            for r in &c.replies {
                out.push_str(&format!("\n> Your earlier answer: {}\n", r.body));
            }
            if let Some(a) = &c.answer {
                let form = c.replies.last().and_then(|r| r.form.as_ref());
                out.push_str(&format!("\nThe reviewer answered:\n{}\n", a.words(form)));
            }
            continue;
        }
        let at = if c.old {
            format!("{} (a removed line)", c.path)
        } else {
            let text = std::fs::read_to_string(root.join(&c.path)).unwrap_or_default();
            let lines: Vec<&str> = text.lines().collect();
            match review::place(c, lines.len() as u32, |n| lines[n as usize - 1]) {
                review::Place::Line(n) => c.at_line(n),
                review::Place::Outdated => format!("{} (the line has changed since)", c.path),
            }
        };
        out.push_str(&format!(
            "\n## Comment {} — {at}\n\n```\n{}\n```\n\n{}\n",
            c.id,
            c.quoted(),
            c.body.trim()
        ));
        if let Some(thread) = &c.thread {
            out.push_str(&format!(
                "\nThis note is on a discussion of the merge request (the reviewer's own, not posted):\n{thread}\n"
            ));
        }
        if let Some(note) = &c.answers {
            out.push_str(&format!(
                "\nThis answers your own note on this line:\n> {}\n",
                note.trim().replace('\n', "\n> ")
            ));
        }
        for r in &c.replies {
            out.push_str(&format!("\n> Your earlier answer: {}\n", r.body));
        }
        if let Some(a) = &c.answer {
            let form = c.replies.last().and_then(|r| r.form.as_ref());
            out.push_str(&format!("\nThe reviewer answered:\n{}\n", a.words(form)));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use artoo_agents_api::comment::send;

    fn chat(session: &str, repo: &str, branch: &str, listening: bool) -> Chat {
        Chat {
            session: session.into(),
            repo: repo.into(),
            project: "/p".into(),
            branch: branch.into(),
            beat: if listening { now() } else { now() - 3600 },
            ..Default::default()
        }
    }

    #[test]
    fn a_note_on_a_plugins_place_reaches_the_agent_with_its_context_and_tool() {
        let mut note = artoo_agents_api::places::note(
            artoo_agents_api::comment::On {
                kind: "jira.issue".into(),
                data: r#"{"key":"PAY-412","para":4}"#.into(),
                reads: "PAY-412 · description · ¶4".into(),
                digest: "Done when the refund equals what was captured.\nCurrencies with 0 and 3 minor units have tests.".into(),
                tool: Some("jira_issue".into()),
            },
            "Work on this, but leave the report export out.".into(),
        );
        note.id = 7;
        let text = brief(Path::new("/no/such/root"), &[&note]);
        assert!(text.contains("## Comment 7 — PAY-412 · description · ¶4"));
        assert!(text.contains("> Done when the refund equals what was captured.\n> Currencies"));
        assert!(text.contains("Work on this, but leave the report export out."));
        assert!(text.contains("Call the tool `jira_issue` for more of it."));
        assert!(!text.contains("```"));
    }

    #[test]
    fn a_folders_note_reaches_the_agent_as_the_folder_and_all_in_it() {
        let note = review::Comment {
            id: 4,
            path: "src/".into(),
            body: "Use Money everywhere here.".into(),
            ..Default::default()
        };
        let here = std::env::temp_dir();
        std::fs::create_dir_all(here.join("src")).ok();
        let text = brief(&here, &[&note]);
        assert!(text.contains("## Comment 4 — src/ (the folder and everything in it)"));
        assert!(text.contains("Use Money everywhere here."));
        assert!(!text.contains("```"));
        let gone = brief(Path::new("/no/such/root"), &[&note]);
        assert!(gone.contains("src/ (a folder that is gone since)"));
    }

    #[test]
    fn a_note_goes_to_the_chat_on_its_branch_else_the_default() {
        let main = Path::new("/p");
        // Newest first: a chat in a worktree on feature/x, one in the main tree on main.
        let chats = [
            chat("w", "/p-x", "feature/x", true),
            chat("m", "/p", "main", true),
            chat("gone", "/p-y", "feature/y", false),
        ];
        let to = |branch, chosen| route(branch, chosen, &chats, main).map(|c| c.session.as_str());
        assert_eq!(to(Some("feature/x"), None), Some("w"));
        assert_eq!(to(Some("main"), None), Some("m"));
        // No chat on its branch (its chat is gone): the main tree's.
        assert_eq!(to(Some("feature/y"), None), Some("m"));
        assert_eq!(to(None, None), Some("m"));
        // Picked with Change…, while it listens.
        assert_eq!(to(Some("feature/x"), Some("m")), Some("m"));
        assert_eq!(to(Some("feature/x"), Some("gone")), Some("w"));
        // None listens: nowhere.
        assert_eq!(route(Some("main"), None, &chats[2..], main), None);
    }

    fn comment(id: u64) -> Comment {
        Comment {
            id,
            path: "src/a.rs".into(),
            line: 2,
            code: "let x = 1;".into(),
            body: "Clamp this.".into(),
            ..Default::default()
        }
    }

    #[test]
    fn a_question_with_a_form_is_kept_and_the_answer_goes_with_the_next_send() {
        let form: review::Form = serde_json::from_value(serde_json::json!({
            "kind": "one",
            "options": [{ "label": "Only after a failure" }, { "label": "In both cases" }]
        }))
        .unwrap();
        let mut comments = vec![comment(1)];
        let batch = send(&mut comments, &[1], "s1", "Fix login");
        let ask = Answer {
            uid: "u1".into(),
            id: 1,
            batch,
            kind: Kind::Reply,
            body: "When should lastForm be cleared?".into(),
            form: Some(form),
            at: 2,
        };
        assert!(apply(&mut comments, &[ask]));
        assert_eq!(
            comments[0].replies[0].form.as_ref().unwrap().options.len(),
            2
        );
        comments[0].answer = Some(review::Answered {
            picks: vec![1],
            note: "and on sign-out".into(),
            ..Default::default()
        });
        comments[0].thread = Some("> Ember: is this cleared?".into());
        comments[0].answers = Some("Kept as it was.\nThe test covers it.".into());
        let text = brief(Path::new("/nope"), &[&comments[0]]);
        assert!(text.contains("The reviewer answered:\nPicked: In both cases\nand on sign-out"));
        assert!(text.contains("> Ember: is this cleared?"));
        assert!(text.contains(
            "This answers your own note on this line:\n> Kept as it was.\n> The test covers it."
        ));
        let confirm = review::Form {
            kind: review::FormKind::Confirm,
            ..Default::default()
        };
        assert_eq!(confirm.choices().len(), 2);
    }

    #[test]
    fn a_sent_comment_waits_for_its_chat_until_delivered() {
        let mut comments = vec![comment(1), comment(2)];
        let batch = send(&mut comments, &[1], "s1", "Fix login");
        assert_eq!(waiting(&comments, "s1").len(), 1);
        assert!(waiting(&comments, "s2").is_empty());
        let delivered = Answer {
            uid: uid(),
            id: 1,
            batch,
            kind: Kind::Delivered,
            body: String::new(),
            form: None,
            at: 0,
        };
        assert!(apply(&mut comments, std::slice::from_ref(&delivered)));
        assert!(waiting(&comments, "s1").is_empty());
        assert!(!apply(&mut comments, &[delivered]));
    }

    #[test]
    fn a_question_asked_again_keeps_the_answer_it_had() {
        use artoo_agents_api::comment::{Answered, Form, FormKind, FormOption};
        let mut comments = vec![comment(1)];
        let form = Form {
            kind: FormKind::One,
            options: ["a", "b"]
                .map(|l| FormOption {
                    label: l.into(),
                    ..Default::default()
                })
                .to_vec(),
        };
        let ask = |batch: String, uid: &str| Answer {
            uid: uid.into(),
            id: 1,
            batch,
            kind: Kind::Reply,
            body: "Which?".into(),
            form: Some(form.clone()),
            at: 5,
        };
        let first = send(&mut comments, &[1], "s1", "");
        apply(&mut comments, &[ask(first, "u1")]);
        let picked = Answered {
            picks: vec![1],
            ..Default::default()
        };
        comments[0].answer = Some(picked.clone());
        let second = send(&mut comments, &[1], "s1", "");
        apply(&mut comments, &[ask(second, "u2")]);
        let c = &comments[0];
        assert_eq!(c.replies.len(), 2);
        assert_eq!(c.replies[0].answered, Some(picked));
        assert_eq!(c.replies[1].answered, None);
        assert_eq!(c.answer, None);
    }

    #[test]
    fn an_answer_counts_once_and_only_for_its_sending() {
        let mut comments = vec![comment(1)];
        let first = send(&mut comments, &[1], "s1", "");
        let done = Answer {
            uid: "u1".into(),
            id: 1,
            batch: first,
            kind: Kind::Addressed,
            body: "Clamped to 0..100.".into(),
            form: None,
            at: 5,
        };
        apply(&mut comments, std::slice::from_ref(&done));
        apply(&mut comments, std::slice::from_ref(&done));
        assert_eq!(comments[0].state, State::Addressed);
        assert_eq!(comments[0].replies.len(), 1);
        // Reopened and sent again: the old answer is about the first sending.
        comments[0].state = State::Draft;
        send(&mut comments, &[1], "s1", "");
        let mut again = done.clone();
        again.uid = "u2".into();
        apply(&mut comments, &[again]);
        assert_eq!(comments[0].state, State::Sent);
    }

    #[test]
    fn chats_listen_while_fresh() {
        let dir = tempfile::tempdir().unwrap();
        let bus = Bus::at(dir.path().join("agents"));
        let root = PathBuf::from("/r");
        let chat = Chat {
            session: "s1".into(),
            agent: "Claude Code".into(),
            repo: root.clone(),
            // Its own watcher's process.
            watcher: 4242,
            beat: now(),
            ..Default::default()
        };
        bus.listen(&chat).unwrap();
        let chats = bus.chats(&root);
        assert_eq!(chats.len(), 1);
        assert!(chats[0].listening());
        // Half a minute quiet: a watcher has stopped, a session the app writes (Codex's, no watcher) has not.
        let quiet = Chat {
            beat: now() - 30,
            ..chat.clone()
        };
        assert!(!quiet.listening() && !quiet.limited());
        let codex = Chat {
            agent: "Codex".into(),
            watcher: 0,
            ..quiet
        };
        assert!(codex.listening() && codex.limited());
        assert!(bus.chats(Path::new("/other")).is_empty());
        bus.stop("s1");
        assert!(bus.chats(&root).is_empty());
    }

    #[test]
    fn a_window_is_there_while_its_process_lives_and_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let bus = Bus::at(dir.path().join("agents"));
        let here = Presence {
            pid: std::process::id(),
            projects: vec!["/p".into()],
            beat: now(),
        };
        bus.present(1, &here).unwrap();
        // A process that has ended: this test binary run once more, only to list the tests.
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .arg("--list")
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let gone = child.id();
        child.wait().unwrap();
        let ended = Presence {
            pid: gone,
            ..here.clone()
        };
        bus.present(2, &ended).unwrap();
        // Alive but quiet for long: not there, kept.
        let quiet = Presence {
            beat: now() - 120,
            ..here.clone()
        };
        bus.present(3, &quiet).unwrap();
        assert_eq!(bus.running(), vec![here]);
        let files = std::fs::read_dir(dir.path().join("agents/apps")).unwrap();
        assert_eq!(files.count(), 2, "the ended process's file is cleared");
        bus.absent(std::process::id(), 1);
        assert!(bus.running().is_empty());
    }

    #[test]
    fn an_event_is_taken_once() {
        let dir = tempfile::tempdir().unwrap();
        let bus = Bus::at(dir.path().join("agents"));
        let event = event("/p".into(), What::Open);
        bus.event(&event).unwrap();
        assert_eq!(bus.events(), vec![event.clone()]);
        assert!(bus.take_event(&event.uid));
        assert!(!bus.take_event(&event.uid));
        assert!(bus.events().is_empty());
    }

    #[test]
    fn a_place_pointed_at_reaches_the_app_and_is_said_short() {
        let dir = tempfile::tempdir().unwrap();
        let bus = Bus::at(dir.path().join("agents"));
        let places = [
            Place::File {
                path: "src/ui/nav.rs".into(),
                line: Some(785),
            },
            Place::Commit {
                id: "a1b2c3d4e5f6".into(),
            },
            Place::Request {
                iid: 12,
                note: None,
                draft: Some(9),
            },
        ];
        for place in &places {
            let what = What::Point {
                place: place.clone(),
            };
            bus.event(&event("/p".into(), what)).unwrap();
        }
        let read: Vec<String> = bus
            .events()
            .into_iter()
            .filter_map(|e| match e.what {
                What::Point { place } => Some(place.said(false)),
                _ => None,
            })
            .collect();
        assert_eq!(read.len(), 3);
        for said in ["nav.rs:785", "commit a1b2c3d", "!12, a pending comment"] {
            assert!(read.iter().any(|r| r == said), "{read:?}");
        }
    }

    #[test]
    fn answers_reach_the_comments_the_app_saved() {
        let dir = tempfile::tempdir().unwrap();
        let bus = Bus::at(dir.path().join("agents"));
        let root = PathBuf::from("/r");
        let mut comments = vec![comment(1)];
        let batch = send(&mut comments, &[1], "s1", "");
        let file = bus.comments_file(&root);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, serde_json::to_string(&comments).unwrap()).unwrap();
        let answer = Answer {
            uid: uid(),
            id: 1,
            batch,
            kind: Kind::Addressed,
            body: "Done.".into(),
            form: None,
            at: 1,
        };
        bus.answer(&root, &answer).unwrap();
        assert_eq!(bus.comments(&root)[0].state, State::Addressed);
    }
}
