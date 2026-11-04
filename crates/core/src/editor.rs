//! Opening a file in the reviewer's editor at a line, through the command the editor ships (inside its app bundle on
//! macOS, on the PATH elsewhere — `platform`). An editor is data — its key, its name, where its command is and the
//! arguments that open a file at a line — so a module adds one by registering it (`artoo_shell_api::add_editor`);
//! Artoo ships three, `BUILT_IN`.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::platform::{self, Launcher};

/// An editor Artoo opens a file in at a line.
#[derive(Clone, Copy, Debug)]
pub struct Editor {
    /// A stable id (`zed`): Settings and `open-with.txt` keep the choice by it.
    pub key: &'static str,
    /// Its name in the menus ("VS Code").
    pub label: &'static str,
    /// Where its command-line launcher lives.
    pub launcher: Launcher,
    /// The arguments that open a file at a line, `{file}` and `{line}` in them replaced.
    pub args: &'static [&'static str],
}

/// One editor is another with the same key.
impl PartialEq for Editor {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}

impl Eq for Editor {}

pub const ANDROID_STUDIO: Editor = Editor {
    key: "android-studio",
    label: "Android Studio",
    launcher: Launcher {
        bundle: "Android Studio",
        in_bundle: "Contents/MacOS/studio",
        on_path: &["studio", "studio.sh", "studio64"],
    },
    args: &["--line", "{line}", "{file}"],
};

pub const ZED: Editor = Editor {
    key: "zed",
    label: "Zed",
    launcher: Launcher {
        bundle: "Zed",
        in_bundle: "Contents/MacOS/cli",
        on_path: &["zed", "zeditor"],
    },
    args: &["{file}:{line}"],
};

pub const VS_CODE: Editor = Editor {
    key: "vscode",
    label: "VS Code",
    launcher: Launcher {
        bundle: "Visual Studio Code",
        in_bundle: "Contents/Resources/app/bin/code",
        on_path: &["code"],
    },
    args: &["-g", "{file}:{line}"],
};

/// The editors Artoo ships, in the order the menus list them.
pub const BUILT_IN: [Editor; 3] = [ANDROID_STUDIO, ZED, VS_CODE];

impl Editor {
    /// Its app's name as the system lists it ("Visual Studio Code"): the same app is not offered
    /// twice in Open in's ▾ (#78).
    pub fn bundle(self) -> &'static str {
        self.launcher.bundle
    }

    fn program(self) -> Option<PathBuf> {
        platform::launcher(&self.launcher)
    }

    pub fn installed(self) -> bool {
        self.program().is_some()
    }

    /// The arguments that open `file` at `line`.
    fn args(self, file: &Path, line: u32) -> Vec<String> {
        let (file, line) = (file.display().to_string(), line.to_string());
        self.args
            .iter()
            .map(|arg| arg.replace("{file}", &file).replace("{line}", &line))
            .collect()
    }

    /// Opens `file` at `line`; the error says what to do about it.
    pub fn open(self, file: &Path, line: u32) -> Result<(), String> {
        let Some(program) = self.program() else {
            return Err(format!("{} is not installed.", self.label));
        };
        // A folder opens as the editor's project (#78).
        if file.is_dir() {
            return Command::new(program)
                .arg(file)
                .spawn()
                .map(|_| ())
                .map_err(|e| format!("Could not start {}: {e}", self.label));
        }
        if !file.exists() {
            return Err(format!(
                "{} is not in the working tree, so there is nothing to open.",
                file.display()
            ));
        }
        Command::new(program)
            .args(self.args(file, line.max(1)))
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("Could not start {}: {e}", self.label))
    }
}

/// The editor of `editors` with key `key`.
pub fn find(key: &str, editors: &[Editor]) -> Option<Editor> {
    editors.iter().copied().find(|e| e.key == key)
}

/// The editor to use of `editors`: the chosen one if installed, else the first installed.
pub fn pick(chosen: Option<&str>, editors: &[Editor]) -> Option<Editor> {
    chosen
        .and_then(|key| find(key, editors))
        .filter(|e| e.installed())
        .or_else(|| editors.iter().copied().find(|e| e.installed()))
}

/// The editors' names as a sentence lists them: "Android Studio, Zed or VS Code".
pub fn names(editors: &[Editor]) -> String {
    match editors.split_last() {
        None => String::new(),
        Some((last, [])) => last.label.to_owned(),
        Some((last, rest)) => {
            let rest: Vec<&str> = rest.iter().map(|e| e.label).collect();
            format!("{} or {}", rest.join(", "), last.label)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_editor_gets_the_file_and_the_line() {
        let file = Path::new("/work/repo/src/a.rs");
        let path = file.display().to_string();
        assert_eq!(
            ANDROID_STUDIO.args(file, 12),
            ["--line".to_string(), "12".into(), path.clone()]
        );
        assert_eq!(ZED.args(file, 12), [format!("{path}:12")]);
        assert_eq!(
            VS_CODE.args(file, 12),
            ["-g".to_string(), format!("{path}:12")]
        );
    }

    #[test]
    fn keys_find_their_editor() {
        for editor in BUILT_IN {
            assert_eq!(find(editor.key, &BUILT_IN), Some(editor));
        }
        assert_eq!(find("emacs", &BUILT_IN), None);
    }

    #[test]
    fn names_read_as_a_sentence() {
        assert_eq!(names(&BUILT_IN), "Android Studio, Zed or VS Code");
        assert_eq!(names(&[ZED]), "Zed");
        assert_eq!(names(&[]), "");
    }
}
