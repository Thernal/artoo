//! The window's frame (#165 stage 4): what surrounds the pages features draw — the keys and their card, toasts, the
//! context menu, the repository tabs and the rail, the title bar, the stripe and its tool windows, the page bar and
//! the spaces' menu, the body's row and its splitters, which input has the keyboard, search everywhere's window,
//! Settings' frame, Set up's, the welcome, so far.
//! The tab (the app's) composes: it makes the features' entities and their hosts, and hands the frame what it draws
//! and what a click on it does.

mod bar;
mod frame;
pub mod keyboard;
pub mod keymap;
pub mod menu;
mod palette;
mod rail;
mod settings;
mod setup;
mod shortcuts;
mod spaces;
pub mod tabs;
mod title;
pub mod toast;
mod tools;
mod welcome;

/// The frame's renderers, for every window (the kits' wiring: `LocalX provides XImpl`).
pub fn install(cx: &mut gpui::App) {
    use artoo_shell_api::render::provide;
    use std::rc::Rc;
    provide::<dyn artoo_shell_api::bar::BarRenderer>(Rc::new(bar::Renderer), cx);
    provide::<dyn artoo_shell_api::title::TitleRenderer>(Rc::new(title::Renderer), cx);
    provide::<dyn artoo_shell_api::tabs::TabsRenderer>(Rc::new(rail::Renderer), cx);
    provide::<dyn artoo_shell_api::tools::ToolsRenderer>(Rc::new(tools::Renderer), cx);
    provide::<dyn artoo_shell_api::spaces::SpacesRenderer>(Rc::new(spaces::Renderer), cx);
    provide::<dyn artoo_shell_api::palette::PaletteRenderer>(Rc::new(palette::Renderer), cx);
    provide::<dyn artoo_shell_api::settings::SettingsRenderer>(Rc::new(settings::Renderer), cx);
    provide::<dyn artoo_shell_api::setup::SetupRenderer>(Rc::new(setup::Renderer), cx);
    provide::<dyn artoo_shell_api::welcome::WelcomeRenderer>(Rc::new(welcome::Renderer), cx);
    provide::<dyn artoo_shell_api::frame::FrameRenderer>(Rc::new(frame::Renderer), cx);
    provide::<dyn artoo_shell_api::shortcuts::ShortcutsRenderer>(Rc::new(shortcuts::Renderer), cx);
}
