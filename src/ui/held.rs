//! The keyboard helpers (#193): ⌘ held a moment shows the keys of what is on screen over their controls — the
//! repositories' ⌘1…, the title bar's ⌘B ⌘[ ⌘] —, ⌃⌘ the spaces'. The state is the kit's
//! (`artoo_ui_kit::keys::held`), read as things are drawn; Settings → Keyboard turns them off.

use super::Workspace;
use gpui::Context;

impl Workspace {
    /// ⌘ (or ⌃⌘) held alone for a moment: the keys of what is on screen show over it, until it is let go —
    /// Settings → Keyboard's helpers on (#193).
    pub(super) fn modifiers_changed(
        &mut self,
        event: &gpui::ModifiersChangedEvent,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        use artoo_ui_kit::keys::{self, Held};
        let m = event.modifiers;
        let want = match (m.platform, m.control, m.alt || m.shift || m.function) {
            _ if !self.settings.key_helpers => None,
            (true, false, false) => Some(Held::Cmd),
            (true, true, false) => Some(Held::CtrlCmd),
            _ => None,
        };
        self.held_gen += 1;
        let Some(held) = want else {
            if keys::set_held(None) {
                window.refresh();
            }
            return;
        };
        let generation = self.held_gen;
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(350))
                .await;
            this.update_in(cx, |this, window, _| {
                if this.held_gen == generation && keys::set_held(Some(held)) {
                    window.refresh();
                }
            })
            .ok();
        })
        .detach();
    }
}
