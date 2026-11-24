//! Plugins found and installed from inside the app (#176): GitHub's repositories with the topic `artoo-plugin`, each
//! release's asset `<id>-<artoo-version>-<target>.zip` the build for one Artoo. Only a build for this Artoo is
//! installable, and its stamp is checked before it is kept.
//!
//! What is read, and when (Design's handoff P8): the list is one search, run when the plugins' page first opens and
//! kept for the session — typing filters it here, so GitHub's anonymous limit is not spent on keystrokes. A
//! repository's releases (its build for this Artoo, its downloads) and its README are read when it is selected or
//! installed. The installed plugins' releases are read for updates as Artoo starts and once a day after.
//!
//! What is installed, updated or removed is staged and applied at the next start: plugins load only as the app starts,
//! and a library that is loaded can't be replaced on Windows.

use crate::{file, load};
use artoo_core::{applog, http, platform, storage};
use gpui::{App, Global};
use semver::Version;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

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
    name: String,
    html_url: String,
    description: Option<String>,
    stargazers_count: u64,
    owner: Owner,
    #[serde(default)]
    license: Option<License>,
    #[serde(default)]
    pushed_at: Option<String>,
}

#[derive(Deserialize)]
struct Owner {
    login: String,
}

#[derive(Deserialize)]
struct License {
    spdx_id: Option<String>,
    name: Option<String>,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    published_at: Option<String>,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    download_count: u64,
}

/// A repository offering a plugin, as the search says it: all a row knows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Offer {
    /// `owner/name`.
    pub repo: String,
    pub owner: String,
    /// Its name as a row says it: the repository's, without `artoo`, in words (`artoo-sprint-board`: `Sprint board`).
    pub name: String,
    pub about: String,
    pub stars: u64,
    pub url: String,
    /// Its licence's short name (`MIT`); empty when it has none.
    pub license: String,
    /// Its last push, in seconds since the epoch; 0 unknown.
    pub pushed: i64,
    /// The host of a team's repository, without the scheme (`gitlab.example.com`); empty for GitHub's public search.
    pub host: String,
    /// Seen only with a token: a lock beside it.
    pub private: bool,
}

impl Offer {
    /// What its state (details, install under way, the trust question) is kept under: `owner/name` for GitHub's
    /// public search, `host/owner/name` for a team's host. A public repository can be named like a team's, so the
    /// source is part of it.
    pub(crate) fn key(&self) -> String {
        detail_key(&self.host, &self.repo)
    }

    /// The author trusted, by source too: trusting `acme` on GitHub is not trusting `acme` on a team's host. A public
    /// author is the login alone, as the file has kept them.
    pub(crate) fn trust_key(&self) -> String {
        if self.host.is_empty() {
            self.owner.to_lowercase()
        } else {
            format!("{}/{}", self.host, self.owner).to_lowercase()
        }
    }
}

/// `repo`'s state key on `host` (empty: GitHub's public search).
pub(crate) fn detail_key(host: &str, repo: &str) -> String {
    if host.is_empty() {
        repo.to_owned()
    } else {
        format!("{host}/{repo}")
    }
}

/// What a repository's releases and README say, read when it is selected or installed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Detail {
    pub build: Build,
    /// Its assets' downloads, over the releases read.
    pub downloads: u64,
    /// When the release of its build for this Artoo came out, else its newest; 0 unknown.
    pub published: i64,
    /// Its README, as written; none when it has none or it could not be read.
    pub readme: Option<String>,
    /// The team's host its build is fetched from with the token; empty for GitHub's public one.
    pub from: String,
}

/// A repository's details, as far as they have come.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Read {
    Reading,
    Got(Detail),
    Failed(String),
}

/// A newer build of an installed plugin, from its own repository.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Update {
    pub version: String,
    pub zip: String,
    /// Its repository, `owner/name`.
    pub repo: String,
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

/// An asset's `<id>-<artoo-version>` apart. Both may hold dashes (`my-plugin-2.0.0-rc.1`): the version starts at the
/// first dash that leaves one.
fn id_and_version(name: &str) -> Option<(&str, &str)> {
    (name.match_indices('-'))
        .map(|(at, _)| (&name[..at], &name[at + 1..]))
        .find(|(id, version)| valid_id(id) && Version::parse(version).is_ok())
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
            if let Some((_, version)) = (asset.name.strip_suffix(&any)).and_then(id_and_version)
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

/// The file of a fixture's answer to `url`: `search.json`, `<owner>-<name>.json` for its releases,
/// `<owner>-<name>.md` for its README.
fn fixture_name(url: &str) -> String {
    match url.strip_prefix("https://api.github.com/repos/") {
        Some(rest) if rest.ends_with("/readme") => {
            format!("{}.md", rest.trim_end_matches("/readme").replace('/', "-"))
        }
        Some(rest) => format!(
            "{}.json",
            rest.split("/releases")
                .next()
                .unwrap_or("")
                .replace('/', "-")
        ),
        None => "search.json".to_owned(),
    }
}

/// GitHub's answer, or a fixture's: `ARTOO_PLUGINS_FIXTURE` is a folder of them (`fixture_name`). A README is asked
/// for raw.
fn get(url: &str) -> Result<String, String> {
    if let Some(dir) = std::env::var_os("ARTOO_PLUGINS_FIXTURE") {
        return std::fs::read_to_string(Path::new(&dir).join(fixture_name(url)))
            .map_err(|e| e.to_string());
    }
    let accept = if url.ends_with("/readme") {
        "application/vnd.github.raw"
    } else {
        "application/vnd.github+json"
    };
    let request = http::Request::get(url)
        .header("Accept", accept)
        .header("User-Agent", format!("Artoo/{ARTOO}"))
        .follow()
        .timeout(20);
    let answer = http::send(&request).map_err(|e| e.to_string())?;
    match answer.status {
        200 => Ok(answer.text()),
        404 => Err("not found".to_owned()),
        403 | 429 => Err("GitHub asks to wait: too many requests from this address".to_owned()),
        code => Err(format!("GitHub answered {code}")),
    }
}

/// A repository's name as a plugin's (`artoo-sprint-board`: `Sprint board`): `artoo` taken off either end, its dashes
/// and underscores spaces, its first letter capital.
pub(crate) fn name_of(repository: &str) -> String {
    let lower = repository.to_lowercase();
    let mut name = repository;
    for affix in ["artoo-plugin-", "artoo-", "artoo_"] {
        if lower.starts_with(affix) && repository.len() > affix.len() {
            name = &repository[affix.len()..];
            break;
        }
    }
    let lower = name.to_lowercase();
    for affix in ["-artoo-plugin", "-artoo", "_artoo"] {
        if lower.ends_with(affix) && name.len() > affix.len() {
            name = &name[..name.len() - affix.len()];
            break;
        }
    }
    let words = name.replace(['-', '_'], " ");
    let mut chars = words.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => repository.to_owned(),
    }
}

/// `2026-10-08T12:34:56Z` as seconds since the epoch; 0 if unreadable (GitHub writes its times so).
pub(crate) fn epoch(stamp: &str) -> i64 {
    let (date, time) = stamp.split_once('T').unwrap_or((stamp, "00:00:00"));
    let mut d = date.split('-').filter_map(|p| p.parse::<i64>().ok());
    let (Some(y), Some(m), Some(day)) = (d.next(), d.next(), d.next()) else {
        return 0;
    };
    let mut t = (time.trim_end_matches('Z').split(':')).filter_map(|p| p.parse::<f64>().ok());
    let (h, mi, s) = (
        t.next().unwrap_or(0.) as i64,
        t.next().unwrap_or(0.) as i64,
        t.next().unwrap_or(0.) as i64,
    );
    // Howard Hinnant's civil-to-days.
    let y = if m <= 2 { y - 1 } else { y };
    let (era, yoe) = (y.div_euclid(400), y.rem_euclid(400));
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    (era * 146_097 + doe - 719_468) * 86400 + h * 3600 + mi * 60 + s
}

fn offer_of(repo: Repo) -> Offer {
    let license = (repo.license)
        .and_then(|l| (l.spdx_id.filter(|id| !id.is_empty() && id != "NOASSERTION")).or(l.name))
        .unwrap_or_default();
    Offer {
        name: name_of(&repo.name),
        repo: repo.full_name,
        owner: repo.owner.login,
        about: repo.description.unwrap_or_default(),
        stars: repo.stargazers_count,
        url: repo.html_url,
        license,
        pushed: repo.pushed_at.as_deref().map_or(0, epoch),
        host: String::new(),
        private: false,
    }
}

/// The topic a plugin's repository carries.
const TOPIC: &str = "artoo-plugin";

/// A repository a team's host found, as a row has it.
fn team_offer(host: &str, r: artoo_hosts_api::team::TeamRepo) -> Offer {
    Offer {
        name: name_of(&r.name),
        repo: r.full_name,
        owner: r.owner,
        about: r.description,
        stars: r.stars,
        url: r.url,
        license: r.license,
        pushed: r.pushed,
        host: host.to_owned(),
        private: r.private,
    }
}

/// A host Artoo holds a token for and whether plugins are searched on it (Sources).
#[derive(Clone, Default)]
pub(crate) struct TeamSource {
    /// `gitlab.payments-platform-eng.example.com`.
    pub host: String,
    /// Whose token, by username; empty when not known.
    pub who: String,
    /// What searching it reaches: `Private repositories`.
    pub holds: String,
    /// Searched; the person can switch it off.
    pub on: bool,
    pub found: Found,
}

/// The file of the hosts switched off in Sources: a host a line.
const SOURCES_OFF: &str = "plugin-sources-off.txt";

/// Every repository offering a plugin, as the search says them (off the window's thread).
fn find() -> Result<Vec<Offer>, String> {
    let search: Search = serde_json::from_str(&get(SEARCH)?).map_err(|e| e.to_string())?;
    Ok(search.items.into_iter().map(offer_of).collect())
}

/// A repository's releases, read for its build for this Artoo.
fn releases(repo: &str) -> Result<Vec<Release>, String> {
    let text = get(&format!(
        "https://api.github.com/repos/{repo}/releases?per_page=10"
    ))?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

/// What a repository's releases and README say (off the window's thread): its README missing is no failure.
fn detail(repo: &str) -> Result<Detail, String> {
    let releases = releases(repo)?;
    let build = build_for(&releases, ARTOO, TARGET);
    let downloads = (releases.iter())
        .flat_map(|r| &r.assets)
        .map(|a| a.download_count)
        .sum();
    let ours = match &build {
        Build::Ready { version, .. } => {
            (releases.iter()).find(|r| r.tag_name.trim_start_matches('v') == version.as_str())
        }
        _ => releases.iter().find(|r| !r.draft),
    };
    Ok(Detail {
        build,
        downloads,
        published: ours
            .and_then(|r| r.published_at.as_deref())
            .map_or(0, epoch),
        readme: get(&format!("https://api.github.com/repos/{repo}/readme")).ok(),
        from: String::new(),
    })
}

/// A team host's repository: its latest release and README through the host's token (off the window's thread). Its
/// build is the asset the public path looks for; `{artoo}` and `{target}` in an asset's name stand for this Artoo's,
/// so a fixture is the same on every system.
fn team_detail(
    team: &dyn artoo_hosts_api::team::Team,
    host: &str,
    repo: &str,
) -> Result<Detail, String> {
    let release = team.release(host, repo).map_err(|e| e.to_string())?;
    let releases: Vec<Release> = (release.iter())
        .map(|r| Release {
            tag_name: r.tag.clone(),
            draft: false,
            published_at: None,
            assets: (r.assets.iter())
                .map(|a| Asset {
                    name: a.name.replace("{artoo}", ARTOO).replace("{target}", TARGET),
                    browser_download_url: a.url.clone(),
                    download_count: 0,
                })
                .collect(),
        })
        .collect();
    Ok(Detail {
        build: build_for(&releases, ARTOO, TARGET),
        downloads: 0,
        published: release.as_ref().map_or(0, |r| r.published),
        readme: team.readme(host, repo).ok().flatten(),
        from: host.to_owned(),
    })
}

/// `owner/name` of a GitHub repository's address; none for another host's.
pub(crate) fn github_repo(url: &str) -> Option<String> {
    let rest = url.trim().trim_end_matches('/');
    let rest = rest.strip_suffix(".git").unwrap_or(rest);
    let rest = (rest.strip_prefix("https://github.com/"))
        .or_else(|| rest.strip_prefix("http://github.com/"))?;
    let mut parts = rest.split('/');
    let (owner, name) = (parts.next()?, parts.next()?);
    (!owner.is_empty() && !name.is_empty() && parts.next().is_none())
        .then(|| format!("{owner}/{name}"))
}

/// The installed plugins' newer builds, by id: each from its own repository (off the window's thread).
fn updates_of(installed: &[(String, String, String)]) -> BTreeMap<String, Update> {
    let mut out = BTreeMap::new();
    for (id, version, repository) in installed {
        let Some(repo) = github_repo(repository) else {
            continue;
        };
        let Ok(releases) = releases(&repo) else {
            continue;
        };
        if let Build::Ready {
            id: built,
            version: offered,
            zip,
        } = build_for(&releases, ARTOO, TARGET)
            && built == *id
            && newer(&offered, version)
        {
            out.insert(
                id.clone(),
                Update {
                    version: offered,
                    zip,
                    repo,
                },
            );
        }
    }
    out
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

/// Where a build comes from: a public address, or a team host's asset fetched with its token.
enum Source {
    Public(String),
    Team(
        std::sync::Arc<dyn artoo_hosts_api::team::Team>,
        String,
        String,
    ),
}

/// Downloads a build and stages it for the next start (off the window's thread).
fn download(id: &str, source: &Source) -> Result<(), String> {
    let root = staged_root().ok_or("no data folder")?;
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let zip = root.join(format!("{id}.zip.part"));
    let got = match source {
        Source::Public(url) => {
            let request = http::Request::get(url.as_str())
                .output(zip.clone())
                .follow()
                .timeout(300);
            match http::send(&request) {
                Ok(answer) if answer.ok() => Ok(()),
                Ok(_) => Err("the download failed".to_owned()),
                Err(e) if e.kind == http::Kind::NoCurl => Err(e.to_string()),
                Err(_) => Err("the download failed".to_owned()),
            }
        }
        Source::Team(team, host, url) => team
            .download(host, url, &zip)
            .map_err(|e| format!("the download failed: {e}")),
    };
    // Everything after the bytes is one path: the stamp check, the folder, the restart.
    let staged = got.and_then(|()| stage(&zip, id, &root, file::sdk_build().as_deref()));
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
    /// What each repository's releases and README say, by `owner/name`, once asked.
    pub(crate) details: BTreeMap<String, Read>,
    /// The repository whose trust question is up (P5): its author not trusted yet, Install pressed.
    pub(crate) question: Option<String>,
    /// Repositories whose install waits on their releases being read.
    pub(crate) starting: BTreeSet<String>,
    /// By id.
    pub(crate) done: BTreeMap<String, Done>,
    /// The installed plugins' newer builds, by id.
    pub(crate) updates: BTreeMap<String, Update>,
    /// The authors a plugin was installed from: not asked again (`plugins-trusted.txt`).
    pub(crate) trusted: BTreeSet<String>,
    /// The hosts with a token, each searched or switched off (Sources, #179).
    pub(crate) team: Vec<TeamSource>,
}

impl Global for Market {}

/// The file of the authors trusted: a login a line.
const TRUSTED: &str = "plugins-trusted.txt";

/// The authors trusted, as the file has them.
pub(crate) fn trusted() -> BTreeSet<String> {
    (storage::read(TRUSTED).lines())
        .map(|l| l.trim().to_lowercase())
        .filter(|l| !l.is_empty())
        .collect()
}

/// `owner` trusted from now on: its plugins install without the question.
pub(crate) fn trust(owner: &str, cx: &mut App) {
    let market = cx.global_mut::<Market>();
    if market.trusted.insert(owner.to_lowercase()) {
        let text: String = market.trusted.iter().map(|o| format!("{o}\n")).collect();
        storage::save(TRUSTED, text);
    }
}

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
    if cx.global::<Market>().team.is_empty() {
        ask_team(cx);
    }
}

/// Whether this run may reach the team's hosts: a test or a snapshot only answers from the hosts' fixture.
fn may_ask_team() -> bool {
    !(platform::sandboxed() || storage::snapshot_run())
        || std::env::var_os("ARTOO_MR_FIXTURE").is_some()
}

/// Reads which hosts hold a token (nothing is asked of a host), then searches each that is on, off the window's
/// thread. Reads only.
pub(crate) fn ask_team(cx: &mut App) {
    let Some(team) = artoo_hosts_api::team::get(cx).filter(|_| may_ask_team()) else {
        return;
    };
    let off: BTreeSet<String> = storage::read(SOURCES_OFF)
        .lines()
        .map(|l| l.trim().to_owned())
        .collect();
    let held = team.hosts();
    cx.global_mut::<Market>().team = (held.iter())
        .map(|h| TeamSource {
            host: h.host.clone(),
            who: h.who.clone(),
            holds: h.holds.clone(),
            on: !off.contains(&h.host),
            found: if off.contains(&h.host) {
                Found::NotAsked
            } else {
                Found::Asking
            },
        })
        .collect();
    for h in held.into_iter().filter(|h| !off.contains(&h.host)) {
        search_team(team.clone(), h.host, cx);
    }
}

fn search_team(team: std::sync::Arc<dyn artoo_hosts_api::team::Team>, host: String, cx: &mut App) {
    cx.spawn(async move |cx| {
        let asked = host.clone();
        let found = cx
            .background_executor()
            .spawn(async move { team.search(&asked, TOPIC) })
            .await;
        cx.update(|cx| {
            let found = match found {
                Ok(repos) => Found::Got(repos.into_iter().map(|r| team_offer(&host, r)).collect()),
                Err(why) => Found::Failed(why.to_string()),
            };
            if let Some(s) = (cx.global_mut::<Market>().team.iter_mut()).find(|s| s.host == host) {
                s.found = found;
            }
        });
    })
    .detach();
}

/// A host switched on or off in Sources: kept, and searched when it comes on.
pub(crate) fn set_team(host: &str, on: bool, cx: &mut App) {
    let market = cx.global_mut::<Market>();
    let Some(source) = market.team.iter_mut().find(|s| s.host == host) else {
        return;
    };
    source.on = on;
    source.found = if on { Found::Asking } else { Found::NotAsked };
    let off: String = (market.team.iter())
        .filter(|s| !s.on)
        .map(|s| format!("{}\n", s.host))
        .collect();
    storage::save(SOURCES_OFF, off);
    if on && let Some(team) = artoo_hosts_api::team::get(cx).filter(|_| may_ask_team()) {
        search_team(team, host.to_owned(), cx);
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
    install_from(id, url, "", cx);
}

/// `install`, a build of a team's host (`host`) fetched with its token, or GitHub's public one (`host` empty).
fn install_from(id: String, url: String, host: &str, cx: &mut App) {
    let source = if host.is_empty() {
        Source::Public(url.clone())
    } else if let Some(team) = artoo_hosts_api::team::get(cx) {
        Source::Team(team, host.to_owned(), url.clone())
    } else {
        return;
    };
    let market = cx.global_mut::<Market>();
    market.question = None;
    market.done.insert(id.clone(), Done::Installing);
    // A snapshot's sample installs nothing: its row shows the bar.
    if url.is_empty() {
        return;
    }
    cx.spawn(async move |cx| {
        let key = id.clone();
        let result = cx
            .background_executor()
            .spawn(async move { download(&key, &source) })
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
        if let Ok(Detail {
            build: Build::Ready { id, zip, .. },
            ..
        }) = detail(&offer.repo)
            && id == wanted
        {
            install(id, zip, cx);
        }
    }
}

/// `repo`'s releases and README read, once a run (a row selected, its page opened).
pub(crate) fn read(repo: &str, host: &str, cx: &mut App) {
    let key = detail_key(host, repo);
    if cx.global::<Market>().details.contains_key(&key) {
        return;
    }
    // A team's repository is read from its own host, with its token: a snapshot only from the hosts' fixture.
    let team = if host.is_empty() {
        if !may_ask() {
            return;
        }
        None
    } else {
        match artoo_hosts_api::team::get(cx).filter(|_| may_ask_team()) {
            Some(team) => Some(team),
            None => return,
        }
    };
    cx.global_mut::<Market>()
        .details
        .insert(key.clone(), Read::Reading);
    let repo = repo.to_owned();
    let host = host.to_owned();
    cx.spawn(async move |cx| {
        let asked = repo.clone();
        let from = host.clone();
        let got = cx
            .background_executor()
            .spawn(async move {
                match team {
                    Some(team) => team_detail(team.as_ref(), &from, &asked),
                    None => detail(&asked),
                }
            })
            .await;
        cx.update(|cx| {
            let read = got.map_or_else(Read::Failed, Read::Got);
            let ready = match &read {
                Read::Got(Detail {
                    build: Build::Ready { id, zip, .. },
                    from,
                    ..
                }) => Some((id.clone(), zip.clone(), from.clone())),
                _ => None,
            };
            let market = cx.global_mut::<Market>();
            market.details.insert(key.clone(), read);
            // Install was pressed before its build was known.
            if market.starting.remove(&key) {
                match ready {
                    Some((id, zip, from)) => install_from(id, zip, &from, cx),
                    None => {
                        let why = "it has no build for this Artoo".to_owned();
                        applog::line(&format!("plugin {key}: not installed: {why}"));
                    }
                }
            }
        });
    })
    .detach();
}

/// `offer` installed (P5): its build for this Artoo, read first when it is not known yet; its author trusted.
pub(crate) fn install_offer(offer: &Offer, cx: &mut App) {
    trust(&offer.trust_key(), cx);
    let market = cx.global_mut::<Market>();
    market.question = None;
    match market.details.get(&offer.key()) {
        Some(Read::Got(Detail {
            build: Build::Ready { id, zip, .. },
            from,
            ..
        })) => {
            let (id, zip, from) = (id.clone(), zip.clone(), from.clone());
            install_from(id, zip, &from, cx);
        }
        Some(Read::Got(_)) => {}
        Some(Read::Failed(_)) => {
            market.details.remove(&offer.key());
            market.starting.insert(offer.key());
            read(&offer.repo, &offer.host, cx);
        }
        Some(Read::Reading) => {
            market.starting.insert(offer.key());
        }
        None => {
            market.starting.insert(offer.key());
            read(&offer.repo, &offer.host, cx);
        }
    }
}

/// The installed plugins' updates read (as Artoo starts, then once a day): their own repositories' releases.
pub(crate) fn check_updates(installed: Vec<(String, String, String)>, cx: &mut App) {
    if !may_ask() || installed.is_empty() {
        return;
    }
    cx.spawn(async move |cx| {
        let updates = cx
            .background_executor()
            .spawn(async move { updates_of(&installed) })
            .await;
        cx.update(|cx| cx.global_mut::<Market>().updates = updates);
    })
    .detach();
}

/// Every update waiting installed (`Update all`).
pub(crate) fn update_all(cx: &mut App) {
    let market = cx.global::<Market>();
    let waiting: Vec<(String, String)> = (market.updates.iter())
        .filter(|(id, _)| !market.done.contains_key(*id))
        .map(|(id, u)| (id.clone(), u.zip.clone()))
        .collect();
    for (id, zip) in waiting {
        install(id, zip, cx);
    }
}

/// The updates not installed yet this run: the foot row's number.
pub(crate) fn waiting(cx: &App) -> usize {
    cx.try_global::<Market>().map_or(0, |m| {
        (m.updates.keys())
            .filter(|id| !matches!(m.done.get(*id), Some(Done::Installed | Done::Installing)))
            .count()
    })
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

/// Sample rows of every kind, for the page's snapshots (`plugins`).
pub(crate) fn sample() -> Market {
    let day = 86_400;
    let now = artoo_ui_kit::format::now();
    let offer = |repo: &str, about: &str, stars, license: &str, days: i64| {
        let (owner, name) = repo.split_once('/').unwrap_or(("", repo));
        Offer {
            repo: repo.to_owned(),
            owner: owner.to_owned(),
            name: name_of(name),
            about: about.to_owned(),
            stars,
            url: format!("https://github.com/{repo}"),
            license: license.to_owned(),
            pushed: now - days * day,
            host: String::new(),
            private: false,
        }
    };
    let ready = |id: &str, version: &str| Build::Ready {
        id: id.to_owned(),
        version: version.to_owned(),
        zip: String::new(),
    };
    let got = |build, downloads, days: i64, readme: &str| {
        Read::Got(Detail {
            build,
            downloads,
            published: now - days * day,
            readme: Some(readme.to_owned()),
            from: String::new(),
        })
    };
    const README: &str = "## What it does\n\nShows an issue's status, assignee and sprint beside the request that \
                          closes it. Signs in the first time it needs to.\n\n## Setting it up\n\n- Sign in the first \
                          time it asks. The token is kept in your system's keychain.\n";
    Market {
        found: Found::Got(vec![
            offer(
                "someone/artoo-jira",
                "Tickets beside the request that closes them",
                1204,
                "MIT",
                3,
            ),
            offer(
                "kim/artoo-sprint-board",
                "This sprint's tickets as a board, beside your branches",
                812,
                "Apache-2.0",
                7,
            ),
            offer(
                "sana/artoo-linear",
                "Linear issues on the requests that mention them",
                643,
                "MIT",
                12,
            ),
            offer(
                "someone/artoo-confluence",
                "The design page a request links to, read in place",
                590,
                "MIT",
                1,
            ),
            offer(
                "ember/artoo-review-timer",
                "How long you have been on this request",
                388,
                "MIT",
                20,
            ),
            offer(
                "juniper/artoo-standup-notes",
                "Yesterday's commits and reviews, as a standup",
                341,
                "",
                40,
            ),
            offer(
                "tomas/artoo-rainbow-brackets",
                "Matching brackets in matching colours",
                296,
                "MIT",
                90,
            ),
            offer(
                "noor/artoo-release-timeline",
                "Milestones and their requests on one line",
                174,
                "BSD-3-Clause",
                5,
            ),
            offer(
                "lin/artoo-private-notes",
                "A note on a line that only you can read",
                96,
                "MIT",
                2,
            ),
        ]),
        details: BTreeMap::from([
            (
                "someone/artoo-jira".to_owned(),
                got(ready("jira", "1.4.0"), 3412, 3, README),
            ),
            (
                "kim/artoo-sprint-board".to_owned(),
                got(ready("sprint", "2.1.3"), 1980, 7, README),
            ),
            (
                "someone/artoo-confluence".to_owned(),
                got(ready("confluence", "1.3.0"), 1207, 1, README),
            ),
            (
                "tomas/artoo-rainbow-brackets".to_owned(),
                got(Build::Other(vec!["1.2.0".to_owned()]), 88, 90, README),
            ),
        ]),
        question: None,
        starting: BTreeSet::new(),
        done: BTreeMap::new(),
        updates: BTreeMap::from([
            (
                "confluence".to_owned(),
                Update {
                    version: "1.3.0".to_owned(),
                    zip: String::new(),
                    repo: "someone/artoo-confluence".to_owned(),
                },
            ),
            (
                "timer".to_owned(),
                Update {
                    version: "0.4.0".to_owned(),
                    zip: String::new(),
                    repo: "ember/artoo-review-timer".to_owned(),
                },
            ),
        ]),
        trusted: BTreeSet::from(["someone".to_owned()]),
        team: Vec::new(),
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
            published_at: None,
            assets: names
                .iter()
                .map(|n| Asset {
                    name: n.to_string(),
                    browser_download_url: format!("https://x/{n}"),
                    download_count: 0,
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
    fn a_repositorys_name_reads_as_a_plugins() {
        assert_eq!(name_of("artoo-jira"), "Jira");
        assert_eq!(name_of("artoo-sprint-board"), "Sprint board");
        assert_eq!(name_of("review_timer-artoo"), "Review timer");
        assert_eq!(name_of("artoo"), "Artoo");
        assert_eq!(
            github_repo("https://github.com/me/plugin.git"),
            Some("me/plugin".to_owned())
        );
        assert_eq!(github_repo("https://gitlab.com/me/plugin"), None);
        assert_eq!(epoch("1970-01-02T00:00:01Z"), 86_401);
    }

    #[test]
    fn a_newer_version_is_told_by_semver() {
        assert!(newer("0.10.0", "0.9.0"));
        assert!(!newer("0.9.0", "0.9.0") && !newer("x", "0.9.0"));
    }

    #[test]
    fn a_prerelease_of_artoo_is_read_out_of_an_assets_name() {
        const T: &str = "aarch64-apple-darwin";
        let name = format!("my-plugin-2.0.0-rc.1-{T}.zip");
        let releases = [release("v0.1.0", false, &[&name])];
        assert!(matches!(
            build_for(&releases, "2.0.0-rc.1", T),
            Build::Ready { id, .. } if id == "my-plugin"
        ));
        assert!(matches!(
            build_for(&releases, "1.3.0", T),
            Build::Other(others) if others == ["2.0.0-rc.1"]
        ));
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

    struct Fake(Option<artoo_hosts_api::team::TeamRelease>);

    impl artoo_hosts_api::team::Team for Fake {
        fn hosts(&self) -> Vec<artoo_hosts_api::team::TeamHost> {
            Vec::new()
        }
        fn search(&self, _: &str, _: &str) -> anyhow::Result<Vec<artoo_hosts_api::team::TeamRepo>> {
            Ok(Vec::new())
        }
        fn release(
            &self,
            _: &str,
            _: &str,
        ) -> anyhow::Result<Option<artoo_hosts_api::team::TeamRelease>> {
            Ok(self.0.clone())
        }
        fn readme(&self, _: &str, _: &str) -> anyhow::Result<Option<String>> {
            Ok(Some("## What it does".to_owned()))
        }
        fn download(&self, _: &str, _: &str, _: &Path) -> anyhow::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn a_team_repositorys_release_gives_the_same_build_as_a_public_ones() {
        use artoo_hosts_api::team::{TeamAsset, TeamRelease};
        let team = Fake(Some(TeamRelease {
            tag: "v0.9.2".to_owned(),
            published: 100,
            assets: vec![TeamAsset {
                name: "jira-{artoo}-{target}.zip".to_owned(),
                url: "https://gl/x/jira.zip".to_owned(),
            }],
        }));
        let d = team_detail(&team, "gl", "platform/artoo-jira").unwrap();
        assert_eq!(
            d.build,
            Build::Ready {
                id: "jira".to_owned(),
                version: "0.9.2".to_owned(),
                zip: "https://gl/x/jira.zip".to_owned()
            }
        );
        assert_eq!((d.from.as_str(), d.published), ("gl", 100));
        assert_eq!(d.readme.as_deref(), Some("## What it does"));
        let none = team_detail(&Fake(None), "gl", "platform/artoo-jira").unwrap();
        assert_eq!(none.build, Build::Nothing);
    }

    #[test]
    fn a_public_and_a_team_repository_named_alike_stay_apart() {
        let public = Offer {
            repo: "acme/jira".to_owned(),
            owner: "acme".to_owned(),
            name: "Jira".to_owned(),
            about: String::new(),
            stars: 0,
            url: "https://github.com/acme/jira".to_owned(),
            license: String::new(),
            pushed: 0,
            host: String::new(),
            private: false,
        };
        let team = Offer {
            host: "gl.example.com".to_owned(),
            url: "https://gl.example.com/acme/jira".to_owned(),
            private: true,
            ..public.clone()
        };
        assert_eq!(public.key(), "acme/jira");
        assert_eq!(team.key(), "gl.example.com/acme/jira");
        assert_ne!(public.key(), team.key());
        // Trusting the public author is not trusting the team's, and the file's old lines stay public.
        assert_eq!(public.trust_key(), "acme");
        assert_eq!(team.trust_key(), "gl.example.com/acme");
        // What was read of one is not what the other shows.
        let mut details = BTreeMap::new();
        details.insert(public.key(), Read::Reading);
        assert!(!details.contains_key(&team.key()));
        assert!(
            !same_repository(&public.url, &team.url),
            "an installed plugin is told by its repository's address, host included"
        );
    }
}
