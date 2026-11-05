//! Claude Code's chats (#171 phase 5: one of the agents' kinds). Each config folder on this machine (`~/.claude`,
//! `~/.claude-*`) is a profile with its own MCP servers: Connect adds Artoo's to every one with `claude mcp add`, and
//! the skill beside it. A chat runs Artoo's watcher in the background (`agent-watch`), so it listens by itself; its
//! transcripts name it and say which chats in a folder were used lately.

use crate::agents;
use artoo_agents_api::provider::{Me, Provider};
use std::path::{Path, PathBuf};

/// Claude Code, as the agents offer it.
pub(crate) struct ClaudeCode;

impl Provider for ClaudeCode {
    fn name(&self) -> &'static str {
        "Claude Code"
    }

    fn me(&self) -> Option<Me> {
        let var = |name: &str| std::env::var(name).ok().filter(|v| !v.is_empty());
        let session = var("CLAUDE_CODE_SESSION_ID")?;
        let pid = var("CLAUDE_PID").and_then(|p| p.parse().ok()).unwrap_or(0);
        let config = var("CLAUDE_CONFIG_DIR")
            .map(PathBuf::from)
            .or_else(|| Some(std::env::home_dir()?.join(".claude")));
        Some(Me {
            session,
            pid,
            config,
        })
    }

    fn title(&self, me: &Me) -> String {
        me.config
            .as_deref()
            .and_then(|c| transcript(c, &me.session))
            .map(|t| title(&t))
            .unwrap_or_default()
    }

    fn connects(&self) -> Option<&'static str> {
        Some(
            "Adds Artoo's MCP server to every Claude Code profile; a chat already open needs a restart.",
        )
    }

    fn connect(&self) -> Result<String, String> {
        connect()
    }

    fn found(&self) -> Option<String> {
        let profiles = profiles()
            .iter()
            .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .collect::<Vec<_>>()
            .join(", ");
        Some(if profiles.is_empty() {
            "No Claude Code profile found.".to_owned()
        } else {
            format!("Profiles: {profiles}")
        })
    }

    fn add_command(&self, server: &str) -> String {
        format!("claude mcp add -s user artoo -- {server}")
    }

    fn connected_as_gitlance(&self) -> bool {
        profiles().iter().any(|p| {
            !connected(p)
                && p.join("skills")
                    .join(agents::OLD_NAME)
                    .join("SKILL.md")
                    .is_file()
        })
    }

    fn chats(&self, root: &Path) -> Vec<(String, String)> {
        chats(root)
    }

    fn program(&self) -> Option<&'static str> {
        Some("claude")
    }
}

/// Chat `session`'s transcript in config folder `config`, to name it by its first message.
fn transcript(config: &Path, session: &str) -> Option<PathBuf> {
    std::fs::read_dir(config.join("projects"))
        .ok()?
        .flatten()
        .map(|dir| dir.path().join(format!("{session}.jsonl")))
        .find(|f| f.is_file())
}

/// Claude Code's chats in `root` that are not listening: from its transcripts, touched in the last
/// day, newest first, as (session, title). Each config folder (`~/.claude`, `~/.claude-*`) counts.
pub(crate) fn chats(root: &Path) -> Vec<(String, String)> {
    let Some(home) = std::env::home_dir() else {
        return Vec::new();
    };
    let folder = folder(root);
    let mut found: Vec<(std::time::SystemTime, String, PathBuf)> = Vec::new();
    for config in std::fs::read_dir(&home).into_iter().flatten().flatten() {
        let name = config.file_name().to_string_lossy().into_owned();
        if name != ".claude" && !name.starts_with(".claude-") {
            continue;
        }
        let dir = config.path().join("projects").join(&folder);
        for file in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = file.path();
            if path.extension().is_none_or(|x| x != "jsonl") {
                continue;
            }
            let Ok(modified) = file.metadata().and_then(|m| m.modified()) else {
                continue;
            };
            let day = std::time::Duration::from_secs(24 * 3600);
            if modified.elapsed().is_ok_and(|age| age > day) {
                continue;
            }
            let session = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            found.push((modified, session, path));
        }
    }
    found.sort_by_key(|f| std::cmp::Reverse(f.0));
    found
        .into_iter()
        .take(8)
        .map(|(_, session, path)| (session, title(&path)))
        .collect()
}

/// Where Claude Code keeps the transcripts of chats started in `dir`: the path with every
/// character but letters and digits made `-`.
pub(crate) fn folder(dir: &Path) -> String {
    dir.display()
        .to_string()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect()
}

/// A chat's first message, shortened: what the person asked it first.
pub(crate) fn title(transcript: &Path) -> String {
    let text = std::fs::read_to_string(transcript).unwrap_or_default();
    for line in text.lines().take(400) {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if value["type"] != "user" {
            continue;
        }
        let content = &value["message"]["content"];
        let said = match content {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Array(parts) => parts
                .iter()
                .filter(|p| p["type"] == "text")
                .filter_map(|p| p["text"].as_str())
                .collect::<Vec<_>>()
                .join(" "),
            _ => String::new(),
        };
        let said = said.split_whitespace().collect::<Vec<_>>().join(" ");
        // Commands, hooks and reminders are written in tags; the person's words are not.
        if said.is_empty() || said.starts_with('<') || said.starts_with("Caveat:") {
            continue;
        }
        return shorten(&said, 60);
    }
    "A chat without messages yet".to_owned()
}

fn shorten(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let cut: String = text.chars().take(max - 1).collect();
    format!("{}…", cut.trim_end())
}

/// Claude Code's config folders on this machine: `~/.claude` and every `~/.claude-*` profile
/// (`CLAUDE_CONFIG_DIR`), each with its own MCP servers.
pub(crate) fn profiles() -> Vec<PathBuf> {
    let Some(home) = std::env::home_dir() else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = std::fs::read_dir(&home)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            (name == ".claude" || name.starts_with(".claude-"))
                && e.path().join("projects").is_dir()
        })
        .map(|e| e.path())
        .collect();
    found.sort();
    found
}

/// The `claude` program: on the PATH, or where its installers put it (an app opened from the
/// Finder does not get the shell's PATH).
fn program() -> Option<PathBuf> {
    let home = std::env::home_dir()?;
    let known = [
        home.join(".local/bin/claude"),
        home.join(".claude/local/claude"),
        PathBuf::from("/opt/homebrew/bin/claude"),
        PathBuf::from("/usr/local/bin/claude"),
    ];
    let on_path = std::env::var_os("PATH")
        .map(|p| {
            std::env::split_paths(&p)
                .map(|d| d.join("claude"))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    on_path.into_iter().chain(known).find(|p| p.is_file())
}

/// The skill written beside the server in each Claude Code profile (#158): when to open the changes for the
/// reviewer, kept in the agent's context apart from the server's instructions, which come once.
pub(crate) const SKILL: &str = include_str!("../../../assets/skills/artoo/SKILL.md");

/// Writes the skill into a Claude Code profile, replacing an older one: `<profile>/skills/artoo/SKILL.md`.
pub(crate) fn write_skill(profile: &Path) -> std::io::Result<()> {
    let dir = profile.join("skills").join("artoo");
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join("SKILL.md"), SKILL)
}

/// Whether Artoo connected Claude Code profile `profile`: its skill is there.
pub(crate) fn connected(profile: &Path) -> bool {
    profile.join("skills/artoo/SKILL.md").is_file()
}

/// Adds Artoo's MCP server to every Claude Code profile, for every project (user scope),
/// replacing an older entry, and the skill beside it. Returns what happened, per profile.
pub(crate) fn connect() -> Result<String, String> {
    let claude = program().ok_or("Claude Code's `claude` was not found.")?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let profiles = profiles();
    let mut done = Vec::new();
    for profile in &profiles {
        let env = |c: &mut std::process::Command| {
            // `~/.claude` is the default profile: no variable names it.
            if profile.file_name().is_some_and(|n| n != ".claude") {
                c.env("CLAUDE_CONFIG_DIR", profile);
            }
        };
        // GitLance's entry too, the name Artoo had: its program may be gone.
        for name in ["artoo", agents::OLD_NAME] {
            let mut remove = std::process::Command::new(&claude);
            remove.args(["mcp", "remove", "-s", "user", name]);
            env(&mut remove);
            let _ = remove.output();
        }
        let mut add = std::process::Command::new(&claude);
        add.args(["mcp", "add", "-s", "user", "artoo", "--"])
            .arg(&exe)
            .arg("mcp");
        env(&mut add);
        let out = add.output().map_err(|e| e.to_string())?;
        let name = profile
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if out.status.success() {
            write_skill(profile).map_err(|e| format!("{name}: {e}"))?;
            let _ = std::fs::remove_dir_all(profile.join("skills").join(agents::OLD_NAME));
            done.push(name);
        } else {
            return Err(format!(
                "{name}: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
    }
    if done.is_empty() {
        return Err("No Claude Code profile found (~/.claude).".into());
    }
    Ok(format!(
        "Connected: {}. New chats start the watcher; open chats need a restart or “listen to Artoo”.",
        done.join(", ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_transcript_is_named_by_the_first_thing_said() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("s.jsonl");
        std::fs::write(
            &file,
            [
                r#"{"type":"summary","summary":"x"}"#,
                r#"{"type":"user","message":{"role":"user","content":"<command-name>/clear</command-name>"}}"#,
                r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"Fix the   login flow on mobile"}]}}"#,
            ]
            .join("\n"),
        )
        .unwrap();
        assert_eq!(title(&file), "Fix the login flow on mobile");
        assert_eq!(folder(Path::new("/Users/a/My.Repo")), "-Users-a-My-Repo");
    }

    #[test]
    fn the_skill_is_written_into_a_profile_and_names_the_moments() {
        let profile = tempfile::tempdir().unwrap();
        write_skill(profile.path()).unwrap();
        // Again, over an older one.
        write_skill(profile.path()).unwrap();
        let file = profile.path().join("skills/artoo/SKILL.md");
        let text = std::fs::read_to_string(file).unwrap();
        let (_, front) = text.split_once("---\n").unwrap();
        let (front, _) = front.split_once("\n---").unwrap();
        assert!(front.contains("name: artoo"));
        for moment in ["after a commit", "rewritten", "addressed", "to approve"] {
            assert!(front.contains(moment), "{moment}");
        }
    }
}
