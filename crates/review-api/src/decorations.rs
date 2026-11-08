//! What a feature or a plugin puts on a diff's files besides their text (#171 phase 9): a mark in a line's gutter (a
//! linter's warning, coverage), a row under a line (what a tool says of it), words beside the file's name (its code
//! owners). A feature offers a decorator in its `install`; the tab asks every one, in their order, for each file it
//! lays out, and for the header of each file it draws. A decorator keeps what it read (a file's report, its owners)
//! and answers from it: it is asked as often as the rows are laid out.

use artoo_core::git::FileDiff;
use gpui::{AnyElement, App, Global};
use std::path::Path;
use std::rc::Rc;

/// What puts decorations on a diff's files.
#[derive(Clone)]
pub struct Decorator {
    /// Its stable id: `review.codeowners`.
    pub id: &'static str,
    /// Its place among the decorators: a lower one's rows come first under a line, its badge first in the header.
    pub order: i32,
    /// What it puts on a file (`Decorating`).
    pub decorate: Decorate,
}

/// What a decorator puts on a file (`Decorator::decorate`).
pub type Decorate = Rc<dyn Fn(&Decorating, &App) -> Decorations>;

/// A row under a line drawn, given how far the code is indented (`Under::draw`).
pub type DrawUnder = Rc<dyn Fn(f32, &App) -> AnyElement>;

/// The file a decorator is asked about.
pub struct Decorating<'a> {
    pub file: &'a FileDiff,
    /// The repository's folder.
    pub root: Option<&'a Path>,
}

/// What a decorator puts on one file.
#[derive(Default)]
pub struct Decorations {
    /// Marks in the gutter.
    pub marks: Vec<Mark>,
    /// Rows under lines.
    pub rows: Vec<Under>,
    /// Beside the file's name in its header.
    pub badge: Option<AnyElement>,
}

/// A mark in a line's gutter, in a colour of the decorator's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mark {
    /// On the old side's line (`true`) or the new side's.
    pub old: bool,
    pub line: u32,
    /// `0xRRGGBB`.
    pub rgb: u32,
}

/// A row under a line, drawn by its decorator.
#[derive(Clone)]
pub struct Under {
    pub old: bool,
    pub line: u32,
    /// The row, given how far the code is indented.
    pub draw: DrawUnder,
}

#[derive(Default)]
struct Decorators {
    all: Vec<Decorator>,
    refused: Vec<String>,
}

impl Global for Decorators {}

impl Decorators {
    /// `decorator` kept, or its id refused (and given back) when one is offered already.
    fn offer(&mut self, decorator: Decorator) -> Option<&'static str> {
        if self.all.iter().any(|d| d.id == decorator.id) {
            self.refused.push(format!("decorator {}", decorator.id));
            return Some(decorator.id);
        }
        self.all.push(decorator);
        None
    }

    fn ordered(&self) -> Vec<Decorator> {
        let mut all = self.all.clone();
        all.sort_by_key(|d| d.order);
        all
    }
}

/// Offers a decorator (a feature's `install`). An id offered already is refused, and the log says so.
pub fn add(cx: &mut App, decorator: Decorator) {
    if let Some(id) = cx.default_global::<Decorators>().offer(decorator) {
        artoo_core::applog::line(&format!("decorator {id} offered twice: the second refused"));
    }
}

/// The decorators offered, by their order (the order they were offered in among equals).
pub fn all(cx: &App) -> Vec<Decorator> {
    cx.try_global::<Decorators>()
        .map(Decorators::ordered)
        .unwrap_or_default()
}

/// The decorators offered a second time and refused (the contributions check: none).
pub fn refused(cx: &App) -> Vec<String> {
    cx.try_global::<Decorators>()
        .map(|d| d.refused.clone())
        .unwrap_or_default()
}

/// What every decorator puts on `file`, in their order.
pub fn of(file: &FileDiff, root: Option<&Path>, cx: &App) -> Vec<Decorations> {
    let at = Decorating { file, root };
    all(cx).iter().map(|d| (d.decorate)(&at, cx)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decorator(id: &'static str, order: i32) -> Decorator {
        Decorator {
            id,
            order,
            decorate: Rc::new(|_, _| Decorations::default()),
        }
    }

    #[test]
    fn decorators_keep_their_order_and_a_second_id_is_refused() {
        let mut decorators = Decorators::default();
        assert_eq!(decorators.offer(decorator("lint", 20)), None);
        assert_eq!(decorators.offer(decorator("coverage", 10)), None);
        assert_eq!(decorators.offer(decorator("lint", 5)), Some("lint"));
        let ids: Vec<_> = decorators.ordered().iter().map(|d| d.id).collect();
        assert_eq!(ids, ["coverage", "lint"]);
        assert_eq!(decorators.refused, ["decorator lint"]);
    }
}
