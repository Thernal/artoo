//! The page of the space on screen (#71), under its bar (`bar.rs`): the Inbox, Code's project or
//! a file, the list of branches or requests at their space's root (#72), a branch's or a
//! request's page — each its feature's pages (`render_pages`).
//! Code is the project's tree beside its folder page or a file's tabs (`code.rs`, #75).

use super::spaces::Space;
use super::{Workspace, gap, px};
use artoo_shell_api::Slot;
use gpui::{AnyElement, Context, div, prelude::*};

impl Workspace {
    pub(super) fn render_space(&self, cx: &mut Context<Self>) -> AnyElement {
        let body = match (self.nav.space, self.nav_page()) {
            // A page a feature draws: the Inbox, Code's tree beside its folder page or a file's tabs
            // (#75), the lists of Branches and Merge requests.
            (space, page) if let Some(pages) = self.pages_drawing(space, page) => {
                self.render_pages(pages, cx)
            }
            // The Inbox is the requests'.
            (Space::Inbox, _) => div().flex_1(),
            // Their pages are the features'.
            (Space::Branches | Space::Requests, _) => div().flex_1(),
            // Code without the pages the tab gives it: nothing to draw.
            (Space::Code, _) => div().flex_1(),
        };
        div()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            // One bar over every page; zen has its own line instead.
            .children(self.render_page_bar(cx).map(|bar| {
                self.zen_strip(
                    "bar-zen",
                    self.zen.is_none(),
                    44. + gap(),
                    div().flex().flex_col().child(bar).child(div().h(px(gap()))),
                    cx,
                )
            }))
            .children(
                self.shown_diff(cx)
                    .as_ref()
                    .filter(|_| self.zen.is_some() || self.motion.recent("zen"))
                    .map(|_| {
                        self.zen_strip(
                            "zen-head",
                            self.zen.is_some(),
                            36. + gap(),
                            div()
                                .flex()
                                .flex_col()
                                .child(self.rv().render_zen_head(
                                    self.header_title(cx),
                                    self.zen_one,
                                    cx,
                                ))
                                .child(div().h(px(gap()))),
                            cx,
                        )
                    }),
            )
            .child(body)
            .into_any_element()
    }

    /// What the page is of, the parts' (a request being written, a request's line, a branch's, the
    /// head over a diff), while a diff is shown: zen folds it up as it does the bar (the most a
    /// commit's header is tall, its message open or not).
    pub(super) fn render_identity(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let identity = self.shown_diff(cx)
            .is_some()
            .then_some(())
            .filter(|_| self.zen.is_none() || self.motion.recent("zen"))
            .and_then(|_| self.piece(Slot::Identity, cx))?;
        Some(
            self.zen_strip(
                "identity-zen",
                self.zen.is_none(),
                if self.branches().is_some_and(|b| b.message_open(cx)) {
                    320.
                } else {
                    110.
                },
                div()
                    .flex()
                    .flex_col()
                    .child(identity)
                    .child(div().h(px(gap()))),
                cx,
            ),
        )
    }
}
