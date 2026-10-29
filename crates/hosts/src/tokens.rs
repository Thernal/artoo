//! The tokens Artoo keeps itself, one file per host under `~/.config/artoo/tokens`, readable by the
//! owner only: what Settings saves, read by GitLab's and GitHub's side alike.

use anyhow::{Result, anyhow};
use std::path::{Path, PathBuf};

/// The token this app saved for `host` (`https://host`), as written.
pub fn saved(host: &str) -> Option<String> {
    std::fs::read_to_string(file(&home()?, host)).ok()
}

/// Keeps `token` for `host` (`https://host`).
pub(crate) fn save(host: &str, token: &str) -> Result<()> {
    let home = home().ok_or_else(|| anyhow!("no home directory"))?;
    let file = file(&home, host);
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&file, format!("{}\n", token.trim()))?;
    crate::platform::owner_only(&file)?;
    Ok(())
}

/// Removes the token kept for `host` (a wrong one, after a failed test).
pub(crate) fn forget(host: &str) {
    if let Some(home) = home() {
        let _ = std::fs::remove_file(file(&home, host));
    }
}

/// The home folder tokens are kept under; with the connect checks' fixture, its `home/`, so a
/// check or a test never writes the developer's own (#156).
pub(crate) fn home() -> Option<PathBuf> {
    match crate::connect::fixture() {
        Some(dir) => Some(dir.join("home")),
        None => std::env::home_dir(),
    }
}

fn file(home: &Path, host: &str) -> PathBuf {
    let safe: String = host
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    home.join(".config/artoo/tokens").join(safe)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_file_name_is_one_safe_segment() {
        let home = Path::new("/h");
        assert_eq!(
            file(home, "git.example.com:8443"),
            Path::new("/h/.config/artoo/tokens/git.example.com_8443")
        );
        assert_eq!(
            file(home, "../x"),
            Path::new("/h/.config/artoo/tokens/.._x")
        );
    }
}
