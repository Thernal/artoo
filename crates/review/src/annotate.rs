//! Who wrote each line (⌥⌘B, and a long hover on a line number, D19): the revision a side is read at, the blame read
//! so far, the tip under the pointer.

use crate::diff::Header;
use crate::diff_view::AnnCell;
use crate::review::{Review, ReviewEvent};
use artoo_core::git::{BlameLine, Repo};
use artoo_ui_kit::format;
use artoo_ui_kit::hover::{NoTip, TipOwned};
use gpui::{AnyView, Context, SharedString, WeakEntity, prelude::*};
use std::rc::Rc;

/// The revision a side of the diff is read at, for its blame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BlameAt {
    Rev(git2::Oid),
    /// A commit's first parent: the old side of a commit's diff.
    ParentOf(git2::Oid),
    Head,
    /// The files on disk.
    Working,
}

/// A side of a file: where it is read, and its path there.
pub type Side = (BlameAt, String);

/// Blame read for the line numbers' long hover: by side and path, and the tips waiting for it
/// (each with its line).
#[derive(Default)]
pub struct Blames {
    pub read: std::collections::HashMap<Side, Rc<Vec<Option<BlameLine>>>>,
    pub waiting: std::collections::HashMap<Side, Vec<(WeakEntity<TipOwned>, u32)>>,
}

impl Blames {
    /// Forgets what the working tree's files said: they changed.
    pub fn working_changed(&mut self) {
        self.read.retain(|(at, _), _| *at != BlameAt::Working);
    }
}

/// The line number under the pointer, and the tip saying who wrote it once the pointer rested.
pub struct BlameTip {
    pub key: (usize, bool, u32),
    pub at: gpui::Point<gpui::Pixels>,
    pub view: Option<AnyView>,
    pub _wait: gpui::Task<()>,
}

/// What line `line` (1-based) of a blame says.
pub fn blame_text(lines: &[Option<BlameLine>], line: u32) -> SharedString {
    match line.checked_sub(1).and_then(|i| lines.get(i as usize)) {
        Some(Some(b)) => format!(
            "{}\n{} · {} · {}",
            b.summary,
            format::short(b.id),
            b.author,
            artoo_ui_kit::hover::full_date(b.time)
        )
        .into(),
        Some(None) => "Not committed yet".into(),
        // The read failed, or the file is shorter there.
        None => "Who wrote it is not known here".into(),
    }
}

impl Review {
    /// The annotation column of the open file read again: blame in the background (`annotation_job`,
    /// `annotated`), said when it came (`ReviewEvent::Changed`).
    pub fn refresh_annotations(&mut self, cx: &mut Context<Self>) {
        let Some((root, rev, path)) = self.annotation_job() else {
            return;
        };
        self.annot_task = Some(cx.spawn(async move |this, cx| {
            let lines = cx
                .background_executor()
                .spawn(async move { Repo::open(&root)?.blame(rev, &path) })
                .await;
            this.update(cx, |rv, cx| {
                if let Ok(lines) = lines {
                    rv.annotated(lines, rev);
                    cx.emit(ReviewEvent::Changed);
                }
            })
            .ok();
        }));
    }

    /// The pointer came on line number `line` of file `ix` (`at`), or went off it: after a long rest
    /// its tip says who wrote the line. True when the tip shown went.
    pub fn hover_line_number(
        &mut self,
        ix: usize,
        old: bool,
        line: u32,
        at: Option<gpui::Point<gpui::Pixels>>,
        cx: &mut Context<Self>,
    ) -> bool {
        let key = (ix, old, line);
        let Some(at) = at else {
            let shown = self.blame_tip.as_ref().is_some_and(|t| t.key == key);
            if shown {
                self.blame_tip = None;
            }
            return shown;
        };
        let wait = cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(artoo_ui_kit::hover::LONG)
                .await;
            this.update(cx, |rv, cx| {
                let view = rv.blame_view(ix, old, line, cx);
                if let Some(tip) = rv.blame_tip.as_mut().filter(|t| t.key == key) {
                    tip.view = Some(view);
                    cx.emit(ReviewEvent::Changed);
                }
            })
            .ok();
        });
        self.blame_tip = Some(BlameTip {
            key,
            at,
            view: None,
            _wait: wait,
        });
        false
    }

    /// Who wrote line `line` of file `ix`, read in the background the first time a side of a file
    /// is asked about.
    fn blame_view(&mut self, ix: usize, old: bool, line: u32, cx: &mut Context<Self>) -> AnyView {
        let (Some(root), Some(key)) = (self.root.clone(), self.blame_at(ix, old)) else {
            return cx.new(|_| NoTip).into();
        };
        if let Some(lines) = self.blames.read.get(&key) {
            let text = blame_text(lines, line);
            return cx.new(|_| TipOwned(text)).into();
        }
        let tip = cx.new(|_| TipOwned("Reading who wrote this line…".into()));
        let waiting = self.blames.waiting.entry(key.clone()).or_default();
        waiting.push((tip.downgrade(), line));
        if waiting.len() == 1 {
            cx.spawn(async move |this, cx| {
                let (at, path) = key.clone();
                let lines = cx
                    .background_executor()
                    .spawn(async move {
                        let repo = Repo::open(&root)?;
                        match at {
                            BlameAt::Rev(rev) => repo.blame(rev, &path),
                            BlameAt::ParentOf(id) => repo.blame(repo.parent_of(id)?, &path),
                            BlameAt::Head => repo.blame(repo.resolve("HEAD")?, &path),
                            BlameAt::Working => repo.blame_working(&path),
                        }
                    })
                    .await;
                this.update(cx, |rv, cx| {
                    let lines = Rc::new(lines.unwrap_or_default());
                    for (tip, line) in rv.blames.waiting.remove(&key).unwrap_or_default() {
                        let text = blame_text(&lines, line);
                        tip.update(cx, |tip, cx| {
                            tip.0 = text;
                            cx.notify();
                        })
                        .ok();
                    }
                    rv.blames.read.insert(key, lines);
                })
                .ok();
            })
            .detach();
        }
        tip.into()
    }

    /// Where file `ix`'s side is read for its blame, and its path there.
    fn blame_at(&self, ix: usize, old: bool) -> Option<Side> {
        let diff = self.diff.as_ref()?;
        let file = diff.files.get(ix)?;
        let path = if old { &file.old_path } else { &file.new_path };
        let at = match (&diff.header, old) {
            (Header::WorkingTree { .. }, true) => BlameAt::Head,
            (Header::WorkingTree { .. }, false) => BlameAt::Working,
            (Header::Commit(c), true) => BlameAt::ParentOf(c.id),
            (Header::Compare { start, .. }, true) => BlameAt::Rev(*start),
            (Header::Versions { from, .. }, true) => BlameAt::Rev(from.tip),
            (Header::Interdiff { old, .. }, true) => BlameAt::Rev(old.id),
            (_, false) => BlameAt::Rev(self.viewed_rev()?),
        };
        Some((at, path.clone()?))
    }

    /// The tip of the line number the pointer rests on, under the pointer.
    pub fn render_blame_tip(&self) -> Option<gpui::AnyElement> {
        let tip = self.blame_tip.as_ref()?;
        let view = tip.view.clone()?;
        Some(
            gpui::deferred(
                gpui::anchored()
                    .position(tip.at + gpui::point(gpui::px(0.), gpui::px(10.)))
                    .snap_to_window_with_margin(gpui::px(8.))
                    .child(view),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }

    /// What blame reads for the annotation column: the repository, the revision shown and the open
    /// file; none when the column is off or the file has no new side. The blame under way stops.
    pub fn annotation_job(&mut self) -> Option<(std::path::PathBuf, git2::Oid, String)> {
        self.annot_task = None;
        if !self.annotate {
            return None;
        }
        let (Some(root), Some(rev), Some(path)) =
            (self.root.clone(), self.viewed_rev(), self.current_path())
        else {
            return None;
        };
        // A deleted file has no new side to annotate.
        if self
            .diff
            .as_ref()
            .and_then(|d| d.files.get(self.file))
            .is_none_or(|f| f.new_path.is_none())
        {
            return None;
        }
        Some((root, rev, path))
    }

    /// Blame's `lines` of the open file at `rev`: the annotation column's cells.
    pub fn annotated(&mut self, lines: Vec<Option<BlameLine>>, rev: git2::Oid) {
        let mut previous = None;
        self.annotations = lines
            .into_iter()
            .map(|line| {
                let line = line?;
                let first = previous != Some(line.id);
                previous = Some(line.id);
                Some(AnnCell {
                    id: line.id,
                    label: first.then(|| {
                        // The age first: the name may be long, the age is what scans.
                        let ago = format::ago(line.time);
                        SharedString::from(format!(
                            "{} · {}",
                            ago.trim_end_matches(" ago"),
                            line.author
                        ))
                    }),
                    tip: SharedString::from(format!(
                        "{} · {} · {} · {}\nclick to open the commit",
                        line.summary,
                        format::short(line.id),
                        line.author,
                        format::ago(line.time)
                    )),
                    current: line.id == rev,
                })
            })
            .map(|c| {
                c.unwrap_or(AnnCell {
                    id: rev,
                    label: None,
                    tip: SharedString::default(),
                    current: false,
                })
            })
            .collect();
    }
}
