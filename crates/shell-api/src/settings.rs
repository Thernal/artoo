//! Settings' contract: its sections in the left column, the one on with its heading and Done, its rows fading in
//! (#56, 14). The sections are offered (`add_settings_section`), each drawn by its owner in each tab: the window
//! root's own, or the rows a feature's parts put under it (`Parts::settings`), any owner's rows in their places
//! (`Rows::order`); the frame draws the page around them.

use crate::{Hook, render};
use gpui::{AnyElement, AnyView, App, Context, Entity, Window, div, prelude::*};
use std::rc::Rc;

/// A section of Settings (`add_settings_section`): its place in the left column and its title. What it holds its
/// owner draws in each tab: the window root's own, a feature's as its parts' rows (`Parts::settings`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Section {
    /// A stable id (`agents`): what parts put rows under, what a toast's button opens.
    pub id: &'static str,
    /// Its words in the left column and over its rows.
    pub title: &'static str,
    /// Its place in the left column, the lowest first; those of one order as they were offered.
    pub order: u16,
}

/// About's id: the window root draws it, the updates' rows go into it (#203).
pub const ABOUT: &str = "about";

/// A setting by its words (`add_setting`): what Settings' search and ⌘K find, and the section that holds it (#203).
/// The window root's rows are found with their live controls; another owner's lead to their section.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Named {
    /// The section's id (`Section::id`).
    pub section: &'static str,
    /// Its name on its row.
    pub label: &'static str,
    /// Its row's line, if it has one.
    pub detail: &'static str,
    /// What it answers to beyond its label: what it is about, the old section's name.
    pub words: &'static str,
}

impl Named {
    pub const fn new(section: &'static str, label: &'static str) -> Self {
        Self {
            section,
            label,
            detail: "",
            words: "",
        }
    }

    pub const fn detail(self, detail: &'static str) -> Self {
        Self { detail, ..self }
    }

    pub const fn words(self, words: &'static str) -> Self {
        Self { words, ..self }
    }

    /// Whether every word typed is in its label, its line, its words or its section's title.
    pub fn found(&self, query: &str, section: &str) -> bool {
        let hay = format!("{} {} {} {section}", self.label, self.detail, self.words).to_lowercase();
        let mut words = query.split_whitespace().peekable();
        words.peek().is_some() && words.all(|w| hay.contains(&w.to_lowercase()))
    }
}

/// Rows a part draws under a section of Settings (`Parts::settings`), its own or another's: Appearance holds the
/// themes' and the window root's faces and panes.
#[derive(Clone)]
pub struct Rows {
    /// The section's id (`Section::id`).
    pub section: &'static str,
    /// Their place among the section's rows, the lowest first; those of one order as their features were installed.
    pub order: u16,
    pub view: AnyView,
    /// What Set up's part for the section draws of them, if anything (Look: the Window themes, five to a row).
    pub short: Option<AnyView>,
    /// While it says so, these rows alone fill the section, given the page's height (the theme editor).
    pub fills: Option<Fills>,
    /// Esc while the section is on, before Settings closes: whether they took it (the editor's token, then the
    /// editor). It runs inside the tab's update: it reads its own state, never the tab.
    pub escape: Option<Took>,
    /// Settings closed (esc, Done, a place): what was begun in them ends (the theme being edited). Asked as the tab
    /// is drawn while Settings is shut.
    pub closed: Option<Hook>,
    /// Their section was opened (a click in the left column, a toast's button): what they show is read anew (the
    /// hosts' ways in looked for). It runs inside the tab's update: it defers what reads the `Frame`.
    pub opened: Option<Hook>,
    /// Each of their settings as a row of its own, by its name (`Named::label`): what Settings' search shows, live,
    /// for a hit (#203). A setting with none leads to its section.
    pub hits: Vec<(&'static str, AnyView)>,
}

/// One of `owner`'s rows as a view of its own (`Rows::hit`): `draw` draws it from `owner`, again whenever `owner`
/// changes.
pub fn row_view<T: 'static>(
    owner: Entity<T>,
    draw: fn(&mut T, &mut Context<T>) -> AnyElement,
    cx: &mut App,
) -> AnyView {
    cx.new(|cx| {
        cx.observe(&owner, |_, _, cx| cx.notify()).detach();
        OneRow { owner, draw }
    })
    .into()
}

/// A row of another's, drawn by it (`row_view`).
struct OneRow<T: 'static> {
    owner: Entity<T>,
    draw: fn(&mut T, &mut Context<T>) -> AnyElement,
}

impl<T: 'static> Render for OneRow<T> {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let draw = self.draw;
        self.owner.update(cx, draw)
    }
}

/// Whether rows fill their section now (`Rows::fills`).
pub type Fills = Rc<dyn Fn(&App) -> bool>;

/// Whether rows took a key (`Rows::escape`).
pub type Took = Rc<dyn Fn(&mut App) -> bool>;

impl Rows {
    pub fn new(section: &'static str, order: u16, view: impl Into<AnyView>) -> Self {
        Self {
            section,
            order,
            view: view.into(),
            short: None,
            fills: None,
            escape: None,
            closed: None,
            opened: None,
            hits: Vec::new(),
        }
    }

    /// Setting `label`'s row as a view of its own (`row_view`), for Settings' search (#203).
    pub fn hit(mut self, label: &'static str, view: impl Into<AnyView>) -> Self {
        self.hits.push((label, view.into()));
        self
    }

    pub fn short(self, view: impl Into<AnyView>) -> Self {
        Self {
            short: Some(view.into()),
            ..self
        }
    }

    pub fn fills(self, now: impl Fn(&App) -> bool + 'static) -> Self {
        Self {
            fills: Some(Rc::new(now)),
            ..self
        }
    }

    pub fn escape(self, took: impl Fn(&mut App) -> bool + 'static) -> Self {
        Self {
            escape: Some(Rc::new(took)),
            ..self
        }
    }

    pub fn closed(self, end: impl Fn(&mut App) + 'static) -> Self {
        Self {
            closed: Some(Rc::new(end)),
            ..self
        }
    }

    pub fn opened(self, read: impl Fn(&mut App) + 'static) -> Self {
        Self {
            opened: Some(Rc::new(read)),
            ..self
        }
    }
}

/// The page as the tab sees it.
pub struct Page {
    /// The sections offered, in their order.
    pub sections: Vec<Section>,
    /// The section on, by its id.
    pub on: &'static str,
    /// Its rows.
    pub content: AnyElement,
    /// Rows that fill the section take the page's height (the theme editor: its tokens scroll beside a preview that
    /// stays).
    pub editing: bool,
    /// The search field at the head of the left column (#203).
    pub search: AnyElement,
    /// While something is typed in it: how many settings each section has that answer to it. The content is then
    /// the hits under their sections, with its own heading.
    pub found: Option<Vec<(&'static str, usize)>>,
}

/// What a click on the frame does.
pub trait SettingsHost {
    /// A section in the left column, by its id.
    fn section(&self, id: &'static str, window: &mut Window, cx: &mut App);
    /// Done: Settings closes.
    fn done(&self, window: &mut Window, cx: &mut App);
}

/// Draws the page: the frame's.
pub trait SettingsRenderer {
    fn page(&self, page: Page, host: Rc<dyn SettingsHost>) -> AnyElement;
}

/// The page, drawn by the frame installed.
pub fn page(page: Page, host: Rc<dyn SettingsHost>, cx: &App) -> AnyElement {
    match render::renderer::<dyn SettingsRenderer>(cx) {
        Some(renderer) => renderer.page(page, host),
        None => div().into_any_element(),
    }
}

#[cfg(test)]
mod tests {
    use super::Named;

    #[test]
    fn a_setting_is_found_by_any_word_of_its_name_its_line_its_words_or_its_section() {
        let fold = Named::new("review", "Fold generated files")
            .detail("Lock files and generated code.")
            .words("diff collapse");
        for query in [
            "fold",
            "GENERATED files",
            "lock",
            "diff",
            "collapse",
            "review fold",
        ] {
            assert!(fold.found(query, "Review"), "{query}");
        }
        for query in ["", "  ", "font", "fold font"] {
            assert!(!fold.found(query, "Review"), "{query}");
        }
    }
}
