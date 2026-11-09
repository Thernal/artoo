//! Plugins found and installed from inside the app (#176): GitHub's repositories with the topic `artoo-plugin`, each
//! release's asset `<id>-<artoo-version>-<target>.zip` the build for one Artoo. Only a build for this Artoo is
//! installable, and its stamp is checked before it is kept.
//!
//! What is installed, updated or removed is staged and applied at the next start: plugins load only as the app starts,
//! and a library that is loaded can't be replaced on Windows.

use crate::{file, load};
use artoo_core::{applog, platform, storage};
use gpui::{App, Global};
use semver::Version;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const SEARCH: &str = "https://api.github.com/search/repositories?q=topic:artoo-plugin&sort=stars&order=desc&per_page=20";
const ARTOO: &str = env!("CARGO_PKG_VERSION");
const TARGET: &str = env!("ARTOO_TARGET");
/// The largest library taken out of an asset.
const MOST: u64 = 512 * 1024 * 1024;

#[derive(Deserialize)]
struct Search {
    items: Vec<Repo>,
}

#[derive(Deserialize)]
struct Repo {
    full_name: String,
    html_url: String,
    description: Option<String>,
    stargazers_count: u64,
    owner: Owner,
}

#[derive(Deserialize)]
struct Owner {
    login: String,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

/// A repository offering a plugin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Offer {
    pub repo: String,
    pub owner: String,
    pub about: String,
    pub stars: u64,
    pub url: String,
    pub build: Build,
}

/// What it has for this Artoo.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Build {
    Ready {
        id: String,
        version: String,
        zip: String,
    },
    /// None for this Artoo: the Artoo releases it has builds for.
    Other(Vec<String>),
    Nothing,
}

/// A name safe to be a folder's: an id comes from a file on GitHub.
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && !id.starts_with('.')
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
}

fn build_for(releases: &[Release], artoo: &str, target: &str) -> Build {
    let exact = format!("-{artoo}-{target}.zip");
    let any = format!("-{target}.zip");
    let mut others: Vec<String> = Vec::new();
    for release in releases.iter().filter(|r| !r.draft) {
        for asset in &release.assets {
            if let Some(id) = asset.name.strip_suffix(&exact)
                && valid_id(id)
            {
                return Build::Ready {
                    id: id.to_owned(),
                    version: release.tag_name.trim_start_matches('v').to_owned(),
                    zip: asset.browser_download_url.clone(),
                };
            }
            if let Some((id, version)) =
                (asset.name.strip_suffix(&any)).and_then(|n| n.rsplit_once('-'))
                && valid_id(id)
                && Version::parse(version).is_ok()
                && !others.iter().any(|o| o == version)
            {
                others.push(version.to_owned());
            }
        }
    }
    if others.is_empty() {
        Build::Nothing
    } else {
        Build::Other(others)
    }
}

/// Whether `offered` is a newer version than `installed`.
pub(crate) fn newer(offered: &str, installed: &str) -> bool {
    match (Version::parse(offered), Version::parse(installed)) {
        (Ok(offered), Ok(installed)) => offered > installed,
        _ => false,
    }
}

/// Whether a plugin's stamp names this repository: what lets a build count as its update, with no warning. An id alone
/// would let any repository offer a new version of someone else's plugin.
pub(crate) fn same_repository(stamped: &str, offered: &str) -> bool {
    let plain = |url: &str| {
        let url = url.trim().trim_end_matches('/');
        url.strip_suffix(".git").unwrap_or(url).to_lowercase()
    };
    !stamped.trim().is_empty() && plain(stamped) == plain(offered)
}

/// GitHub's answer, or a fixture's: `ARTOO_PLUGINS_FIXTURE` is a folder of `search.json` and `<owner>-<name>.json`.
fn get(url: &str) -> Result<String, String> {
    if let Some(dir) = std::env::var_os("ARTOO_PLUGINS_FIXTURE") {
        let name = match url.strip_prefix("https://api.github.com/repos/") {
            Some(rest) => format!(
                "{}.json",
                rest.split("/releases")
                    .next()
                    .unwrap_or("")
                    .replace('/', "-")
            ),
            None => "search.json".to_owned(),
        };
        return std::fs::read_to_string(Path::new(&dir).join(name)).map_err(|e| e.to_string());
    }
    let agent = format!("User-Agent: Artoo/{ARTOO}");
    let out = Command::new("curl")
        .args([
            "-sS",
            "-L",
            "--max-time",
            "20",
            "-H",
            "Accept: application/vnd.github+json",
            "-H",
            &agent,
        ])
        .args(["-w", "\n%{http_code}", url])
        .stdin(Stdio::null())
        .output()
        .map_err(|_| "curl is not installed".to_owned())?;
    if !out.status.success() {
        return Err("GitHub can't be reached".to_owned());
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let (body, code) = text.rsplit_once('\n').unwrap_or(("", &text));
    match code.trim() {
        "200" => Ok(body.to_owned()),
        "403" | "429" => Err("GitHub asks to wait: too many requests from this address".to_owned()),
        code => Err(format!("GitHub answered {code}")),
    }
}

/// Every repository offering a plugin, each with what it has for this Artoo (off the window's thread).
fn find() -> Result<Vec<Offer>, String> {
    let search: Search = serde_json::from_str(&get(SEARCH)?).map_err(|e| e.to_string())?;
    let offer = |repo: Repo| {
        let releases = get(&format!(
            "https://api.github.com/repos/{}/releases?per_page=10",
            repo.full_name
        ))
        .ok()
        .and_then(|text| serde_json::from_str::<Vec<Release>>(&text).ok());
        Offer {
            build: releases.map_or(Build::Nothing, |r| build_for(&r, ARTOO, TARGET)),
            repo: repo.full_name,
            owner: repo.owner.login,
            about: repo.description.unwrap_or_default(),
            stars: repo.stargazers_count,
            url: repo.html_url,
        }
    };
    Ok(search.items.into_iter().map(offer).collect())
}

fn staged_root() -> Option<PathBuf> {
    Some(platform::data_dir()?.join("plugins-staged"))
}

/// The library of a zip, out into `dir`: the first file of this system's kind at its top level.
fn unpack(zip: &Path, dir: &Path) -> Result<PathBuf, String> {
    let file = std::fs::File::open(zip).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("not a zip: {e}"))?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        // By its name alone: a path in an archive decides nothing about where it lands.
        let Some(name) = entry
            .enclosed_name()
            .filter(|n| n.components().count() == 1)
        else {
            continue;
        };
        if !entry.is_file()
            || name
                .extension()
                .is_none_or(|e| e != std::env::consts::DLL_EXTENSION)
            || entry.size() > MOST
        {
            continue;
        }
        let out = dir.join(name);
        let mut to = std::fs::File::create(&out).map_err(|e| e.to_string())?;
        std::io::copy(&mut entry, &mut to).map_err(|e| e.to_string())?;
        return Ok(out);
    }
    Err("it holds no library for this system".to_owned())
}

/// A downloaded zip staged as `id`, once its stamp says it is that plugin and is built for this app.
fn stage(zip: &Path, id: &str, into: &Path, ours: Option<&str>) -> Result<(), String> {
    let dir = into.join(id);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let checked = unpack(zip, &dir).and_then(|library| {
        let stamp = file::stamp_of(&library).map_err(|why| format!("this file {why}"))?;
        if stamp.id != id {
            return Err(format!("it says it is {}", stamp.id));
        }
        match load::foreign(&stamp, ours) {
            Some(why) => Err(why),
            None => Ok(()),
        }
    });
    if checked.is_err() {
        let _ = std::fs::remove_dir_all(&dir);
    } else {
        let _ = std::fs::remove_file(into.join(format!("{id}.remove")));
    }
    checked
}

/// Downloads a build and stages it for the next start (off the window's thread).
fn download(id: &str, url: &str) -> Result<(), String> {
    let root = staged_root().ok_or("no data folder")?;
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let zip = root.join(format!("{id}.zip.part"));
    let got = Command::new("curl")
        .args(["-fsSL", "--max-time", "300", "-o"])
        .arg(&zip)
        .arg(url)
        .stdin(Stdio::null())
        .status();
    let staged = match got {
        Ok(status) if status.success() => stage(&zip, id, &root, file::sdk_build().as_deref()),
        Ok(_) => Err("the download failed".to_owned()),
        Err(_) => Err("curl is not installed".to_owned()),
    };
    let _ = std::fs::remove_file(&zip);
    staged
}

/// What the last run staged, applied before anything is looked for: `id.remove` takes a plugin away, a folder replaces
/// its plugin's.
fn apply(staged: &Path, plugins: &Path) {
    let names = std::fs::read_dir(staged).into_iter().flatten().flatten();
    for entry in names {
        let name = entry.file_name().to_string_lossy().into_owned();
        if let Some(id) = name.strip_suffix(".remove").filter(|id| valid_id(id)) {
            let _ = std::fs::remove_dir_all(plugins.join(id));
            let _ = std::fs::remove_file(entry.path());
        } else if entry.path().is_dir() && valid_id(&name) {
            let _ = std::fs::create_dir_all(plugins);
            let _ = std::fs::remove_dir_all(plugins.join(&name));
            if let Err(e) = std::fs::rename(entry.path(), plugins.join(&name)) {
                applog::line(&format!("plugin {name}: not installed: {e}"));
            }
        }
    }
}

/// Applies what the last run staged (the first thing the plugins' loading does).
pub(crate) fn apply_staged() {
    if let (Some(staged), Some(plugins)) = (staged_root(), load::folder()) {
        apply(&staged, &plugins);
    }
}

/// What the search has come to.
#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) enum Found {
    #[default]
    NotAsked,
    Asking,
    Got(Vec<Offer>),
    Failed(String),
}

/// What was done to a plugin this run.
#[derive(Clone, PartialEq, Eq)]
pub(crate) enum Done {
    Installing,
    Installed,
    Removed,
    Failed(String),
}

#[derive(Default)]
pub(crate) struct Market {
    pub(crate) found: Found,
    /// The repository whose warning is up, before its install.
    pub(crate) asking: Option<String>,
    /// By id.
    pub(crate) done: BTreeMap<String, Done>,
}

impl Global for Market {}

/// Whether this run may ask GitHub: a test or a snapshot only answers from a fixture.
fn may_ask() -> bool {
    !(platform::sandboxed() || storage::snapshot_run())
        || std::env::var_os("ARTOO_PLUGINS_FIXTURE").is_some()
}

/// Looks for plugins, once a run unless asked again.
pub(crate) fn opened(cx: &mut App) {
    if matches!(cx.global::<Market>().found, Found::NotAsked) {
        ask(cx);
    }
}

pub(crate) fn ask(cx: &mut App) {
    if !may_ask() {
        cx.global_mut::<Market>().found = Found::Got(Vec::new());
        return;
    }
    cx.global_mut::<Market>().found = Found::Asking;
    cx.spawn(async move |cx| {
        let found = cx.background_executor().spawn(async { find() }).await;
        cx.update(|cx| {
            cx.global_mut::<Market>().found = match found {
                Ok(offers) => Found::Got(offers),
                Err(why) => Found::Failed(why),
            };
        });
    })
    .detach();
}

/// Installs or updates: applied at the next start.
pub(crate) fn install(id: String, url: String, cx: &mut App) {
    let market = cx.global_mut::<Market>();
    market.asking = None;
    market.done.insert(id.clone(), Done::Installing);
    cx.spawn(async move |cx| {
        let key = id.clone();
        let result = cx
            .background_executor()
            .spawn(async move { download(&key, &url) })
            .await;
        if let Err(why) = &result {
            applog::line(&format!("plugin {id}: not installed: {why}"));
        }
        cx.update(|cx| {
            let done = result.map_or_else(Done::Failed, |()| Done::Installed);
            cx.global_mut::<Market>().done.insert(id, done);
        });
    })
    .detach();
}

/// A plugin of the fixture's installed, as its button does (the snapshot view `plugin-install:<id>`).
pub(crate) fn install_found(wanted: &str, cx: &mut App) {
    for offer in find().unwrap_or_default() {
        if let Build::Ready { id, zip, .. } = offer.build
            && id == wanted
        {
            install(id, zip, cx);
        }
    }
}

/// Removes a plugin at the next start, and whatever of it was staged.
pub(crate) fn remove(id: &str, cx: &mut App) {
    if let Some(root) = staged_root()
        && valid_id(id)
        && !storage::snapshot_run()
    {
        let _ = std::fs::create_dir_all(&root);
        let _ = std::fs::remove_dir_all(root.join(id));
        let _ = std::fs::write(root.join(format!("{id}.remove")), "");
    }
    cx.global_mut::<Market>()
        .done
        .insert(id.to_owned(), Done::Removed);
}

/// Sample rows of every kind, for the section's snapshot.
pub(crate) fn sample() -> Market {
    let offer = |repo: &str, about: &str, stars, build| Offer {
        repo: repo.to_owned(),
        owner: repo.split('/').next().unwrap_or("").to_owned(),
        about: about.to_owned(),
        stars,
        url: format!("https://github.com/{repo}"),
        build,
    };
    let ready = |id: &str, version: &str| Build::Ready {
        id: id.to_owned(),
        version: version.to_owned(),
        zip: String::new(),
    };
    Market {
        found: Found::Got(vec![
            offer(
                "someone/artoo-jira",
                "Your Jira issues beside the code.",
                214,
                ready("jira", "0.5.0"),
            ),
            offer(
                "someone/artoo-linear",
                "Linear's issues in the side panel.",
                88,
                ready("linear", "1.2.0"),
            ),
            offer(
                "ana/artoo-notes",
                "Notes that stay with a branch.",
                31,
                Build::Other(vec!["1.2.0".to_owned()]),
            ),
            offer(
                "bo/artoo-rainbow",
                "A trial, nothing released.",
                2,
                Build::Nothing,
            ),
        ]),
        asking: Some("someone/artoo-linear".to_owned()),
        done: BTreeMap::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn release(tag: &str, draft: bool, names: &[&str]) -> Release {
        Release {
            tag_name: tag.to_owned(),
            draft,
            assets: names
                .iter()
                .map(|n| Asset {
                    name: n.to_string(),
                    browser_download_url: format!("https://x/{n}"),
                })
                .collect(),
        }
    }

    const T: &str = "aarch64-apple-darwin";

    #[test]
    fn the_build_for_this_artoo_is_the_newest_release_having_one() {
        let name = format!("my-plugin-1.3.0-{T}.zip");
        let releases = [
            release("v0.3.0", true, &[&name]),
            release("v0.2.0", false, &[&name]),
            release("v0.1.0", false, &[&name]),
        ];
        assert_eq!(
            build_for(&releases, "1.3.0", T),
            Build::Ready {
                id: "my-plugin".to_owned(),
                version: "0.2.0".to_owned(),
                zip: format!("https://x/{name}"),
            }
        );
    }

    #[test]
    fn without_one_it_says_which_artoo_it_has_builds_for() {
        let releases = [release(
            "v1.0.0",
            false,
            &[
                &format!("p-1.2.0-{T}.zip"),
                &format!("p-1.1.0-{T}.zip"),
                "p-1.3.0-x86_64-pc-windows-msvc.zip",
            ],
        )];
        assert_eq!(
            build_for(&releases, "1.3.0", T),
            Build::Other(vec!["1.2.0".to_owned(), "1.1.0".to_owned()])
        );
        assert_eq!(
            build_for(&[release("v1", false, &["x.tar"])], "1.3.0", T),
            Build::Nothing
        );
    }

    #[test]
    fn an_id_from_github_cannot_climb_out_of_the_plugins_folder() {
        assert!(valid_id("my-plugin_1.x"));
        for bad in ["", "..", ".hidden", "a/b", "a\\b"] {
            assert!(!valid_id(bad), "{bad}");
        }
        let name = format!("../evil-1.3.0-{T}.zip");
        assert_eq!(
            build_for(&[release("v1", false, &[&name])], "1.3.0", T),
            Build::Nothing
        );
    }

    #[test]
    fn a_newer_version_is_told_by_semver() {
        assert!(newer("0.10.0", "0.9.0"));
        assert!(!newer("0.9.0", "0.9.0") && !newer("x", "0.9.0"));
    }

    #[test]
    fn only_a_plugins_own_repository_can_update_it() {
        let own = "https://github.com/me/plugin";
        assert!(same_repository("https://github.com/Me/plugin.git", own));
        assert!(same_repository("https://github.com/me/plugin/", own));
        assert!(!same_repository(
            "https://github.com/someone-else/plugin",
            own
        ));
        assert!(!same_repository("", own));
    }

    fn zip_of(path: &Path, entries: &[(&str, &[u8])]) {
        let file = std::fs::File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        for (name, body) in entries {
            zip.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(body).unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn only_a_library_at_the_top_of_a_zip_is_taken_and_it_must_be_a_plugin() {
        let dir = tempfile::tempdir().unwrap();
        let ext = std::env::consts::DLL_EXTENSION;
        let z = dir.path().join("a.zip");
        let lib = format!("libp.{ext}");
        let climbing = format!("../libevil.{ext}");
        let nested = format!("deep/libnested.{ext}");
        zip_of(
            &z,
            &[
                ("README.md", b"r"),
                (&climbing, b"x"),
                (&nested, b"x"),
                (&lib, b"not a library"),
            ],
        );
        let out = unpack(&z, dir.path()).unwrap();
        assert_eq!(out, dir.path().join(&lib));
        assert!(
            !dir.path()
                .parent()
                .unwrap()
                .join(format!("libevil.{ext}"))
                .exists()
        );
        // Not a plugin: nothing stays staged.
        let staged = dir.path().join("staged");
        std::fs::create_dir_all(&staged).unwrap();
        let why = stage(&z, "p", &staged, Some("SDK")).unwrap_err();
        assert!(why.starts_with("this file is not a library"), "{why}");
        assert!(!staged.join("p").exists());
    }

    #[test]
    fn staged_changes_are_applied_at_the_start() {
        let dir = tempfile::tempdir().unwrap();
        let (staged, plugins) = (dir.path().join("staged"), dir.path().join("plugins"));
        for folder in [
            plugins.join("old"),
            plugins.join("gone"),
            staged.join("new"),
            staged.join("old"),
        ] {
            std::fs::create_dir_all(&folder).unwrap();
        }
        std::fs::write(plugins.join("old/was"), "").unwrap();
        std::fs::write(staged.join("old/is"), "").unwrap();
        std::fs::write(staged.join("gone.remove"), "").unwrap();
        apply(&staged, &plugins);
        assert!(plugins.join("new").is_dir());
        assert!(plugins.join("old/is").exists() && !plugins.join("old/was").exists());
        assert!(!plugins.join("gone").exists());
        assert_eq!(std::fs::read_dir(&staged).unwrap().count(), 0);
    }
}
