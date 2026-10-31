//! Motion that reads the workspace's state (#110): a tree's rows fading in under a folder just opened,
//! and what folds open and shut. The curves, springs and marks are the kit's (`artoo_ui_kit::motion`).

pub(super) use artoo_ui_kit::motion::*;
use gpui::IntoElement;

impl super::Workspace {
    /// Folder `dir` of a tree opened: the rows it shows fade in. A tree's rows keep one height
    /// (a uniform list), so only their alpha moves (#110).
    pub(super) fn tree_opening(&mut self, dir: &str, cx: &mut gpui::Context<Self>) {
        let generation = self.tree_opened.as_ref().map_or(0, |(_, g)| g + 1);
        self.tree_opened = Some((dir.to_owned(), generation));
        self.mark_motion("tree-open", cx);
    }

    /// Row `path` of a tree, fading in when a folder just opened above it.
    pub(super) fn tree_row(&self, path: &str, row: gpui::AnyElement) -> gpui::AnyElement {
        use gpui::{div, prelude::*};
        match &self.tree_opened {
            Some((dir, generation))
                if self.motion.recent("tree-open")
                    && path
                        .strip_prefix(dir.as_str())
                        .is_some_and(|r| r.starts_with('/')) =>
            {
                appear(
                    div().child(row),
                    gpui::SharedString::from(format!("tree-in-{generation}-{path}")),
                    BASE,
                    |row, v| row.opacity(v),
                )
                .into_any_element()
            }
            _ => row,
        }
    }

    /// Something that folds open and shut (the kit's `folding`): `moving` names the mark its toggle set
    /// (`mark_motion`).
    pub(super) fn folding(
        &self,
        key: impl Into<String>,
        moving: &str,
        open: bool,
        content: impl IntoElement + 'static,
    ) -> Option<gpui::AnyElement> {
        folding(
            &self.folds,
            &self.heights,
            self.motion.recent(moving),
            key,
            open,
            content,
        )
    }
}
