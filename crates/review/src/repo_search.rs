//! What the repository's search (#206) reads and how, apart from what it draws: no file is read until a query needs
//! it, each is searched whole as bytes by one compiled pattern, a line is cut out only where a hit is, and what was
//! read stays in a small cache so the next, longer query reads nothing again.

use regex::bytes::{Regex, RegexBuilder};
use std::collections::HashMap;
use std::ops::Range;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

/// No file longer than this is searched: a minified bundle is not read.
pub(crate) const MAX_BYTES: u64 = 1 << 20;
/// The bytes kept between queries.
const CACHE_BYTES: usize = 64 << 20;
/// A line longer than this is shown around its hit.
const LONG_LINE: usize = 600;

/// The repository's files as git's index lists them (no submodule, no link): paths only, nothing read.
pub(crate) fn repository_paths(root: &Path) -> Vec<String> {
    let Ok(repo) = git2::Repository::open(root) else {
        return Vec::new();
    };
    let Ok(index) = repo.index() else {
        return Vec::new();
    };
    index
        .iter()
        // A blob, plain or executable; a link (0o120000) and a submodule (0o160000) are not text to search.
        .filter(|e| e.mode & 0o170000 == 0o100000)
        .map(|e| String::from_utf8_lossy(&e.path).into_owned())
        .collect()
}

/// A query made ready to search with.
pub(crate) struct Compiled {
    re: Regex,
    /// A plain query asked for as a whole word: a hit must stand alone, checked by hand (`\b` is wrong beside a
    /// sign: `->` has none).
    word: bool,
    /// The query is plain text (not a regular expression): a longer one can be looked for only where it was.
    pub plain: bool,
}

/// `query` compiled under the toggles; the regular expression's error, in a line, when it is not one.
pub(crate) fn compile(
    query: &str,
    case: bool,
    word: bool,
    regex: bool,
) -> Result<Compiled, String> {
    let source = if !regex {
        regex::escape(query)
    } else if word {
        format!(r"\b(?:{query})\b")
    } else {
        query.to_owned()
    };
    let re = RegexBuilder::new(&source)
        .case_insensitive(!case)
        .build()
        .map_err(|e| match e {
            regex::Error::Syntax(s) => s.lines().last().unwrap_or("").trim().to_owned(),
            other => other.to_string(),
        })?;
    Ok(Compiled {
        re,
        word: word && !regex,
        plain: !regex,
    })
}

/// A line a query found: its number, its text (cut around the hit when it is long) and the hit in that text.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Line {
    pub line: u32,
    pub text: String,
    pub range: Range<usize>,
}

/// What a file is to a query.
#[derive(Debug, Default)]
pub(crate) struct InFile {
    /// It holds the query's text, a whole word or not: a longer query can be in it.
    pub holds: bool,
    /// It has a hit.
    pub hit: bool,
    /// Its lines with a hit, the first hit of each, no more than asked for.
    pub lines: Vec<Line>,
}

/// Whether the char before `at` in `bytes` and the one from `end` are not a word's (letters, digits, `_`).
fn stands_alone(bytes: &[u8], at: usize, end: usize) -> bool {
    let wordy = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_');
    let before = (1..=4.min(at))
        .find_map(|n| std::str::from_utf8(&bytes[at - n..at]).ok())
        .and_then(|s| s.chars().next_back());
    let after = (1..=4.min(bytes.len() - end))
        .find_map(|n| std::str::from_utf8(&bytes[end..end + n]).ok())
        .and_then(|s| s.chars().next());
    !wordy(before) && !wordy(after)
}

/// The lines of `bytes` that `q` finds, the first hit on each and at most `room` of them.
pub(crate) fn search(bytes: &[u8], q: &Compiled, room: usize) -> InFile {
    let mut found = InFile::default();
    // The line counted so far: its number, where it starts; and where the last line with a hit ends.
    let (mut number, mut counted) = (1u32, 0usize);
    let mut done_to = 0usize;
    for m in q.re.find_iter(bytes) {
        if m.is_empty() {
            continue;
        }
        found.holds = true;
        if m.start() < done_to || (q.word && !stands_alone(bytes, m.start(), m.end())) {
            continue;
        }
        found.hit = true;
        if found.lines.len() >= room {
            break;
        }
        let start = bytes[..m.start()]
            .iter()
            .rposition(|&b| b == b'\n')
            .map_or(0, |n| n + 1);
        let end = bytes[m.end()..]
            .iter()
            .position(|&b| b == b'\n')
            .map_or(bytes.len(), |n| m.end() + n);
        number += bytes[counted..start]
            .iter()
            .filter(|&&b| b == b'\n')
            .count() as u32;
        counted = start;
        done_to = end;
        found
            .lines
            .push(cut_line(bytes, start..end, m.range(), number));
    }
    found
}

/// The line `line` (`span` of `bytes`) as shown: around the hit when long, its `\r` gone.
fn cut_line(bytes: &[u8], span: Range<usize>, hit: Range<usize>, number: u32) -> Line {
    let mut from = span.start;
    let mut to = span.end;
    if bytes[from..to].ends_with(b"\r") {
        to -= 1;
    }
    let long = to - from > LONG_LINE;
    if long {
        from = hit.start.saturating_sub(LONG_LINE / 4).max(from);
        to = (hit.end + LONG_LINE / 2).min(to);
        // On a char's start: a continuation byte is 0b10xx_xxxx.
        while from > span.start && bytes[from] & 0xC0 == 0x80 {
            from -= 1;
        }
        while to < span.end && to > from && bytes[to] & 0xC0 == 0x80 {
            to += 1;
        }
    }
    let lossy = |r: Range<usize>| String::from_utf8_lossy(&bytes[r]).into_owned();
    let lead = if long && from > span.start { "…" } else { "" };
    let before = lossy(from..hit.start.max(from));
    let found = lossy(hit.start.max(from)..hit.end.min(to));
    let after = lossy(hit.end.min(to)..to);
    let start = lead.len() + before.len();
    Line {
        line: number,
        range: start..start + found.len(),
        text: format!("{lead}{before}{found}{after}"),
    }
}

/// A file's bytes as last read, by its index in the paths: none when it is binary or too long.
struct Kept {
    modified: Option<SystemTime>,
    len: u64,
    bytes: Option<Arc<[u8]>>,
    used: u64,
}

/// What was read, kept up to `CACHE_BYTES`; a file whose time or length changed is read again.
#[derive(Default)]
pub(crate) struct Cache {
    files: HashMap<usize, Kept>,
    size: usize,
    tick: u64,
}

impl Cache {
    fn get(
        &mut self,
        file: usize,
        modified: Option<SystemTime>,
        len: u64,
    ) -> Option<Option<Arc<[u8]>>> {
        self.tick += 1;
        let tick = self.tick;
        let kept = self.files.get_mut(&file)?;
        if kept.modified != modified || kept.len != len {
            return None;
        }
        kept.used = tick;
        Some(kept.bytes.clone())
    }

    fn put(
        &mut self,
        file: usize,
        modified: Option<SystemTime>,
        len: u64,
        bytes: Option<Arc<[u8]>>,
    ) {
        self.tick += 1;
        let size = bytes.as_ref().map_or(0, |b| b.len());
        if let Some(old) = self.files.insert(
            file,
            Kept {
                modified,
                len,
                bytes,
                used: self.tick,
            },
        ) {
            self.size -= old.bytes.map_or(0, |b| b.len());
        }
        self.size += size;
        while self.size > CACHE_BYTES {
            let Some((&oldest, _)) = self.files.iter().min_by_key(|(_, k)| k.used) else {
                break;
            };
            if let Some(gone) = self.files.remove(&oldest) {
                self.size -= gone.bytes.map_or(0, |b| b.len());
            }
        }
    }
}

/// File `file` (`path` under `root`), from the cache or the disk: none when it is gone, a folder, too long or binary.
fn read(root: &Path, path: &str, file: usize, cache: &Mutex<Cache>) -> Option<Arc<[u8]>> {
    let meta = std::fs::metadata(root.join(path)).ok()?;
    if !meta.is_file() || meta.len() > MAX_BYTES {
        return None;
    }
    let modified = meta.modified().ok();
    if let Some(kept) = cache.lock().ok()?.get(file, modified, meta.len()) {
        return kept;
    }
    let bytes = std::fs::read(root.join(path)).ok()?;
    let bytes = (!artoo_core::files::binary(&bytes)).then(|| Arc::<[u8]>::from(bytes));
    if let Ok(mut cache) = cache.lock() {
        cache.put(file, modified, meta.len(), bytes.clone());
    }
    bytes
}

/// A file to look at: always, or (narrowing) only when it changed after `since`.
#[derive(Clone, Copy)]
pub(crate) struct Look {
    pub file: usize,
    pub always: bool,
}

/// What a scan shares: where, what, which files, what was read, and whether the hits are all in (`full`: only
/// whether files hold the query is still wanted).
pub(crate) struct Scan {
    pub root: std::path::PathBuf,
    pub paths: Arc<Vec<String>>,
    pub query: Compiled,
    pub cache: Arc<Mutex<Cache>>,
    pub full: AtomicBool,
    pub since: Option<SystemTime>,
}

/// `chunk` of the files searched, in order: each one looked at with what it is to the query, at most `room` lines.
pub(crate) fn scan(scan: &Scan, chunk: &[Look], room: usize) -> Vec<(usize, InFile)> {
    let mut out = Vec::new();
    for look in chunk {
        let Some(path) = scan.paths.get(look.file) else {
            continue;
        };
        if !look.always {
            // Not where the last query was: only a file changed since could hold this one.
            let changed = std::fs::metadata(scan.root.join(path))
                .and_then(|m| m.modified())
                .ok()
                .zip(scan.since)
                .is_none_or(|(m, since)| m >= since);
            if !changed {
                continue;
            }
        }
        let Some(bytes) = read(&scan.root, path, look.file, &scan.cache) else {
            continue;
        };
        let room = if scan.full.load(Ordering::Relaxed) {
            0
        } else {
            room
        };
        let found = search(&bytes, &scan.query, room);
        if found.holds {
            out.push((look.file, found));
        }
    }
    out
}

/// The last query that was searched to its end: the files that hold its text.
pub(crate) struct Narrow {
    pub query: String,
    pub case: bool,
    pub files: Vec<usize>,
    pub since: SystemTime,
}

impl Narrow {
    /// Whether plain `query` (with `case`) can be looked for only where this one was: it holds this one's text.
    pub fn applies(&self, query: &str, case: bool) -> bool {
        if case != self.case || query.is_empty() {
            return false;
        }
        if case {
            query.contains(&self.query)
        } else {
            // Folded the same way the search folds: only ASCII is sure to.
            query.is_ascii()
                && self.query.is_ascii()
                && query
                    .to_ascii_lowercase()
                    .contains(&self.query.to_ascii_lowercase())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(
        text: &str,
        query: &str,
        case: bool,
        word: bool,
        regex: bool,
    ) -> Vec<(u32, String, String)> {
        let q = compile(query, case, word, regex).unwrap();
        search(text.as_bytes(), &q, 100)
            .lines
            .into_iter()
            .map(|l| (l.line, l.text[l.range.clone()].to_owned(), l.text))
            .collect()
    }

    #[test]
    fn finds_the_first_hit_of_each_line_with_its_number() {
        let text = "one\nlet Backoff = backoff;\n\nthree backoff\n";
        assert_eq!(
            lines(text, "backoff", false, false, false),
            vec![
                (2, "Backoff".into(), "let Backoff = backoff;".into()),
                (4, "backoff".into(), "three backoff".into()),
            ]
        );
        assert_eq!(lines(text, "backoff", true, false, false).len(), 2);
        assert_eq!(lines(text, "Backoff", true, false, false).len(), 1);
    }

    #[test]
    fn a_whole_word_stands_alone_beside_signs_too() {
        assert_eq!(
            lines(
                "backoffs\nbackoff_delay backoff",
                "backoff",
                false,
                true,
                false
            )
            .iter()
            .map(|l| l.0)
            .collect::<Vec<_>>(),
            vec![2]
        );
        // `\b` would refuse it: no word char on either side of `->`.
        assert_eq!(lines("a -> b", "->", false, true, false).len(), 1);
        assert_eq!(lines("x->y", "->", false, true, false).len(), 0);
    }

    #[test]
    fn a_file_holds_a_word_it_has_only_inside_another() {
        let q = compile("backoff", false, true, false).unwrap();
        let found = search(b"backoffs", &q, 10);
        assert!(found.holds && !found.hit);
    }

    #[test]
    fn a_regular_expression_and_its_error() {
        assert_eq!(
            lines("fn retry_after(", r"retry_\w+", false, false, true)[0].1,
            "retry_after"
        );
        assert!(compile("(", false, false, true).is_err());
        // Plain text is never a pattern.
        assert_eq!(lines("a(b", "(", false, false, false).len(), 1);
    }

    #[test]
    fn a_long_line_is_cut_around_its_hit() {
        let text = format!("{}needle{}\r\n", "a".repeat(5000), "b".repeat(5000));
        let found = lines(&text, "needle", true, false, false);
        let (_, hit, shown) = &found[0];
        assert_eq!(hit, "needle");
        assert!(shown.starts_with('…') && shown.len() < LONG_LINE + 10);
        assert!(!shown.ends_with('\r'));
    }

    #[test]
    fn room_stops_the_lines_not_the_answer() {
        let q = compile("x", false, false, false).unwrap();
        let found = search(b"x\nx\nx", &q, 0);
        assert!(found.holds && found.hit && found.lines.is_empty());
    }

    #[test]
    fn a_longer_query_narrows_a_plain_one() {
        let narrow = Narrow {
            query: "refu".into(),
            case: false,
            files: vec![],
            since: SystemTime::now(),
        };
        assert!(narrow.applies("Refund", false));
        assert!(!narrow.applies("Refund", true));
        assert!(!narrow.applies("ref", false));
    }

    #[test]
    fn a_narrowed_scan_looks_elsewhere_only_at_files_changed_since() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.kt"), "val refund = 1").unwrap();
        std::fs::write(dir.path().join("b.kt"), "val refund = 2").unwrap();
        let paths = Arc::new(vec!["a.kt".to_owned(), "b.kt".to_owned()]);
        let scan_since = |since: SystemTime| Scan {
            root: dir.path().to_owned(),
            paths: paths.clone(),
            query: compile("refund", false, false, false).unwrap(),
            cache: Arc::default(),
            full: AtomicBool::new(false),
            since: Some(since),
        };
        // b.kt did not hold the last query and has not changed since: not read.
        let looks = [
            Look {
                file: 0,
                always: true,
            },
            Look {
                file: 1,
                always: false,
            },
        ];
        let later = SystemTime::now() + std::time::Duration::from_secs(60);
        let found = scan(&scan_since(later), &looks, 10);
        assert_eq!(found.iter().map(|(f, _)| *f).collect::<Vec<_>>(), [0]);
        // Changed after the last query: read.
        let earlier = SystemTime::now() - std::time::Duration::from_secs(60);
        let found = scan(&scan_since(earlier), &looks, 10);
        assert_eq!(found.iter().map(|(f, _)| *f).collect::<Vec<_>>(), [0, 1]);
        // Full: whether a file holds it, no lines.
        let full = scan_since(earlier);
        full.full.store(true, Ordering::Relaxed);
        assert!(
            scan(&full, &looks, 10)
                .iter()
                .all(|(_, f)| f.hit && f.lines.is_empty())
        );
    }

    /// What a search costs on this repository: listing, a first query read from the disk, a longer one narrowed.
    #[test]
    fn this_repository_is_searched_lazily() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let started = std::time::Instant::now();
        let paths = Arc::new(repository_paths(&root));
        let listed = started.elapsed();
        let cache: Arc<Mutex<Cache>> = Arc::default();
        let run = |query: &str, only: Option<&[usize]>| {
            let scan = Scan {
                root: root.clone(),
                paths: paths.clone(),
                query: compile(query, false, false, false).unwrap(),
                cache: cache.clone(),
                full: AtomicBool::new(false),
                since: Some(SystemTime::now()),
            };
            let looks: Vec<Look> = (0..paths.len())
                .map(|file| Look {
                    file,
                    always: only.is_none_or(|o| o.contains(&file)),
                })
                .collect();
            let at = std::time::Instant::now();
            let found = scan_all(&scan, &looks);
            (found, at.elapsed())
        };
        let (first, took_first) = run("refu", None);
        let holding: Vec<usize> = first.iter().map(|(f, _)| *f).collect();
        let (second, took_second) = run("refund", Some(&holding));
        eprintln!(
            "search: {} paths listed in {listed:?}; `refu` {} files in {took_first:?}; `refund` {} in {took_second:?}",
            paths.len(),
            first.len(),
            second.len()
        );
        assert!(!first.is_empty() && second.len() <= first.len());
    }

    fn scan_all(s: &Scan, looks: &[Look]) -> Vec<(usize, InFile)> {
        scan(s, looks, usize::MAX)
    }

    #[test]
    fn the_paths_are_the_index_blobs_only() {
        let dir = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();
        std::fs::write(dir.path().join("a.kt"), "x").unwrap();
        std::fs::create_dir(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/b.kt"), "y").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("a.kt")).unwrap();
        index.add_path(Path::new("src/b.kt")).unwrap();
        index.write().unwrap();
        assert_eq!(repository_paths(dir.path()), ["a.kt", "src/b.kt"]);
    }

    #[test]
    fn the_cache_reads_a_file_again_once_it_changed() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "first").unwrap();
        let cache = Mutex::new(Cache::default());
        assert_eq!(&*read(dir.path(), "a.txt", 0, &cache).unwrap(), b"first");
        std::fs::write(dir.path().join("a.txt"), "second!").unwrap();
        assert_eq!(&*read(dir.path(), "a.txt", 0, &cache).unwrap(), b"second!");
        std::fs::write(dir.path().join("b.bin"), b"\0\x01").unwrap();
        assert!(read(dir.path(), "b.bin", 1, &cache).is_none());
    }
}
