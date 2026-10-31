//! The page of the space on screen (#71), under its bar (`bar.rs`): the Inbox, Code's project or
//! a file, the list of branches or requests at their space's root (#72, their features'), else a
//! branch's or request's panes with its identity over the diff.
//! Code is the project's tree beside its folder page or a file's tabs (`code.rs`, #75).

use super::nav::Section;
use super::spaces::Space;
use super::{Split, Workspace, gap, motion, px};
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
            // Their own pages are the features'.
            (Space::Branches | Space::Requests, None) => div().flex_1(),
            // A request's Overview and Commits, a branch's Commits, are pages of their own
            // (#73, #74).
            (Space::Requests | Space::Branches, Some(_))
                if let Some(tab @ (Section::Overview | Section::Commits)) = self.page_tab(cx) =>
            {
                self.render_tab_page(tab, cx)
            }
            (Space::Branches | Space::Requests, Some(_)) => self.render_item(cx),
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
                )
            }))
            .children(
                self.review
                    .read(cx)
                    .diff
                    .as_ref()
                    .filter(|_| self.zen.is_some() || self.motion.recent("zen"))
                    .map(|diff| {
                        self.zen_strip(
                            "zen-head",
                            self.zen.is_some(),
                            36. + gap(),
                            div()
                                .flex()
                                .flex_col()
                                .child(
                                    self.review
                                        .read(cx)
                                        .render_zen_head(self.header_title(diff, cx), self.zen_one),
                                )
                                .child(div().h(px(gap()))),
                        )
                    }),
            )
            .child(body)
            .into_any_element()
    }

    /// A branch's or a request's page under its bar: the side panel; the item's identity over the
    /// diff; the tool window.
    fn render_item(&self, cx: &mut Context<Self>) -> gpui::Div {
        // What the page is of: the parts' (a request being written, a request's line, a branch's, the head over
        // a diff).
        let identity = self
            .review
            .read(cx)
            .diff
            .is_some()
            .then_some(())
            .filter(|_| self.zen.is_none() || self.motion.recent("zen"))
            .and_then(|_| self.piece(Slot::Identity, cx));
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .flex()
            .children(
                self.render_side(cx)
                    .filter(|_| !self.compact())
                    .map(|side| {
                        // It slides in and out at its width (#56, 4, 16).
                        let room = self.layout.sidebar + super::frame::handle_width();
                        motion::toward(
                            div()
                                .flex()
                                .flex_none()
                                .overflow_hidden()
                                .child(side)
                                .child(self.handle(Split::Sidebar, cx)),
                            "side-pane",
                            self.layout.show_sidebar,
                            self.motion.recent("side"),
                            motion::BASE,
                            // It fades as it narrows (the developer, 2026-10-23).
                            move |pane, v| pane.max_w(px(room * v)).opacity(v),
                        )
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    // Zen folds it up as it does the bar (the most a commit's header is tall,
                    // its message open or not).
                    .children(identity.map(|identity| {
                        self.zen_strip(
                            "identity-zen",
                            self.zen.is_none(),
                            if self.history.read(cx).message_open {
                                320.
                            } else {
                                110.
                            },
                            div()
                                .flex()
                                .flex_col()
                                .child(identity)
                                .child(div().h(px(gap()))),
                        )
                    }))
                    .children(self.pieces(Slot::OverDiff, cx))
                    .child(self.render_diff(cx))
                    .children(self.render_zen_keys()),
            )
            .children(self.render_tool(cx))
            // Narrow, the side panel opens over the content (#80).
            .children(
                self.side_drawer()
                    .then(|| self.render_side(cx))
                    .flatten()
                    .map(|side| self.drawer(side)),
            )
    }
}
