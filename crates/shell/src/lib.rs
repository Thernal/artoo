//! The window's frame (#165 stage 4): what surrounds the pages features draw — the keyboard card, the
//! repository tabs and the rail, the title bar, the stripe and its tool windows, the page bar and
//! the spaces' menu, the body's row and its splitters, search everywhere's window and what it lists and does
//! (`search_window.rs`), Settings' frame, Set up's, the welcome, so far.
//! The tab (the app's) composes: it makes the features' entities and their hosts, and hands the frame what it draws
//! and what a click on it does.

mod bar;
mod frame;
mod palette;
mod rail;
mod search_window;
mod settings;
mod setup;
mod shortcuts;
mod spaces;
mod title;
mod tools;
mod welcome;

/// The frame's renderers, for every window (the kits' wiring: `LocalX provides XImpl`), and the editors Artoo
/// ships, which Open in opens files in.
pub fn install(cx: &mut gpui::App) {
    use artoo_shell_api::render::provide;
    use std::rc::Rc;
    for editor in artoo_core::editor::BUILT_IN {
        artoo_shell_api::add_editor(cx, editor);
    }
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
    // ⌘K's window in every tab, its handle in the tab's services.
    artoo_shell_api::add_parts(cx, search_window::parts);
}
