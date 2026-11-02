//! What the tab's watch found on the branch open (#49): newer commits held back until the user asks for them, their
//! pill over the commits, and the versions that arrived since the user last looked. The watch reads the repository;
//! the history keeps what it found (`History::reloaded`) and shows it when asked (`History::apply_pending`).

use crate::history::{Act, Acts, Event, History, Older, Selection};
use crate::reading::COMMIT_PAGE;
use artoo_core::git::{BranchRef, CommitInfo, Repo, Version};
use artoo_ui_kit::{ROW_RADIUS, px, theme};
use gpui::{Div, Stateful, div, prelude::*};
use std::sync::Arc;

/// What a refresh found on the open branch, held back until the user asks for it.
pub struct Pending {
    pub commits: Vec<CommitInfo>,
    pub versions: Vec<Version>,
    /// New commits on top of the ones shown; `None` when the branch was rewritten.
    pub new: Option<usize>,
}

pub use artoo_branches_api::{Applied, Reloaded};

impl History {
    /// The repository changed under the window: the branches and the open branch's versions and history read again
    /// in the background, as much history as is listed (the pages scrolled to, #25); what it found kept and said
    /// (`Event::Reloaded`).
    pub fn reload(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(root) = self.root.clone() else {
            return;
        };
        let refname = self
            .branch
            .and_then(|ix| self.branches.get(ix))
            .map(|b| b.refname.clone());
        let count = self.commits.len().max(COMMIT_PAGE);
        self.reload = Some(cx.spawn(async move |this, cx| {
            let loaded = cx
                .background_executor()
                .spawn(async move {
                    let repo = Repo::open(&root)?;
                    let branches = repo.branches()?;
                    let tip = refname
                        .as_ref()
                        .and_then(|r| branches.iter().find(|b| &b.refname == r))
                        .map(|b| b.tip);
                    let open = match (&refname, tip) {
                        (Some(r), Some(tip)) => Some((
                            r.clone(),
                            repo.versions(r).unwrap_or_default(),
                            repo.log(tip, 0, count, None)?,
                        )),
                        _ => None,
                    };
                    anyhow::Ok((branches, refname, open, repo.decorations()))
                })
                .await;
            this.update(cx, |h, cx| {
                let reloaded = match loaded {
                    Ok((branches, refname, open, decor)) => {
                        h.decor = decor;
                        h.reloaded(branches, refname, open)
                    }
                    // Mid-rebase the repository can be unreadable for a moment; the next change retries.
                    Err(_) => Reloaded::Unreadable,
                };
                cx.emit(Event::Reloaded(reloaded));
            })
            .ok();
        }));
    }

    /// The repository re-read: its `branches`, and of the branch open (`refname`) its versions and commits when it
    /// is still there. What it found is kept, or said.
    pub fn reloaded(
        &mut self,
        branches: Vec<BranchRef>,
        refname: Option<String>,
        open: Option<(String, Vec<Version>, Vec<CommitInfo>)>,
    ) -> Reloaded {
        self.branches = Arc::new(branches);
        let Some(refname) = refname else {
            return Reloaded::Listed;
        };
        let Some((_, versions, commits)) = open else {
            let fallback = self
                .branches
                .iter()
                .position(|b| b.is_head)
                .or((!self.branches.is_empty()).then_some(0));
            return Reloaded::Gone { refname, fallback };
        };
        self.branch = self.branches.iter().position(|b| b.refname == refname);
        let old_first = self.commits.first().map(|c| c.id);
        let first_same = old_first == commits.first().map(|c| c.id);
        if first_same && versions.len() == self.versions.len() {
            self.pending = None;
            return Reloaded::Same;
        }
        let new = old_first.and_then(|id| commits.iter().position(|c| c.id == id));
        if first_same {
            let seen = self.versions.len();
            self.mark_new_versions(versions.len());
            self.versions = versions;
            return Reloaded::Versions { seen };
        }
        self.pending = Some(Pending {
            commits,
            versions,
            new,
        });
        Reloaded::Held
    }

    /// Shows what a refresh found: the new commits at the top, the selection kept where it still holds; with how
    /// many versions were listed before. `None` when nothing was held back.
    pub fn apply_pending(&mut self, cx: &mut gpui::Context<Self>) -> Option<(Applied, usize)> {
        let pending = self.pending.take()?;
        let selected = match self.selection {
            Selection::Commit(ix) => self.commits.get(ix).map(|c| c.id),
            _ => None,
        };
        let seen = self.versions.len();
        self.mark_new_versions(pending.versions.len());
        self.versions = pending.versions;
        self.commits = pending.commits;
        // Older history goes on from the new tip.
        if self.older.is_some() {
            self.older = self.commits.first().map(|tip| Older {
                tip: tip.id,
                task: None,
            });
        }
        self.refilter();
        self.index_paths(cx);
        let applied = match (self.selection, selected) {
            (Selection::Commit(_), Some(id)) => {
                match self.commits.iter().position(|c| c.id == id) {
                    // The commit is still there, further down: only its index moved.
                    Some(ix) => {
                        self.selection = Selection::Commit(ix);
                        Applied::Kept
                    }
                    None => Applied::Newest,
                }
            }
            (Selection::Versions { to, .. }, _) if to < self.versions.len() => Applied::Kept,
            (Selection::WorkingTree, _) => Applied::Kept,
            _ => Applied::Cleared,
        };
        Some((applied, seen))
    }

    /// What was found on the branch no longer applies: another one is open, or it is read anew.
    pub fn forget_news(&mut self) {
        self.pending = None;
        self.new_from = None;
        self.reload = None;
    }

    /// Versions of `len` arrived: those past the ones listed are new.
    pub fn mark_new_versions(&mut self, len: usize) {
        if len > self.versions.len() {
            self.new_from = Some(self.versions.len());
        }
    }

    /// Something found the user has not looked at.
    pub fn has_news(&self) -> bool {
        self.pending.is_some() || self.new_from.is_some()
    }

    /// "↑ 2 new commits · Show", above the commit list; a click asks `Act::ShowNew`.
    pub fn pill(&self, acts: &Acts) -> Option<Stateful<Div>> {
        let pending = self.pending.as_ref()?;
        let text = match pending.new {
            Some(1) => "1 new commit".to_owned(),
            Some(n) => format!("{n} new commits"),
            None => "The branch was rewritten".to_owned(),
        };
        let acts = acts.clone();
        Some(
            div()
                .id("pill")
                .flex_none()
                .flex()
                .items_center()
                .gap_2()
                .mx(px(6.))
                .mb(px(6.))
                .px(px(10.))
                .py(px(6.))
                .rounded(px(ROW_RADIUS))
                .bg(theme::info_bg())
                .text_size(px(11.))
                .text_color(theme::accent())
                .cursor_pointer()
                .hover(|s| s.bg(theme::selected()))
                .child("↑")
                .child(div().flex_1().child(text))
                .child(div().text_color(theme::muted()).child("Show"))
                .on_click(move |_, _, cx| acts(Act::ShowNew, cx)),
        )
    }
}
