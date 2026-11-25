//! A new merge request's draft kept past its page (#222): what was written in the form, per project, in a file of
//! its own in the data folder (`request-drafts.json`) beside the projects' — never in their file, so
//! rolling this back is not reading it. One draft per branch of a project; created or discarded, it goes.

use serde::{Deserialize, Serialize};
use std::path::Path;

const FILE: &str = "request-drafts.json";

/// What the form held when it was left.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Kept {
    /// The branch the request is made from, by its name.
    pub source: String,
    pub target: String,
    pub title: String,
    pub description: String,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub reviewers: Vec<String>,
    #[serde(default)]
    pub milestone: Option<u64>,
    /// "Draft" as the host knows it: the request is created as a draft.
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub delete_branch: bool,
    #[serde(default)]
    pub squash: bool,
    /// When it was kept, in seconds since the epoch.
    #[serde(default)]
    pub saved: i64,
}

/// Every project's drafts, by the project's folder.
#[derive(Default, Serialize, Deserialize)]
struct Store {
    #[serde(default)]
    projects: std::collections::BTreeMap<String, Vec<Kept>>,
}

/// A folder as a key: with or without its last slash.
fn key(root: &Path) -> String {
    root.to_string_lossy().trim_end_matches('/').to_owned()
}

fn parse(text: &str) -> Store {
    serde_json::from_str(text).unwrap_or_default()
}

fn text(store: &Store) -> String {
    serde_json::to_string_pretty(store).unwrap_or_default()
}

/// The drafts of the project at `root`, the last kept first.
pub fn load(root: &Path) -> Vec<Kept> {
    of(&crate::storage::read(FILE), root)
}

fn of(file: &str, root: &Path) -> Vec<Kept> {
    let mut drafts = parse(file).projects.remove(&key(root)).unwrap_or_default();
    drafts.sort_by_key(|d| std::cmp::Reverse(d.saved));
    drafts
}

/// Keeps `draft` for the project at `root`, in place of the one of its branch.
pub fn keep(root: &Path, draft: Kept) {
    let file = crate::storage::read(FILE);
    crate::storage::save(FILE, with(&file, root, draft));
}

fn with(file: &str, root: &Path, draft: Kept) -> String {
    let mut store = parse(file);
    let list = store.projects.entry(key(root)).or_default();
    list.retain(|d| d.source != draft.source);
    list.push(draft);
    text(&store)
}

/// An agent's draft kept for the project at `root`, so a crash or a restart does not lose it. A draft of
/// the branch already kept stays when its words are not the agent's earlier ones (`earlier`: title and
/// description): it is the reviewer's own, and an agent never writes over it. Whether it was kept.
pub fn keep_agent(root: &Path, draft: Kept, earlier: Option<(&str, &str)>) -> bool {
    let file = crate::storage::read(FILE);
    match with_agent(&file, root, draft, earlier) {
        Some(next) => {
            crate::storage::save(FILE, next);
            true
        }
        None => false,
    }
}

fn with_agent(
    file: &str,
    root: &Path,
    draft: Kept,
    earlier: Option<(&str, &str)>,
) -> Option<String> {
    let kept = of(file, root)
        .into_iter()
        .find(|d| d.source == draft.source);
    if let Some(k) = kept
        && earlier != Some((k.title.as_str(), k.description.as_str()))
    {
        return None;
    }
    Some(with(file, root, draft))
}

/// The draft of `source` is gone: created, or discarded.
pub fn remove(root: &Path, source: &str) {
    let file = crate::storage::read(FILE);
    if let Some(next) = without(&file, root, source) {
        crate::storage::save(FILE, next);
    }
}

/// The file without `source`'s draft; `None` when it had none.
fn without(file: &str, root: &Path, source: &str) -> Option<String> {
    let mut store = parse(file);
    let list = store.projects.get_mut(&key(root))?;
    let before = list.len();
    list.retain(|d| d.source != source);
    if list.len() == before {
        return None;
    }
    if list.is_empty() {
        store.projects.remove(&key(root));
    }
    Some(text(&store))
}

/// Now, in seconds since the epoch.
pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_agents_draft_is_kept_and_never_writes_over_the_reviewers() {
        let root = Path::new("/p");
        let agent = |title: &str| Kept {
            source: "feat".into(),
            title: title.into(),
            description: "d".into(),
            ..Kept::default()
        };
        let file = with_agent("", root, agent("first"), None).unwrap();
        assert_eq!(of(&file, root)[0].title, "first");
        // The agent drafts again: its own earlier words are replaced.
        let file = with_agent(&file, root, agent("second"), Some(("first", "d"))).unwrap();
        assert_eq!(of(&file, root)[0].title, "second");
        // The reviewer edited it: the agent's next one is left out.
        let edited = with(&file, root, agent("mine"));
        assert!(with_agent(&edited, root, agent("third"), Some(("second", "d"))).is_none());
        assert_eq!(of(&edited, root)[0].title, "mine");
    }

    fn kept(source: &str, saved: i64) -> Kept {
        Kept {
            source: source.into(),
            target: "main".into(),
            title: format!("Title of {source}"),
            description: "Line one\n\nLine two".into(),
            labels: vec!["bug".into()],
            reviewers: vec!["mira".into()],
            milestone: Some(3),
            draft: true,
            delete_branch: true,
            squash: false,
            saved,
        }
    }

    #[test]
    fn a_kept_draft_reads_back_whole() {
        let root = Path::new("/work/ledger-api/");
        let file = with("", root, kept("feature/cache", 10));
        assert_eq!(of(&file, root), vec![kept("feature/cache", 10)]);
        // The same folder without its last slash is the same project.
        assert_eq!(of(&file, Path::new("/work/ledger-api")).len(), 1);
    }

    #[test]
    fn a_draft_is_per_project_and_per_branch_newest_first() {
        let (a, b) = (Path::new("/work/a"), Path::new("/work/b"));
        let file = with("", a, kept("one", 5));
        let file = with(&file, a, kept("two", 9));
        let file = with(&file, b, kept("other", 7));
        // Keeping a branch again replaces its draft.
        let mut again = kept("one", 12);
        again.title = "Changed".into();
        let file = with(&file, a, again);
        let drafts = of(&file, a);
        assert_eq!(
            drafts.iter().map(|d| d.source.as_str()).collect::<Vec<_>>(),
            ["one", "two"]
        );
        assert_eq!(drafts[0].title, "Changed");
        assert_eq!(of(&file, b).len(), 1);
    }

    #[test]
    fn removing_goes_alone_and_leaves_the_others() {
        let (a, b) = (Path::new("/work/a"), Path::new("/work/b"));
        let file = with(&with("", a, kept("one", 5)), b, kept("two", 6));
        assert_eq!(without(&file, a, "missing"), None);
        let next = without(&file, a, "one").expect("removed");
        assert!(of(&next, a).is_empty());
        assert_eq!(of(&next, b).len(), 1);
        assert!(!next.contains("/work/a"), "an empty project leaves no key");
    }

    #[test]
    fn an_unreadable_store_is_no_drafts() {
        assert!(of("not json {", Path::new("/work/a")).is_empty());
        assert!(of("", Path::new("/work/a")).is_empty());
    }
}
