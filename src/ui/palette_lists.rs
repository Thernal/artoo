//! The lists the tab shows alone in ⌘K's window, offered as a feature offers its own (`artoo_shell_api::search::List`,
//! D18): the places opened lately (⌘E), the lines marked with F3 (⌘F3), the functions and types the open file changes
//! (⌘⇧O), switching to a branch or a merge request (⌘B), opening a repository (⌘T, the rail's +), a face for the
//! interface or the code (#24, a face's Other…). Their finds read the tab, which the window lets them: it finds them
//! outside the tab's update.

use super::keymap::Keymap;
use super::{Workspace, fonts, format, theme};
use artoo_shell_api::search::{Ask, Find, Finds, List, Opens, Said, Says};
use git2::Oid;
use gpui::{App, Context, SharedString, WeakEntity};
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

pub(super) const RECENT: &str = "recent";
pub(super) const BOOKMARKS: &str = "bookmarks";
pub(super) const STRUCTURE: &str = "structure";
pub(super) const SWITCH: &str = "switch";
pub(super) const PROJECTS: &str = "projects";
const FACES_UI: &str = "faces";
const FACES_CODE: &str = "code-faces";

/// The list of the faces for the code, or the interface's.
pub(super) fn faces_of(code: bool) -> &'static str {
    if code { FACES_CODE } else { FACES_UI }
}

/// The tab's lists, reading it through `tab`.
pub(super) fn lists(tab: WeakEntity<Workspace>) -> Vec<List> {
    vec![
        list(
            RECENT,
            recent(tab.clone()),
            says(
                "Recent places — files, commits, merge requests",
                "Nothing opened yet.",
            ),
        ),
        list(
            BOOKMARKS,
            bookmarks(tab.clone()),
            says(
                "Bookmarks — lines marked with F3",
                "No bookmarks yet: click a line and press F3.",
            ),
        ),
        list(
            STRUCTURE,
            structure(tab.clone()),
            says(
                "Functions and types this file changes",
                "No function or type of this file is changed (or the language is not known).",
            ),
        ),
        switch(tab.clone()),
        list(
            PROJECTS,
            projects(tab.clone()),
            Rc::new(|_| Said {
                title: Some("Open a repository".to_owned()),
                placeholder: "Open a repository — type to filter, or a path",
                empty: "None by that name. Browse… finds it on disk.",
            }),
        ),
        faces(false, tab.clone()),
        faces(true, tab),
    ]
}

/// A list that only finds.
fn list(id: &'static str, find: Finds, says: Says) -> List {
    List {
        id,
        says,
        find,
        opened: None,
        closed: None,
        back: None,
        tab: None,
    }
}

/// A list's words, the same however it stands.
fn says(placeholder: &'static str, empty: &'static str) -> Says {
    Rc::new(move |_| Said {
        title: None,
        placeholder,
        empty,
    })
}

/// Finds reading the tab; none once it is gone.
fn reading(
    tab: WeakEntity<Workspace>,
    find: impl Fn(&Workspace, Ask, &App) -> Vec<Find> + 'static,
) -> Finds {
    Rc::new(move |ask, cx| (tab.upgrade()).map_or_else(Vec::new, |t| find(t.read(cx), ask, cx)))
}

/// A pick the tab does, once ⌘K has closed.
fn doing(
    tab: &WeakEntity<Workspace>,
    act: impl Fn(&mut Workspace, &mut Context<Workspace>) + 'static,
) -> Opens {
    let tab = tab.clone();
    Opens::Run(Rc::new(move |cx| {
        tab.update(cx, |this, cx| act(this, cx)).ok();
    }))
}

fn words(text: &str) -> Vec<String> {
    text.split_whitespace().map(str::to_lowercase).collect()
}

fn matches(words: &[String], hay: &str) -> bool {
    words.iter().all(|w| crate::rank::word_matches(w, hay))
}

/// The places opened lately, newest first.
fn recent(tab: WeakEntity<Workspace>) -> Finds {
    let picks = tab.clone();
    reading(tab, move |this, ask, _| {
        let words = words(ask.text);
        (this.recents.iter())
            .filter(|r| matches(&words, &format!("{} {}", r.label, r.detail)))
            .take(14)
            .map(|r| {
                let key = r.key.clone();
                let open = doing(&picks, move |this, cx| this.open_recent(key.clone(), cx));
                Find::new("Recent", r.label.clone(), r.detail.clone(), open)
            })
            .collect()
    })
}

/// The lines marked in the open diff, by their file and line.
fn bookmarks(tab: WeakEntity<Workspace>) -> Finds {
    let picks = tab.clone();
    reading(tab, move |this, ask, cx| {
        let words = words(ask.text);
        (this.bookmark_list(cx).into_iter())
            .filter(|m| matches(&words, &format!("{} {}", m.path, m.text)))
            .map(|m| {
                let name = m.path.rsplit('/').next().unwrap_or(&m.path);
                let label = format!("{name}:{}", m.line);
                let (path, line) = (m.path.clone(), m.line);
                let open = doing(&picks, move |this, cx| {
                    this.jump_to_line(&path, line, false, cx)
                });
                Find::new("Bookmarks", label, m.text, open)
            })
            .collect()
    })
}

/// What the open file changes: its functions and types, each at its line.
fn structure(tab: WeakEntity<Workspace>) -> Finds {
    let picks = tab.clone();
    reading(tab, move |this, ask, cx| {
        let words = words(ask.text);
        let diff = this.shown_diff(cx);
        let Some(file) = diff.as_ref().and_then(|d| d.files.get(this.diff_file(cx))) else {
            return Vec::new();
        };
        let name = file.path().rsplit('/').next().unwrap_or_default();
        let group = SharedString::from(format!("Changes in {name}"));
        (crate::symbols::changed(file).into_iter())
            .filter(|s| matches(&words, &s.name))
            .map(|s| {
                let (old, line) = (s.old, s.line);
                let open = doing(&picks, move |this, cx| {
                    if let Some(path) = this.current_path(cx) {
                        this.jump_to_line(&path, line, old, cx);
                    }
                });
                let label = format!("{}  {} {}", s.mark, s.kind, s.name);
                let detail = format!("{}line {}", if s.old { "old " } else { "" }, s.line);
                Find::new(group.clone(), label, detail, open)
            })
            .collect()
    })
}

/// Switching to a branch or a merge request; ⇥ keeps only what is not reviewed yet.
fn switch(tab: WeakEntity<Workspace>) -> List {
    let only: Rc<Cell<bool>> = Rc::default();
    let (said, found, opened, flipped) = (only.clone(), only.clone(), only.clone(), only);
    let picks = tab.clone();
    List {
        id: SWITCH,
        says: Rc::new(move |_| match said.get() {
            true => Said {
                title: None,
                placeholder: "Switch to — only what is not reviewed yet   ⇥ all",
                empty: "Nothing left to review here.",
            },
            false => Said {
                title: None,
                placeholder: "Switch to a branch or a merge request   ⇥ only not reviewed",
                empty: "No branch or merge request by that name.",
            },
        }),
        find: reading(tab, move |this, ask, cx| {
            switch_finds(this, ask.text, found.get(), &picks, cx)
        }),
        opened: Some(Rc::new(move |_| opened.set(false))),
        closed: None,
        back: None,
        tab: Some(Rc::new(move |_| flipped.set(!flipped.get()))),
    }
}

/// The merge requests (when the project has them), then the local branches; each says how far its review got.
fn switch_finds(
    this: &Workspace,
    query: &str,
    open_only: bool,
    tab: &WeakEntity<Workspace>,
    cx: &App,
) -> Vec<Find> {
    let state = |head: Option<Oid>| match head {
        Some(h) if this.rv().head_reviewed(h, cx) => (true, "✓ reviewed".to_owned()),
        Some(h) => match this.rv().head_part(h, cx) {
            Some((done, total)) => (false, format!("{done}/{total} reviewed")),
            None => (false, "not reviewed".to_owned()),
        },
        None => (false, String::new()),
    };
    let list = this.request_list(cx);
    let requests = crate::rank::rank(list.iter(), query, |m| &m.title)
        .into_iter()
        .chain(
            // `!12` or a branch name finds a request too.
            list.iter().filter(|m| {
                let q = query.trim().trim_start_matches('!');
                !q.is_empty()
                    && (m.iid.to_string() == q || crate::rank::word_matches(q, &m.source))
                    && crate::rank::score(query, &m.title).is_none()
            }),
        )
        .filter_map(|m| {
            let (done, said) = state(Oid::from_str(&m.head).ok());
            let iid = m.iid;
            let open = doing(tab, move |this, cx| this.open_request(iid, cx));
            let label = format!("{} {}", super::words::number(m.iid), m.title);
            let detail = format!("{} · {said}", m.source);
            (!(open_only && done)).then(|| Find::new("Merge requests", label, detail, open))
        });
    let list = this.branch_list(cx);
    let local = (list.iter().enumerate()).filter(|(_, b)| b.kind == crate::git::RefKind::Local);
    let branches = crate::rank::rank(local, query, |(_, b)| &b.name)
        .into_iter()
        .filter_map(|(ix, b)| {
            let (done, said) = state(Some(b.tip));
            let open = doing(tab, move |this, cx| this.open_branch(ix, None, cx));
            let detail = format!("{} · {said}", format::short(b.tip));
            (!(open_only && done)).then(|| Find::new("Branches", b.name.clone(), detail, open))
        });
    requests.take(10).chain(branches.take(10)).collect()
}

/// Opening a repository: in a tab of its own, as + means; one that has it already comes on screen.
fn projects(tab: WeakEntity<Workspace>) -> Finds {
    let picks = tab.clone();
    reading(tab, move |this, ask, cx| {
        let open = |path: PathBuf| {
            doing(&picks, move |this, cx| match this.root {
                None => this.request_open(path.clone(), None, cx),
                Some(_) => this.open_in_new_tab(path.clone(), None, cx),
            })
        };
        let key = cx.global::<Keymap>().shown(&super::Open);
        let browse = Find::new(
            "On disk",
            "Browse…".to_owned(),
            String::new(),
            doing(&picks, |this, cx| this.browse(cx)),
        )
        .key(key.unwrap_or_else(|| "⌘O".to_owned()));
        let mut finds = project_finds(
            ask.text,
            &crate::storage::recent(),
            this.root.as_deref(),
            open,
        );
        finds.push(browse);
        finds
    })
}

/// A folder typed as a path, then the repositories opened before ranked by the typed words, the one on screen
/// marked.
fn project_finds(
    query: &str,
    recent: &[PathBuf],
    here: Option<&Path>,
    open: impl Fn(PathBuf) -> Opens,
) -> Vec<Find> {
    let mut finds = Vec::new();
    let typed = query.trim();
    let expanded = match typed.strip_prefix('~') {
        Some(rest) => std::env::home_dir().map(|h| h.join(rest.trim_start_matches('/'))),
        None => Some(PathBuf::from(typed)),
    };
    if typed.starts_with(['/', '~'])
        && let Some(path) = expanded.filter(|p| p.is_dir())
    {
        let label = path.display().to_string();
        finds.push(
            Find::new("Folder", label, String::new(), open(path))
                .key("↵".to_owned())
                .in_folder(),
        );
    }
    let named: Vec<(String, &PathBuf)> = recent
        .iter()
        .map(|p| (crate::labels::project_name(p), p))
        .collect();
    // By the name first, then by the whole path as Go to file does: `work ibam` finds
    // ~/Desktop/Work/ABB/ibam, `abb` every project under ABB (the developer, 2026-10-23).
    let ranked: Vec<&(String, &PathBuf)> = if typed.is_empty() {
        named.iter().collect()
    } else {
        let words = words(typed);
        crate::rank::rank(named.iter(), typed, |(name, _)| name.as_str())
            .into_iter()
            .chain(named.iter().filter(|(name, path)| {
                crate::rank::score(typed, name).is_none()
                    && matches(&words, &format::home_relative(path))
            }))
            .collect()
    };
    finds.extend(ranked.into_iter().map(|(name, path)| {
        let detail = match here == Some(path.as_path()) {
            true => "on screen".to_owned(),
            false => format::home_relative(path),
        };
        Find::new("Opened before", name.clone(), detail, open((*path).clone())).in_folder()
    }));
    finds
}

/// Every face installed for the interface or (`code`) the code, each drawn in itself, opening on the one in use; ↵
/// keeps it.
fn faces(code: bool, tab: WeakEntity<Workspace>) -> List {
    let installed: Rc<RefCell<Vec<String>>> = Rc::default();
    let read = installed.clone();
    let placeholder = match code {
        true => "The code's face — each in its own type",
        false => "The interface's face — each in its own type",
    };
    List {
        id: faces_of(code),
        says: says(placeholder, "No face installed by that name."),
        find: Rc::new(move |ask, _| {
            let words = words(ask.text);
            let faces = theme::faces();
            let current = if code { faces.code } else { faces.ui };
            let find = |group: &'static str, face: Option<String>| {
                let picked = face.clone();
                let open = doing(&tab, move |this, cx| {
                    this.choose_face(code, picked.clone(), cx)
                });
                let detail = if face == current { "current" } else { "" };
                Find::new(
                    group,
                    fonts::face_label(code, face.as_deref()),
                    detail.to_owned(),
                    open,
                )
                .face(fonts::face_family(code, face.as_deref()))
                .current(face == current)
            };
            let default = fonts::face_label(code, None);
            (matches(&words, &default)
                .then(|| find("Default", None))
                .into_iter())
            .chain(
                (installed.borrow().iter())
                    .filter(|f| matches(&words, f))
                    .map(|f| find("Installed", Some(f.clone()))),
            )
            .collect()
        }),
        opened: Some(Rc::new(move |cx| *read.borrow_mut() = fonts::installed(cx))),
        closed: None,
        back: None,
        tab: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_project_is_found_by_its_name_then_by_its_path() {
        let recent = [
            PathBuf::from("/w/ABB/ibam-android-app"),
            PathBuf::from("/w/Thernal/artoo"),
            PathBuf::from("/w/ABB/notes"),
        ];
        let labels = |q: &str| {
            project_finds(q, &recent, None, |_| Opens::Run(Rc::new(|_| {})))
                .into_iter()
                .filter(|f| f.group == "Opened before")
                .map(|f| f.label)
                .collect::<Vec<_>>()
        };
        assert_eq!(labels("ibam"), ["ibam-android-app"]);
        // The folder above them: every project under it.
        assert_eq!(labels("abb"), ["ibam-android-app", "notes"]);
        // A name match comes before a path match.
        assert_eq!(labels("notes"), ["notes"]);
        assert_eq!(labels("abb notes"), ["notes"]);
    }
}
