//! One viewer for any image in the app (#211, Design task management round 8, F2): an image opened whole is the
//! app's, never a page's own. A page hands the tab's viewer its images and the one to show
//! (`artoo_images_api::Viewer`); the viewer draws it over the window — its name, which of the page's images it is,
//! its size, who added it and when —, steps through the page's images with ← →, saves it to Downloads (⌘S), opens it
//! where it lives (⌥⌘J), and goes with esc.

use gpui::App;

mod viewer;

/// The viewer in every tab: its card over the window, offered in the tab's services.
pub fn install(cx: &mut App) {
    artoo_shell_api::add_parts(cx, viewer::parts);
}
