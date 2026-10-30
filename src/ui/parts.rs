//! The tab's side of the shell's contract (#165 stage 4, `artoo_shell_api`): the parts features made for it,
//! drawn where the frame has room for them, the keys an open card takes, and the `Frame` they ask.

use super::Workspace;
use artoo_shell_api::{Frame, Overlay, Run, Section};
use artoo_ui_kit::markdown::Selecting;
use gpui::{AnyView, App, Context, KeyDownEvent, SharedString, WeakEntity, Window};

/// A tab as its parts reach it.
pub(super) struct TabFrame(pub WeakEntity<Workspace>);

impl Frame for TabFrame {
    fn toast(&self, text: SharedString, cx: &mut App) {
        self.0.update(cx, |this, cx| this.toast_info(text, cx)).ok();
    }

    fn error(&self, text: SharedString, cx: &mut App) {
        self.0
            .update(cx, |this, cx| {
                this.error = Some(text);
                cx.notify();
            })
            .ok();
    }

    fn redraw(&self, cx: &mut App) {
        self.0.update(cx, |_, cx| cx.notify()).ok();
    }

    fn selecting(&self, cx: &App) -> Option<Selecting> {
        let tab = self.0.upgrade()?;
        Some(tab.read(cx).selecting(self.0.clone()))
    }
}

impl Workspace {
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
        let busy = self.shortcuts || self.label_edit.is_some() || self.typing().is_some();
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
