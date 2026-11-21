//! Plugins from private hosts (#179): which hosts hold a token, and a read-only search of their repositories for a
//! topic (`artoo_hosts_api::team`). GitHub's search and GitLab's project list answer for what the token can see, the
//! private repositories among them; both are GETs. `ARTOO_MR_FIXTURE=<dir>` answers from `<dir>/team.json` — hosts,
//! each with its `repos`, or a `refused` text — and never reaches a network.

use crate::git::WebRemote;
use crate::{fixture, github, gitlab, tokens};
use anyhow::{Result, anyhow};
use artoo_hosts_api::team::{Team, TeamHost, TeamRepo};
use serde_json::Value;

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
