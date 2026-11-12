//! Recent places (⌘E): the files, commits and merge requests opened lately, newest first, in the
//! palette. Kept for the session; ⌘[ and ⌘] stay for stepping back and forward. Designed in
//! `../Design/mockups/ide-ideas/a-ideas.html` (9). Only what was chosen is kept — a file clicked or picked, a
//! line gone to, a commit opened —, never a row the selection passed over (#192), and a file keeps its owner: the
//! request it was read in, which it opens again.

use super::{Workspace, format};
use artoo_core::links::{self, DiffLine, Target};
use git2::Oid;
use gpui::{App, Context};

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Key {
    /// A file, with the request it was read in (none: a branch's, Code's).
    File {
        path: String,
        request: Option<u64>,
    },
    Commit(Oid),
    Request(u64),
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub key: Key,
    pub label: String,
    pub detail: String,
    /// The line gone to in it, a file's: where it was left (#192).
    pub line: Option<u32>,
    /// A file's request or folder, its detail before the line.
    owner: String,
}

const KEEP: usize = 30;

impl Workspace {
    /// Puts a place first in the list (the same place twice is one entry).
    pub(super) fn remember_place(&mut self, key: Key, label: String, detail: String) {
        self.remember_at(key, label, detail, String::new(), None);
    }

    fn remember_at(
        &mut self,
        key: Key,
        label: String,
        detail: String,
        owner: String,
        line: Option<u32>,
    ) {
        self.recents.retain(|e| e.key != key);
        self.recents.insert(
            0,
            Entry {
                key,
                label,
                detail,
                line,
                owner,
            },
        );
        self.recents.truncate(KEEP);
    }

    /// The file on screen is being left (another page, another file): its place in Recent keeps the line it is at
    /// (#192), Code's the top line shown, a diff's the change the keyboard is at.
    pub(super) fn keep_line(&mut self, cx: &App) {
        let Some((path, line)) = self.line_here(cx) else {
            return;
        };
        let entry = (self.recents.iter_mut())
            .find(|e| matches!(&e.key, Key::File { path: p, .. } if *p == path));
        if let Some(entry) = entry {
            entry.line = Some(line);
            entry.detail = file_detail(&entry.owner, Some(line));
        }
    }

    /// The file the page reads and the line it is at, as Open in opens it (`open_target`).
    fn line_here(&self, cx: &App) -> Option<(String, u32)> {
        use super::nav::{Page, Section};
        match (self.nav.space, self.shown_page()) {
            (super::spaces::CODE, Some(Page::File(path))) => {
                let line = self.file_top_line(&path, cx);
                Some((path, line))
            }
            (super::spaces::CODE, _) => None,
            (_, Some(_)) if matches!(self.page_tab(cx), None | Some(Section::Changes)) => {
                Some((self.current_path(cx)?, self.rv().change_line(cx)?))
            }
            _ => None,
        }
    }

    /// File `path` chosen (at `line`): kept with the request on screen, said `AuthApi.kt  !398 · line 23`.
    pub(super) fn remember_file(&mut self, path: &str, line: Option<u32>, cx: &App) {
        let request = self
            .showing_request(cx)
            .then(|| self.current_request(cx))
            .flatten();
        let name = path.rsplit('/').next().unwrap_or(path).to_owned();
        let owner = request.map_or_else(
            || path.rsplit_once('/').map_or("", |(dir, _)| dir).to_owned(),
            super::words::number,
        );
        let key = Key::File {
            path: path.to_owned(),
            request,
        };
        // Chosen again without a line, it keeps the one it was left at.
        let line = line.or_else(|| {
            (self.recents.iter())
                .find(|e| e.key == key)
                .and_then(|e| e.line)
        });
        let detail = file_detail(&owner, line);
        self.remember_at(key, name, detail, owner, line);
    }

    /// Goes back to a place — a file to its request and line —; says so when it is not in what is open now.
    pub(super) fn open_recent(&mut self, key: Key, line: Option<u32>, cx: &mut Context<Self>) {
        match key {
            Key::File {
                path,
                request: Some(iid),
            } if !(self.showing_request(cx) && self.current_request(cx) == Some(iid)) => {
                let line = Some(DiffLine {
                    file: links::path_hash(&path),
                    old: None,
                    new: Some(line.unwrap_or(1)),
                });
                self.go_to(
                    Target::Request {
                        iid,
                        note: None,
                        line,
                    },
                    cx,
                );
            }
            Key::File { path, .. } => {
                let ix = self
                    .shown_diff(cx)
                    .as_ref()
                    .and_then(|d| d.files.iter().position(|f| f.path() == path));
                match (ix, line) {
                    (Some(_), Some(line)) => self.jump_to_line(&path, line, false, cx),
                    (Some(ix), None) => {
                        self.select_file(ix, cx);
                        self.follow_loaded(cx);
                    }
                    // Not in the diff on screen: read in Code (#71).
                    (None, _) => self.open_file_view(path, cx),
                }
            }
            Key::Commit(id) => match self.commit_at(id, cx) {
                Some(ix) => self.open_commit(ix, cx),
                None => {
                    self.error = Some(
                        format!("{} is not in the open list of commits.", format::short(id)).into(),
                    );
                    cx.notify();
                }
            },
            Key::Request(iid) => self.open_request(iid, cx),
        }
    }
}

/// A file's detail in Recent: `!398 · line 23`, `core/network · line 23`, or its owner alone.
fn file_detail(owner: &str, line: Option<u32>) -> String {
    match line {
        Some(line) if owner.is_empty() => format!("line {line}"),
        Some(line) => format!("{owner} · line {line}"),
        None => owner.to_owned(),
    }
}
