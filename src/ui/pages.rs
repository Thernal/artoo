//! The page of the space on screen (#71), under its bar (`bar.rs`): the Inbox, Code's project or
//! a file, the list of branches or requests at their space's root (`roots.rs`, #72), else a
//! branch's or request's panes with its identity over the diff.
//! Code is the project's tree beside its folder page or a file's tabs (`code.rs`, #75).

use super::nav::Section;
use super::spaces::Space;
use super::{Split, Workspace, gap, motion, px};
use gpui::{AnyElement, Context, div, prelude::*};

impl Workspace {
    pub(super) fn render_space(&self, cx: &mut Context<Self>) -> AnyElement {
        let body = match (self.nav.space, self.nav_page()) {
            (Space::Inbox, _) => div()
                .relative()
                .flex_1()
                .min_h_0()
                .flex()
                .child(shrinks(self.render_inbox(cx)))
                .children(self.render_tool(cx)),
            // A space whose pages a feature draws: Code's tree beside its folder page or a file's
            // tabs (#75).
            (space, _) if let Some(pages) = self.space_pages(space) => self.render_pages(pages, cx),
            (Space::Branches, None) => div()
                .relative()
                .flex_1()
                .min_h_0()
                .flex()
                .child(shrinks(self.render_branches_root(cx)))
                .children(self.render_tool(cx)),
            (Space::Requests, None) => div()
                .relative()
                .flex_1()
                .min_h_0()
                .flex()
                .child(shrinks(self.render_requests_root(cx)))
                .children(self.render_tool(cx)),
            // A request's Overview and Commits, a branch's Commits, are pages of their own
            // (#73, #74).
            (Space::Requests | Space::Branches, Some(_))
                if let Some(tab @ (Section::Overview | Section::Commits)) = self.page_tab() =>
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
                                .child(self.render_zen_head(diff, cx))
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
        let identity = self
            .review
            .read(cx)
            .diff
            .clone()
            .filter(|_| self.zen.is_none() || self.motion.recent("zen"))
            .map(|diff| {
                if let Some(head) = self.render_create_header(cx) {
                    head
                } else if let Some(head) = self.render_request_line(cx) {
                    head
                } else if let Some(head) = self.render_branch_line(cx) {
                    head
                } else {
                    self.render_header(&diff, cx).into_any_element()
                }
            });
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
                            if self.body_expanded { 320. } else { 110. },
                            div()
                                .flex()
                                .flex_col()
                                .child(identity)
                                .child(div().h(px(gap()))),
                        )
                    }))
                    .children(self.render_notes_summary(cx))
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

/// A root page beside the tool window: it gives way to it, never pushing it past the window's
/// edge — its list keeps its width, its preview narrows (the developer, 2026-10-23: Agents ran off
/// the Merge requests page).
fn shrinks(page: impl IntoElement) -> gpui::Div {
    // A column: the page is stretched across to its width, whatever its content's least width.
    div()
        .flex_1()
        .min_w_0()
        .min_h_0()
        .flex()
        .flex_col()
        .child(page)
}
