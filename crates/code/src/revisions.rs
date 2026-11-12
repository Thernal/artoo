//! `Commit id…` in the revision chip's menu (#208): a list in ⌘K's window that reads what is typed as a revision — a
//! commit id, short or whole, a tag, a branch, `HEAD~3` — and offers it with its message; ↵ makes Code read its files
//! there, the chip tinted with its short id.

use crate::space::CodeSpace;
use artoo_core::git::Repo;
use artoo_shell_api::search::{Find, List, Opens, Said};
use artoo_ui_kit::format;
use gpui::WeakEntity;
use std::rc::Rc;

/// The list's id, which the menu opens.
pub const LIST: &str = "code.revision";

/// The list over Code `code`.
pub fn list(code: WeakEntity<CodeSpace>) -> List {
    let finds = code.clone();
    List {
        id: LIST,
        says: Rc::new(|_| Said {
            title: Some("Read the files at".to_owned()),
            placeholder: "A commit id, a tag or a branch — ↵ reads the files there".into(),
            empty: "No commit by that id.",
        }),
        find: Rc::new(move |ask, cx| {
            let text = ask.text.trim();
            let Some(code) = finds.upgrade() else {
                return Vec::new();
            };
            let Some(root) = code.read(cx).root().map(std::path::Path::to_path_buf) else {
                return Vec::new();
            };
            if text.is_empty() {
                return Vec::new();
            }
            let Ok(repo) = Repo::open(&root) else {
                return Vec::new();
            };
            let Ok(id) = repo.resolve(text) else {
                return Vec::new();
            };
            let summary = repo.commit(id).map(|c| c.summary).unwrap_or_default();
            let short = format::short(id);
            let read = finds.clone();
            let label = short.clone();
            let open = Opens::Run(Rc::new(move |cx| {
                read.update(cx, |c, cx| c.set_revision(Some((label.clone(), id)), cx))
                    .ok();
            }));
            vec![Find::new(
                "",
                format!("{short} · {summary}"),
                String::new(),
                open,
            )]
        }),
        opened: None,
        closed: None,
        tab: None,
        back: None,
    }
}
