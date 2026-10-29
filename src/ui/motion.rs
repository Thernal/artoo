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

    /// Something that folds open and shut (#110; the developer, 2026-10-23: every fold moves, in
    /// size and in alpha): it grows from nothing to the height its content was last drawn at,
    /// fading in, and back the same way. `moving` names the mark its toggle set
    /// (`mark_motion`); shut and still, it is not drawn. Open and still, it takes whatever height
    /// its content has.
    pub(super) fn folding(
        &self,
        key: impl Into<String>,
        moving: &str,
        open: bool,
        content: impl IntoElement + 'static,
    ) -> Option<gpui::AnyElement> {
        use gpui::{div, prelude::*};
        let key: String = key.into();
        // Shut, it is drawn only on its way there; open, it comes in while its mark is fresh.
        let shutting = self.folds.shutting(&key, open);
        if !open && !shutting {
            return None;
        }
        let moving = self.motion.recent(moving);
        let height = self.heights.get(&key);
        let inner = div()
            .relative()
            .flex_none()
            .child(content)
            .child(self.heights.measure(key.clone()));
        Some(
            toward(
                div().flex_none().overflow_hidden().child(inner),
                gpui::SharedString::from(format!("folding-{key}")),
                open,
                moving,
                BASE,
                move |fold, v| {
                    // Not yet measured, it starts from nothing; open and still, it is free.
                    let fold = if v >= 0.999 {
                        fold
                    } else {
                        fold.max_h(super::px(height * v))
                    };
                    fold.opacity(v)
                },
            )
            .into_any_element(),
        )
    }
}
