//! A tab's chats (#77, #94, #117): this repository's, read again on every beat of the poll — the stripe shows who
//! listens, a toast who comes and goes —, the one a branch's notes go to (`route_of`, with the one picked with
//! Change…), a chat's mark, and what a chat working in a worktree of its own has changed there. The reviewer's
//! comments they carry are the tab's, handed in.

use crate::agents::{self, Bus, Chat};
use crate::review::{Comment, State};
use artoo_core::{git, labels};
use artoo_ui_kit::theme;
use gpui::{Context, Rgba, Task};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// The chats of a tab's repository.
#[derive(Default)]
pub struct Chats {
    /// As last read; one long gone with nothing of the review on it is left out (#77).
    pub list: Vec<Chat>,
    /// Read once: from then on their coming and going is said.
    known: bool,
    /// The chat picked with Change… for a branch's notes ("" for any branch), by session (#77).
    pub routes: HashMap<String, String>,
    /// What each chat working in a worktree of its own has changed there, by session (#117), and the polls until
    /// it is read again.
    pub edits: HashMap<String, HashSet<String>>,
    edits_in: u32,
    edits_task: Option<Task<()>>,
}

impl Chats {
    /// Chat `session` is here and listening: a note sent to it and not taken is not waiting.
    pub fn listens(&self, session: &str) -> bool {
        self.list
            .iter()
            .any(|c| c.session == session && c.listening())
    }

    /// The sessions listening now.
    pub fn listening(&self) -> HashSet<String> {
        self.list
            .iter()
            .filter(|c| c.listening())
            .map(|c| c.session.clone())
            .collect()
    }

    /// A chat's mark (#77): its agent's first letter on a hue of its own — the code palette's, never the agent's
    /// purple or the public orange — by its place among the chats here.
    pub fn mark(&self, session: &str) -> (char, Rgba) {
        let hues = [
            theme::accent(),
            theme::added(),
            theme::renamed(),
            theme::removed(),
        ];
        let mut sessions: Vec<&str> = self.list.iter().map(|c| c.session.as_str()).collect();
        sessions.sort_unstable();
        let at = sessions.iter().position(|s| *s == session).unwrap_or(0);
        let letter = self
            .list
            .iter()
            .find(|c| c.session == session)
            .and_then(|c| c.agent.chars().next())
            .unwrap_or('?');
        (letter, hues[at % hues.len()])
    }

    /// The chat a note on `branch` goes to (`agents::route`), with the one picked for it; `main` the project.
    pub fn route_of(&self, branch: Option<&str>, main: &Path) -> Option<&Chat> {
        let chosen = branch
            .and_then(|b| self.routes.get(b))
            .or(self.routes.get(""))
            .map(String::as_str);
        agents::route(branch, chosen, &self.list, main)
    }

    /// Reads `root`'s chats again; whether they changed. One long gone with none of `comments` on it is not
    /// listed (#77).
    pub fn refresh(&mut self, root: Option<&Path>, comments: &[Comment]) -> bool {
        let mut chats = match (root, Bus::new()) {
            (Some(root), Some(bus)) => named(bus.chats(root)),
            _ => Vec::new(),
        };
        chats.retain(|c| {
            c.listening()
                || agents::now() - c.beat < 24 * 3600
                || comments
                    .iter()
                    .any(|n| n.sent.as_ref().is_some_and(|s| s.session == c.session))
        });
        let changed = chats != self.list;
        self.list = chats;
        changed
    }

    /// The poll's beat: `root`'s chats read again, and what a chat in a worktree of its own changed when it is time
    /// (`here` the tab's working tree). Whether they changed, and what to say of each that began listening or went
    /// since the last beat — none the first time.
    pub fn beat(
        &mut self,
        root: Option<&Path>,
        here: Option<PathBuf>,
        comments: &[Comment],
        cx: &mut Context<Self>,
    ) -> (bool, Vec<String>) {
        let before = self.list.clone();
        let changed = self.refresh(root, comments);
        let said = if changed && self.known {
            self.said(&before, comments)
        } else {
            Vec::new()
        };
        self.known = true;
        if let Some(here) = here {
            self.read_edits(here, cx);
        }
        (changed, said)
    }

    /// A line for each chat that began listening or went since `before`; one taking notes to work on says nothing
    /// (#77).
    fn said(&self, before: &[Chat], comments: &[Comment]) -> Vec<String> {
        let name = |c: &Chat| {
            if c.title.is_empty() {
                c.agent.clone()
            } else {
                format!("{} · {}", c.agent, c.title)
            }
        };
        let working = |session: &str| {
            comments.iter().any(|c| {
                c.state == State::Sent
                    && c.sent
                        .as_ref()
                        .is_some_and(|s| s.session == session && s.delivered)
            })
        };
        let mut said = Vec::new();
        for c in &self.list {
            let was = before.iter().find(|b| b.session == c.session);
            match (was.map(Chat::listening), c.listening()) {
                (None, true) => said.push(format!("{} is listening", name(c))),
                (Some(true), false) if !working(&c.session) => {
                    said.push(format!("{} stopped listening", name(c)))
                }
                _ => {}
            }
        }
        for b in before {
            if b.listening() && !self.list.iter().any(|c| c.session == b.session) {
                said.push(format!("{} stopped listening", name(b)));
            }
        }
        said
    }

    /// Every fifth beat (10 s): what each chat in a worktree of its own has changed there, read in the background
    /// (#117). A chat in this working tree, `here`, is left out: its edits are not its own to tell.
    fn read_edits(&mut self, here: PathBuf, cx: &mut Context<Self>) {
        if self.edits_in > 0 {
            self.edits_in -= 1;
            return;
        }
        self.edits_in = 4;
        let same = |a: &Path, b: &Path| match (a.canonicalize(), b.canonicalize()) {
            (Ok(a), Ok(b)) => a == b,
            _ => a.components().eq(b.components()),
        };
        let away: Vec<(String, PathBuf)> = self
            .list
            .iter()
            .filter(|c| !c.repo.as_os_str().is_empty() && !same(&c.repo, &here))
            .map(|c| (c.session.clone(), c.repo.clone()))
            .collect();
        if away.is_empty() {
            if !self.edits.is_empty() {
                self.edits.clear();
                cx.notify();
            }
            return;
        }
        self.edits_task = Some(cx.spawn(async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move {
                    away.into_iter()
                        .filter_map(|(session, repo)| {
                            let paths = git::Repo::open(&repo).ok()?.dirty_paths().ok()?;
                            Some((session, paths.into_iter().collect()))
                        })
                        .collect::<HashMap<_, _>>()
                })
                .await;
            this.update(cx, |this, cx| {
                if this.edits != read {
                    this.edits = read;
                    cx.notify();
                }
            })
            .ok();
        }));
    }
}

/// Chats by the names they were given (`labels`), the rest by their first message.
pub fn named(mut chats: Vec<Chat>) -> Vec<Chat> {
    for c in &mut chats {
        if let Some(name) = labels::chat(&c.session) {
            c.title = name;
        }
    }
    chats
}
