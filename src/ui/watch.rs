//! Auto-refresh: notice that the repository changed under the window (a commit, an amend, a rebase,
//! a checkout made elsewhere) and re-read it, without moving what the user is looking at. It only
//! reads — fetching is a separate, opt-in feature.

use super::px;
use super::{Selection, Workspace, theme};
use crate::git;
use artoo_branches_api::{Applied, Reloaded};
use gpui::{App, ClickEvent, Context, Rgba, Task, div, prelude::*};
use std::path::PathBuf;
use std::time::Duration;

/// How often the git directory is checked.
const POLL: Duration = Duration::from_millis(1500);
/// A change is acted on once it has been still for this long, so a rebase is one refresh.
const SETTLE: Duration = Duration::from_millis(300);
/// How long "Repository changed" stays in the title bar.
const FLASH: Duration = Duration::from_millis(1500);

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Status {
    UpToDate,
    Changed,
    Reloaded,
}

impl Status {
    fn label(self) -> &'static str {
        match self {
            Status::UpToDate => "Up to date",
            Status::Changed => "Repository changed",
            Status::Reloaded => "Reloaded",
        }
    }

    fn color(self) -> Rgba {
        match self {
            Status::UpToDate => theme::added(),
            Status::Changed => theme::accent(),
            Status::Reloaded => theme::warning(),
        }
    }
}

pub struct Watch {
    git_dir: Option<PathBuf>,
    fingerprint: u64,
    pub status: Status,
    flash: Option<Task<()>>,
}

impl Default for Watch {
    fn default() -> Self {
        Self {
            git_dir: None,
            fingerprint: 0,
            status: Status::UpToDate,
            flash: None,
        }
    }
}

impl Watch {
    /// A repository was opened: watch its git directory from this state.
    pub fn watch(&mut self, git_dir: PathBuf, fingerprint: u64) {
        self.git_dir = Some(git_dir);
        self.fingerprint = fingerprint;
        self.status = Status::UpToDate;
    }
}

impl Workspace {
    /// The user moved to another branch, or reads it anew: what was found on the old one no
    /// longer applies (the history's news), nor does a read under way.
    pub(super) fn forget_news(&mut self, cx: &mut App) {
        if let Some(branches) = self.branches() {
            branches.forget_news(cx);
        }
    }

    /// Starts the polling loop for the life of the window.
    pub(super) fn start_watching(
        shown: std::rc::Rc<std::cell::Cell<bool>>,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(POLL).await;
                // A tab behind another looks seldom (#87).
                if !shown.get() {
                    cx.background_executor().timer(POLL * 6).await;
                    if !shown.get() {
                        continue;
                    }
                }
                let Ok(job) = this.update(cx, |this, _| this.watch_job()) else {
                    return;
                };
                let Some((dir, known)) = job else {
                    continue;
                };
                let seen = {
                    let dir = dir.clone();
                    cx.background_executor()
                        .spawn(async move { git::fingerprint(&dir) })
                        .await
                };
                if seen == known {
                    continue;
                }
                cx.background_executor().timer(SETTLE).await;
                let settled = cx
                    .background_executor()
                    .spawn(async move { git::fingerprint(&dir) })
                    .await;
                // Still changing: the next tick looks again.
                if settled == seen
                    && this
                        .update(cx, |this, cx| this.repository_changed(seen, cx))
                        .is_err()
                {
                    return;
                }
            }
        })
        .detach();
    }

    fn watch_job(&self) -> Option<(PathBuf, u64)> {
        if !self.settings.auto_refresh {
            return None;
        }
        Some((self.watch.git_dir.clone()?, self.watch.fingerprint))
    }

    fn set_status(&mut self, status: Status, cx: &mut Context<Self>) {
        self.watch.status = status;
        self.watch.flash = (status != Status::UpToDate).then(|| {
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(FLASH).await;
                this.update(cx, |this, cx| {
                    this.watch.status = Status::UpToDate;
                    cx.notify();
                })
                .ok();
            })
        });
        cx.notify();
    }

    /// The repository changed: it is read again in the background (`Branches::reload`).
    fn repository_changed(&mut self, fingerprint: u64, cx: &mut Context<Self>) {
        self.watch.fingerprint = fingerprint;
        if self.root.is_none() {
            return;
        }
        self.set_status(Status::Changed, cx);
        if let Some(branches) = self.branches() {
            branches.reload(cx);
        }
    }

    /// What the re-read found (`Event::Reloaded`): the status says it; a branch gone falls back to the current one,
    /// a comparison follows its latest version.
    pub(super) fn apply_reload(&mut self, reloaded: Reloaded, cx: &mut Context<Self>) {
        if self.nav.space == super::spaces::Space::Branches && self.at_root_list() {
            self.read_branch_facts(cx);
        }
        match reloaded {
            Reloaded::Listed | Reloaded::Same | Reloaded::Held | Reloaded::Unreadable => {
                self.set_status(Status::UpToDate, cx)
            }
            Reloaded::Gone { refname, fallback } => {
                // The open branch is gone: fall back to the checked-out one, and say so.
                self.error = Some(
                    format!("{refname} no longer exists — showing the current branch.").into(),
                );
                self.clear_branch(cx);
                if let Some(ix) = fallback {
                    self.select_branch(ix, None, cx);
                }
                self.set_status(Status::Reloaded, cx)
            }
            Reloaded::Versions { seen } => {
                self.follow_latest_version(seen, cx);
                self.set_status(Status::Reloaded, cx)
            }
        }
    }

    /// Shows what a refresh found: the new commits at the top, the selection and scroll kept.
    pub(super) fn apply_pending(&mut self, cx: &mut Context<Self>) {
        let Some((applied, seen)) = self.branches().and_then(|b| b.apply_pending(cx)) else {
            return;
        };
        match applied {
            Applied::Kept => {}
            Applied::Newest => self.select_commit(0, cx),
            Applied::Cleared => self.clear_diff(cx),
        }
        self.follow_latest_version(seen, cx);
        self.set_status(Status::Reloaded, cx);
    }

    /// A version comparison that ended at the latest version moves on to a newer one, in place:
    /// same file, same scroll position, and a notice that can undo it. Commits never need this —
    /// a commit's diff cannot change.
    fn follow_latest_version(&mut self, seen: usize, cx: &mut Context<Self>) {
        let Some(branches) = self.branches() else {
            return;
        };
        let Selection::Versions { from, to } = branches.selection(cx) else {
            return;
        };
        let versions = branches.versions(cx);
        let latest = versions.len().saturating_sub(1);
        if seen == 0 || to + 1 != seen || latest <= to {
            return;
        }
        let place = self.rv().place(cx);
        self.rv().restore_at(place.clone(), cx);
        let file = place.map(|(path, _)| path);
        let undo = Some((from, to));
        self.load_versions(from, latest, file, cx);
        let number = versions[latest].0;
        // `load_versions` clears the notice of the diff it replaces; this one belongs to the new.
        let notice = super::Notice {
            text: format!("v{number} arrived — this comparison now ends at v{number}."),
            undo,
        };
        self.rv().set_notice(Some(notice), cx);
    }

    /// Goes back to the comparison a notice replaced, at the same place in the file.
    pub(super) fn undo_notice(&mut self, cx: &mut Context<Self>) {
        let Some(notice) = self.rv().take_notice(cx) else {
            return;
        };
        let Some((from, to)) = notice.undo else {
            return;
        };
        let place = self.rv().place(cx);
        self.rv().restore_at(place.clone(), cx);
        let file = place.map(|(path, _)| path);
        self.load_versions(from, to, file, cx);
    }

    /// The title bar's quiet status.
    pub(super) fn render_status(&self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        if !self.settings.auto_refresh || self.root.is_none() {
            return None;
        }
        let status = self.watch.status;
        // Quiet while everything is fine: a status that is always on stops being read.
        if status == Status::UpToDate {
            return None;
        }
        Some(
            // A click refreshes, like ⌘R.
            div()
                .id("status")
                .flex()
                .items_center()
                .gap_2()
                .h(px(24.))
                .px_2()
                .rounded(px(super::ROW_RADIUS))
                .text_size(px(11.))
                .text_color(theme::muted())
                .cursor_pointer()
                .hover(|s| s.bg(theme::hover()).text_color(theme::text()))
                .child(div().size(px(7.)).rounded_full().bg(status.color()))
                .child(status.label())
                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                    this.refresh(&super::Refresh, window, cx)
                })),
        )
    }
}
