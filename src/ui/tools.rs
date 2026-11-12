//! The right edge of the redesign (Design `design/shell/`, #49): an icon stripe that opens one tool
//! window at a time beside the page — only what is about it (#76) — and Submit review in the same place
//! (the bell's Notifications window retired into the Inbox, #72). Every tool window is a feature's
//! (`artoo_shell_api::Tool`: Code's Outline, the requests' Discussions, the Agents window, Report a
//! problem), in the spaces it names, in its order. The same icon or esc closes it; which one is open
//! and its width are kept in `layout.txt`, by its id. No stripe until a repository is open, none in zen.

use super::zones::Zone;
use super::{Split, Workspace, px};
use artoo_shell_api::tools;
use gpui::{Context, Rgba, div, prelude::*};
use std::rc::Rc;

pub use artoo_shell_api::tools::ToolView;

impl Workspace {
    /// The tool windows of the space on screen, by their ids, in their order (#76): those about the
    /// page first (Code's Outline, a request's Discussions), then those in every space.
    fn space_tools(&self) -> Vec<&'static str> {
        let space = self.nav.space.name();
        let page = self.nav_page().is_some();
        let mut tools: Vec<&artoo_shell_api::Tool> = self
            .part_tools()
            .filter(|t| t.spaces.is_empty() || t.spaces.contains(&space))
            .filter(|t| !t.not_in.contains(&space))
            .filter(|t| page || !t.on_pages)
            .collect();
        tools.sort_by_key(|t| t.order);
        tools.into_iter().map(|t| t.id).collect()
    }

    /// The tool window named `name` (`layout.txt`'s word for it), whichever spaces have it.
    pub(super) fn tool_named(&self, name: &str) -> Option<&'static str> {
        self.part_tool(name).map(|t| t.id)
    }

    /// The tool window open now, when its space has it; a request's details are its Overview
    /// (#73).
    pub(super) fn tool(&self) -> Option<&'static str> {
        self.root.as_ref()?;
        let tool = self.tool_named(&self.layout.tool)?;
        self.space_tools().contains(&tool).then_some(tool)
    }

    /// The window on the right is drawn: a tool, or Submit review.
    pub(super) fn tool_shown(&self, cx: &gpui::App) -> bool {
        self.zen.is_none() && (self.submitting(cx) || self.tool().is_some())
    }

    /// Opens `tool` (and gives it the keyboard), or closes the one open.
    pub(super) fn set_tool(&mut self, tool: Option<&'static str>, cx: &mut Context<Self>) {
        let name = tool.unwrap_or("");
        // Floating, it and the drawer are never open together (#80).
        if tool.is_some() && self.tool_floats() {
            self.close_drawer(cx);
        }
        // Opened or closed, it slides (#56, 3); one tool for another is no motion.
        let was = self.tool();
        if was.is_some() != tool.is_some() {
            self.last_tool = was.or(tool);
            self.mark_motion("tool", cx);
        }
        if self.layout.tool != name {
            self.layout.tool = name.to_owned();
            self.save_layout();
        }
        // The one that goes lets go of what it keeps only while open (the search's reads).
        if let Some(id) = was
            && was != tool
        {
            self.tool_closed(id, cx);
        }
        // A window gets ready before it is drawn (the report takes the window's picture).
        if let Some(id) = tool
            && was != tool
        {
            self.tool_opened(id, cx);
        }
        if tool.is_some() {
            self.set_zone(Zone::Tool, cx);
        } else if self.zone == Zone::Tool {
            self.set_zone(
                if self.shown_diff(cx).is_some() {
                    Zone::Diff
                } else {
                    Zone::Commits
                },
                cx,
            );
        }
        cx.notify();
    }

    /// Tool window `id`: its title and what is said after it, and its view under it (the body and
    /// its foot).
    fn part_tool_view(&self, id: &str, cx: &gpui::App) -> Option<ToolView> {
        let tool = self.part_tool(id)?;
        let count = match &tool.count {
            Some(count) => count(cx),
            None => tool
                .badge
                .as_ref()
                .map(|badge| badge(cx))
                .filter(|b| b.count > 0)
                .map(|b| b.what),
        };
        let title = (tool.heading.as_ref())
            .and_then(|heading| heading(cx))
            .map_or_else(|| tool.title.into(), Into::into);
        Some(ToolView {
            title,
            count,
            actions: Vec::new(),
            body: tool.view.clone().into_any_element(),
            foot: None,
        })
    }

    /// The tool window floats over the diff instead of taking its width when, the side panel folded first, the diff
    /// would still fall under 480 px beside it (#80, #195).
    pub(super) fn tool_floats(&self) -> bool {
        let side = self.layout.show_sidebar && !self.side_folded();
        self.diff_room(side, true) < super::narrow::ROOM
    }

    pub(super) fn toggle_tool(&mut self, tool: &'static str, cx: &mut Context<Self>) {
        let open = self.tool() == Some(tool);
        self.set_tool((!open).then_some(tool), cx);
    }

    /// The icon stripe on the right edge.
    pub(super) fn render_stripe(&self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let zen = self.zen.is_some() && !self.motion.recent("zen");
        // The Inbox has none: the page takes its width (#195).
        if self.root.is_none() || zen || self.settings_open || self.at_inbox() {
            return None;
        }
        let open = self.tool();
        let count = |n: usize, color: Rgba| (n > 0).then(|| (n.min(99).to_string(), color));
        let buttons = self
            .space_tools()
            .into_iter()
            .filter_map(|id| {
                // Only the windows you work in (#195).
                let tool = self.part_tool(id).filter(|t| t.stripe)?;
                // What its badge says, and what is under it: the Agents window's dot per chat
                // that listens, on its own hue (#77).
                let (badge, what, under) = match &tool.badge {
                    None => (None, String::new(), None),
                    Some(badge) => {
                        let badge = badge(cx);
                        (count(badge.count, badge.color), badge.what, badge.under)
                    }
                };
                // What the window names says its tip, before its key: `Search in payments-service  ⌥⌘F`.
                let tip = match tool.heading.as_ref().and_then(|heading| heading(cx)) {
                    Some(named) => match tool.tip.split_once("  ") {
                        Some((_, key)) => format!("{named}  {key}").into(),
                        None => named.into(),
                    },
                    None => tool.tip.into(),
                };
                Some(tools::Button {
                    id,
                    icon: tool.icon,
                    tip,
                    on: open == Some(id),
                    badge,
                    what,
                    under,
                })
            })
            .collect();
        let tab = cx.weak_entity();
        Some(tools::stripe(
            buttons,
            Rc::new(move |name, _, cx| {
                tab.update(cx, |this, cx| {
                    if let Some(tool) = this.tool_named(name) {
                        this.toggle_tool(tool, cx)
                    }
                })
                .ok();
            }),
            cx,
        ))
    }

    pub(super) fn render_tool(&self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        // Closed a moment ago, it is drawn on its way out (#56, 3).
        let shown = self.tool_shown(cx);
        let leaving = !shown && self.zen.is_none() && self.motion.recent("tool");
        if !shown && !leaving {
            return None;
        }
        let view = match self.submit_tool(cx) {
            Some(view) => view,
            None => {
                let id = self.tool().or(self.last_tool.filter(|_| leaving))?;
                self.part_tool_view(id, cx)?
            }
        };
        let submitting = self.submitting(cx);
        let tab = cx.weak_entity();
        let close = Rc::new(move |_: &mut gpui::Window, cx: &mut gpui::App| {
            tab.update(cx, |this, cx| {
                if submitting {
                    this.close_submit(cx)
                } else {
                    this.set_tool(None, cx)
                }
            })
            .ok();
        });
        // A narrow window has no room beside the diff: the window floats over its right edge.
        let over = self.tool_floats();
        Some(tools::window(
            view,
            tools::Place {
                width: self.layout.tool_width,
                over,
                handle: (!over).then(|| self.handle(Split::Tool, cx).into_any_element()),
                handle_width: if over {
                    0.
                } else {
                    artoo_shell_api::frame::handle_width()
                },
                border: self.zone_border(Zone::Tool),
                shown,
                recent: self.motion.recent("tool"),
            },
            close,
            cx,
        ))
    }

    /// The gap between the page and the stripe, a tool window open or not: opening one moves
    /// nothing outside it, the header above included (#68).
    pub(super) fn stripe_gap(&self) -> Option<impl IntoElement + use<>> {
        (self.root.is_some() && self.zen.is_none() && !self.settings_open)
            .then(|| div().flex_none().w(px(super::gap())))
    }
}
