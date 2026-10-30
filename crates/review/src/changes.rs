//! The Changes list's rows: the diff's files as a tree or a flat list, or those whose path holds the words typed;
//! the working tree's two groups (#22) each under its header, folding on its own.

use artoo_core::git::FileDiff;
use std::collections::{BTreeMap, HashSet};

/// One row of the files island.
pub enum FileItem {
    /// A group's header (#22): its fold, its name, how many files it holds.
    Group {
        group: Group,
        count: usize,
        folded: bool,
    },
    Dir {
        path: String,
        name: String,
        depth: usize,
        count: usize,
        collapsed: bool,
    },
    File {
        ix: usize,
        depth: usize,
    },
}

#[derive(Default)]
struct Node {
    dirs: BTreeMap<String, Node>,
    files: Vec<usize>,
}

impl Node {
    fn count(&self) -> usize {
        self.files.len() + self.dirs.values().map(Node::count).sum::<usize>()
    }
}

/// The tree's rows: folders before files, a chain of folders with nothing else folded into one
/// (`src/ui`), a collapsed folder hiding what is under it.
fn flatten(
    node: &Node,
    depth: usize,
    prefix: &str,
    collapsed: &HashSet<String>,
    out: &mut Vec<FileItem>,
) {
    for (name, child) in &node.dirs {
        let mut path = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        let mut shown = name.clone();
        let mut cur = child;
        while cur.files.is_empty() && cur.dirs.len() == 1 {
            let (n, c) = cur.dirs.iter().next().expect("one folder");
            path = format!("{path}/{n}");
            shown = format!("{shown}/{n}");
            cur = c;
        }
        let is_collapsed = collapsed.contains(&path);
        out.push(FileItem::Dir {
            path: path.clone(),
            name: shown,
            depth,
            count: cur.count(),
            collapsed: is_collapsed,
        });
        if !is_collapsed {
            flatten(cur, depth + 1, &path, collapsed, out);
        }
    }
    out.extend(node.files.iter().map(|&ix| FileItem::File { ix, depth }));
}

/// The files of `items`, in their order.
pub fn files_of(items: Vec<FileItem>) -> Vec<usize> {
    items
        .into_iter()
        .filter_map(|item| match item {
            FileItem::File { ix, .. } => Some(ix),
            FileItem::Dir { .. } | FileItem::Group { .. } => None,
        })
        .collect()
}

/// Whether `path` is under folder `dir` (a chain like `src/ui` is one folder of the tree).
pub fn inside(path: &str, dir: &str) -> bool {
    path.strip_prefix(dir)
        .is_some_and(|rest| rest.starts_with('/'))
}

/// The working tree's two groups (#22, A): what is staged, then what is not.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Group {
    Staged,
    NotStaged,
}

impl Group {
    /// The group file `ix` is in, when the files are grouped with the first `staged` staged.
    pub fn of(staged: Option<usize>, ix: usize) -> Option<Group> {
        staged.map(|n| {
            if ix < n {
                Group::Staged
            } else {
                Group::NotStaged
            }
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Group::Staged => "Staged",
            Group::NotStaged => "Not staged",
        }
    }

    /// Its key among the folded folders, and the start of its folders' keys: no path in git
    /// starts with a slash, so the two groups' folders fold apart.
    pub fn key(self) -> &'static str {
        match self {
            Group::Staged => "/staged",
            Group::NotStaged => "/not-staged",
        }
    }
}

/// A row's key in the tree: a file's path, in a group under the group's key (`/staged/src/a.kt`),
/// so a folder's key is a start of the keys of the files under it.
pub fn row_key(staged: Option<usize>, ix: usize, path: &str) -> String {
    match Group::of(staged, ix) {
        Some(group) => format!("{}/{path}", group.key()),
        None => path.to_owned(),
    }
}

/// The folder a folder's key names, without its group: `/staged/src` → `src`.
pub fn folder_of(key: &str) -> &str {
    [Group::Staged, Group::NotStaged]
        .into_iter()
        .find_map(|g| key.strip_prefix(g.key()))
        .map_or(key, |rest| rest.trim_start_matches('/'))
}

/// The rows for `files`: a tree, a flat list, or — whatever `list` says — a flat list of the files
/// whose path holds every word of `filter`. With `staged` they are in two groups, each under its
/// header and folding on its own, a filter typed opening both; a group with no file is left out.
pub fn file_items(
    files: &[FileDiff],
    list: bool,
    filter: &str,
    collapsed: &HashSet<String>,
    staged: Option<usize>,
) -> Vec<FileItem> {
    let words: Vec<String> = filter.split_whitespace().map(str::to_lowercase).collect();
    let Some(staged) = staged else {
        return file_rows(files, 0..files.len(), list, &words, collapsed, "");
    };
    let mut out = Vec::new();
    for (group, ixs) in [
        (Group::Staged, 0..staged.min(files.len())),
        (Group::NotStaged, staged.min(files.len())..files.len()),
    ] {
        let rows = file_rows(files, ixs.clone(), list, &words, collapsed, group.key());
        let count = ixs.filter(|&ix| matches(&files[ix], &words)).count();
        if count == 0 {
            continue;
        }
        let folded = words.is_empty() && collapsed.contains(group.key());
        out.push(FileItem::Group {
            group,
            count,
            folded,
        });
        if !folded {
            out.extend(rows);
        }
    }
    out
}

/// Whether `file`'s path holds every word typed.
fn matches(file: &FileDiff, words: &[String]) -> bool {
    let path = file.path().to_lowercase();
    words.iter().all(|w| path.contains(w))
}

/// The rows of files `ixs`, their folders keyed under `prefix` (a group's key, or none).
fn file_rows(
    files: &[FileDiff],
    ixs: std::ops::Range<usize>,
    list: bool,
    words: &[String],
    collapsed: &HashSet<String>,
    prefix: &str,
) -> Vec<FileItem> {
    if !words.is_empty() {
        return ixs
            .filter(|&ix| matches(&files[ix], words))
            .map(|ix| FileItem::File { ix, depth: 0 })
            .collect();
    }
    if list {
        return ixs.map(|ix| FileItem::File { ix, depth: 0 }).collect();
    }
    let mut root = Node::default();
    for (ix, file) in files.iter().enumerate().take(ixs.end).skip(ixs.start) {
        let mut node = &mut root;
        let mut parts: Vec<&str> = file.path().split('/').collect();
        parts.pop();
        for dir in parts {
            node = node.dirs.entry(dir.to_owned()).or_default();
        }
        node.files.push(ix);
    }
    let mut out = Vec::new();
    flatten(&root, 0, prefix, collapsed, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use artoo_core::git::ChangeKind;

    fn file(path: &str) -> FileDiff {
        FileDiff {
            old_path: Some(path.into()),
            new_path: Some(path.into()),
            change: ChangeKind::Modified,
            added: 1,
            removed: 0,
            hunks: Vec::new(),
            old_text: None,
            new_text: None,
            note: None,
            old_image: None,
            new_image: None,
            pictures: Vec::new(),
        }
    }

    #[test]
    fn a_collapsed_folder_hides_its_files_from_the_rows_only() {
        let files: Vec<FileDiff> = ["A/a.kt", "B/b.kt", "c.kt", "C/d.kt", "e.kt", "src/ui/f.rs"]
            .into_iter()
            .map(file)
            .collect();
        let collapsed = HashSet::from(["B".to_owned(), "src/ui".to_owned()]);
        // Folders come before files: A/a.kt, B/b.kt, C/d.kt, src/ui/f.rs, c.kt, e.kt.
        assert_eq!(
            files_of(file_items(&files, false, "", &HashSet::new(), None)),
            [0, 1, 3, 5, 2, 4]
        );
        assert_eq!(
            files_of(file_items(&files, false, "", &collapsed, None)),
            [0, 3, 2, 4]
        );
        // What opens for a file: the folders it is under, a chain as one (#167).
        assert!(inside("B/b.kt", "B") && inside("src/ui/f.rs", "src/ui"));
        assert!(!inside("Bx/b.kt", "B") && !inside("B", "B") && !inside("c.kt", "B"));
    }

    /// The rows as words: `#Group n`, `dir/`, a file's index.
    fn shown(items: &[FileItem]) -> Vec<String> {
        items
            .iter()
            .map(|item| match item {
                FileItem::Group { group, count, .. } => format!("#{} {count}", group.name()),
                FileItem::Dir { path, .. } => format!("{path}/"),
                FileItem::File { ix, .. } => ix.to_string(),
            })
            .collect()
    }

    #[test]
    fn the_working_trees_groups_fold_apart_and_a_filter_opens_both() {
        // Staged: a/x.kt, b.kt; not staged: a/x.kt again (partly staged), a/y.kt.
        let files: Vec<FileDiff> = ["a/x.kt", "b.kt", "a/x.kt", "a/y.kt"]
            .into_iter()
            .map(file)
            .collect();
        let none = HashSet::new();
        assert_eq!(
            shown(&file_items(&files, false, "", &none, Some(2))),
            [
                "#Staged 2",
                "/staged/a/",
                "0",
                "1",
                "#Not staged 2",
                "/not-staged/a/",
                "2",
                "3"
            ]
        );
        // A folder folded in one group stays open in the other; a group folds whole.
        let folded = HashSet::from(["/staged/a".to_owned(), "/not-staged".to_owned()]);
        assert_eq!(
            shown(&file_items(&files, false, "", &folded, Some(2))),
            ["#Staged 2", "/staged/a/", "1", "#Not staged 2"]
        );
        // Typed, both are open and count what matches; a group with nothing left goes.
        assert_eq!(
            shown(&file_items(&files, false, "x", &folded, Some(2))),
            ["#Staged 1", "0", "#Not staged 1", "2"]
        );
        assert_eq!(
            shown(&file_items(&files, false, "b", &none, Some(2))),
            ["#Staged 1", "1"]
        );
        // Nothing staged: the one group still says what it is.
        assert_eq!(
            shown(&file_items(&files[2..], true, "", &none, Some(0))),
            ["#Not staged 2", "0", "1"]
        );
        // A folder's key starts its files' keys, in their group only.
        assert_eq!(row_key(Some(2), 0, "a/x.kt"), "/staged/a/x.kt");
        assert_eq!(row_key(Some(2), 2, "a/x.kt"), "/not-staged/a/x.kt");
        assert_eq!(row_key(None, 2, "a/x.kt"), "a/x.kt");
        assert!(inside(&row_key(Some(2), 0, "a/x.kt"), "/staged/a"));
        assert!(!inside(&row_key(Some(2), 2, "a/x.kt"), "/staged/a"));
        assert_eq!(folder_of("/staged/a/b"), "a/b");
        assert_eq!(folder_of("/not-staged"), "");
        assert_eq!(folder_of("a/b"), "a/b");
    }
}
