//! The tab's side of the shell's contract (#165 stage 4, `artoo_shell_api`): the parts features made for it,
//! drawn where the frame has room for them, the keys an open card or a tool window takes, and the `Frame` they
//! ask.

use super::menu::{Act, Entry};
use super::nav::Page;
use super::recents::Key;
use super::spaces::Space;
use super::tools::Tool;
use super::zones::Zone;
use super::{Workspace, motion, px};
use artoo_core::route::Route;
use artoo_shell_api::{
    Drawing, Frame, MenuItem, Overlay, Pages, Places, Run, Section, Services, Slot,
};
use artoo_ui_kit::markdown::Selecting;
use gpui::{
    AnyElement, AnyView, App, Context, Div, KeyDownEvent, Pixels, Point, SharedString, WeakEntity,
    Window, div, prelude::*,
};
use std::rc::Rc;

/// A tab as its parts reach it. What it changes waits until the update under way is over: a part asks from
/// its own update, and the tab may be what called it.
pub(super) struct TabFrame(pub WeakEntity<Workspace>, pub Rc<Services>);

impl TabFrame {
    fn later(
        &self,
        cx: &mut App,
        f: impl FnOnce(&mut Workspace, &mut Context<Workspace>) + 'static,
    ) {
        let tab = self.0.clone();
        cx.defer(move |cx| {
            tab.update(cx, f).ok();
        });
    }
}

impl Frame for TabFrame {
    fn toast(&self, text: SharedString, cx: &mut App) {
        self.later(cx, |this, cx| this.toast_info(text, cx));
    }

    fn error(&self, text: SharedString, cx: &mut App) {
        self.later(cx, |this, cx| {
            this.error = Some(text);
            cx.notify();
        });
    }

    fn redraw(&self, cx: &mut App) {
        self.later(cx, |_, cx| cx.notify());
    }

    fn selecting(&self, cx: &App) -> Option<Selecting> {
        let tab = self.0.upgrade()?;
        Some(tab.read(cx).selecting(self.0.clone()))
    }

    fn places(&self, cx: &App) -> Places {
        let Some(tab) = self.0.upgrade() else {
            return Places::default();
        };
        let this = tab.read(cx);
        Places {
            root: this.root.clone(),
            project: this.project.clone(),
            recent: this.recent.clone(),
            web: this.web.clone(),
            host: this.host.as_ref().map(|h| h.host()),
        }
    }

    fn show_tool(&self, id: Option<&'static str>, cx: &mut App) {
        self.later(cx, move |this, cx| this.set_tool(id.map(Tool::Part), cx));
    }

    fn tool_typing(&self, id: &str, cx: &App) -> bool {
        self.0.upgrade().is_some_and(|tab| {
            let this = tab.read(cx);
            matches!(this.tool(), Some(Tool::Part(open)) if open == id)
                && this.has_keyboard(super::typing::Input::Tool, cx)
        })
    }

    fn menu(&self, at: Point<Pixels>, groups: Vec<Vec<MenuItem>>, cx: &mut App) {
        let groups = groups
            .into_iter()
            .map(|group| {
                group
                    .into_iter()
                    .map(|item| {
                        let entry = match item.run {
                            Some(run) => Entry::new(item.label, Act::Part(run)),
                            None => Entry::off(item.label),
                        };
                        let entry = entry.key(item.key);
                        match item.checked {
                            Some(on) => entry.checked(on),
                            None => entry,
                        }
                    })
                    .collect()
            })
            .collect();
        self.later(cx, move |this, cx| this.open_menu(at, groups, cx));
    }

    fn close_drawer(&self, cx: &mut App) {
        self.later(cx, |this, cx| this.close_drawer(cx));
    }

    fn open(&self, route: Route, cx: &mut App) {
        self.later(cx, move |this, cx| {
            let mut drawn = this.parts.iter().filter_map(|p| p.pages.as_ref());
            match Page::from_route(&route)
                .filter(|_| drawn.any(|p| p.kinds.contains(&route.kind.as_str())))
            {
                Some(page) => this.open_page(page, cx),
                None => this.toast_info(format!("Nothing here opens {}", route.kind).into(), cx),
            }
        });
    }

    fn open_space(&self, space: &'static str, cx: &mut App) {
        self.later(cx, move |this, cx| {
            if let Some(space) = Space::named(space) {
                this.show(space, None, cx);
            }
        });
    }

    fn services(&self) -> Rc<Services> {
        self.1.clone()
    }

    fn list_page(&self, list: AnyElement, preview: AnyElement, cx: &mut App) -> AnyElement {
        self.0
            .update(cx, |this, cx| this.list_page(list, preview, cx))
            .unwrap_or_else(|_| div().into_any_element())
    }

    fn filter(&self, cx: &App) -> String {
        self.0
            .upgrade()
            .map(|tab| tab.read(cx).bfilter.clone())
            .unwrap_or_default()
    }

    fn list_width(&self, cx: &App) -> f32 {
        self.0
            .upgrade()
            .map_or(0., |tab| super::roots::list_width(tab.read(cx).layout.list))
    }

    fn remember(&self, route: Route, cx: &mut App) {
        self.later(cx, move |this, _| {
            if let Some(Page::File(path)) = Page::from_route(&route) {
                this.remember_place(Key::File(path.clone()), path, "file".to_owned());
            }
        });
    }
}

impl Workspace {
    /// The pages a feature draws in `space`, when one does.
    pub(super) fn space_pages(&self, space: Space) -> Option<&Pages> {
        self.parts
            .iter()
            .filter_map(|p| p.pages.as_ref())
            .find(|p| p.space == space.name())
    }

    /// The pages a feature draws in `space` when they draw `page`: one of their kinds, or the
    /// space's own (`None`).
    pub(super) fn pages_drawing(&self, space: Space, page: Option<&Page>) -> Option<&Pages> {
        self.space_pages(space).filter(|p| {
            page.is_none_or(|page| {
                !p.kinds.is_empty() && p.kinds.contains(&page.route().kind.as_str())
            })
        })
    }

    /// The pages of `space` told what is on screen: `page`, or the space's own (`None`); `opened`
    /// anew, or gone back to.
    pub(super) fn show_pages(
        &self,
        space: Space,
        page: Option<&Page>,
        opened: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(show) = self.space_pages(space).map(|p| p.show.clone()) {
            show(page.map(Page::route).as_ref(), opened, cx);
        }
    }

    /// Page `page` opened: on screen in its space, kept in the history, its pages told so.
    pub(super) fn open_page(&mut self, page: Page, cx: &mut Context<Self>) {
        self.arrive(page.clone(), cx);
        self.show_pages(page.space(), Some(&page), true, cx);
        cx.notify();
    }

    /// A space's pages a feature draws: its panel beside them — sliding and fading in and out with
    /// ⌘J as the side panel does, with its handle, so no gap is left to vanish at the end (the
    /// developer, 2026-10-23), the handle resizing it as it does the side panel —, the tool window
    /// after them. Narrow, the panel opens over the page (#80).
    pub(super) fn render_pages(&self, pages: &Pages, cx: &mut Context<Self>) -> Div {
        let width = self.layout.sidebar;
        let beside = !self.compact() && (self.layout.show_sidebar || self.motion.recent("side"));
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .flex()
            .children(pages.side.as_ref().filter(|_| beside).map(|side| {
                let room = width + super::frame::handle_width();
                motion::toward(
                    div()
                        .flex()
                        .flex_none()
                        .overflow_hidden()
                        .child(side(width, cx))
                        .child(self.handle(super::Split::Sidebar, cx)),
                    SharedString::from(format!("{}-side-pane", pages.space)),
                    self.layout.show_sidebar,
                    self.motion.recent("side"),
                    motion::BASE,
                    move |pane, v| pane.max_w(px(room * v)).opacity(v),
                )
            }))
            .child(pages.view.clone())
            .children(self.render_tool(cx))
            .children(
                pages
                    .side
                    .as_ref()
                    .filter(|_| self.side_drawer())
                    .map(|side| self.drawer(side(width, cx))),
            )
    }

    /// The parts' tool windows, in the order the features were installed.
    pub(super) fn part_tools(&self) -> impl Iterator<Item = &artoo_shell_api::Tool> {
        self.parts.iter().filter_map(|p| p.tool.as_ref())
    }

    pub(super) fn part_tool(&self, id: &str) -> Option<&artoo_shell_api::Tool> {
        self.part_tools().find(|t| t.id == id)
    }

    /// A part's tool window is open, focused and being written in: typed keys go to it.
    pub(super) fn tool_typing(&self) -> bool {
        let Some(Tool::Part(id)) = self.tool() else {
            return false;
        };
        self.zone == Zone::Tool && self.part_tool(id).is_some_and(|t| t.typing.get())
    }

    /// The first piece the parts draw in `slot` for the route on screen (`artoo_shell_api::Piece`), in the order
    /// they were installed.
    pub(super) fn piece(&self, slot: Slot, cx: &mut Context<Self>) -> Option<AnyElement> {
        let route = self.nav_page()?.route();
        let drawing = self.drawing(cx);
        self.parts
            .iter()
            .flat_map(|p| &p.pieces)
            .filter(|p| p.slot == slot)
            .find_map(|p| (p.draw)(&route, &drawing, cx))
    }

    /// Every piece the parts draw in `slot` for the route on screen, in the order they were installed.
    pub(super) fn pieces(&self, slot: Slot, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(route) = self.nav_page().map(|p| p.route()) else {
            return Vec::new();
        };
        let drawing = self.drawing(cx);
        self.parts
            .iter()
            .flat_map(|p| &p.pieces)
            .filter(|p| p.slot == slot)
            .filter_map(|p| (p.draw)(&route, &drawing, cx))
            .collect()
    }

    /// What a piece is handed as it is drawn.
    fn drawing(&self, cx: &Context<Self>) -> Drawing {
        Drawing {
            selecting: self.selecting(cx.weak_entity()),
            web: self.web.clone(),
        }
    }

    /// Piece `id` of a part's, for the route on screen.
    pub(super) fn piece_named(&self, id: &str, cx: &mut Context<Self>) -> Option<AnyElement> {
        let route = self.nav_page()?.route();
        let drawing = self.drawing(cx);
        let piece = self
            .parts
            .iter()
            .flat_map(|p| &p.pieces)
            .find(|p| p.id == id)?;
        (piece.draw)(&route, &drawing, cx)
    }

    /// Field `id` of a part's (`artoo_shell_api::Input`).
    pub(super) fn part_input(&self, id: &str) -> Option<&artoo_shell_api::Input> {
        self.parts
            .iter()
            .flat_map(|p| &p.inputs)
            .find(|i| i.id == id)
    }

    /// A key while a part's field has the keyboard: the field's; esc lets it go.
    pub(super) fn part_input_key(
        &mut self,
        id: &'static str,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(key) = self.part_input(id).map(|i| i.key.clone()) else {
            return;
        };
        match key(event, window, cx) {
            artoo_shell_api::Typed::Leave => self.leave_input(cx),
            artoo_shell_api::Typed::Kept => cx.notify(),
        }
    }

    /// A key while a part's tool window has the keyboard: the window's.
    pub(super) fn tool_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(Tool::Part(id)) = self.tool() else {
            return;
        };
        if let Some(key) = self.part_tool(id).map(|t| t.key.clone()) {
            key(event, window, cx);
            cx.notify();
        }
    }

    /// A part's tool window opened: it gets ready before it is drawn.
    pub(super) fn tool_opened(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(opened) = self.part_tool(id).map(|t| t.opened.clone()) {
            opened(cx);
        }
    }

    /// The parts' card that is open, if one is.
    fn open_overlay(&self) -> Option<&Overlay> {
        self.parts
            .iter()
            .filter_map(|p| p.overlay.as_ref())
            .find(|o| o.open.get())
    }

    pub(super) fn overlay_open(&self) -> bool {
        self.open_overlay().is_some()
    }

    /// The open card's view, drawn over the tab.
    pub(super) fn overlay_view(&self) -> Option<AnyView> {
        self.open_overlay().map(|o| o.view.clone())
    }

    /// A key while a part's card is open: the card's.
    pub(super) fn overlay_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(key) = self.open_overlay().map(|o| o.key.clone()) {
            key(event, window, cx);
            cx.notify();
        }
    }

    /// Escape on an open card that did not take it.
    pub(super) fn close_overlay(&mut self, cx: &mut Context<Self>) {
        if let Some(close) = self.open_overlay().map(|o| o.close.clone()) {
            close(cx);
            cx.notify();
        }
    }

    /// Each frame: a card that opens on its own may, unless something is being written or a card is up.
    pub(super) fn poll_parts(&mut self, cx: &mut Context<Self>) {
        let busy = self.shortcuts || self.label_edit.is_some() || self.typing(cx).is_some();
        if busy {
            return;
        }
        let polls: Vec<_> = self
            .parts
            .iter()
            .filter_map(|p| p.overlay.as_ref()?.poll.clone())
            .collect();
        for poll in polls {
            poll(cx);
        }
    }

    /// The parts' items in the title bar that are shown now.
    pub(super) fn title_items(&self, cx: &App) -> Vec<AnyView> {
        self.parts
            .iter()
            .filter_map(|p| p.title.as_ref())
            .filter(|t| (t.shown)(cx))
            .map(|t| t.view.clone())
            .collect()
    }

    /// The parts' rows under a section of Settings.
    pub(super) fn settings_rows(&self, section: Section) -> Vec<AnyView> {
        self.parts
            .iter()
            .flat_map(|p| &p.settings)
            .filter(|(s, _)| *s == section)
            .map(|(_, view)| view.clone())
            .collect()
    }

    /// The parts' actions, as this tab runs them.
    pub(super) fn part_actions(&self) -> Vec<(Box<dyn gpui::Action>, Run)> {
        self.parts
            .iter()
            .flat_map(|p| &p.actions)
            .map(|(act, run)| (act.boxed_clone(), run.clone()))
            .collect()
    }

    /// A snapshot view a part opens (`ARTOO_SNAPSHOT_VIEW`); false when no part has it.
    #[cfg(feature = "snapshot")]
    pub(super) fn open_part_snapshot(&mut self, name: &str, cx: &mut Context<Self>) -> bool {
        let open = self
            .parts
            .iter()
            .flat_map(|p| &p.snapshots)
            .find(|(n, _)| *n == name)
            .map(|(_, open)| open.clone());
        if let Some(open) = &open {
            open(cx);
            cx.notify();
        }
        open.is_some()
    }
}
