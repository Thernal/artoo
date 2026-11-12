//! Which files are generated or locked, so a diff can fold them: nobody reviews a 20 000-line
//! lock file by reading it. The repository says so first — its `.gitattributes`' `linguist-generated`, as GitHub reads
//! it (#205) —, then the file's name.

use regex::Regex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

/// Files whose whole name says they are written by a tool.
const NAMES: &[&str] = &[
    "Cargo.lock",
    "package-lock.json",
    "npm-shrinkwrap.json",
    "yarn.lock",
    "pnpm-lock.yaml",
    "bun.lock",
    "Podfile.lock",
    "Gemfile.lock",
    "poetry.lock",
    "composer.lock",
    "go.sum",
    "pubspec.lock",
    "Package.resolved",
    "flake.lock",
    "mix.lock",
    "gradle.lockfile",
];

/// Endings that mark a generated or minified file.
const ENDINGS: &[&str] = &[
    ".lock",
    ".min.js",
    ".min.css",
    ".map",
    ".pb.go",
    "_pb2.py",
    ".g.dart",
    ".freezed.dart",
    ".designer.cs",
];

/// A file with fewer changed lines than this is shown even if it is generated.
pub const FOLD_ABOVE: usize = 40;

/// Whether `path` is generated in the repository at `root`: what its `.gitattributes` says, else its name.
pub fn is_generated_in(root: Option<&Path>, path: &str) -> bool {
    (root.and_then(|r| attributes(r).generated(path))).unwrap_or_else(|| is_generated(path))
}

/// What a repository's `.gitattributes` says of `linguist-generated`: each line's pattern and what it sets — true,
/// false, or back to unsaid (`!`) — in the file's order, the last line that matches winning (gitattributes(5)).
#[derive(Debug, Default)]
pub struct Attributes {
    rules: Vec<(Regex, Option<bool>)>,
}

impl Attributes {
    pub fn parse(text: &str) -> Self {
        let rules = (text.lines())
            .filter_map(|line| {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    return None;
                }
                let mut words = line.split_whitespace();
                let pattern = words.next()?;
                let said = (words)
                    .filter_map(|w| match w {
                        "linguist-generated" | "linguist-generated=true" => Some(Some(true)),
                        "-linguist-generated" | "linguist-generated=false" => Some(Some(false)),
                        "!linguist-generated" => Some(None),
                        _ => None,
                    })
                    .next_back()?;
                Some((pattern_regex(pattern)?, said))
            })
            .collect();
        Self { rules }
    }

    /// What it says of `path`, from the repository's root; None when no line does.
    pub fn generated(&self, path: &str) -> Option<bool> {
        (self.rules.iter().rev())
            .find(|(pattern, _)| pattern.is_match(path))
            .and_then(|(_, said)| *said)
    }
}

/// A `.gitattributes` pattern as a regular expression over a path from the root: one with no `/` matches a name at any
/// depth, one with a `/` the path from the root; `*` and `?` stay within a folder, `**` crosses them. A folder's
/// pattern (`dist/`) reaches no file in it, as git reads it.
fn pattern_regex(pattern: &str) -> Option<Regex> {
    if pattern.ends_with('/') {
        return None;
    }
    let anchored = pattern.contains('/');
    let chars: Vec<char> = pattern
        .strip_prefix('/')
        .unwrap_or(pattern)
        .chars()
        .collect();
    let mut re = String::from(if anchored { "^" } else { "(^|/)" });
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '*' if chars.get(i + 1) == Some(&'*') => {
                let folder_start = i == 0 || chars[i - 1] == '/';
                if folder_start && chars.get(i + 2) == Some(&'/') {
                    re.push_str("(.*/)?");
                    i += 3;
                } else {
                    re.push_str(".*");
                    i += 2;
                }
                continue;
            }
            '*' => re.push_str("[^/]*"),
            '?' => re.push_str("[^/]"),
            '[' => match chars[i + 1..].iter().position(|&c| c == ']') {
                Some(end) => {
                    let class: String = chars[i + 1..i + 1 + end].iter().collect();
                    let class = match class.strip_prefix('!') {
                        Some(rest) => format!("^{rest}"),
                        None => class,
                    };
                    re.push('[');
                    re.push_str(&class.replace('\\', "\\\\"));
                    re.push(']');
                    i += end + 2;
                    continue;
                }
                None => re.push_str("\\["),
            },
            c => re.push_str(&regex::escape(c.encode_utf8(&mut [0; 4]))),
        }
        i += 1;
    }
    re.push('$');
    Regex::new(&re).ok()
}

/// A repository's `.gitattributes` as last read: when it was looked at, its file's time, what it says.
struct Read {
    checked: Instant,
    stamp: Option<SystemTime>,
    attributes: Arc<Attributes>,
}

static READ: Mutex<Option<HashMap<PathBuf, Read>>> = Mutex::new(None);

/// The `.gitattributes` at `root`, read again when its file changed — looked at no more than every two seconds, since
/// a diff asks for each file as it draws.
pub fn attributes(root: &Path) -> Arc<Attributes> {
    let mut read = READ.lock().unwrap_or_else(|e| e.into_inner());
    let read = read.get_or_insert_with(HashMap::new);
    let now = Instant::now();
    if let Some(r) = read.get(root)
        && now.duration_since(r.checked) < Duration::from_secs(2)
    {
        return r.attributes.clone();
    }
    let file = root.join(".gitattributes");
    let stamp = std::fs::metadata(&file).and_then(|m| m.modified()).ok();
    if let Some(r) = read.get_mut(root)
        && r.stamp == stamp
    {
        r.checked = now;
        return r.attributes.clone();
    }
    let text = std::fs::read_to_string(&file).unwrap_or_default();
    let attributes = Arc::new(Attributes::parse(&text));
    read.insert(
        root.to_owned(),
        Read {
            checked: now,
            stamp,
            attributes: attributes.clone(),
        },
    );
    attributes
}

/// Whether `path` is generated by its name alone: a lock file, a minified one, one in a `generated` folder.
pub fn is_generated(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    NAMES.contains(&name)
        || ENDINGS.iter().any(|e| name.ends_with(e))
        || name.contains(".generated.")
        || path.contains("/generated/")
        || path.contains("/__generated__/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_and_generated_files_are_recognised() {
        for path in [
            "Cargo.lock",
            "web/package-lock.json",
            "app/build/generated/Foo.kt",
            "dist/app.min.js",
            "api/user.pb.go",
            "lib/model.g.dart",
            "src/schema.generated.ts",
        ] {
            assert!(is_generated(path), "{path}");
        }
    }

    #[test]
    fn the_repositorys_attributes_come_before_the_name() {
        let attributes = Attributes::parse(
            "# GitHub's word for what it folds\n\
             *.snap linguist-generated\n\
             api/openapi/** linguist-generated=true\n\
             /docs/*.html linguist-generated\n\
             Cargo.lock -linguist-generated\n\
             vendor/ linguist-generated\n\
             **/fixtures/[a-c]*.json linguist-generated\n\
             api/openapi/README.md !linguist-generated\n\
             *.txt text eol=lf\n",
        );
        let said = |path: &str| attributes.generated(path);
        assert_eq!(said("src/__snapshots__/list.snap"), Some(true));
        assert_eq!(said("api/openapi/client/user.ts"), Some(true));
        assert_eq!(said("docs/index.html"), Some(true));
        // Anchored to the root: not a docs folder deeper down, nor a file deeper in it.
        assert_eq!(said("site/docs/index.html"), None);
        assert_eq!(said("docs/api/index.html"), None);
        // Told not generated, though its name would fold it.
        assert_eq!(said("Cargo.lock"), Some(false));
        // A folder's pattern reaches nothing in it.
        assert_eq!(said("vendor/lib.js"), None);
        assert_eq!(said("tests/fixtures/big.json"), Some(true));
        assert_eq!(said("tests/fixtures/zed.json"), None);
        // The last line that matches wins, and `!` takes the word back.
        assert_eq!(said("api/openapi/README.md"), None);
        assert_eq!(said("notes.txt"), None);
    }

    #[test]
    fn ordinary_files_are_not() {
        for path in [
            "src/main.rs",
            "README.md",
            "lockfile_docs.md",
            "src/generator.rs",
        ] {
            assert!(!is_generated(path), "{path}");
        }
    }
}
