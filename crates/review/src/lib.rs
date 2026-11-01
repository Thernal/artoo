//! The review (#165 stage 4): a diff read file by file or in one scroll. The entity (`review::Review`) with its
//! events and its host, what it keeps (the selection, the marks, blame, the stack, the pins, the diff, images,
//! pages, the file view, the Changes list's rows) and how it draws its rows, the files' headers and the file read
//! in Code, the diff's column (`column`) and the file's pane in it (`pane`), zen's head (`zen`), the Changes list
//! (`changes_view`), Find in files (`find_files`) and the find bar (`find`), whose field is one of its parts' inputs
//! as the line typed after `c` is (`goline`).

pub mod annotate;
pub mod changes;
pub mod changes_view;
pub mod column;
pub mod composer;
pub mod diff;
pub mod diff_view;
pub mod file_view;
pub mod find;
pub mod find_files;
pub mod goline;
pub mod image_view;
pub mod item;
pub mod lines;
pub mod lists;
pub mod md_page;
pub mod pane;
pub mod pins;
pub mod review;
pub mod reviewed;
pub mod select;
pub mod stack;
pub mod zen;

use artoo_shell_api::{Frame, Parts};
use gpui::{App, Entity, Window};
use std::rc::Rc;

/// The review's parts in every tab: the find bar's field and the line typed after `c` as its inputs; its item
/// page's middle among the tab's services (`item`).
pub fn install(cx: &mut App) {
    artoo_shell_api::add_parts(cx, parts);
}

/// The parts of a tab that offers its review; none in one that does not.
fn parts(frame: Rc<dyn Frame>, _: &mut Window, cx: &mut App) -> Parts {
    let Some(review) = frame.services().get::<Entity<review::Review>>() else {
        return Parts::default();
    };
    review.update(cx, |rv, _| rv.frame = Some(Rc::downgrade(&frame)));
    frame.services().set(item::view(review.clone(), &frame, cx));
    Parts {
        inputs: vec![find::input(review.clone()), goline::input(review)],
        ..Default::default()
    }
}
