//! The Changes list as the review keeps it: a file's key in the tree, the rows shown (`changes::file_items`) and
//! the files in their order, the folders opened to show a file, the list scrolled to it.

use crate::changes::{FileItem, file_items, files_of, inside, row_key};
use crate::review::Review;
use gpui::ScrollStrategy;
use std::collections::HashSet;

impl Review {
    /// File `ix`'s key in the tree (`row_key`).
    pub fn file_key(&self, ix: usize) -> Option<String> {
        let diff = self.diff.as_ref()?;
        Some(row_key(diff.tags.staged, ix, diff.files.get(ix)?.path()))
    }

    /// Opens every collapsed folder file `ix` is under, at any depth, and its group.
    pub fn open_folders_of(&mut self, ix: usize) {
        if let Some(key) = self.file_key(ix) {
            self.collapsed.retain(|dir| !inside(&key, dir));
        }
    }

    pub fn items(&self) -> Vec<FileItem> {
        match &self.diff {
            Some(diff) => file_items(
                &diff.files,
                self.options.list_files,
                &self.pfilter,
                &self.collapsed,
                diff.tags.staged,
            ),
            None => Vec::new(),
        }
    }

    /// File indexes in the order the island shows them: what ← and → walk through.
    pub fn file_order(&self) -> Vec<usize> {
        files_of(self.items())
    }

    /// File indexes in the tree's order with every folder open: what `v` walks, so a collapsed
    /// folder's files are not passed over (#167). A filter still narrows it.
    pub fn file_order_all(&self) -> Vec<usize> {
        match &self.diff {
            Some(diff) => files_of(file_items(
                &diff.files,
                self.options.list_files,
                &self.pfilter,
                &HashSet::new(),
                diff.tags.staged,
            )),
            None => Vec::new(),
        }
    }

    /// Scrolls the files island to show file `ix`.
    pub fn scroll_file_into_view(&self, ix: usize) {
        let at = self
            .items()
            .iter()
            .position(|item| matches!(item, FileItem::File { ix: i, .. } if *i == ix));
        if let Some(at) = at {
            self.file_scroll.scroll_to_item(at, ScrollStrategy::Nearest);
        }
    }
}
