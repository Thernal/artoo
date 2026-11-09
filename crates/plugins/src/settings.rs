//! Settings → Plugins (#175): each plugin found as Artoo started, what became of it and why, its switch, and the
//! plugins' folder. A switch writes `plugins.txt` at once and takes effect at the next start: a plugin's code, once
//! loaded, stays until the app quits.

use crate::load::{OFF, Plugin, State};
use artoo_plugin_api::{ABI, Stamp};
use artoo_shell_api::settings::{Rows, Section};
use artoo_shell_api::{Frame, Parts, Snap};
use artoo_ui_kit::controls::{outline_button, switch, tag};
use artoo_ui_kit::{ROW_RADIUS, hover, px, theme};
use gpui::{
    App, AppContext, ClickEvent, Context, FontWeight, Global, IntoElement, Render, SharedString,
    Window, div, prelude::*,
};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::rc::Rc;

/// The section's id.
const SECTION: &str = "plugins";

/// The plugins as this run loaded them, and the ids turned off now: what the next start reads.
#[derive(Default)]
struct Loaded {
    plugins: Vec<Plugin>,
    off: BTreeSet<String>,
    /// This Artoo's SDK build, what a plugin must be built against (`Artoo sdk-build`).
    sdk: Option<String>,
}

impl Global for Loaded {}

/// Keeps what was loaded and offers the section, after Keyboard, and its rows in each tab.
pub(crate) fn install(plugins: Vec<Plugin>, cx: &mut App) {
    cx.set_global(Loaded {
        plugins,
        off: crate::load::off(),
        sdk: crate::file::sdk_build(),
    });
    artoo_shell_api::add_settings_section(
        cx,
        Section {
            id: SECTION,
            title: "Plugins",
            order: 80,
        },
    );
    artoo_shell_api::add_parts(cx, parts);
}

fn parts(_: Rc<dyn Frame>, _: &mut Window, cx: &mut App) -> Parts {
    let view = cx.new(|cx| {
        cx.observe_global::<Loaded>(|_, cx| cx.notify()).detach();
        PluginsSettings
    });
    let sample: Snap = Rc::new(|_, cx| cx.set_global(sample()));
    Parts {
        settings: vec![Rows::new(SECTION, 10, view)],
        snapshots: vec![("plugins-sample", sample)],
        ..Default::default()
    }
}

/// Turns `id` on or off for the next start, and writes it down.
fn flip(id: &str, cx: &mut App) {
    let loaded = cx.global_mut::<Loaded>();
    if !loaded.off.remove(id) {
        loaded.off.insert(id.to_owned());
    }
    let body: String = loaded.off.iter().map(|id| format!("{id}\n")).collect();
    artoo_core::storage::save(OFF, body);
}

/// What a plugin's row says under its name: what became of it, and what the next start does.
fn said(plugin: &Plugin, off_now: bool) -> Option<(String, bool)> {
    match &plugin.state {
        State::Refused(why) => Some((format!("Not loaded: {why}."), true)),
        State::On if off_now => Some((
            "Turned off: it goes when Artoo starts again.".to_owned(),
            false,
        )),
        State::Off if !off_now => Some((
            "Turned on: it loads when Artoo starts again.".to_owned(),
            false,
        )),
        State::On | State::Off => None,
    }
}

struct PluginsSettings;

impl Render for PluginsSettings {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let loaded = cx.global::<Loaded>();
        let folder = crate::load::folder();
        let rows: Vec<_> = (loaded.plugins.iter().enumerate())
            .map(|(i, p)| row(i, p, loaded.off.contains(&p.id())))
            .collect();
        let none = rows.is_empty().then(|| {
            div()
                .px(px(12.))
                .py(px(10.))
                .rounded(px(ROW_RADIUS))
                .bg(theme::base())
                .text_size(px(13.))
                .text_color(theme::muted())
                .child("No plugins are installed.")
        });
        let sdk = (loaded.sdk.clone()).map(|build| {
            div()
                .text_size(px(11.))
                .text_color(theme::faint())
                .child(format!("Built against: {ABI}, SDK build {build}"))
        });
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div().text_size(px(12.)).text_color(theme::muted()).child(
                    "Each in a folder of its own in the plugins folder, loaded as Artoo starts. One turned on or off \
                     here comes or goes when Artoo starts again.",
                ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .children(rows)
                    .children(none),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        outline_button("Open plugins folder")
                            .id("plugins-folder")
                            .on_click(move |_: &ClickEvent, _, cx| {
                                if let Some(dir) = crate::load::folder() {
                                    let _ = std::fs::create_dir_all(&dir);
                                    cx.reveal_path(&dir);
                                }
                            }),
                    )
                    .children(folder.map(|f| {
                        hover::cut("plugins-folder-path", f.display().to_string())
                            .font_family(theme::code_font())
                            .text_size(px(11.))
                            .text_color(theme::faint())
                    })),
            )
            .children(sdk)
    }
}

/// A plugin's row: its name, version and whether it is a developer's; its id and repository; what became of it;
/// its switch (none for a file that is no plugin).
fn row(i: usize, plugin: &Plugin, off_now: bool) -> impl IntoElement {
    let id = plugin.id();
    let (name, version, repository) = match &plugin.stamp {
        Some(s) => (s.name.clone(), s.version.clone(), s.repository.clone()),
        None => (id.clone(), String::new(), String::new()),
    };
    let line = said(plugin, off_now).map(|(text, bad)| {
        div()
            .text_size(px(12.))
            .text_color(if bad {
                theme::warning()
            } else {
                theme::muted()
            })
            .child(text)
    });
    // Where a developer's plugin, or a file that is none, was found.
    let path = (plugin.dev || plugin.stamp.is_none()).then(|| {
        hover::cut(("plugin-path", i), plugin.path.display().to_string())
            .font_family(theme::code_font())
            .text_size(px(11.))
            .text_color(theme::faint())
    });
    let toggle = plugin.stamp.is_some().then(|| {
        let id = id.clone();
        div()
            .id(("plugin-switch", i))
            .child(switch("plugin-on", !off_now))
            .on_click(move |_: &ClickEvent, _, cx| flip(&id, cx))
    });
    div()
        .flex()
        .items_center()
        .gap_4()
        .py(px(10.))
        .border_b_1()
        .border_color(theme::island_border())
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .child(div().font_weight(FontWeight::SEMIBOLD).child(name))
                        .when(!version.is_empty(), |d| {
                            d.child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(theme::faint())
                                    .child(version),
                            )
                        })
                        .when(plugin.dev, |d| d.child(tag("dev", theme::agent()))),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .text_size(px(12.))
                        .text_color(theme::muted())
                        .when(plugin.stamp.is_some(), |d| d.child(SharedString::from(id)))
                        .when(plugin.stamp.is_some() && !repository.is_empty(), |d| {
                            d.child("·")
                        })
                        .children(link_of(i, &repository)),
                )
                .children(line)
                .children(path),
        )
        .children(toggle)
}

/// A repository's link, without its scheme.
fn link_of(i: usize, repository: &str) -> Option<impl IntoElement> {
    (!repository.is_empty()).then(|| {
        let url = repository.to_owned();
        div()
            .id(("plugin-repository", i))
            .cursor_pointer()
            .text_color(theme::accent())
            .hover(|s| s.underline())
            .child(repository.trim_start_matches("https://").to_owned())
            .on_click(move |_: &ClickEvent, _, cx| cx.open_url(&url))
    })
}

/// Four plugins, one of each state, for the section's snapshot (`plugins-sample,settings:plugins`).
fn sample() -> Loaded {
    let stamp = |id: &str, name: &str, version: &str, requires: &[&str]| Stamp {
        abi: ABI.to_owned(),
        build: "SDK".to_owned(),
        id: id.to_owned(),
        name: name.to_owned(),
        version: version.to_owned(),
        repository: format!("https://github.com/someone/artoo-{id}"),
        provides: Vec::new(),
        requires: requires.iter().map(|r| r.to_string()).collect(),
    };
    let plugin = |stamp: Stamp, dev: bool, state: State| Plugin {
        path: PathBuf::from(format!(
            "~/code/artoo-{0}/target/debug/libartoo_{0}.dylib",
            stamp.id
        )),
        dev,
        stamp: Some(stamp),
        state,
    };
    Loaded {
        plugins: vec![
            plugin(stamp("jira", "Jira", "0.4.1", &[]), false, State::On),
            plugin(
                stamp("confluence", "Confluence", "0.2.0", &[]),
                false,
                State::Off,
            ),
            plugin(
                stamp("sprint", "Sprint board", "1.0.0", &["jira.issues@1"]),
                false,
                State::Refused("needs jira.issues@1, which no plugin on offers".to_owned()),
            ),
            plugin(stamp("hello", "Hello", "0.1.0", &[]), true, State::On),
        ],
        off: ["confluence".to_owned()].into(),
        sdk: Some("5C0F3B9E-2A41-3D7C-9B1E-6F2A8C4D0E17".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plugin(state: State) -> Plugin {
        Plugin {
            path: PathBuf::from("x.dylib"),
            dev: false,
            stamp: None,
            state,
        }
    }

    #[test]
    fn a_row_says_what_the_next_start_does() {
        assert_eq!(said(&plugin(State::On), false), None);
        let goes = said(&plugin(State::On), true).unwrap();
        assert!(goes.0.starts_with("Turned off") && !goes.1);
        let comes = said(&plugin(State::Off), false).unwrap();
        assert!(comes.0.starts_with("Turned on"));
        let refused = said(&plugin(State::Refused("why".to_owned())), false).unwrap();
        assert_eq!(refused, ("Not loaded: why.".to_owned(), true));
    }
}
