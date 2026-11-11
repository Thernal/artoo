//! The card over a branch's diff reviewed to its last file (`artoo_ui_kit::done`): the next branch of its kind whose
//! review is not done.

use crate::history::{Act, Acts, History};
use crate::roots::BranchRow;
use artoo_branches_api::{
    BranchCommitRoute, BranchCommitsRoute, BranchRoute, CompareRoute, VersionsRoute,
    WorkingTreeRoute,
};
use artoo_core::route::{Place, Route};
use artoo_review_api::Review;
use artoo_shell_api::{Piece, Services, Slot};
use artoo_ui_kit::done::{DoneEvent, DoneLook, render_done_card};
use git2::Oid;
use gpui::{App, Entity};
use std::rc::Rc;

impl History {
    /// After the branch open, the next branch of the same kind (local or remote) whose review is not done
    /// (`reviewed`), going round; never the checked-out one.
    pub fn next_unreviewed(&self, reviewed: &dyn Fn(Oid) -> bool) -> Option<usize> {
        let at = self.branch?;
        let kind = self.branches.get(at)?.kind;
        let n = self.branches.len();
        (1..n).map(|step| (at + step) % n).find(|&ix| {
            let b = &self.branches[ix];
            b.kind == kind && !b.is_head && !reviewed(b.tip)
        })
    }
}

/// A route of the branches' kinds.
fn of_branches(route: &Route) -> bool {
    [
        BranchRoute::KIND,
        BranchCommitRoute::KIND,
        BranchCommitsRoute::KIND,
        VersionsRoute::KIND,
        WorkingTreeRoute::KIND,
        CompareRoute::KIND,
    ]
    .contains(&route.kind.as_str())
}

/// The card over a branch's diff once its last file is marked: the next branch to review (`next_unreviewed`).
pub fn piece(history: Entity<History>, services: Rc<Services>) -> Piece {
    Piece {
        id: "branches.done",
        slot: Slot::OverRows,
        draw: Rc::new(move |route, _, cx| {
            if !of_branches(route) {
                return None;
            }
            let review = services.get::<Rc<dyn Review>>()?;
            let files = review.done(cx)?;
            let acts = services.get::<Acts>()?;
            let cx: &App = cx;
            let h = history.read(cx);
            let next = h.next_unreviewed(&|tip| review.head_reviewed(tip, cx));
            let look = DoneLook {
                files,
                request: None,
                submit: None,
                next: next.map(|ix| (format!("Next: {}", h.branches[ix].name), "")),
            };
            Some(render_done_card(
                look,
                Rc::new(move |event, cx| match event {
                    DoneEvent::Next => {
                        review.close_done(cx);
                        if let Some(ix) = next {
                            acts(Act::OpenRow(BranchRow::Branch(ix)), cx);
                        }
                    }
                    _ => review.close_done(cx),
                }),
            ))
        }),
    }
}
