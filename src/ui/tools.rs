//! The right edge of the redesign (Design `design/shell/`, #49): an icon stripe that opens one tool
//! window at a time beside the page — only what is about it (#76): Code Outline, Merge requests
//! Discussions — and Submit review in the same place (the bell's Notifications window retired into
//! the Inbox, #72). The same icon or esc closes it; which one is open and its width are kept in
//! `layout.txt`. No stripe until a repository is open, none in zen. A feature's tool window
//! (`artoo_shell_api::Tool`: the Agents window, Report a problem) is in the spaces it names, after
//! these.

use super::zones::Zone;
use super::{Split, Workspace, px, theme};
use artoo_shell_api::tools;
use gpui::{Context, Rgba, div, prelude::*};
use std::rc::Rc;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    /// The declarations of the file read in Code (#76).
    Outline,
    Discussions,
    /// A feature's, by its id (`agents`: the Agents window; `report`: Report a problem, #118).
    Part(&'static str),
}

impl Tool {
    pub(super) fn name(self) -> &'static str {
        match self {
            Tool::Outline => "outline",
            Tool::Discussions => "discussions",
            Tool::Part(id) => id,
        }
    }
}

pub use artoo_shell_api::tools::ToolView;

impl Workspace {
    /// The tool windows of the space on screen (#76): Code Outline, a request's page Discussions;
    /// then the features' in the spaces they name (the Agents window in all but the Inbox).
    fn space_tools(&self) -> Vec<Tool> {
        use super::spaces::Space;
        let own: &[Tool] = match self.nav.space {
            Space::Code => &[Tool::Outline],
            Space::Requests if self.nav_page().is_some() => &[Tool::Discussions],
            _ => &[],
        };
        let space = self.nav.space.name();
        own.iter()
            .copied()
            .chain(
                self.part_tools()
                    .filter(|t| t.spaces.is_empty() || t.spaces.contains(&space))
                    .map(|t| Tool::Part(t.id)),
            )
            .collect()
    }

    /// The tool window named `name` (`layout.txt`'s word for it), whichever spaces have it.
    pub(super) fn tool_named(&self, name: &str) -> Option<Tool> {
        [Tool::Outline, Tool::Discussions]
            .into_iter()
            .chain(self.part_tools().map(|t| Tool::Part(t.id)))
            .find(|t| t.name() == name)
    }

    /// The tool window open now, when its space has it; a request's details are its Overview
    /// (#73).
    pub(super) fn tool(&self) -> Option<Tool> {
        self.root.as_ref()?;
        let tool = self.tool_named(&self.layout.tool)?;
        self.space_tools().contains(&tool).then_some(tool)
    }

    fn tool_icon(&self, tool: Tool) -> &'static str {
        match tool {
            Tool::Outline => "list",
            Tool::Discussions => "message",
            Tool::Part(id) => self.part_tool(id).map_or("", |t| t.icon),
        }
    }

    fn tool_tip(&self, tool: Tool) -> &'static str {
        match tool {
            Tool::Outline => "Outline",
            Tool::Discussions => "Discussions  ⌘⇧D",
            Tool::Part(id) => self.part_tool(id).map_or("", |t| t.tip),
        }
    }

    /// The window on the right is drawn: a tool, or Submit review.
    pub(super) fn tool_shown(&self, cx: &gpui::App) -> bool {
        self.zen.is_none() && (self.submitting(cx) || self.tool().is_some())
    }

    /// Opens `tool` (and gives it the keyboard), or closes the one open.
    pub(super) fn set_tool(&mut self, tool: Option<Tool>, cx: &mut Context<Self>) {
        let name = tool.map_or("", Tool::name);
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
        // A feature's window gets ready before it is drawn (the report takes the window's picture).
        if let Some(Tool::Part(id)) = tool
            && was != tool
        {
            self.tool_opened(id, cx);
        }
        if tool.is_some() {
            self.set_zone(Zone::Tool, cx);
        } else if self.zone == Zone::Tool {
            self.set_zone(
                if self.review.read(cx).diff.is_some() {
                    Zone::Diff
                } else {
                    Zone::Commits
                },
                cx,
            );
        }
        cx.notify();
    }

    /// A feature's tool window: its title and its badge's count, and its view under it (the body
    /// and its foot).
    fn part_tool_view(&self, id: &str, cx: &gpui::App) -> Option<ToolView> {
        let tool = self.part_tool(id)?;
        Some(ToolView {
            title: tool.title.into(),
            count: tool
                .badge
                .as_ref()
                .map(|badge| badge(cx))
                .filter(|b| b.count > 0)
                .map(|b| b.what),
            actions: Vec::new(),
            body: tool.view.clone().into_any_element(),
            foot: None,
        })
    }

    /// Under 1 000 px the tool window floats over the diff instead of taking its width (#80).
    pub(super) fn tool_floats(&self) -> bool {
        self.design_width() < super::narrow::FLOAT
    }

    pub(super) fn toggle_tool(&mut self, tool: Tool, cx: &mut Context<Self>) {
        let open = self.tool() == Some(tool);
        self.set_tool((!open).then_some(tool), cx);
    }

    /// The icon stripe on the right edge.
    pub(super) fn render_stripe(&self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let zen = self.zen.is_some() && !self.motion.recent("zen");
        if self.root.is_none() || zen || self.settings_open {
            return None;
        }
        let open = self.tool();
        let count = |n: usize, color: Rgba| (n > 0).then(|| (n.min(99).to_string(), color));
        let buttons = self
            .space_tools()
            .into_iter()
            .map(|tool| {
                let button = |badge, what, under| tools::Button {
                    id: tool.name(),
                    icon: self.tool_icon(tool),
                    tip: self.tool_tip(tool),
                    on: open == Some(tool),
                    badge,
                    what,
                    under,
                };
                match tool {
                    Tool::Outline => button(None, String::new(), None),
                    // What its badge says, and what is under it: the Agents window's dot
                    // per chat that listens, on its own hue (#77).
                    Tool::Part(id) => match self.part_tool(id).and_then(|t| t.badge.clone()) {
                        None => button(None, String::new(), None),
                        Some(badge) => {
                            let badge = badge(cx);
                            button(count(badge.count, badge.color), badge.what, badge.under)
                        }
                    },
                    Tool::Discussions => button(
                        count(self.comments_total(cx), theme::accent()),
                        super::plural(self.comments_total(cx), "comment"),
                        None,
                    ),
                }
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
            None => match self.tool().or(self.last_tool.filter(|_| leaving))? {
                Tool::Outline => self.outline_tool(cx),
                Tool::Discussions => self.discussions_tool(cx),
                Tool::Part(id) => self.part_tool_view(id, cx)?,
            },
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
                    artoo_shell::frame::handle_width()
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
