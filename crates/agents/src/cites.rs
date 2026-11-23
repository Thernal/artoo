//! `path:line` in the agent's words (#220): a file of the session's folder named with a line — `src/login.rs:44`,
//! `src/login.rs:44:9`, in backticks or bare — is found, and drawn as the file's glyph and `name:line` in a hairline
//! (`talk`). Only a path that is a file inside the folder counts; a time (`10:31`) names none.

use std::path::{Component, Path};

/// One citation: the path as written (relative to the folder) and its line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cite {
    pub path: String,
    pub line: u32,
}

/// The citations in `text` that name a file inside `folder`, in order, each once.
pub fn scan(text: &str, folder: &Path) -> Vec<Cite> {
    let mut out: Vec<Cite> = Vec::new();
    for word in text.split(|c: char| c.is_whitespace() || "`'\"()[]<>,;".contains(c)) {
        let Some(cite) = parse(word) else { continue };
        if inside(&cite.path, folder) && !out.contains(&cite) {
            out.push(cite);
        }
    }
    out
}

/// `path:line` or `path:line:col`, with a trailing `.`, `:` or `,` dropped.
fn parse(word: &str) -> Option<Cite> {
    let word = word.trim_end_matches(['.', ':', ',', '!', '?']);
    let mut parts = word.split(':');
    let path = parts.next()?;
    let line: u32 = parts.next()?.parse().ok()?;
    if let Some(col) = parts.next() {
        col.parse::<u32>().ok()?;
    }
    if parts.next().is_some() || line == 0 || path.is_empty() {
        return None;
    }
    // A file has a name with an extension: `10:31` and `v1:2` are not files.
    let name = Path::new(path).file_name()?.to_str()?;
    let ext = name.rsplit_once('.').map(|(_, e)| e)?;
    (!ext.is_empty() && ext.chars().all(|c| c.is_ascii_alphanumeric()) && ext.len() <= 8).then(
        || Cite {
            path: path.trim_start_matches("./").to_owned(),
            line,
        },
    )
}

/// `path` is a relative path without `..` that is a file of `folder`.
fn inside(path: &str, folder: &Path) -> bool {
    let p = Path::new(path);
    p.components()
        .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
        && folder.join(p).is_file()
}

/// The lines around `line` (two above, two below) of the file, numbered; the named one is flagged.
pub fn around(folder: &Path, cite: &Cite) -> Vec<(u32, String, bool)> {
    let Ok(text) = std::fs::read_to_string(folder.join(&cite.path)) else {
        return Vec::new();
    };
    let first = cite.line.saturating_sub(2).max(1);
    text.lines()
        .enumerate()
        .map(|(i, l)| (i as u32 + 1, l))
        .skip_while(|(n, _)| *n < first)
        .take_while(|(n, _)| *n <= cite.line + 2)
        .map(|(n, l)| (n, l.chars().take(120).collect(), n == cite.line))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("artoo-cites-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("src/login.rs"), "a\nb\nc\nd\ne\nf\n").unwrap();
        dir
    }

    #[test]
    fn a_file_of_the_folder_with_a_line_is_found() {
        let f = folder();
        let found = scan("The bug is in src/login.rs:4, fixed.", &f);
        assert_eq!(
            found,
            vec![Cite {
                path: "src/login.rs".into(),
                line: 4
            }]
        );
    }

    #[test]
    fn a_column_and_backticks_are_allowed() {
        let f = folder();
        let found = scan("see `src/login.rs:4:9` and ./src/login.rs:4", &f);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 4);
    }

    #[test]
    fn a_path_outside_the_folder_or_missing_is_not_found() {
        let f = folder();
        assert!(scan("../etc/passwd.txt:3 /etc/hosts.conf:1", &f).is_empty());
        assert!(scan("src/gone.rs:3", &f).is_empty());
    }

    #[test]
    fn a_time_is_not_a_file() {
        let f = folder();
        assert!(scan("Done at 10:31 and 10:31:05.", &f).is_empty());
    }

    #[test]
    fn the_lines_around_mark_the_named_one() {
        let f = folder();
        let lines = around(
            &f,
            &Cite {
                path: "src/login.rs".into(),
                line: 1,
            },
        );
        assert_eq!(lines.iter().map(|l| l.0).collect::<Vec<_>>(), [1, 2, 3]);
        assert!(lines[0].2 && !lines[1].2);
    }
}
