//! Plugins found, checked, ordered and installed (#175), once, as the app starts, after every feature.
//!
//! Where: a folder of its own under `<data>/plugins/` (`plugins/jira/libjira.dylib`), and for a plugin being written,
//! which wins over an installed one of the same id, what `ARTOO_PLUGIN_DEV` names — a library or a folder of them,
//! several apart as `PATH` is — and the local list of the plugins' page (`plugins-local.txt`). A plugin's stamp is read first: one built for another SDK is refused with words,
//! before anything of it runs. Then the ones turned off (`plugins.txt`, the plugins' page) are left out, and the
//! rest installed each after what it requires: loaded with every symbol bound (a missing one refuses it, rather than
//! stopping the app at its first call), installed inside `catch_unwind`. A plugin refused or off takes the plugins
//! needing it with it.

use crate::crash::{self, Crash, Starting};
use crate::file;
use artoo_plugin_api::{ABI, Stamp};
use gpui::App;
use std::collections::{BTreeMap, BTreeSet};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

/// The file of the plugins turned off: an id a line.
pub(crate) const OFF: &str = "plugins.txt";

/// The file of the local list: a library or a folder a line.
const LOCAL: &str = "plugins-local.txt";

/// Where a plugin was found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Origin {
    /// In the plugins' folder.
    Installed,
    /// Named by `ARTOO_PLUGIN_DEV`.
    Env,
    /// Through this entry of the local list.
    Local(PathBuf),
}

/// What became of a plugin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum State {
    /// Installed.
    On,
    /// Turned off on the plugins' page: not loaded.
    Off,
    /// Off because it ended two starts as it installed.
    Struck,
    /// Not installed, and why: another SDK, a missing contract, a failed load, a panic in its install.
    Refused(String),
}

/// A plugin as found.
#[derive(Clone, Debug)]
pub(crate) struct Plugin {
    pub path: PathBuf,
    pub origin: Origin,
    /// Its stamp, when it has one.
    pub stamp: Option<Stamp>,
    pub state: State,
}

impl Plugin {
    /// Whether it is one being written, rather than installed.
    pub(crate) fn dev(&self) -> bool {
        self.origin != Origin::Installed
    }

    /// Its id, or its file's name when it has no stamp.
    pub(crate) fn id(&self) -> String {
        match &self.stamp {
            Some(stamp) => stamp.id.clone(),
            None => (self.path.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
        }
    }
}

/// The plugins' folder: `<data>/plugins`.
pub(crate) fn folder() -> Option<PathBuf> {
    Some(artoo_core::platform::data_dir()?.join("plugins"))
}

/// The libraries of this system in `dir`, by name.
fn libraries_in(dir: &Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = (std::fs::read_dir(dir).into_iter().flatten().flatten())
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .is_some_and(|e| e == std::env::consts::DLL_EXTENSION)
        })
        .collect();
    found.sort();
    found
}

/// What was built in a plugin's own project: the newest of each library in `target/debug`, `target/release` and, as the
/// template builds, the same under `target/<triple>`.
fn built_in(project: &Path) -> Vec<PathBuf> {
    let target = project.join("target");
    let triples = (std::fs::read_dir(&target).into_iter().flatten().flatten())
        .map(|e| e.path())
        .filter(|p| p.is_dir());
    let mut newest: BTreeMap<std::ffi::OsString, (std::time::SystemTime, PathBuf)> =
        BTreeMap::new();
    for root in std::iter::once(target.clone()).chain(triples) {
        for profile in ["debug", "release"] {
            for library in libraries_in(&root.join(profile)) {
                let Some(name) = library.file_name().map(|n| n.to_owned()) else {
                    continue;
                };
                let when = (library.metadata().and_then(|m| m.modified()))
                    .unwrap_or(std::time::UNIX_EPOCH);
                if newest.get(&name).is_none_or(|(have, _)| when > *have) {
                    newest.insert(name, (when, library));
                }
            }
        }
    }
    newest.into_values().map(|(_, path)| path).collect()
}

/// What an entry of the local list stands for: a library, the libraries in a folder, or what a plugin's own project
/// built. The entry itself when there is nothing there yet, so its row says so.
fn local_libraries(entry: &Path) -> Vec<PathBuf> {
    let found = if !entry.is_dir() {
        Vec::new()
    } else {
        let here = libraries_in(entry);
        if here.is_empty() {
            built_in(entry)
        } else {
            here
        }
    };
    if found.is_empty() {
        vec![entry.to_owned()]
    } else {
        found
    }
}

/// Every plugin's file: a developer's first (`ARTOO_PLUGIN_DEV`, then the local list), then each folder of the plugins'
/// folder. A test or a snapshot reads no plugins' folder: it is the user's.
fn files(
    dev: Option<std::ffi::OsString>,
    local: &[PathBuf],
    folder: Option<PathBuf>,
) -> Vec<(PathBuf, Origin)> {
    let mut out = Vec::new();
    for path in dev.iter().flat_map(std::env::split_paths) {
        if path.is_dir() {
            out.extend(libraries_in(&path).into_iter().map(|p| (p, Origin::Env)));
        } else {
            out.push((path, Origin::Env));
        }
    }
    for entry in local {
        let found = local_libraries(entry).into_iter();
        out.extend(found.map(|p| (p, Origin::Local(entry.clone()))));
    }
    let plugins = (folder.iter())
        .flat_map(|f| std::fs::read_dir(f).into_iter().flatten().flatten())
        .map(|e| e.path())
        .filter(|p| p.is_dir());
    let mut theirs: Vec<PathBuf> = plugins.flat_map(|dir| libraries_in(&dir)).collect();
    theirs.sort();
    out.extend(theirs.into_iter().map(|p| (p, Origin::Installed)));
    out
}

/// The local list.
pub(crate) fn local() -> Vec<PathBuf> {
    (artoo_core::storage::read(LOCAL).lines())
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(PathBuf::from)
        .collect()
}

/// Writes down the local list.
pub(crate) fn save_local(entries: &[PathBuf]) {
    let body: String = entries
        .iter()
        .map(|p| format!("{}\n", p.display()))
        .collect();
    artoo_core::storage::save(LOCAL, body);
}

/// The ids turned off.
pub(crate) fn off() -> BTreeSet<String> {
    let text = artoo_core::storage::read(OFF);
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Why a stamp is not this SDK's, if it is not.
pub(crate) fn foreign(stamp: &Stamp, ours: Option<&str>) -> Option<String> {
    if stamp.abi != ABI {
        return Some(format!(
            "built for another Artoo ({}); this one is {ABI}",
            stamp.abi
        ));
    }
    match ours {
        None => Some("this Artoo's SDK could not be read, so no plugin is checked".to_owned()),
        Some(ours) if stamp.build != ours => Some(format!(
            "built against another build of Artoo's SDK ({}); this one is {ours}",
            stamp.build
        )),
        Some(_) => None,
    }
}

/// The plugins as found and checked: each stamp read and compared with this SDK's build (`ours`), the second of an id
/// refused, the ones in `off` off. Nothing is loaded.
fn checked(
    files: Vec<(PathBuf, Origin)>,
    ours: Option<&str>,
    off: &BTreeSet<String>,
) -> Vec<Plugin> {
    let mut out: Vec<Plugin> = Vec::new();
    for (path, origin) in files {
        let read = if path.is_dir() {
            Err(String::new())
        } else {
            file::stamp_of(&path)
        };
        let (stamp, state) = match read {
            Err(why) if why.is_empty() => (
                None,
                State::Refused("nothing has been built in this folder yet".to_owned()),
            ),
            Err(why) => (None, State::Refused(format!("this file {why}"))),
            Ok(stamp) => {
                let state = if let Some(first) = out.iter().find(|p| p.id() == stamp.id) {
                    State::Refused(format!(
                        "{} is loaded from {} already",
                        stamp.id,
                        first.path.display()
                    ))
                } else if let Some(why) = foreign(&stamp, ours) {
                    State::Refused(why)
                } else if off.contains(&stamp.id) {
                    State::Off
                } else {
                    State::On
                };
                (Some(stamp), state)
            }
        };
        out.push(Plugin {
            path,
            origin,
            stamp,
            state,
        });
    }
    out
}

/// The order the plugins still on are installed in, each after the plugins offering what it requires; one requiring
/// what none offers is refused, saying what it lacks — and so, in turn, are those that needed it.
fn ordered(plugins: &mut [Plugin]) -> Vec<usize> {
    let mut offered: BTreeSet<String> = BTreeSet::new();
    let mut order = Vec::new();
    loop {
        let next = (0..plugins.len()).find(|&i| {
            let p = &plugins[i];
            p.state == State::On
                && !order.contains(&i)
                && (p.stamp.iter()).all(|s| s.requires.iter().all(|r| offered.contains(r)))
        });
        let Some(i) = next else { break };
        order.push(i);
        offered.extend(plugins[i].stamp.iter().flat_map(|s| s.provides.clone()));
    }
    for (i, plugin) in plugins.iter_mut().enumerate() {
        if plugin.state == State::On && !order.contains(&i) {
            let missing: Vec<String> = (plugin.stamp.iter())
                .flat_map(|s| s.requires.iter())
                .filter(|r| !offered.contains(*r))
                .cloned()
                .collect();
            plugin.state = State::Refused(format!(
                "needs {}, which no plugin on offers",
                missing.join(", ")
            ));
        }
    }
    order
}

/// What a panic said.
fn said(panic: &(dyn std::any::Any + Send)) -> String {
    (panic.downcast_ref::<&str>().map(|s| s.to_string()))
        .or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "a panic".to_owned())
}

/// Loads `path` with every symbol bound now, and gives its install. The library stays loaded while the app runs.
fn open(path: &Path) -> Result<fn(&mut App), String> {
    // SAFETY: the stamp says it was built against this SDK, so what it runs as it loads is this build's.
    let library = unsafe { artoo_core::platform::libraries::open(path) };
    let library = library.map_err(|e| format!("did not load: {e}"))?;
    let library: &'static libloading::Library = Box::leak(Box::new(library));
    // SAFETY: `plugin!` writes `artoo_plugin_install` with this signature, in this SDK's ABI.
    let install = unsafe { library.get::<fn(&mut App)>(b"artoo_plugin_install") }
        .map_err(|e| format!("has no install: {e}"))?;
    Ok(*install)
}

/// Writes down the ids turned off.
pub(crate) fn save_off(off: &BTreeSet<String>) {
    let body: String = off.iter().map(|id| format!("{id}\n")).collect();
    artoo_core::storage::save(OFF, body);
}

/// Finds, checks, orders and installs the plugins (the app's `install`, after every feature); with what ended the
/// app last time, if a plugin did.
pub(crate) fn load(cx: &mut App) -> (Vec<Plugin>, Option<Crash>) {
    let quiet = artoo_core::platform::sandboxed() || artoo_core::storage::snapshot_run();
    let last = crash::take(quiet);
    if !quiet {
        crate::market::apply_staged();
    }
    let folder = if quiet { None } else { folder() };
    let listed = if quiet { Vec::new() } else { local() };
    let files = files(std::env::var_os("ARTOO_PLUGIN_DEV"), &listed, folder);
    if files.is_empty() {
        return (Vec::new(), last);
    }
    let ours = file::sdk_build();
    let (mut starting, struck) = Starting::read();
    let mut off = off();
    if !struck.is_empty() {
        off.extend(struck.iter().cloned());
        save_off(&off);
    }
    let mut plugins = checked(files, ours.as_deref(), &off);
    for plugin in &mut plugins {
        if plugin.state == State::Off && struck.contains(&plugin.id()) {
            plugin.state = State::Struck;
        }
    }
    let mut order = ordered(&mut plugins);
    if !order.is_empty() {
        crash::watch();
    }
    while let Some(i) = (!order.is_empty()).then(|| order.remove(0)) {
        let (id, path) = (plugins[i].id(), plugins[i].path.clone());
        starting.begin(&id);
        let installed = open(&path).and_then(|install| {
            if let Some(range) = file::loaded_range(&path, install as *const ()) {
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                crash::add_image(&id, &name, range);
            }
            // What it offers as it installs is known to be its own (the notices' sources, #196).
            artoo_shell_api::notices::guest(cx, Some(id.clone()));
            artoo_plugin_api::tools::installing(cx, Some(&id));
            let installed = catch_unwind(AssertUnwindSafe(|| install(cx)))
                .map_err(|panic| format!("stopped as it installed: {}", said(&*panic)));
            artoo_shell_api::notices::guest(cx, None);
            artoo_plugin_api::tools::installing(cx, None);
            installed
        });
        starting.done(&id);
        if let Err(why) = installed {
            // Caught: the app goes on, and the panic hook's report is no one's end.
            crash::forget();
            // Its tools are not given to agents (#213).
            artoo_plugin_api::tools::drop_of(cx, &id);
            plugins[i].state = State::Refused(why);
            // What it offered is not offered: the rest are ordered again without it.
            order = ordered(&mut plugins)
                .into_iter()
                .filter(|j| order.contains(j))
                .collect();
        }
    }
    for plugin in &plugins {
        let state = match &plugin.state {
            State::On => "on".to_owned(),
            State::Off => "off".to_owned(),
            State::Struck => "off: it ended two starts".to_owned(),
            State::Refused(why) => format!("refused: {why}"),
        };
        artoo_core::applog::line(&format!(
            "plugin {} ({}): {state}",
            plugin.id(),
            plugin.path.display()
        ));
    }
    (plugins, last)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stamp(id: &str, provides: &[&str], requires: &[&str]) -> Stamp {
        let list = |l: &[&str]| l.iter().map(|s| s.to_string()).collect();
        Stamp {
            abi: ABI.to_owned(),
            build: "SDK".to_owned(),
            id: id.to_owned(),
            name: id.to_owned(),
            version: "1.0.0".to_owned(),
            repository: String::new(),
            provides: list(provides),
            requires: list(requires),
        }
    }

    fn on(stamp: Stamp) -> Plugin {
        Plugin {
            path: PathBuf::from(format!("{}.dylib", stamp.id)),
            origin: Origin::Installed,
            stamp: Some(stamp),
            state: State::On,
        }
    }

    #[test]
    fn a_plugin_installs_after_what_it_requires_and_not_without_it() {
        let mut plugins = vec![
            on(stamp("greeter", &[], &["hello.api@1"])),
            on(stamp("hello", &["hello.api@1"], &[])),
            on(stamp("lonely", &[], &["nobody.api@1"])),
            on(stamp("fan", &[], &["greeter.api@1"])),
        ];
        assert_eq!(ordered(&mut plugins), [1, 0]);
        assert_eq!(
            plugins[2].state,
            State::Refused("needs nobody.api@1, which no plugin on offers".to_owned())
        );
        assert!(matches!(&plugins[3].state, State::Refused(why) if why.contains("greeter.api@1")));
    }

    #[test]
    fn one_off_takes_the_plugins_needing_it_with_it() {
        let mut plugins = vec![
            on(stamp("hello", &["hello.api@1"], &[])),
            on(stamp("greeter", &[], &["hello.api@1"])),
        ];
        plugins[0].state = State::Off;
        assert_eq!(ordered(&mut plugins), Vec::<usize>::new());
        assert!(
            matches!(&plugins[1].state, State::Refused(why) if why.starts_with("needs hello.api@1"))
        );
    }

    #[test]
    fn a_stamp_of_another_sdk_is_refused_with_what_it_was_built_for() {
        let ours = stamp("x", &[], &[]);
        assert_eq!(foreign(&ours, Some("SDK")), None);
        let other_build = foreign(&ours, Some("OTHER")).unwrap();
        assert!(other_build.starts_with("built against another build of Artoo's SDK (SDK)"));
        let mut old = ours.clone();
        old.abi = "artoo-plugin-sdk/1.2.0 rustc 1.90.0".to_owned();
        assert!(
            foreign(&old, Some("SDK"))
                .unwrap()
                .starts_with("built for another Artoo (artoo-plugin-sdk/1.2.0")
        );
        assert!(foreign(&ours, None).is_some());
    }

    #[test]
    fn a_developers_plugins_come_first_and_each_installed_one_is_in_its_folder() {
        let dir = tempfile::tempdir().unwrap();
        let ext = std::env::consts::DLL_EXTENSION;
        let installed = dir.path().join("plugins");
        for id in ["b", "a"] {
            std::fs::create_dir_all(installed.join(id)).unwrap();
            std::fs::write(installed.join(id).join(format!("lib{id}.{ext}")), "").unwrap();
            std::fs::write(installed.join(id).join("README.md"), "").unwrap();
        }
        let dev = dir.path().join("dev");
        std::fs::create_dir_all(&dev).unwrap();
        std::fs::write(dev.join(format!("libc.{ext}")), "").unwrap();
        let found = files(
            Some(dev.clone().into_os_string()),
            &[],
            Some(installed.clone()),
        );
        let names: Vec<(String, bool)> = (found.iter())
            .map(|(p, origin)| {
                let name = p.file_name().unwrap().to_string_lossy().into_owned();
                (name, *origin == Origin::Env)
            })
            .collect();
        assert_eq!(
            names,
            [
                (format!("libc.{ext}"), true),
                (format!("liba.{ext}"), false),
                (format!("libb.{ext}"), false)
            ]
        );
        // None of them is a library: each refused, by its file's name, with why.
        let checked = checked(found, Some("SDK"), &BTreeSet::new());
        assert_eq!(checked[1].id(), format!("liba.{ext}"));
        assert!(
            matches!(&checked[1].state, State::Refused(why) if why.starts_with("this file is not a library"))
        );
    }

    #[test]
    fn a_local_entry_is_a_library_a_folder_or_a_project_and_says_so_when_nothing_is_there() {
        let dir = tempfile::tempdir().unwrap();
        let ext = std::env::consts::DLL_EXTENSION;
        let write = |path: PathBuf| {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, "").unwrap();
            path
        };
        let library = write(dir.path().join(format!("alone/liba.{ext}")));
        let alone = std::slice::from_ref(&library);
        assert_eq!(local_libraries(&library), alone);
        assert_eq!(local_libraries(library.parent().unwrap()), alone);

        let project = dir.path().join("project");
        let old = write(project.join(format!("target/debug/libp.{ext}")));
        let new = write(project.join(format!("target/aarch64-apple-darwin/release/libp.{ext}")));
        let past = std::time::SystemTime::now() - std::time::Duration::from_secs(600);
        let file = std::fs::File::options().write(true).open(&old).unwrap();
        file.set_modified(past).unwrap();
        assert_eq!(local_libraries(&project), [new]);

        let empty = dir.path().join("empty");
        std::fs::create_dir_all(&empty).unwrap();
        let plugins = checked(
            files(None, std::slice::from_ref(&empty), None),
            Some("SDK"),
            &BTreeSet::new(),
        );
        assert_eq!(plugins[0].origin, Origin::Local(empty));
        assert!(
            matches!(&plugins[0].state, State::Refused(why) if why.starts_with("nothing has been built"))
        );
    }
}
