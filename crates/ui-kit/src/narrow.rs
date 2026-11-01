//! Narrow windows' rules (#80): under 1280 px the menu folds to icons and the diff is unified, under 1100 the side
//! panel folds into the menu's column and the page is compact, under 1000 a tool window floats over the diff. The
//! steps are the design's pixels, so at a larger interface size they come sooner (#24). The tab lays out by them.

/// Under it the menu folds and the diff is unified.
pub const MENU: f32 = 1280.;
/// Under it the side panel folds into the menu's column, and the page is compact.
pub const SIDE: f32 = 1100.;
/// Under it a tool window floats over the diff.
pub const FLOAT: f32 = 1000.;

/// The menu folded in a window `width` px wide: as `set` from 1280 px, else folded unless
/// `chosen` there.
fn menu_fold(width: f32, set: bool, chosen: Option<bool>) -> bool {
    if width < MENU {
        chosen.unwrap_or(true)
    } else {
        set
    }
}

/// What a narrow window keeps for the session: the menu's fold chosen while it is narrow, and
/// whether the side panel is open as a drawer.
#[derive(Default)]
pub struct Narrowed {
    menu: Option<bool>,
    pub drawer: bool,
}

impl Narrowed {
    /// The window, `width` design pixels wide, has crossed a step: what it folded comes back as it was.
    pub fn follow(&mut self, width: f32) {
        if width >= MENU {
            self.menu = None;
        }
        if width >= SIDE {
            self.drawer = false;
        }
    }

    /// The menu folded to icons in a window `width` wide: `set` (the setting), or under 1280 px folded unless
    /// opened there.
    pub fn menu_folded(&self, width: f32, set: bool) -> bool {
        menu_fold(width, set, self.menu)
    }

    /// The menu folded (or opened) while the window is narrow.
    pub fn fold_menu(&mut self, folded: bool) {
        self.menu = Some(folded);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_menu_folds_under_1280_unless_opened_there() {
        // Wide: the setting.
        assert!(!menu_fold(1480., false, None));
        assert!(menu_fold(1480., true, Some(false)));
        // Narrow: folded, whatever the setting, until ◧ or ⌘B opens it there.
        assert!(menu_fold(1200., false, None));
        assert!(!menu_fold(1200., true, Some(false)));
        assert!(!menu_fold(MENU, false, None));
    }
}
