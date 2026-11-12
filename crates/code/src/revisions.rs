//! The revision chip's list (#208), dropped from the chip: `Filter, or a commit id` over the working tree, HEAD, the
//! local branches, each remote's and every tag, in a list that scrolls. What is typed filters them, and read as a
//! revision git knows — a commit id, short or whole, `HEAD~3` — it is offered too, with its message. ↵ makes Code
//! read its files there, the chip tinted with its name.

use crate::space::CodeSpace;
use artoo_core::git::{RefKind, Repo};
use artoo_core::rank::word_matches;
use artoo_shell_api::search::{Find, List, Opens, Said};
use artoo_ui_kit::format;
use git2::Oid;
use gpui::{App, WeakEntity};
use std::rc::Rc;

/// The list's id, which the chip drops.
pub const LIST: &str = "code.revision";

/// The list over Code `code`.
pub fn list(code: WeakEntity<CodeSpace>) -> List {
    List {
        id: LIST,
        says: Rc::new(|_| Said {
            title: Some("Read the files at".to_owned()),
            placeholder: "Filter, or a commit id".into(),
            empty: "No branch, tag or commit by that name.",
        }),
        find: Rc::new(move |ask, cx| finds(&code, ask.text.trim(), cx)),
        opened: None,
        closed: None,
        tab: None,
        back: None,
    }
}

/// The revisions `text` filters, and the commit it names when it names one no row is.
fn finds(code: &WeakEntity<CodeSpace>, text: &str, cx: &App) -> Vec<Find> {
    let Some(space) = code.upgrade() else {
        return Vec::new();
    };
    let space = space.read(cx);
    let Some(root) = space.root().map(std::path::Path::to_path_buf) else {
        return Vec::new();
    };
    let now = space.revision().cloned();
    let words: Vec<String> = text.split_whitespace().map(str::to_owned).collect();
    let read_at = |group: String, label: String, detail: String, rev: Option<(String, Oid)>| {
        let current = match (&rev, &now) {
            (None, None) => true,
            (Some(a), Some(b)) => a == b,
            _ => false,
        };
        let read = code.clone();
        let open = Opens::Run(Rc::new(move |cx| {
            read.update(cx, |c, cx| c.set_revision(rev.clone(), cx))
                .ok();
        }));
        Find::new(group, label, detail, open).current(current)
    };
    let shown = |label: &str| words.iter().all(|w| word_matches(w, label));
    let mut out = Vec::new();
    if shown("Working tree") {
        out.push(read_at(
            String::new(),
            "Working tree".to_owned(),
            String::new(),
            None,
        ));
    }
    let all = space.branches(cx);
    if let Some(head) = all.iter().find(|b| b.is_head)
        && shown(&format!("HEAD {}", head.name))
    {
        out.push(read_at(
            String::new(),
            format!("HEAD · {}", head.name),
            format::short(head.tip),
            Some((head.name.clone(), head.tip)),
        ));
    }
    let names = |kind: RefKind| {
        (all.iter())
            .filter(move |b| b.kind == kind && !b.is_head)
            .filter(|b| shown(&b.name))
    };
    for b in names(RefKind::Local) {
        out.push(read_at(
            "Branches".to_owned(),
            b.name.clone(),
            String::new(),
            Some((b.name.clone(), b.tip)),
        ));
    }
    // Each remote its own group, by its name: `origin`.
    for b in names(RefKind::Remote) {
        let remote = b.name.split('/').next().unwrap_or_default().to_owned();
        out.push(read_at(
            remote,
            b.name.clone(),
            String::new(),
            Some((b.name.clone(), b.tip)),
        ));
    }
    let Ok(repo) = Repo::open(&root) else {
        return out;
    };
    // Every tag, the highest version first (`v1.20.0` before `v1.9.0`); the list scrolls.
    let mut tags = repo.tags().unwrap_or_default();
    tags.sort_by(|a, b| by_version(&b.0, &a.0));
    for (name, tip) in tags {
        if shown(&name) {
            out.push(read_at(
                "Tags".to_owned(),
                name.clone(),
                String::new(),
                Some((name, tip)),
            ));
        }
    }
    // What is typed, read as a revision no row is: a commit id, `HEAD~3`.
    let named = (all.iter().map(|b| b.name.as_str())).any(|name| name == text);
    if !text.is_empty()
        && !named
        && !out.iter().any(|f| f.label == text)
        && let Ok(id) = repo.resolve(text)
    {
        let summary = repo.commit(id).map(|c| c.summary).unwrap_or_default();
        let short = format::short(id);
        out.push(read_at(
            "Commit".to_owned(),
            format!("{short} · {summary}"),
            String::new(),
            Some((short, id)),
        ));
    }
    out
}

/// `a` against `b` with their runs of digits read as numbers: `v1.9.0` before `v1.20.0`.
fn by_version(a: &str, b: &str) -> std::cmp::Ordering {
    let parts = |s: &str| {
        let mut out: Vec<(bool, String)> = Vec::new();
        for c in s.chars() {
            let digit = c.is_ascii_digit();
            match out.last_mut() {
                Some((d, run)) if *d == digit => run.push(c),
                _ => out.push((digit, c.to_string())),
            }
        }
        out
    };
    let (a, b) = (parts(a), parts(b));
    for (x, y) in a.iter().zip(&b) {
        let order = match (x, y) {
            ((true, x), (true, y)) => {
                let (x, y) = (x.trim_start_matches('0'), y.trim_start_matches('0'));
                x.len().cmp(&y.len()).then_with(|| x.cmp(y))
            }
            ((_, x), (_, y)) => x.cmp(y),
        };
        if order.is_ne() {
            return order;
        }
    }
    // The same up to where one ends: a pre-release (`v2.0.0-rc1`) comes before its release.
    let pre = |rest: Option<&(bool, String)>| rest.is_some_and(|(_, r)| r.starts_with('-'));
    match a.len().cmp(&b.len()) {
        std::cmp::Ordering::Greater if pre(a.get(b.len())) => std::cmp::Ordering::Less,
        std::cmp::Ordering::Less if pre(b.get(a.len())) => std::cmp::Ordering::Greater,
        order => order,
    }
}

#[cfg(test)]
mod tests {
    use super::by_version;

    #[test]
    fn tags_go_by_their_numbers() {
        let mut tags = vec![
            "v1.9.0",
            "v1.20.0",
            "v1.2.0",
            "release/2.4",
            "v1.10.0",
            "v2.0.0-rc1",
            "v2.0.0",
        ];
        tags.sort_by(|a, b| by_version(b, a));
        assert_eq!(
            tags,
            [
                "v2.0.0",
                "v2.0.0-rc1",
                "v1.20.0",
                "v1.10.0",
                "v1.9.0",
                "v1.2.0",
                "release/2.4"
            ]
        );
    }
}
