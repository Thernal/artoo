//! Plugins (#175): native libraries built against Artoo's SDK, loaded as the app starts, each installing what it
//! adds through the same registries a feature does. A plugin is checked before anything of it runs — its stamp
//! names the SDK it was built against, read from its file — and Settings → Plugins says what became of each.

mod card;
mod crash;
mod file;
mod load;
mod market;
mod settings;

use gpui::App;

/// Loads the plugins and offers Settings → Plugins. The app's last install, after every feature: a plugin finds
/// the registries filled and what the features offer, and a plugin's own services are offered before the next one
/// installs.
pub fn install(cx: &mut App) {
    let (plugins, crash) = load::load(cx);
    // The plugins have installed: a source of notices is offered as one installs, never after (#196).
    artoo_shell_api::notices::close(cx);
    settings::install(plugins, crash, cx);
}

/// `Artoo sdk-build`: the identity of the SDK this app loaded — what `ARTOO_SDK_BUILD` names for a plugin built
/// against it.
pub fn print_sdk_build() {
    match file::sdk_build() {
        Some(build) => println!("{build}"),
        None => {
            eprintln!("This Artoo's SDK could not be read.");
            std::process::exit(1);
        }
    }
}
