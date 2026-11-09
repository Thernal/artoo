//! Repositories a test makes (the `sandbox` feature: the app's tests and the features'). They are here, in the core,
//! because git2's calls generic over their arguments (a path, a name) reach libgit2 from the crate that names them:
//! from the app or a plugin, that is the plugins' SDK's libgit2 (#175), whose C functions it does not export.

use git2::{Oid, Repository, Signature};
use std::path::Path;

/// A new repository in `dir`.
pub fn init(dir: &Path) -> Repository {
    Repository::init(dir).unwrap()
}

/// A commit on `parent` (none: a root commit) whose tree is `files` alone, each a path and its text, by "Test".
pub fn commit(
    repo: &Repository,
    parent: Option<Oid>,
    files: &[(&str, &str)],
    message: &str,
) -> Oid {
    let sig = Signature::now("Test", "test@example.com").unwrap();
    let mut tree = repo.treebuilder(None).unwrap();
    for (path, text) in files {
        let blob = repo.blob(text.as_bytes()).unwrap();
        tree.insert(path, blob, 0o100644).unwrap();
    }
    let tree = repo.find_tree(tree.write().unwrap()).unwrap();
    let parent = parent.map(|p| repo.find_commit(p).unwrap());
    let parents: Vec<_> = parent.iter().collect();
    repo.commit(None, &sig, &sig, message, &tree, &parents)
        .unwrap()
}
