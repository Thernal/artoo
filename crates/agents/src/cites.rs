//! `path:line` in the agent's words (#220): a file of the session's folder named with a line — `src/login.rs:44`,
//! `src/login.rs:44:9`, in backticks or bare — is found, and drawn in the sentence as the file's glyph and
//! `name:line` in a hairline (`talk`). Only a path that is a file inside the folder counts; a time (`10:31`) names none.

use artoo_ui_kit::markdown::{CITE_PAD, CITE_SCHEME};
use std::path::{Component, Path};

/// One citation: the path as written (relative to the folder) and its line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cite {
    pub path: String,
    pub line: u32,
}

/// `text` with each `path:line` that names a file inside `folder` written as a cite link (`ui_kit::markdown`), in
/// the sentence where it stands: `name:line`, its backticks gone when they held it alone. Left as written: inside a
/// fenced block, inside code with other words, inside a link.
pub fn inline(text: &str, folder: &Path) -> String {
    let mut fenced = false;
    text.split('\n')
        .map(|line| {
            if line.trim_start().starts_with("```") {
                fenced = !fenced;
                return line.to_owned();
            }
            if fenced {
                line.to_owned()
            } else {
                inline_line(line, folder)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn inline_line(line: &str, folder: &Path) -> String {
    let split = |c: char| c.is_whitespace() || "`'\"()[]<>,;".contains(c);
    let mut out = String::new();
    // How far of `line` is written to `out`.
    let mut done = 0;
    let mut from = 0;
    for (end, sep) in line
        .char_indices()
        .filter(|(_, c)| split(*c))
        .chain([(line.len(), ' ')])
    {
        let start = from;
        from = end + sep.len_utf8();
        let word = &line[start.min(end)..end];
        let Some((cite, len)) = parse(word) else {
            continue;
        };
        if !inside(&cite.path, folder) {
            continue;
        }
        let (mut a, mut b) = (start, start + len);
        let before = &line[..a];
        if before.ends_with("](") || before.rfind('[') > before.rfind(']') {
            continue;
        }
        if before.matches('`').count() % 2 == 1 {
            // Inside code: only code that is the cite alone becomes one.
            let whole = end - start == word.len() && line[end..].starts_with('`');
            if !(before.ends_with('`') && whole && start + len == end) {
                continue;
            }
            a -= 1;
            b += 1;
        }
        out.push_str(&line[done..a]);
        out.push_str(&link(&cite));
        done = b;
    }
    out.push_str(&line[done..]);
    out
}

fn link(cite: &Cite) -> String {
    let name = cite.path.rsplit('/').next().unwrap_or(&cite.path);
    let mut shown = String::new();
    for c in name.chars() {
        if "\\`*_[]~".contains(c) {
            shown.push('\\');
        }
        shown.push(c);
    }
    format!(
        "[{}{shown}:{}{}]({CITE_SCHEME}{}:{})",
        CITE_PAD.0, cite.line, CITE_PAD.1, cite.path, cite.line
    )
}

/// The cite a link's address (after the scheme) names.
pub fn of_address(address: &str) -> Option<Cite> {
    let (path, line) = address.rsplit_once(':')?;
    Some(Cite {
        path: path.to_owned(),
        line: line.parse().ok()?,
    })
}

/// `path:line` or `path:line:col`, with a trailing `.`, `:` or `,` dropped, and the length of what is left.
fn parse(word: &str) -> Option<(Cite, usize)> {
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
        || {
            let cite = Cite {
                path: path.trim_start_matches("./").to_owned(),
                line,
            };
            (cite, word.len())
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

    fn cite(line: u32) -> String {
        link(&Cite {
            path: "src/login.rs".into(),
            line,
        })
    }

    #[test]
    fn a_cite_inside_a_sentence_keeps_the_words_around_it() {
        let f = folder();
        assert_eq!(
            inline("The bug is in src/login.rs:4, fixed.", &f),
            format!("The bug is in {}, fixed.", cite(4))
        );
        assert_eq!(of_address("src/login.rs:4").unwrap().line, 4);
    }

    #[test]
    fn a_column_and_backticks_of_their_own_are_taken() {
        let f = folder();
        assert_eq!(
            inline("see `src/login.rs:4:9` and ./src/login.rs:4.", &f),
            format!("see {} and {}.", cite(4), cite(4))
        );
    }

    #[test]
    fn code_with_other_words_and_links_stay_as_written() {
        let f = folder();
        for text in [
            "`cat src/login.rs:4` ran",
            "[src/login.rs:4](https://example.com)",
            "```\nsrc/login.rs:4\n```",
        ] {
            assert_eq!(inline(text, &f), text);
        }
    }

    #[test]
    fn a_path_outside_the_folder_or_missing_is_not_found() {
        let f = folder();
        for text in ["../etc/passwd.txt:3 /etc/hosts.conf:1", "src/gone.rs:3"] {
            assert_eq!(inline(text, &f), text);
        }
    }

    #[test]
    fn a_time_is_not_a_file() {
        let f = folder();
        assert_eq!(
            inline("Done at 10:31 and 10:31:05.", &f),
            "Done at 10:31 and 10:31:05."
        );
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
