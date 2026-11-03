//! What other features put on a file's lines, as data the review lays out (#165 stage 4): discussions with
//! their replies, pending comments, the composer, bookmarks, and the agents' comments and notes, placed against
//! the file's text when it is laid out. The tab gathers them (`Workspace::pins_of`), and lays the rows out again
//! when one changes; the review asks no one while it works. A click on a line's mark folds or opens what is there.

use super::Workspace;
use super::composer::Target;
use super::rows::{Pin, Row};
use crate::comment::Place;
use artoo_review_api::NoteTone;
use artoo_review_api::{Anchored, FilePins, Pins};
use gpui::{App, Context};

/// The tab's side: what is pinned on the files a layout lays out, gathered now.
impl Workspace {
    /// The pins of the open file.
    pub(super) fn open_pins(&self, cx: &App) -> Pins {
        self.pins_for(self.current_path(cx), cx)
    }

    /// The pins of every file of the diff.
    pub(super) fn all_pins(&self, cx: &App) -> Pins {
        let paths: Vec<String> = self
            .shown_diff(cx)
            .as_ref()
            .map(|d| d.files.iter().map(|f| f.path().to_owned()).collect())
            .unwrap_or_default();
        self.pins_for(paths, cx)
    }

    /// The pins of the files at `paths`.
    pub(super) fn pins_for(&self, paths: impl IntoIterator<Item = String>, cx: &App) -> Pins {
        paths
            .into_iter()
            .map(|path| {
                let pins = self.pins_of(&path, cx);
                (path, pins)
            })
            .collect()
    }

    /// A click on a line's comment mark: folds what is open there, or opens what is folded.
    pub(super) fn toggle_notes_at(&mut self, old: bool, line: u32, cx: &mut Context<Self>) {
        // A line with nothing but a bookmark: the click takes the mark off.
        let only_bookmark = self.rv().line_marks(cx).iter().any(|n| {
            n.old == old && n.line == line && n.tone == NoteTone::Bookmark && n.count == 1
        });
        if only_bookmark {
            return self.toggle_bookmark_at(line, cx);
        }
        let mine: Vec<usize> = if self.showing_request(cx) {
            self.request_threads_here(cx)
                .into_iter()
                .filter(|(_, o, n)| *o == old && *n == line)
                .map(|(ix, _, _)| ix)
                .collect()
        } else {
            Vec::new()
        };
        let agent: Vec<u64> = self
            .comments(cx)
            .iter()
            .filter(|c| {
                c.old == old
                    && matches!(self.place_in_open_file(c, cx), Some(Place::Line(n)) if n == line)
            })
            .map(|c| c.id)
            .collect();
        let any_open = mine.iter().any(|&ix| self.mr_open(ix, cx))
            || agent.iter().any(|id| !self.toggled_agent.contains(id));
        for ix in mine {
            // Everything there ends up the other way round from how most of it was.
            if self.mr_open(ix, cx) == any_open && !self.toggled_mr.remove(&ix) {
                self.toggled_mr.insert(ix);
            }
        }
        for id in agent {
            if any_open {
                self.toggled_agent.insert(id);
            } else {
                self.toggled_agent.remove(&id);
            }
        }
        self.refresh_rows(cx);
        cx.notify();
    }

    /// What the agents' comments and notes, the request's discussions and pending comments, the composer and
    /// bookmarks put on file `path`'s lines (`pins.rs`).
    pub(super) fn pins_of(&self, path: &str, cx: &App) -> FilePins {
        let notes = self
            .notes_shown(cx)
            .into_iter()
            .flat_map(|n| n.notes.iter())
            .filter(|n| n.path == path)
            .map(|n| Anchored {
                id: n.id,
                old: false,
                line: n.line,
                code: n.code.clone(),
                followed: self.agents.followed(n.id, cx),
            })
            .collect();
        let comments = self
            .comments(cx)
            .iter()
            .filter(|c| !c.archived && c.path == path)
            .map(|c| Anchored {
                id: c.id,
                old: c.old,
                line: c.line,
                code: c.code.clone(),
                followed: self.agent_lines.get(&c.id).copied(),
            })
            .collect();
        // A note written on Code's file is drawn there, not in a diff (#75).
        let compose = self
            .compose
            .as_ref()
            .filter(|c| c.path == path && c.file.is_none());
        let showing = self.showing_request(cx);
        let theirs = if showing {
            self.requests_api()
                .map(|r| r.threads_in(path, cx))
                .unwrap_or_default()
        } else {
            self.since_thread_in(path, cx)
        };
        let drafts = if showing {
            self.requests_api()
                .map(|r| r.drafts_in(path, cx))
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let reply_of = |ix: usize| self.requests_api().and_then(|r| r.draft_thread(ix, cx));
        let mut fixed = Vec::new();
        // A reply waiting to be sent comes right under its own discussion (#166).
        for &(t, old, line) in &theirs {
            fixed.push(Pin {
                old,
                line,
                row: Row::Request(t),
            });
            fixed.extend(
                drafts
                    .iter()
                    .filter(|&&(ix, o, n)| o == old && n == line && reply_of(ix) == Some(t))
                    .map(|&(ix, old, line)| Pin {
                        old,
                        line,
                        row: Row::Draft(ix),
                    }),
            );
        }
        fixed.extend(
            drafts
                .iter()
                .filter(|(ix, ..)| {
                    reply_of(*ix).is_none_or(|t| !theirs.iter().any(|(x, ..)| *x == t))
                })
                .map(|&(ix, old, line)| Pin {
                    old,
                    line,
                    row: Row::Draft(ix),
                }),
        );
        if let Some(c) = compose.filter(|c| {
            !matches!(
                c.target,
                Target::Reply(_)
                    | Target::Edit(_)
                    | Target::ThreadNote(_)
                    | Target::Answer(_)
                    | Target::AnswerValue(_)
                    | Target::InboxReply
            )
        }) {
            fixed.push(Pin {
                old: c.old,
                line: c.line,
                row: Row::Composer,
            });
        }
        let mut marks: Vec<(bool, u32, NoteTone)> = self
            .bookmarks_in(path, cx)
            .into_iter()
            .map(|line| (false, line, NoteTone::Bookmark))
            .collect();
        if showing {
            marks.extend(
                theirs
                    .iter()
                    .map(|&(_, old, line)| (old, line, NoteTone::Request)),
            );
            marks.extend(
                drafts
                    .iter()
                    .map(|&(_, old, line)| (old, line, NoteTone::Draft)),
            );
        }
        FilePins {
            notes,
            comments,
            fixed,
            marks,
            composing: compose.is_some(),
        }
    }

    /// Lays the rows out again after a comment changed, keeping the scroll position.
    pub(super) fn refresh_rows(&mut self, cx: &mut App) {
        // With all files in one scroll the comment may be in any of them; only rows that changed
        // are replaced, so the view stays where it is.
        if self.rv().stacked(cx) {
            return self.relayout_stack(cx);
        }
        let top = self.diff_list(cx).logical_scroll_top();
        self.relayout(cx);
        self.diff_list(cx).scroll_to(top);
    }
}
