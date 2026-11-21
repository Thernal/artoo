//! Plugins from private hosts (#179): which hosts hold a token, and a read-only search of their repositories for a
//! topic (`artoo_hosts_api::team`). GitHub's search and GitLab's project list answer for what the token can see, the
//! private repositories among them; both are GETs. `ARTOO_MR_FIXTURE=<dir>` answers from `<dir>/team.json` — hosts,
//! each with its `repos`, or a `refused` text — and never reaches a network. A repo's `release` (`tag`, `published`,
//! `assets` of `name` and `url`) and its `readme` (a text) answer for its latest release and README; an asset's bytes
//! come from the file beside `team.json` named by its `url`'s last part.

use crate::git::WebRemote;
use crate::{fixture, github, gitlab, tokens};
use anyhow::{Result, anyhow, bail};
use artoo_hosts_api::team::{Team, TeamAsset, TeamHost, TeamRelease, TeamRepo};
use serde_json::Value;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

pub(crate) struct Held;

/// `https___host` as a token file's name has it: the host.
fn host_of_file(name: &str) -> Option<String> {
    name.strip_prefix("https___")
        .filter(|h| !h.is_empty())
        .map(str::to_owned)
}

fn github_like(host: &str) -> bool {
    host.contains("github")
}

fn fixture_team() -> Option<Value> {
    let text = std::fs::read_to_string(fixture::fixture()?.join("team.json")).ok()?;
    serde_json::from_str(&text).ok()
}

fn s(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn repo_of(v: &Value) -> TeamRepo {
    TeamRepo {
        full_name: s(v, "full_name"),
        name: s(v, "name"),
        owner: s(v, "owner"),
        description: s(v, "description"),
        stars: v.get("stars").and_then(Value::as_u64).unwrap_or(0),
        url: s(v, "url"),
        license: s(v, "license"),
        pushed: v.get("pushed").and_then(Value::as_i64).unwrap_or(0),
        private: v.get("private").and_then(Value::as_bool).unwrap_or(true),
    }
}

fn holds(host: &str) -> &'static str {
    if github_like(host) {
        "Private repositories"
    } else {
        "Projects and their releases"
    }
}

impl Team for Held {
    fn hosts(&self) -> Vec<TeamHost> {
        if let Some(team) = fixture_team() {
            return (team
                .get("hosts")
                .and_then(Value::as_array)
                .into_iter()
                .flatten())
            .map(|h| TeamHost {
                host: s(h, "host"),
                who: s(h, "who"),
                holds: holds(&s(h, "host")).to_owned(),
            })
            .collect();
        }
        let Some(home) = tokens::home() else {
            return Vec::new();
        };
        let Ok(entries) = std::fs::read_dir(home.join(".config/artoo/tokens")) else {
            return Vec::new();
        };
        let mut hosts: Vec<String> = entries
            .flatten()
            .filter_map(|e| host_of_file(&e.file_name().to_string_lossy()))
            .collect();
        hosts.sort();
        hosts
            .into_iter()
            .map(|host| TeamHost {
                who: String::new(),
                holds: holds(&host).to_owned(),
                host,
            })
            .collect()
    }

    fn release(&self, host: &str, repo: &str) -> Result<Option<TeamRelease>> {
        if fixture_team().is_some() {
            return Ok(fixture_repo(host, repo)
                .and_then(|r| r.get("release").cloned())
                .map(|r| TeamRelease {
                    tag: s(&r, "tag"),
                    published: r.get("published").and_then(Value::as_i64).unwrap_or(0),
                    assets: (r["assets"].as_array().into_iter().flatten())
                        .map(|a| TeamAsset {
                            name: s(a, "name"),
                            url: s(a, "url"),
                        })
                        .collect(),
                }));
        }
        let github = github_like(host);
        let url = if github {
            github_release_url(host, repo)
        } else {
            gitlab_release_url(host, repo)
        };
        match fetch(host, &url, "application/json", None)? {
            Got::Missing => Ok(None),
            Got::Body(body) => {
                let v: Value = serde_json::from_slice(&body)?;
                Ok(Some(if github {
                    github_release(&v)
                } else {
                    gitlab_release(&v)
                }))
            }
        }
    }

    fn readme(&self, host: &str, repo: &str) -> Result<Option<String>> {
        if fixture_team().is_some() {
            return Ok(
                fixture_repo(host, repo).and_then(|r| r["readme"].as_str().map(str::to_owned))
            );
        }
        let (url, accept) = if github_like(host) {
            (
                github_readme_url(host, repo),
                "application/vnd.github.raw+json",
            )
        } else {
            (gitlab_readme_url(host, repo), "text/plain")
        };
        Ok(match fetch(host, &url, accept, None)? {
            Got::Missing => None,
            Got::Body(body) => Some(String::from_utf8_lossy(&body).into_owned()),
        })
    }

    fn download(&self, host: &str, url: &str, to: &Path) -> Result<()> {
        if let Some(dir) = fixture::fixture().filter(|_| fixture_team().is_some()) {
            let name = url.rsplit('/').next().unwrap_or_default();
            if name.is_empty() || name.contains("..") {
                bail!("no such file");
            }
            std::fs::copy(dir.join(name), to)?;
            return Ok(());
        }
        let accept = if github_like(host) {
            "application/octet-stream"
        } else {
            "*/*"
        };
        match fetch(host, url, accept, Some(to))? {
            Got::Body(_) => Ok(()),
            Got::Missing => bail!("{host} does not have that file"),
        }
    }

    fn search(&self, host: &str, topic: &str) -> Result<Vec<TeamRepo>> {
        if let Some(team) = fixture_team() {
            let found = (team
                .get("hosts")
                .and_then(Value::as_array)
                .into_iter()
                .flatten())
            .find(|h| s(h, "host") == host)
            .ok_or_else(|| anyhow!("{host} is not connected"))?;
            if let Some(why) = found.get("refused").and_then(Value::as_str) {
                return Err(anyhow!("{why}"));
            }
            return Ok((found
                .get("repos")
                .and_then(Value::as_array)
                .into_iter()
                .flatten())
            .map(repo_of)
            .collect());
        }
        // Any repository's address does: the call is under `/search` or `/projects`, not the repository.
        let remote = WebRemote {
            base: format!("https://{host}/team/plugins"),
            github: github_like(host),
        };
        if remote.github {
            let found = github::get(
                &remote,
                &format!("/search/repositories?q=topic:{topic}&sort=stars&order=desc&per_page=30"),
            )?;
            Ok((found
                .get("items")
                .and_then(Value::as_array)
                .into_iter()
                .flatten())
            .map(|r| TeamRepo {
                full_name: s(r, "full_name"),
                name: s(r, "name"),
                owner: r["owner"]["login"].as_str().unwrap_or_default().to_owned(),
                description: s(r, "description"),
                stars: r["stargazers_count"].as_u64().unwrap_or(0),
                url: s(r, "html_url"),
                license: r["license"]["spdx_id"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
                pushed: r["pushed_at"].as_str().map_or(0, epoch),
                private: r["private"].as_bool().unwrap_or(false),
            })
            .collect())
        } else {
            let found = gitlab::get(
                &remote,
                &format!("/projects?topic={topic}&membership=true&order_by=star_count&per_page=30"),
            )?;
            Ok((found.as_array().into_iter().flatten())
                .map(|p| TeamRepo {
                    full_name: s(p, "path_with_namespace"),
                    name: s(p, "path"),
                    owner: p["namespace"]["full_path"]
                        .as_str()
                        .unwrap_or_default()
                        .to_owned(),
                    description: s(p, "description"),
                    stars: p["star_count"].as_u64().unwrap_or(0),
                    url: s(p, "web_url"),
                    license: String::new(),
                    pushed: p["last_activity_at"].as_str().map_or(0, epoch),
                    private: p["visibility"].as_str() != Some("public"),
                })
                .collect())
        }
    }
}

/// The remote any call of `host` is made through: the call's own path is under `/api`, not the repository.
fn remote_of(host: &str) -> WebRemote {
    WebRemote {
        base: format!("https://{host}/team/plugins"),
        github: github_like(host),
    }
}

/// The API's root on `host`: `https://api.github.com`, `https://host/api/v3` (GitHub Enterprise), `https://host/api/v4`.
fn api_root(host: &str) -> String {
    if host == "github.com" {
        "https://api.github.com".to_owned()
    } else if github_like(host) {
        format!("https://{host}/api/v3")
    } else {
        format!("https://{host}/api/v4")
    }
}

/// `group/sub/project` as GitLab's project id in a path: the slashes encoded.
fn project_id(repo: &str) -> String {
    repo.replace('/', "%2F")
}

/// GitHub's latest release of `repo`.
fn github_release_url(host: &str, repo: &str) -> String {
    format!("{}/repos/{repo}/releases/latest", api_root(host))
}

/// GitHub's README of `repo`: raw, with `Accept: application/vnd.github.raw+json`.
fn github_readme_url(host: &str, repo: &str) -> String {
    format!("{}/repos/{repo}/readme", api_root(host))
}

/// GitLab's latest release of a project.
fn gitlab_release_url(host: &str, repo: &str) -> String {
    format!(
        "{}/projects/{}/releases/permalink/latest",
        api_root(host),
        project_id(repo)
    )
}

/// GitLab's README.md of a project at its default branch, raw.
fn gitlab_readme_url(host: &str, repo: &str) -> String {
    format!(
        "{}/projects/{}/repository/files/README.md/raw?ref=HEAD",
        api_root(host),
        project_id(repo)
    )
}

/// GitHub's release: its `assets` have `name` and `url` (the API address, which gives the bytes when asked for
/// `application/octet-stream`; the `browser_download_url` is the web's and needs a session).
fn github_release(v: &Value) -> TeamRelease {
    TeamRelease {
        tag: s(v, "tag_name"),
        published: v["published_at"].as_str().map_or(0, epoch),
        assets: (v["assets"].as_array().into_iter().flatten())
            .map(|a| TeamAsset {
                name: s(a, "name"),
                url: s(a, "url"),
            })
            .collect(),
    }
}

/// GitLab's release: its files are the `assets.links`, the `direct_asset_url` the one that is the file itself.
fn gitlab_release(v: &Value) -> TeamRelease {
    TeamRelease {
        tag: s(v, "tag_name"),
        published: v["released_at"].as_str().map_or(0, epoch),
        assets: (v["assets"]["links"].as_array().into_iter().flatten())
            .map(|l| TeamAsset {
                name: s(l, "name"),
                url: Some(s(l, "direct_asset_url"))
                    .filter(|u| !u.is_empty())
                    .unwrap_or_else(|| s(l, "url")),
            })
            .collect(),
    }
}

/// What a GET answered.
enum Got {
    Body(Vec<u8>),
    Missing,
}

/// One GET with the host's token, by curl as the rest of this crate does it; the token goes in on stdin. `to` takes
/// the body into a file. GitHub's redirect (an asset's bytes sit on another host) is followed — curl keeps
/// `Authorization` off the other host; GitLab's is not, its `PRIVATE-TOKEN` is not one curl keeps off.
fn fetch(host: &str, url: &str, accept: &str, to: Option<&Path>) -> Result<Got> {
    let remote = remote_of(host);
    let (auth, follow) = if remote.github {
        let (token, _) = github::token_source(&remote, true)
            .ok_or_else(|| anyhow!("no token for {host} — add one in Settings"))?;
        (format!("Authorization: Bearer {token}"), true)
    } else {
        let token = gitlab::token(&remote)
            .ok_or_else(|| anyhow!("no token for {host} — add one in Settings"))?;
        (format!("PRIVATE-TOKEN: {token}"), false)
    };
    let quote = |t: &str| {
        t.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
    };
    let mut config = format!(
        "url = \"{}\"\nheader = \"{}\"\nheader = \"Accept: {accept}\"\nuser-agent = \"Artoo\"\n",
        quote(url),
        quote(&auth)
    );
    if follow {
        config.push_str("location\n");
    }
    let mut cmd = Command::new("curl");
    cmd.args([
        "-sS",
        "--max-time",
        if to.is_some() { "300" } else { "25" },
        "-K",
        "-",
    ]);
    match to {
        Some(file) => cmd.arg("-o").arg(file).args(["-w", "%{http_code}"]),
        None => cmd.args(["-w", "\n%{http_code}"]),
    };
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| anyhow!("curl is not installed"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(config.as_bytes())?;
    }
    let out = child.wait_with_output()?;
    if !out.status.success() {
        bail!("Can't reach {host} — is the network on?");
    }
    let (body, code) = match to {
        Some(_) => (
            Vec::new(),
            String::from_utf8_lossy(&out.stdout).into_owned(),
        ),
        None => {
            let at = out.stdout.iter().rposition(|b| *b == b'\n').unwrap_or(0);
            (
                out.stdout[..at].to_vec(),
                String::from_utf8_lossy(&out.stdout[at..]).into_owned(),
            )
        }
    };
    match code.trim() {
        "200" => Ok(Got::Body(body)),
        "404" => Ok(Got::Missing),
        "401" | "403" => bail!("{host} refused the token"),
        other => bail!("{host} answered {other}"),
    }
}

/// The fixture's answer for `repo` on `host`: its entry in `team.json`.
fn fixture_repo(host: &str, repo: &str) -> Option<Value> {
    let team = fixture_team()?;
    let found = (team.get("hosts")?.as_array()?.iter()).find(|h| s(h, "host") == host)?;
    (found.get("repos")?.as_array()?.iter())
        .find(|r| s(r, "full_name") == repo)
        .cloned()
}

/// `2026-11-18T16:40:00Z` as seconds since the epoch; 0 when it does not read.
fn epoch(stamp: &str) -> i64 {
    let n = |r: std::ops::Range<usize>| stamp.get(r).and_then(|t| t.parse::<i64>().ok());
    let (Some(y), Some(m), Some(d)) = (n(0..4), n(5..7), n(8..10)) else {
        return 0;
    };
    let (h, mi, sec) = (
        n(11..13).unwrap_or(0),
        n(14..16).unwrap_or(0),
        n(17..19).unwrap_or(0),
    );
    let y = if m <= 2 { y - 1 } else { y };
    let (era, yoe) = (y.div_euclid(400), y.rem_euclid(400));
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    (era * 146_097 + doe - 719_468) * 86400 + h * 3600 + mi * 60 + sec
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn github_calls_go_to_the_api_of_the_host() {
        assert_eq!(
            github_release_url("github.com", "acme/artoo-x"),
            "https://api.github.com/repos/acme/artoo-x/releases/latest"
        );
        assert_eq!(
            github_readme_url("github.acme.example", "acme/artoo-x"),
            "https://github.acme.example/api/v3/repos/acme/artoo-x/readme"
        );
    }

    #[test]
    fn gitlab_calls_name_the_project_by_its_encoded_path() {
        assert_eq!(
            gitlab_release_url("gl.example.com", "platform/tools/artoo-jira"),
            "https://gl.example.com/api/v4/projects/platform%2Ftools%2Fartoo-jira/releases/permalink/latest"
        );
        assert_eq!(
            gitlab_readme_url("gl.example.com", "platform/artoo-jira"),
            "https://gl.example.com/api/v4/projects/platform%2Fartoo-jira/repository/files/README.md/raw?ref=HEAD"
        );
    }

    #[test]
    fn a_github_release_reads_with_its_api_asset_addresses() {
        let r = github_release(&json!({
            "tag_name": "v0.9.2",
            "published_at": "2026-11-19T10:00:00Z",
            "assets": [{"name": "jira-2.0.0-x.zip", "url": "https://api.github.com/repos/a/b/releases/assets/7",
                        "browser_download_url": "https://github.com/a/b/releases/download/v0.9.2/jira-2.0.0-x.zip"}]
        }));
        assert_eq!(r.tag, "v0.9.2");
        assert_eq!(r.published, epoch("2026-11-19T10:00:00Z"));
        assert_eq!(r.assets.len(), 1);
        assert_eq!(r.assets[0].name, "jira-2.0.0-x.zip");
        assert_eq!(
            r.assets[0].url,
            "https://api.github.com/repos/a/b/releases/assets/7"
        );
    }

    #[test]
    fn a_gitlab_release_reads_its_links_preferring_the_direct_address() {
        let r = gitlab_release(&json!({
            "tag_name": "v0.9.2",
            "released_at": "2026-11-19T10:00:00.000Z",
            "assets": {"links": [
                {"name": "a.zip", "url": "https://gl/x/a", "direct_asset_url": "https://gl/x/-/releases/v0.9.2/downloads/a.zip"},
                {"name": "b.zip", "url": "https://gl/x/b"}
            ], "sources": [{"format": "zip", "url": "https://gl/x/src.zip"}]}
        }));
        assert_eq!(r.tag, "v0.9.2");
        assert!(r.published > 0);
        assert_eq!(r.assets.len(), 2);
        assert_eq!(
            r.assets[0].url,
            "https://gl/x/-/releases/v0.9.2/downloads/a.zip"
        );
        assert_eq!(r.assets[1].url, "https://gl/x/b");
    }

    #[test]
    fn a_release_without_assets_reads_empty() {
        assert!(github_release(&json!({"tag_name": "v1"})).assets.is_empty());
        assert!(gitlab_release(&json!({"tag_name": "v1"})).assets.is_empty());
    }
}
