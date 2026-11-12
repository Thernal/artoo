//! Settings → Plugins (#175): each plugin found as Artoo started, what became of it and why, its switch, the plugins'
//! folder, and the local list a plugin being written is tried from. A switch writes `plugins.txt` at once and takes effect at the next start: a plugin's code, once
//! loaded, stays until the app quits.

use crate::crash::Crash;
use crate::load::{Origin, Plugin, State};
use crate::market::{self, Build, Done, Found, Market, Offer};
use artoo_plugin_api::{ABI, Stamp};
use artoo_shell_api::settings::{Rows, Section};
use artoo_shell_api::{Frame, Parts, Snap};
use artoo_ui_kit::controls::{outline_button, switch, tag};
use artoo_ui_kit::{ROW_RADIUS, hover, px, theme};
use gpui::{
    App, AppContext, ClickEvent, Context, FontWeight, Global, IntoElement, PathPromptOptions,
    Render, SharedString, Window, div, prelude::*,
};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// The section's id.
const SECTION: &str = "plugins";

/// The plugins as this run loaded them, and the ids turned off now: what the next start reads.
#[derive(Default)]
pub(crate) struct Loaded {
    pub(crate) plugins: Vec<Plugin>,
    pub(crate) off: BTreeSet<String>,
    /// The local list now: what the next start reads.
    local: Vec<PathBuf>,
    /// This Artoo's SDK build, what a plugin must be built against (`Artoo sdk-build`).
    sdk: Option<String>,
    /// What ended the app last time, until a tab's card has said it.
    pub(crate) crash: Option<Crash>,
    /// Only what GitHub offers: the snapshot of a part too long to fit under the rest.
    offers_only: bool,
}

impl Global for Loaded {}

/// Keeps what was loaded and offers the section, after Keyboard, and its rows in each tab.
pub(crate) fn install(plugins: Vec<Plugin>, crash: Option<Crash>, cx: &mut App) {
    artoo_plugin_api::set_loaded(cx, on_now(&plugins));
    cx.set_global(Market::default());
    cx.set_global(Loaded {
        plugins,
        off: crate::load::off(),
        local: crate::load::local(),
        sdk: crate::file::sdk_build(),
        crash,
        offers_only: false,
    });
    artoo_shell_api::add_settings_section(
        cx,
        Section {
            id: SECTION,
            title: "Plugins",
            order: 80,
        },
    );
    artoo_shell_api::add_setting(
        cx,
        artoo_shell_api::settings::Named::new(SECTION, "Plugins")
            .words("extensions install folder"),
    );
    artoo_shell_api::add_parts(cx, parts);
    artoo_shell_api::add_parts(cx, crate::card::parts);
}

/// The plugins this run installed, as a report says them: `jira 0.4.1`.
fn on_now(plugins: &[Plugin]) -> Vec<String> {
    (plugins.iter())
        .filter(|p| p.state == State::On)
        .filter_map(|p| p.stamp.as_ref())
        .map(|s| format!("{} {}", s.id, s.version))
        .collect()
}

fn parts(_: Rc<dyn Frame>, _: &mut Window, cx: &mut App) -> Parts {
    let view = cx.new(|cx| {
        cx.observe_global::<Loaded>(|_, cx| cx.notify()).detach();
        cx.observe_global::<Market>(|_, cx| cx.notify()).detach();
        PluginsSettings
    });
    let sample: Snap = Rc::new(|_, cx| {
        cx.set_global(sample());
        cx.set_global(market::sample());
    });
    Parts {
        settings: vec![Rows::new(SECTION, 10, view).opened(market::opened)],
        snapshots: vec![
            ("plugins-sample", sample),
            (
                "plugins-offers",
                Rc::new(|_, cx| {
                    cx.set_global(Loaded {
                        offers_only: true,
                        ..self::sample()
                    });
                    cx.set_global(market::sample());
                }),
            ),
            ("plugin-install:", Rc::new(market::install_found)),
        ],
        ..Default::default()
    }
}

/// Turns `id` on or off for the next start, and writes it down.
fn flip(id: &str, cx: &mut App) {
    let loaded = cx.global_mut::<Loaded>();
    if !loaded.off.remove(id) {
        loaded.off.insert(id.to_owned());
    }
    crate::load::save_off(&loaded.off);
}

/// Turns `id` off for the next start (the card's button).
pub(crate) fn turn_off(id: &str, cx: &mut App) {
    let loaded = cx.global_mut::<Loaded>();
    loaded.off.insert(id.to_owned());
    crate::load::save_off(&loaded.off);
}

/// Asks for a plugin's project folder or library and adds it to the local list, for the next start.
fn add_local(cx: &mut App) {
    let picked = cx.prompt_for_paths(PathPromptOptions {
        files: true,
        directories: true,
        multiple: false,
        prompt: Some("Add".into()),
    });
    cx.spawn(async move |cx| {
        let Ok(Ok(Some(paths))) = picked.await else {
            return;
        };
        let Some(path) = paths.into_iter().next() else {
            return;
        };
        cx.update(|cx| {
            let loaded = cx.global_mut::<Loaded>();
            if !loaded.local.contains(&path) {
                loaded.local.push(path);
                crate::load::save_local(&loaded.local);
            }
        });
    })
    .detach();
}

/// Takes an entry off the local list; what it stands for stays where it is.
fn remove_local(entry: &Path, cx: &mut App) {
    let loaded = cx.global_mut::<Loaded>();
    loaded.local.retain(|p| p.as_path() != entry);
    crate::load::save_local(&loaded.local);
}

/// What a plugin's row says under its name: what became of it, and what the next start does.
fn said(plugin: &Plugin, off_now: bool) -> Option<(String, bool)> {
    match &plugin.state {
        State::Refused(why) => Some((format!("Not loaded: {why}."), true)),
        State::On if off_now => Some((
            "Turned off: it goes when Artoo starts again.".to_owned(),
            false,
        )),
        State::Off | State::Struck if !off_now => Some((
            "Turned on: it loads when Artoo starts again.".to_owned(),
            false,
        )),
        State::Struck => Some((
            "Turned off: it ended Artoo twice as it started.".to_owned(),
            true,
        )),
        State::On | State::Off => None,
    }
}

struct PluginsSettings;

impl Render for PluginsSettings {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let loaded = cx.global::<Loaded>();
        let market_now = cx.global::<Market>();
        let folder = crate::load::folder();
        let row_of = |(i, p): (usize, &Plugin)| {
            let unlisted =
                matches!(&p.origin, Origin::Local(entry) if !loaded.local.contains(entry));
            row(
                i,
                p,
                loaded.off.contains(&p.id()),
                market_now.done.get(&p.id()),
                unlisted,
            )
            .into_any_element()
        };
        let rows: Vec<_> = (loaded.plugins.iter().enumerate())
            .filter(|(_, p)| !p.dev())
            .map(row_of)
            .collect();
        let being_written: Vec<_> = (loaded.plugins.iter().enumerate())
            .filter(|(_, p)| p.dev())
            .map(row_of)
            .collect();
        let local = local_group(loaded, being_written);
        let available = available(market_now, &loaded.plugins);
        if loaded.offers_only {
            return div().flex().flex_col().child(available).into_any_element();
        }
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
            .child(local)
            .child(available)
            .into_any_element()
    }
}

/// The plugins being written: the ones found as Artoo started, what was added since, and how to try one with no
/// repository.
fn local_group(loaded: &Loaded, rows: Vec<gpui::AnyElement>) -> impl IntoElement + use<> {
    let waiting = (loaded.local.iter().enumerate())
        .filter(|(_, entry)| {
            !(loaded.plugins.iter()).any(|p| matches!(&p.origin, Origin::Local(e) if e == *entry))
        })
        .map(|(i, entry)| added_row(i, entry).into_any_element());
    let sdk = (loaded.sdk.clone()).map(|build| {
        div()
            .text_size(px(11.))
            .text_color(theme::faint())
            .child(format!("Built against: {ABI}, SDK build {build}"))
    });
    div()
        .flex()
        .flex_col()
        .gap_3()
        .pt_2()
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .flex_1()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Local"),
                )
                .child(
                    outline_button("Add…")
                        .id("plugins-add-local")
                        .on_click(|_: &ClickEvent, _, cx| add_local(cx)),
                )
                .child(
                    outline_button("Restart Artoo")
                        .id("plugins-restart")
                        .on_click(|_: &ClickEvent, _, cx| cx.restart()),
                ),
        )
        .child(muted(
            "A plugin you are writing, with no repository or release. Add its project folder or its library; after \
             each build, restart Artoo to run it.",
        ))
        .child(div().flex().flex_col().children(rows).children(waiting))
        .children(sdk)
}

/// An entry added since Artoo started.
fn added_row(i: usize, entry: &Path) -> impl IntoElement + use<> {
    let name = (entry.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let key = entry.to_path_buf();
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
                        .child(tag("local", theme::agent())),
                )
                .child(muted("Added: it loads when Artoo starts again."))
                .child(
                    hover::cut(("plugin-added-path", i), entry.display().to_string())
                        .font_family(theme::code_font())
                        .text_size(px(11.))
                        .text_color(theme::faint()),
                ),
        )
        .child(
            outline_button("Remove")
                .id(("plugin-added-remove", i))
                .on_click(move |_: &ClickEvent, _, cx| remove_local(&key, cx)),
        )
}

/// The section's second half: what GitHub offers, the warning over it.
fn available(market: &Market, installed: &[Plugin]) -> impl IntoElement + use<> {
    let body: Vec<gpui::AnyElement> = match &market.found {
        Found::NotAsked | Found::Asking => vec![muted("Looking for plugins…").into_any_element()],
        Found::Failed(why) => {
            vec![muted(format!("Could not look for plugins: {why}.")).into_any_element()]
        }
        Found::Got(offers) if offers.is_empty() => {
            vec![muted("No plugins were found.").into_any_element()]
        }
        Found::Got(offers) => (offers.iter().enumerate())
            .map(|(i, o)| offer_row(i, o, market, installed).into_any_element())
            .collect(),
    };
    div()
        .flex()
        .flex_col()
        .gap_3()
        .pt_2()
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .flex_1()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Available"),
                )
                .child(
                    outline_button("Look again")
                        .id("plugins-look-again")
                        .on_click(|_: &ClickEvent, _, cx| market::ask(cx)),
                ),
        )
        .child(muted(
            "A plugin runs with all of Artoo's rights, over your files and your accounts. Install one only if you \
             trust who made it.",
        ))
        .child(div().flex().flex_col().children(body))
}

fn muted(text: impl Into<SharedString>) -> gpui::Div {
    div()
        .text_size(px(12.))
        .text_color(theme::muted())
        .child(text.into())
}

/// A repository's row: who made it and what it says it is, and what can be done with it.
fn offer_row(
    i: usize,
    offer: &Offer,
    market: &Market,
    installed: &[Plugin],
) -> impl IntoElement + use<> {
    let url = offer.url.clone();
    let right = action(i, offer, market, installed);
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
                        .child(
                            div()
                                .id(("offer-repository", i))
                                .cursor_pointer()
                                .font_weight(FontWeight::SEMIBOLD)
                                .hover(|s| s.underline())
                                .child(offer.repo.clone())
                                .on_click(move |_: &ClickEvent, _, cx| cx.open_url(&url)),
                        )
                        .child(
                            div()
                                .text_size(px(12.))
                                .text_color(theme::faint())
                                .child(format!("★ {}", offer.stars)),
                        ),
                )
                .when(!offer.about.is_empty(), |d| {
                    d.child(muted(offer.about.clone()))
                }),
        )
        .child(div().flex_none().child(right))
}

/// What can be done with an offer now: a button, or why not.
fn action(i: usize, offer: &Offer, market: &Market, installed: &[Plugin]) -> gpui::AnyElement {
    let Build::Ready { id, version, zip } = &offer.build else {
        let text = match &offer.build {
            Build::Other(artoos) => format!("Built for Artoo {}", artoos.join(", ")),
            _ => "No release yet".to_owned(),
        };
        return muted(text).into_any_element();
    };
    match market.done.get(id) {
        Some(Done::Installing) => return muted("Installing…").into_any_element(),
        Some(Done::Installed) => return muted("Comes when Artoo starts again").into_any_element(),
        Some(Done::Removed) => return muted("Goes when Artoo starts again").into_any_element(),
        Some(Done::Failed(why)) => {
            return div()
                .text_size(px(12.))
                .text_color(theme::warning())
                .child(format!("Not installed: {why}"))
                .into_any_element();
        }
        None => {}
    }
    let have = (installed.iter())
        .filter(|p| !p.dev())
        .filter_map(|p| p.stamp.as_ref())
        .find(|s| s.id == *id);
    let label = match have {
        // Another repository's plugin of the same id: this build is not its update.
        Some(stamp) if !market::same_repository(&stamp.repository, &offer.url) => {
            return muted(format!("Another plugin is installed as {id}")).into_any_element();
        }
        Some(stamp) if market::newer(version, &stamp.version) => format!("Update to {version}"),
        Some(_) => return muted("Installed").into_any_element(),
        None => "Install".to_owned(),
    };
    let (key, url, repo) = (id.clone(), zip.clone(), offer.repo.clone());
    // A first install asks: an update is of what the user already trusted.
    if have.is_none() && market.asking.as_deref() != Some(offer.repo.as_str()) {
        return outline_button("Install")
            .id(("offer-install", i))
            .on_click(move |_: &ClickEvent, _, cx| {
                cx.global_mut::<Market>().asking = Some(repo.clone());
            })
            .into_any_element();
    }
    let word = if have.is_none() {
        "Install anyway".to_owned()
    } else {
        label
    };
    div()
        .flex()
        .items_center()
        .gap(px(6.))
        .when(have.is_none(), |d| {
            d.child(
                outline_button("Cancel")
                    .id(("offer-cancel", i))
                    .on_click(|_: &ClickEvent, _, cx| cx.global_mut::<Market>().asking = None),
            )
        })
        .child(
            outline_button(&word)
                .id(("offer-go", i))
                .on_click(move |_: &ClickEvent, _, cx| {
                    market::install(key.clone(), url.clone(), cx)
                }),
        )
        .into_any_element()
}

/// A plugin's row: its name, version and whether it is a developer's; its id and repository; what became of it;
/// its switch (none for a file that is no plugin).
fn row(
    i: usize,
    plugin: &Plugin,
    off_now: bool,
    done: Option<&Done>,
    unlisted: bool,
) -> impl IntoElement {
    let id = plugin.id();
    let (name, version, repository) = match &plugin.stamp {
        Some(s) => (s.name.clone(), s.version.clone(), s.repository.clone()),
        None => (id.clone(), String::new(), String::new()),
    };
    let removed = unlisted || done == Some(&Done::Removed);
    let said_now = if removed {
        Some((
            "Removed: it goes when Artoo starts again.".to_owned(),
            false,
        ))
    } else {
        said(plugin, off_now)
    };
    let line = said_now.map(|(text, bad)| {
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
    let path = (plugin.dev() || plugin.stamp.is_none()).then(|| {
        hover::cut(("plugin-path", i), plugin.path.display().to_string())
            .font_family(theme::code_font())
            .text_size(px(11.))
            .text_color(theme::faint())
    });
    // An installed one can go, and a local one off the list; one named by `ARTOO_PLUGIN_DEV` is that variable's.
    let remove = (plugin.origin != Origin::Env && !removed).then(|| {
        let origin = plugin.origin.clone();
        let folder = plugin
            .path
            .parent()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned());
        outline_button("Remove")
            .id(("plugin-remove", i))
            .on_click(move |_: &ClickEvent, _, cx| match (&origin, &folder) {
                (Origin::Local(entry), _) => remove_local(entry, cx),
                (_, Some(id)) => market::remove(id, cx),
                _ => {}
            })
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
                        .when(plugin.origin == Origin::Env, |d| {
                            d.child(tag("dev", theme::agent()))
                        })
                        .when(matches!(plugin.origin, Origin::Local(_)), |d| {
                            d.child(tag("local", theme::agent()))
                        }),
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
        .children(remove)
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

/// Plugins, one of each state, for the section's snapshot (`plugins-sample,settings:plugins`).
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
    let hello = PathBuf::from("~/code/artoo-hello");
    let plugin = |stamp: Stamp, origin: Origin, state: State| Plugin {
        path: PathBuf::from(format!(
            "~/code/artoo-{0}/target/debug/libartoo_{0}.dylib",
            stamp.id
        )),
        origin,
        stamp: Some(stamp),
        state,
    };
    Loaded {
        plugins: vec![
            plugin(
                stamp("jira", "Jira", "0.4.1", &[]),
                Origin::Installed,
                State::On,
            ),
            plugin(
                stamp("confluence", "Confluence", "0.2.0", &[]),
                Origin::Installed,
                State::Off,
            ),
            plugin(
                stamp("sprint", "Sprint board", "1.0.0", &["jira.issues@1"]),
                Origin::Installed,
                State::Refused("needs jira.issues@1, which no plugin on offers".to_owned()),
            ),
            plugin(
                stamp("flaky", "Flaky", "0.9.0", &[]),
                Origin::Installed,
                State::Struck,
            ),
            plugin(
                stamp("hello", "Hello", "0.1.0", &[]),
                Origin::Local(hello.clone()),
                State::On,
            ),
            plugin(
                stamp("scratch", "Scratch", "0.1.0", &[]),
                Origin::Env,
                State::On,
            ),
            Plugin {
                path: PathBuf::from("~/code/artoo-draft"),
                origin: Origin::Local(PathBuf::from("~/code/artoo-draft")),
                stamp: None,
                state: State::Refused("nothing has been built in this folder yet".to_owned()),
            },
        ],
        off: ["confluence".to_owned(), "flaky".to_owned()].into(),
        local: vec![
            hello,
            PathBuf::from("~/code/artoo-draft"),
            PathBuf::from("~/code/artoo-notes"),
        ],
        sdk: Some("5C0F3B9E-2A41-3D7C-9B1E-6F2A8C4D0E17".to_owned()),
        crash: None,
        offers_only: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plugin(state: State) -> Plugin {
        Plugin {
            path: PathBuf::from("x.dylib"),
            origin: Origin::Installed,
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
