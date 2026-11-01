//! What the tab's watch found on the branch open (#49): newer commits held back until the user asks for them, their
//! pill over the commits, and the versions that arrived since the user last looked. The watch reads the repository;
//! the history keeps what it found.

use crate::history::{Act, Acts, History};
use artoo_core::git::{CommitInfo, Version};
use artoo_ui_kit::{ROW_RADIUS, px, theme};
use gpui::{Div, Stateful, div, prelude::*};

/// What a refresh found on the open branch, held back until the user asks for it.
pub struct Pending {
    pub commits: Vec<CommitInfo>,
    pub versions: Vec<Version>,
    /// New commits on top of the ones shown; `None` when the branch was rewritten.
    pub new: Option<usize>,
}

impl History {
    /// What was found on the branch no longer applies: another one is open, or it is read anew.
    pub fn forget_news(&mut self) {
        self.pending = None;
        self.new_from = None;
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
