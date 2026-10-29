//! What differs between macOS, Windows and Linux, behind one API: the app's data folder, its icon,
//! where an editor's launcher lives, how the window's title bar is drawn, and which apps open a
//! file (#78). Nothing outside this module asks which OS it runs on.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as os;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
use windows as os;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod linux;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
use linux as os;

use std::path::{Path, PathBuf};

pub use os::{TRANSPARENT_TITLEBAR, alive, launch, owner_only, reduce_motion, set_icon};

/// Whether this is macOS, where the primary modifier is ⌘; elsewhere it is Ctrl.
pub const MAC: bool = os::MAC;

/// A shipped binding, written with macOS keys (`cmd-o`), in this OS's keys.
pub fn keys(mac: &str) -> String {
    if MAC { mac.to_owned() } else { pc_keys(mac) }
}

/// macOS keys in Windows and Linux terms: `cmd-` is `ctrl-`, except ⌘1…⌘9 (the tabs), which are
/// `alt-1`…`alt-9`, as browsers there have them: `ctrl-1`…`ctrl-4` move between zones. ⌃⌘ (the
/// spaces) is Ctrl+Shift, since Ctrl is already ⌘'s. ⌥⌘→ ⌥⌘← (the open items) are Ctrl+PageDown
/// and Ctrl+PageUp, the browsers' there: Ctrl+Alt+arrows switch workspaces on Linux.
pub fn pc_keys(mac: &str) -> String {
    mac.split(' ')
        .map(|stroke| {
            match stroke {
                "cmd-alt-right" => return "ctrl-pagedown".to_owned(),
                "cmd-alt-left" => return "ctrl-pageup".to_owned(),
                _ => {}
            }
            if let Some(key) = stroke.strip_prefix("ctrl-cmd-") {
                return format!("ctrl-shift-{key}");
            }
            let tab = stroke
                .strip_prefix("cmd-")
                .filter(|k| k.len() == 1 && k.as_bytes()[0].is_ascii_digit() && *k != "0");
            match tab {
                Some(digit) => format!("alt-{digit}"),
                None => stroke.replace("cmd-", "ctrl-"),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// This computer's name as its owner gave it ("Ann's MacBook Pro"), for what Artoo makes in
/// its name (#142); the host name where there is no other.
pub fn computer_name() -> String {
    let asked = |program: &str, args: &[&str]| {
        std::process::Command::new(program)
            .args(args)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
            .filter(|n| !n.is_empty())
    };
    let named = if MAC {
        asked("scutil", &["--get", "ComputerName"])
    } else {
        std::env::var("COMPUTERNAME").ok()
    };
    named
        .or_else(|| asked("hostname", &[]))
        .unwrap_or_else(|| "this computer".to_owned())
}

/// A string nothing else this machine makes has: the time in nanoseconds and the process.
pub fn uid() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    format!("{nanos:x}-{:x}", std::process::id())
}

/// Whether this run must leave the user's data folder and the network alone: a test's. A crate's own
/// tests see `cfg!(test)`; another crate's tests turn on `sandbox` through their dev-dependencies, as
/// `cfg!(test)` is false in a crate they only call (#165).
pub fn sandboxed() -> bool {
    cfg!(test) || cfg!(feature = "sandbox")
}

/// The folder for what Artoo remembers between runs. The first time a run asks for it, GitLance's,
/// the name Artoo had, is copied into it when only that one is there (`carry_over`); never in a test
/// (`sandboxed`).
pub fn data_dir() -> Option<PathBuf> {
    let dir = os::data_dir()?;
    static CARRIED: std::sync::Once = std::sync::Once::new();
    if !sandboxed() {
        CARRIED.call_once(|| {
            if let Some(old) = os::old_data_dir() {
                carry_over(&old, &dir);
            }
        });
    }
    Some(dir)
}

/// Copies `old` to `new` when only `old` is there. A copy, not a move: GitLance installed beside
/// Artoo keeps its own folder. It is made beside `new` and renamed into place, so a copy cut short
/// is never taken for the folder.
fn carry_over(old: &Path, new: &Path) {
    if new.exists() || !old.is_dir() {
        return;
    }
    let part = new.with_extension("copying");
    let _ = std::fs::remove_dir_all(&part);
    if copy_dir(old, &part).is_ok() {
        let _ = std::fs::rename(&part, new);
    }
    // Left when the copy failed, or another run put its own in place first.
    let _ = std::fs::remove_dir_all(&part);
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let target = to.join(entry.file_name());
        if kind.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else if kind.is_file() {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// The key window as a PNG, for a report (#118); `None` when it cannot be taken.
pub fn window_picture() -> Option<Vec<u8>> {
    os::window_picture()
}

/// Where an editor's command-line launcher can be found.
#[allow(dead_code)] // each OS reads only its own fields
pub struct Launcher {
    /// The app bundle's name starts with this (macOS): "Android Studio" also finds
    /// "Android Studio Preview.app".
    pub bundle: &'static str,
    /// The launcher inside that bundle (macOS).
    pub in_bundle: &'static str,
    /// The launcher's names on the PATH (Windows, Linux).
    pub on_path: &'static [&'static str],
}

/// The launcher's program, if the editor is installed.
pub fn launcher(launcher: &Launcher) -> Option<PathBuf> {
    os::launcher(launcher)
}

/// `text` as one word for a POSIX shell.
#[cfg(unix)]
fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

/// Opens a terminal window in `dir` running `program` with one argument, `arg` (a new agent chat
/// with its first message).
pub fn open_terminal(dir: &std::path::Path, program: &str, arg: &str) -> std::io::Result<()> {
    os::open_terminal(dir, program, arg)
}

/// Whether the system names the apps that open a file (macOS); elsewhere a file opens in "the
/// default app", what `open_with` without one starts (#78).
pub const NAMES_APPS: bool = os::NAMES_APPS;

/// Whether a whole working tree can be watched for changes cheaply: one stream on macOS
/// (FSEvents) and Windows; Linux's inotify needs a watch per folder, too many in a large tree,
/// so there git is asked on a slower beat instead (#87).
pub const WATCHES_TREES: bool = os::WATCHES_TREES;

/// The apps that can open `file`, by name and where they are, the system's default first (#78);
/// none where the system does not say.
pub fn apps_for(file: &std::path::Path) -> Vec<(String, PathBuf)> {
    os::apps_for(file)
}

/// Opens `file` (or a folder) in `app`, else in what the system opens it with.
pub fn open_with(file: &std::path::Path, app: Option<&std::path::Path>) -> std::io::Result<()> {
    os::open_with(file, app)
}

/// The folders JetBrains IDEs keep their settings under, one folder a version inside
/// (`IntelliJIdea2026.2`, `AndroidStudio2026.1.3`), where Artoo can also read the tokens they
/// keep (#156): macOS's Keychain. None elsewhere: there the passwords are in a store not read yet.
pub fn ide_config_dirs() -> Vec<PathBuf> {
    os::ide_config_dirs()
}

/// Whether the password store holds an item of `service`, asked without reading it: nothing is
/// shown to the developer (#156).
pub fn keychain_has(service: &str) -> bool {
    os::keychain_has(service)
}

/// The secret of `service`'s item: macOS asks the developer first, and this waits for the answer
/// (off the window's thread). `None` when there is none or the developer said no.
pub fn keychain_read(service: &str) -> Option<String> {
    os::keychain_read(service)
}

/// Where Preview is: images and PDFs open there (#78); None off macOS.
pub fn preview() -> Option<PathBuf> {
    os::preview()
}

/// The space the title bar leaves at its start for the window's own buttons.
pub const TITLEBAR_INSET: f32 = os::TITLEBAR_INSET;

/// The first of `names` found in a folder of the PATH, trying `extensions` after each name.
#[cfg(not(target_os = "macos"))]
fn on_path(names: &[&str], extensions: &[String]) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let dirs: Vec<PathBuf> = std::env::split_paths(&path).collect();
    names.iter().find_map(|name| {
        dirs.iter().find_map(|dir| {
            std::iter::once(String::new())
                .chain(extensions.iter().cloned())
                .map(|ext| dir.join(format!("{name}{ext}")))
                .find(|p| p.is_file())
        })
    })
}

#[cfg(test)]
mod tests {
    use super::{carry_over, pc_keys};

    #[test]
    fn mac_keys_become_ctrl_and_tabs_alt() {
        assert_eq!(pc_keys("cmd-shift-f"), "ctrl-shift-f");
        assert_eq!(pc_keys("cmd-alt-1"), "ctrl-alt-1");
        assert_eq!(pc_keys("cmd-3"), "alt-3");
        assert_eq!(pc_keys("cmd-0"), "ctrl-0");
        assert_eq!(pc_keys("cmd-alt-right"), "ctrl-pagedown");
        assert_eq!(pc_keys("cmd--"), "ctrl--");
        assert_eq!(pc_keys("ctrl-2"), "ctrl-2");
        assert_eq!(pc_keys("ctrl-cmd-4"), "ctrl-shift-4");
        assert_eq!(pc_keys("g g"), "g g");
    }

    #[test]
    fn gitlances_folder_is_copied_once_and_kept() {
        let home = tempfile::tempdir().unwrap();
        let (old, new) = (home.path().join("GitLance"), home.path().join("Artoo"));
        std::fs::create_dir_all(old.join("agents/chat")).unwrap();
        std::fs::write(old.join("tokens.txt"), "https://h a-token\n").unwrap();
        std::fs::write(old.join("agents/chat/comments.json"), "[]").unwrap();
        // A copy an earlier run left cut short.
        std::fs::create_dir_all(home.path().join("Artoo.copying")).unwrap();
        carry_over(&old, &new);
        assert_eq!(
            std::fs::read_to_string(new.join("tokens.txt")).unwrap(),
            "https://h a-token\n"
        );
        assert!(new.join("agents/chat/comments.json").is_file());
        assert!(old.join("tokens.txt").is_file(), "GitLance keeps its own");
        assert!(!home.path().join("Artoo.copying").exists());
        // Artoo's folder is there now: GitLance's is not copied over it again.
        std::fs::write(new.join("tokens.txt"), "changed").unwrap();
        carry_over(&old, &new);
        assert_eq!(
            std::fs::read_to_string(new.join("tokens.txt")).unwrap(),
            "changed"
        );
    }
}
