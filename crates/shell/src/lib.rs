//! The window's frame (#165 stage 4): what surrounds the pages features draw — the keys and their card, toasts, the
//! context menu, Settings → Appearance's drawing, the repository tabs and the rail, the title bar, the stripe and its
//! tool windows, narrow windows' rules, the page bar and the spaces' menu, the body's row and its splitters,
//! which input has the keyboard, search everywhere's window,
//! Settings' frame, Set up's, the welcome, so far.
//! The tab (the app's) composes: it makes the features' entities and their hosts, and hands the frame what it draws
//! and what a click on it does.

pub mod appearance;
mod bar;
pub mod fonts;
pub mod frame;
pub mod keyboard;
pub mod keymap;
pub mod menu;
pub mod narrow;
pub mod palette;
pub mod rail;
pub mod settings;
pub mod setup;
pub mod shortcuts;
pub mod spaces;
pub mod tabs;
pub mod title;
pub mod toast;
pub mod tools;
pub mod welcome;

/// The frame's renderers, for every window (the kits' wiring: `LocalX provides XImpl`).
pub fn install(cx: &mut gpui::App) {
    use artoo_shell_api::render::provide;
    use std::rc::Rc;
    provide::<dyn artoo_shell_api::bar::BarRenderer>(Rc::new(bar::Renderer), cx);
}
