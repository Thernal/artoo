//! A project's mark (#223): the letters and the colour the rail draws for a repository. A plugin that lists projects
//! — Tasks' boards, an account's repositories — asks `of` for a project's path and draws what comes back, so its
//! mark is the rail's, including the name and colour the user gave the project (Settings → the tab's right click).

use artoo_core::labels;
use artoo_ui_kit::monogram::{self, hue, hue_at, initials};
use gpui::Rgba;
use std::path::Path;

/// What the rail draws for a project: up to two letters on a tint.
#[derive(Clone, Debug, PartialEq)]
pub struct Mark {
    /// Up to two letters, upper case: the first of the name's first two words.
    pub letters: String,
    /// The tint, by the project's own colour when it has one, else by its folder's name.
    pub hue: Rgba,
}

impl Mark {
    /// The rounded square the rail draws, `size` px on a side.
    pub fn draw(&self, size: f32) -> gpui::Div {
        monogram::monogram(&self.letters, self.hue, size)
    }
}

/// The colour of the project at `root` (`name` when it has no folder): its own hue, else one from its folder's name.
pub fn hue_of(root: Option<&Path>, name: &str) -> Rgba {
    let folder = root
        .and_then(|r| r.file_name())
        .map(|n| n.to_string_lossy().into_owned());
    match root.and_then(|r| labels::project(r).hue) {
        Some(degrees) => hue_at(degrees),
        None => hue(folder.as_deref().unwrap_or(name)),
    }
}

/// The mark of the project at `path`, as the rail draws it.
pub fn of(path: &Path) -> Mark {
    let name = labels::project_name(path);
    Mark {
        letters: initials(&name),
        hue: hue_of(Some(path), &name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn a_project_without_a_name_of_its_own_is_marked_by_its_folder() {
        let mark = of(&PathBuf::from("/work/payments-service"));
        assert_eq!(mark.letters, "PS");
        assert_eq!(mark.hue, hue("payments-service"));
    }

    #[test]
    fn the_mark_is_what_the_rail_draws_for_the_same_project() {
        let root = PathBuf::from("/work/ledger");
        let mark = of(&root);
        assert_eq!(mark.letters, initials(&labels::project_name(&root)));
        assert_eq!(mark.hue, hue_of(Some(&root), "ledger"));
    }

    #[test]
    fn a_path_without_a_folder_name_still_has_a_mark() {
        assert!(!of(&PathBuf::from("/")).letters.is_empty());
    }
}
