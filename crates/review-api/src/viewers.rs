//! The ways a changed file is looked at besides its text diff (#171 phase 9): a document read as a page, an image,
//! and what a plugin adds — a table, a notebook, a PDF. A feature offers a viewer in its `install`; the review asks
//! the viewers in their order, and the first that claims a file draws it, on its own in the pane and among every file
//! in one scroll. A file read as Source (its switch, remembered per repository) is drawn as its text diff whatever
//! claims it.

use crate::Host;
use artoo_core::git::FileDiff;
use gpui::{AnyElement, App, Global};
use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;

/// A way to look at a changed file other than its text diff.
#[derive(Clone)]
pub struct Viewer {
    /// Its stable id: `review.page`.
    pub id: &'static str,
    /// Its place among the viewers that claim the same file: the lowest draws it.
    pub order: i32,
    /// Whether it shows `file`: by its path, its sides' kinds or texts.
    pub claims: Rc<dyn Fn(&FileDiff) -> bool>,
    /// The file drawn (`Drawing`).
    pub draw: Rc<dyn Fn(&Drawing, &App) -> AnyElement>,
}

/// What a viewer is given to draw a file with.
pub struct Drawing<'a> {
    pub file: &'a FileDiff,
    /// Its place among the diff's files.
    pub ix: usize,
    /// On its own in the pane (`true`), or among every file in one scroll.
    pub alone: bool,
    /// What answers its clicks: the tab's (`Host::file`, `Host::pane`).
    pub host: &'a Rc<dyn Host>,
    /// The review drawing it: the review's own viewers read their state there; a plugin's has no use for it.
    pub review: &'a dyn Any,
    /// What the tab drew of it before the pane, taken by the viewer that asked for it (a document's page alone,
    /// which needs the tab's look while the review is being drawn).
    pub ready: RefCell<Option<AnyElement>>,
}

#[derive(Default)]
struct Viewers {
    all: Vec<Viewer>,
    /// The ids offered a second time and refused.
    refused: Vec<String>,
}

impl Global for Viewers {}

/// Offers a viewer (a feature's `install`). An id offered already is refused, and the log says so.
pub fn add(cx: &mut App, viewer: Viewer) {
    if let Some(id) = cx.default_global::<Viewers>().offer(viewer) {
        artoo_core::applog::line(&format!("viewer {id} offered twice: the second refused"));
    }
}

impl Viewers {
    /// `viewer` kept, or its id refused (and given back) when one is offered already.
    fn offer(&mut self, viewer: Viewer) -> Option<&'static str> {
        if self.all.iter().any(|v| v.id == viewer.id) {
            self.refused.push(format!("viewer {}", viewer.id));
            return Some(viewer.id);
        }
        self.all.push(viewer);
        None
    }

    /// By their order, the order they were offered in among equals.
    fn ordered(&self) -> Vec<Viewer> {
        let mut all = self.all.clone();
        all.sort_by_key(|v| v.order);
        all
    }
}

/// The viewers offered, by their order (the order they were offered in among equals).
pub fn all(cx: &App) -> Vec<Viewer> {
    cx.try_global::<Viewers>()
        .map(Viewers::ordered)
        .unwrap_or_default()
}

/// The viewers offered a second time and refused (the contributions check: none).
pub fn refused(cx: &App) -> Vec<String> {
    cx.try_global::<Viewers>()
        .map(|v| v.refused.clone())
        .unwrap_or_default()
}

/// The first of `viewers` (in order) that claims `file`.
pub fn of<'a>(viewers: &'a [Viewer], file: &FileDiff) -> Option<&'a Viewer> {
    viewers.iter().find(|v| (v.claims)(file))
}

#[cfg(test)]
mod tests {
    use super::*;
    use artoo_core::git::ChangeKind;
    use gpui::{IntoElement, div};

    fn viewer(id: &'static str, order: i32, ext: &'static str) -> Viewer {
        Viewer {
            id,
            order,
            claims: Rc::new(move |f: &FileDiff| f.path().ends_with(ext)),
            draw: Rc::new(|_, _| div().into_any_element()),
        }
    }

    fn file(path: &str) -> FileDiff {
        FileDiff {
            old_path: None,
            new_path: Some(path.to_owned()),
            change: ChangeKind::Added,
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
    fn the_first_by_order_draws_and_a_second_id_is_refused() {
        let mut viewers = Viewers::default();
        assert_eq!(viewers.offer(viewer("b.table", 20, ".csv")), None);
        assert_eq!(viewers.offer(viewer("a.sheet", 10, ".csv")), None);
        assert_eq!(viewers.offer(viewer("b.table", 5, ".tsv")), Some("b.table"));
        let all = viewers.ordered();
        assert_eq!(
            all.iter().map(|v| v.id).collect::<Vec<_>>(),
            ["a.sheet", "b.table"]
        );
        assert_eq!(viewers.refused, ["viewer b.table"]);
        assert_eq!(
            of(&all, &file("data/rows.csv")).map(|v| v.id),
            Some("a.sheet")
        );
        assert!(of(&all, &file("README.md")).is_none());
    }
}
