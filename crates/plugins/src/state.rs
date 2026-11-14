//! The plugins as this run loaded them (#175), and what the next start reads: the ones turned off (`plugins.txt`) and
//! the local list (`plugins-local.txt`). A switch writes at once and takes effect at the next start: a plugin's code,
//! once loaded, stays until the app quits. Its tools for agents alone leave at once (#213). Their page is `page`'s
//! (#176), reached from the row at the foot of the spaces' menu.

use crate::crash::Crash;
use crate::load::{Origin, Plugin, State};
use crate::market::{self, Market};
use crate::page;
use artoo_plugin_api::{ABI, Stamp};
use artoo_shell_api::spaces::{self, Foot, Space, SpaceInfo};
use artoo_shell_api::{Action, add_action};
use gpui::{App, Global, PathPromptOptions};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// The plugins as this run loaded them, and the ids turned off now: what the next start reads.
#[derive(Default)]
pub(crate) struct Loaded {
    pub(crate) plugins: Vec<Plugin>,
    pub(crate) off: BTreeSet<String>,
    /// The local list now: what the next start reads.
    pub(crate) local: Vec<PathBuf>,
    /// This Artoo's SDK build, what a plugin must be built against (`Artoo sdk-build`).
    pub(crate) sdk: Option<String>,
    /// What ended the app last time, until a tab's card has said it.
    pub(crate) crash: Option<Crash>,
    /// The ids turned off and the local list as this run started: a change to either waits for a restart.
    pub(crate) started: (BTreeSet<String>, Vec<PathBuf>),
}

impl Global for Loaded {}

impl Loaded {
    /// Whether a switch was turned or the local list changed since Artoo started: the page's bar says to restart.
    pub(crate) fn changed(&self) -> bool {
        self.off != self.started.0 || self.local != self.started.1
    }
}

/// Keeps what was loaded, offers the row at the menu's foot and its page in each tab, and reads the installed
/// plugins' updates now and once a day.
pub(crate) fn install(plugins: Vec<Plugin>, crash: Option<Crash>, cx: &mut App) {
    artoo_plugin_api::set_loaded(cx, on_now(&plugins));
    let mut trusted = market::trusted();
    // Who a plugin is installed from is trusted already.
    trusted.extend(
        (plugins.iter())
            .filter(|p| p.origin == Origin::Installed)
            .filter_map(|p| p.stamp.as_ref())
            .filter_map(|s| market::github_repo(&s.repository))
            .filter_map(|r| r.split('/').next().map(str::to_lowercase)),
    );
    cx.set_global(Market {
        trusted,
        ..Default::default()
    });
    let (off, local) = (crate::load::off(), crate::load::local());
    cx.set_global(Loaded {
        plugins,
        started: (off.clone(), local.clone()),
        off,
        local,
        sdk: crate::file::sdk_build(),
        crash,
    });
    spaces::add_foot(
        cx,
        Foot {
            info: SpaceInfo {
                space: Space(page::SPACE),
                label: "Plugins",
                icon: "puzzle",
                order: 10,
                go: page::GO,
            },
            waiting: market::waiting,
        },
    );
    add_action(
        cx,
        Action::new(page::GO, page::OpenPlugins)
            .words("Browse plugins")
            .order(90),
    );
    // ⌘F returns to the page's own search, where the app's find would go to a list's filter.
    add_action(
        cx,
        Action::new("plugins.find", page::FindPlugins)
            .key("cmd-f", "Workspace && space == plugins"),
    );
    // Settings keeps no list of plugins; its search leads to the page (P10).
    artoo_shell_api::add_setting(
        cx,
        artoo_shell_api::settings::Named::new(artoo_shell_api::settings::ABOUT, "Plugins")
            .words("plugins extensions install folder"),
    );
    artoo_shell_api::add_parts(cx, page::parts);
    artoo_shell_api::add_parts(cx, crate::card::parts);
    daily_updates(cx);
}

/// The installed plugins' updates read as Artoo starts, then once a day.
fn daily_updates(cx: &mut App) {
    cx.spawn(async move |cx| {
        loop {
            cx.update(|cx| market::check_updates(installed(cx), cx));
            cx.background_executor()
                .timer(Duration::from_secs(24 * 60 * 60))
                .await;
        }
    })
    .detach();
}

/// The installed plugins as their updates are read: id, version, repository.
fn installed(cx: &App) -> Vec<(String, String, String)> {
    (cx.global::<Loaded>().plugins.iter())
        .filter(|p| p.origin == Origin::Installed)
        .filter_map(|p| p.stamp.as_ref())
        .map(|s| (s.id.clone(), s.version.clone(), s.repository.clone()))
        .collect()
}

/// The plugins this run installed, as a report says them: `jira 0.4.1`.
fn on_now(plugins: &[Plugin]) -> Vec<String> {
    (plugins.iter())
        .filter(|p| p.state == State::On)
        .filter_map(|p| p.stamp.as_ref())
        .map(|s| format!("{} {}", s.id, s.version))
        .collect()
}

/// Turns `id` on or off for the next start, and writes it down; its tools for agents leave or come back at once.
pub(crate) fn flip(id: &str, cx: &mut App) {
    let loaded = cx.global_mut::<Loaded>();
    let off = !loaded.off.remove(id);
    if off {
        loaded.off.insert(id.to_owned());
    }
    crate::load::save_off(&loaded.off);
    artoo_plugin_api::tools::set_off(cx, id, off);
}

/// Turns `id` off for the next start (the card's button); its tools for agents leave at once.
pub(crate) fn turn_off(id: &str, cx: &mut App) {
    let loaded = cx.global_mut::<Loaded>();
    loaded.off.insert(id.to_owned());
    crate::load::save_off(&loaded.off);
    artoo_plugin_api::tools::set_off(cx, id, true);
}

/// Asks for a plugin's project folder or library and adds it to the local list, for the next start.
pub(crate) fn add_local(cx: &mut App) {
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
pub(crate) fn remove_local(entry: &Path, cx: &mut App) {
    let loaded = cx.global_mut::<Loaded>();
    loaded.local.retain(|p| p.as_path() != entry);
    crate::load::save_local(&loaded.local);
}

/// The plugins' folder shown in Finder, made first when it is not there yet.
pub(crate) fn open_folder(cx: &mut App) {
    if let Some(dir) = crate::load::folder() {
        let _ = std::fs::create_dir_all(&dir);
        cx.reveal_path(&dir);
    }
}

/// Plugins, one of each state, for the page's snapshots (`plugins`).
pub(crate) fn sample() -> Loaded {
    let stamp = |id: &str, name: &str, version: &str, repository: &str, requires: &[&str]| Stamp {
        abi: ABI.to_owned(),
        build: "SDK".to_owned(),
        id: id.to_owned(),
        name: name.to_owned(),
        version: version.to_owned(),
        repository: repository.to_owned(),
        provides: Vec::new(),
        requires: requires.iter().map(|r| r.to_string()).collect(),
    };
    let hello = PathBuf::from("~/Projects/artoo-hello");
    let plugin = |stamp: Stamp, origin: Origin, state: State| Plugin {
        path: PathBuf::from(format!(
            "~/Projects/artoo-{0}/target/debug/libartoo_{0}.dylib",
            stamp.id
        )),
        origin,
        stamp: Some(stamp),
        state,
    };
    let gh = |repo: &str| format!("https://github.com/{repo}");
    let off: BTreeSet<String> = ["notes".to_owned(), "flaky".to_owned()].into();
    let local = vec![hello.clone(), PathBuf::from("~/Projects/artoo-draft")];
    Loaded {
        plugins: vec![
            plugin(
                stamp(
                    "confluence",
                    "Confluence",
                    "1.2.0",
                    &gh("someone/artoo-confluence"),
                    &[],
                ),
                Origin::Installed,
                State::On,
            ),
            plugin(
                stamp(
                    "timer",
                    "Review timer",
                    "0.3.1",
                    &gh("ember/artoo-review-timer"),
                    &[],
                ),
                Origin::Installed,
                State::On,
            ),
            plugin(
                stamp(
                    "notes",
                    "Private notes",
                    "0.9.0",
                    &gh("lin/artoo-private-notes"),
                    &[],
                ),
                Origin::Installed,
                State::Off,
            ),
            plugin(
                stamp(
                    "sprint",
                    "Sprint board",
                    "1.0.0",
                    &gh("kim/artoo-sprint-board"),
                    &["jira.issues@1"],
                ),
                Origin::Installed,
                State::Refused("needs jira.issues@1, which no plugin on offers".to_owned()),
            ),
            plugin(
                stamp("flaky", "Flaky", "0.9.0", &gh("bo/artoo-flaky"), &[]),
                Origin::Installed,
                State::Struck,
            ),
            plugin(
                stamp("hello", "hello-plugin", "0.1.0", "", &[]),
                Origin::Local(hello),
                State::On,
            ),
            plugin(
                stamp("scratch", "Scratch", "0.1.0", "", &[]),
                Origin::Env,
                State::On,
            ),
            Plugin {
                path: PathBuf::from("~/Projects/artoo-draft"),
                origin: Origin::Local(PathBuf::from("~/Projects/artoo-draft")),
                stamp: None,
                state: State::Refused("nothing has been built in this folder yet".to_owned()),
            },
        ],
        started: (off.clone(), local.clone()),
        off,
        local,
        sdk: Some("5C0F3B9E-2A41-3D7C-9B1E-6F2A8C4D0E17".to_owned()),
        crash: None,
    }
}
