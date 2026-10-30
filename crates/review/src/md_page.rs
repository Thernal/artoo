//! A changed markdown file read as a page: the documents read as Source (kept between launches, per repository,
//! in `page-source.txt`), the removed sections and the blocks' comments opened, each document's rows compared.

use artoo_core::git::FileDiff;
use artoo_core::markdown::Block;
use artoo_core::mddiff::{self, Change, Row};
use artoo_core::storage;
use artoo_ui_kit::theme;
use gpui::Rgba;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

/// A document's rows, by the texts compared (their addresses).
type Rows = RefCell<HashMap<(usize, usize), Rc<Vec<Row>>>>;

const SOURCE_FILE: &str = "page-source.txt";

/// How changed documents are read, and their rows kept between frames.
#[derive(Default)]
pub struct Page {
    /// The documents read as Source (the ordinary diff of the text), by path; of the repository
    /// `source_of`.
    source: HashSet<String>,
    source_of: Option<String>,
    /// Removed sections opened, by path and the first old line.
    pub open: HashSet<(String, u32)>,
    /// Blocks whose comments are shown under them: path, side, first line.
    pub notes_open: HashSet<(String, bool, u32)>,
    /// A document's rows, by the texts compared.
    cache: Rows,
    /// One file at a time: the page's own scroll, its children the blocks.
    pub scroll: gpui::ScrollHandle,
    /// The changed blocks among those children, as last drawn; the one n / p last went to.
    pub changes: RefCell<Vec<usize>>,
    pub at: Option<usize>,
}

/// The paths of repository `repo` in the lines of `page-source.txt` (`repo<TAB>path`).
fn source_paths(text: &str, repo: &str) -> HashSet<String> {
    text.lines()
        .filter_map(|l| l.split_once('\t'))
        .filter(|(r, _)| *r == repo)
        .map(|(_, p)| p.to_owned())
        .collect()
}

/// `text` with repository `repo`'s lines replaced by `paths`; other repositories' kept.
fn with_source_paths(text: &str, repo: &str, paths: &HashSet<String>) -> String {
    let mut lines: Vec<String> = text
        .lines()
        .filter(|l| l.split_once('\t').is_some_and(|(r, _)| r != repo))
        .map(str::to_owned)
        .chain(paths.iter().map(|p| format!("{repo}\t{p}")))
        .collect();
    lines.sort_unstable();
    lines.join("\n") + "\n"
}

impl Page {
    /// Another file: n / p start again from the top of its view.
    pub fn leave(&mut self) {
        self.at = None;
    }

    /// Document `path` is read as Source.
    pub fn is_source(&self, path: &str) -> bool {
        self.source.contains(path)
    }

    /// The documents of repository `repo` read as Source, once per repository.
    pub fn load(&mut self, repo: &str) {
        if self.source_of.as_deref() != Some(repo) {
            self.source = source_paths(&storage::read(SOURCE_FILE), repo);
            self.source_of = Some(repo.to_owned());
        }
    }

    /// Document `path` read as Source or not; remembered.
    pub fn set_source(&mut self, path: &str, source: bool) -> bool {
        let changed = if source {
            self.source.insert(path.to_owned())
        } else {
            self.source.remove(path)
        };
        if changed && let Some(repo) = &self.source_of {
            storage::save(
                SOURCE_FILE,
                with_source_paths(&storage::read(SOURCE_FILE), repo, &self.source),
            );
        }
        changed
    }

    pub fn rows(&self, file: &FileDiff) -> Rc<Vec<Row>> {
        let key = |t: &Option<std::sync::Arc<str>>| {
            t.as_ref()
                .map_or(0, |t| std::sync::Arc::as_ptr(t) as *const u8 as usize)
        };
        let key = (key(&file.old_text), key(&file.new_text));
        let mut cache = self.cache.borrow_mut();
        if cache.len() > 16 {
            cache.clear();
        }
        cache
            .entry(key)
            .or_insert_with(|| {
                Rc::new(mddiff::diff(
                    file.old_text.as_deref().unwrap_or_default(),
                    file.new_text.as_deref().unwrap_or_default(),
                ))
            })
            .clone()
    }
}

/// The margin bar's colour for a change.
pub fn bar(change: Change) -> Option<Rgba> {
    match change {
        Change::Same => None,
        Change::Added => Some(theme::added()),
        Change::Changed => Some(theme::warning()),
        Change::Removed => Some(theme::removed()),
    }
}

/// The section a run of removed blocks starts: its heading, when it starts with one.
pub fn heading(block: &Block) -> Option<String> {
    match block {
        Block::Heading(_, i) => Some(i.text.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn documents_read_as_source_are_kept_per_repository() {
        let text = "a\tREADME.md\nb\tdocs/x.md\n";
        assert_eq!(
            source_paths(text, "a"),
            HashSet::from(["README.md".to_owned()])
        );
        let paths = HashSet::from(["CHANGELOG.md".to_owned()]);
        let saved = with_source_paths(text, "a", &paths);
        assert_eq!(saved, "a\tCHANGELOG.md\nb\tdocs/x.md\n");
        assert_eq!(
            source_paths(&saved, "b"),
            HashSet::from(["docs/x.md".to_owned()])
        );
    }
}
