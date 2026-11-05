//! Code's scope of search everywhere (⌘P, `@` in All; `artoo_shell_api::search`): the files of the diff on screen,
//! ranked by their names, then — once something is typed — the project's, read as ⌘K opens. A pick is the tab's
//! (`Host::pick_file`): a file of the diff picked there, else read in Code.

use crate::space::CodeSpace;
use artoo_code_api::Host;
use artoo_core::rank::{rank, score, word_matches};
use artoo_review_api::Review;
use artoo_shell_api::Services;
use artoo_shell_api::search::{AllOf, Ask, Find, Opens, Search};
use gpui::WeakEntity;
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::{Rc, Weak};
use std::sync::Arc;

/// The scope in a tab: the diff on screen through the review's handle there, the project's files through Code's.
pub fn scope(code: WeakEntity<CodeSpace>, services: Weak<Services>, host: Rc<dyn Host>) -> Search {
    let files: Rc<RefCell<Arc<Vec<String>>>> = Rc::default();
    let read = files.clone();
    Search {
        id: artoo_code_api::SEARCH,
        label: "Files",
        placeholder: "Files — this diff's first, then the repository's",
        go: "shell.search-files",
        order: 10,
        all: AllOf {
            few: 3,
            order: 20,
            prefix: Some('@'),
            alone: false,
        },
        find: Rc::new(move |ask, cx| {
            let diff = (services.upgrade())
                .and_then(|s| s.get::<Rc<dyn Review>>())
                .and_then(|review| review.shown(cx));
            let changed: Vec<&str> = (diff.iter())
                .flat_map(|d| d.files.iter().map(|f| f.path()))
                .collect();
            find(ask, &changed, &files.borrow(), &host)
        }),
        opened: Some(Rc::new(move |cx| {
            let found = (code.upgrade()).map(|c| c.read(cx).search_files());
            *read.borrow_mut() = found.unwrap_or_default();
        })),
    }
}

/// The diff's files ranked by their names, then those whose path has the words; the project's the same way, but
/// for those the diff has.
fn find(ask: Ask, changed: &[&str], files: &[String], host: &Rc<dyn Host>) -> Vec<Find> {
    let query = ask.text;
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    let matches = |hay: &str| words.iter().all(|w| word_matches(w, hay));
    let name = |path: &str| path.rsplit('/').next().unwrap_or_default().to_owned();
    let by_path =
        |path: &str| !words.is_empty() && matches(path) && score(query, &name(path)).is_none();
    // In All, a file is one of the Files.
    let group = |own: &'static str| if ask.all { "Files" } else { own };
    let pick = |path: &str, in_diff: bool| {
        let (host, path) = (host.clone(), path.to_owned());
        Opens::Run(Rc::new(move |cx| host.pick_file(path.clone(), in_diff, cx)))
    };
    let mut seen = HashSet::new();
    let mut finds = Vec::new();
    let in_diff = rank(changed.iter(), query, |p| {
        p.rsplit('/').next().unwrap_or_default()
    });
    for path in in_diff
        .into_iter()
        .chain(changed.iter().filter(|p| by_path(p)))
    {
        if seen.insert(*path) {
            finds.push(found(path, group("In this diff"), pick(path, true)));
        }
    }
    if !words.is_empty() {
        let by_name = rank(files.iter(), query, |f| {
            f.rsplit('/').next().unwrap_or_default()
        });
        let more = files.iter().filter(|f| by_path(f));
        finds.extend(
            by_name
                .into_iter()
                .chain(more)
                .filter(|f| seen.insert(f.as_str()))
                .take(60)
                .map(|f| found(f, group("In the repository"), pick(f, false))),
        );
    }
    finds
}

/// A file found: its name, which leads, and its folder, which follows it fainter — as IDEs list files.
fn found(path: &str, group: &'static str, open: Opens) -> Find {
    let (label, detail) = match path.rsplit_once('/') {
        Some((folder, name)) => (name.to_owned(), folder.to_owned()),
        None => (path.to_owned(), String::new()),
    };
    Find::new(group, label, detail, open).file(path.to_owned())
}
