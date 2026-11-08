//! A file's code owners beside its name in the diff (#171 phase 9, the decorators' second): read from the
//! repository's CODEOWNERS in the working tree — where GitHub or GitLab looks for it, the first found —, read
//! again when it changes. Nothing is said of a file no rule owns, nor in a repository without the file.

use artoo_core::codeowners::{self, Rule};
use artoo_review_api::decorations::{Decorating, Decorations, Decorator};
use artoo_ui_kit::hover::Hover;
use artoo_ui_kit::{icons, px, theme};
use gpui::{App, InteractiveElement, IntoElement, ParentElement, Styled, div};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::SystemTime;

/// The owners' id among the decorators.
pub const ID: &str = "review.codeowners";

/// Where either host looks, in GitHub's order, then GitLab's own place.
const PLACES: [&str; 4] = [
    ".github/CODEOWNERS",
    "CODEOWNERS",
    "docs/CODEOWNERS",
    ".gitlab/CODEOWNERS",
];

/// The rules read, of a repository, from a file as it was then.
#[derive(Default)]
struct Read {
    root: PathBuf,
    file: Option<(PathBuf, SystemTime)>,
    rules: Vec<Rule>,
}

/// The repository's rules, read again when its CODEOWNERS changed or another repository is asked about.
fn rules(read: &RefCell<Read>, root: &Path) -> Vec<Rule> {
    let found = PLACES.iter().map(|p| root.join(p)).find_map(|path| {
        let changed = std::fs::metadata(&path).ok()?.modified().ok()?;
        Some((path, changed))
    });
    let mut read = read.borrow_mut();
    if read.root != root || read.file != found {
        read.rules = (found.as_ref())
            .and_then(|(path, _)| std::fs::read_to_string(path).ok())
            .map(|text| codeowners::parse(&text))
            .unwrap_or_default();
        read.root = root.to_owned();
        read.file = found;
    }
    read.rules.clone()
}

/// The owners in words: `@ana`, `@ana and @ben`, `@ana, @ben and 2 more`.
fn words(owners: &[String]) -> String {
    let at: Vec<String> = owners.iter().map(|o| format!("@{o}")).collect();
    match at.as_slice() {
        [] => String::new(),
        [one] => one.clone(),
        [a, b] => format!("{a} and {b}"),
        [a, b, rest @ ..] => format!("{a}, {b} and {} more", rest.len()),
    }
}

pub fn decorator() -> Decorator {
    let read = Rc::new(RefCell::new(Read::default()));
    Decorator {
        id: ID,
        order: 100,
        decorate: Rc::new(move |at: &Decorating, _: &App| {
            let Some(root) = at.root else {
                return Decorations::default();
            };
            let rules = rules(&read, root);
            let owners = codeowners::owners(&rules, at.file.path());
            if owners.is_empty() {
                return Decorations::default();
            }
            let all = owners
                .iter()
                .map(|o| format!("@{o}"))
                .collect::<Vec<_>>()
                .join(", ");
            Decorations {
                badge: Some(
                    div()
                        .id("owners")
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap(px(4.))
                        .text_size(px(12.))
                        .text_color(theme::muted())
                        .child(
                            icons::icon("user-check")
                                .size(px(12.))
                                .text_color(theme::faint()),
                        )
                        .child(words(owners))
                        .tip(format!("Code owners: {all}"))
                        .into_any_element(),
                ),
                ..Default::default()
            }
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owners_are_said_in_few_words() {
        let o = |names: &[&str]| names.iter().map(|n| n.to_string()).collect::<Vec<_>>();
        assert_eq!(words(&o(&["ana"])), "@ana");
        assert_eq!(words(&o(&["ana", "ben"])), "@ana and @ben");
        assert_eq!(
            words(&o(&["ana", "ben", "cem", "dan"])),
            "@ana, @ben and 2 more"
        );
    }

    #[test]
    fn the_rules_are_read_again_when_the_file_changes() {
        let dir = tempfile::tempdir().unwrap();
        let read = RefCell::new(Read::default());
        assert!(rules(&read, dir.path()).is_empty());
        std::fs::create_dir(dir.path().join(".github")).unwrap();
        std::fs::write(dir.path().join(".github/CODEOWNERS"), "*.kt @ana\n").unwrap();
        let rules = rules(&read, dir.path());
        assert_eq!(codeowners::owners(&rules, "app/Main.kt"), ["ana"]);
        assert!(codeowners::owners(&rules, "README.md").is_empty());
    }
}
