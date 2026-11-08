//! The review (#165 stage 4): a diff read file by file or in one scroll. The entity (`review::Review`) with its
//! events and its host, what it keeps (the selection, the marks, blame, the stack, the pins, the diff, images,
//! pages, the file view, the Changes list's rows) and how it draws its rows, the files' headers and the file read
//! in Code, the diff's column (`column`) and the file's pane in it (`pane`), zen's head (`zen`), the Changes list
//! (`changes_view`), Find in files (`find_files`) and the find bar (`find`), whose field is one of its parts' inputs
//! as the line typed after `c` is (`goline`).

mod annotate;
mod changes;
mod changes_view;
mod column;
mod diff;
mod diff_view;
mod file_view;
mod find;
mod find_files;
mod goline;
mod handle;
mod image_view;
mod item;
mod lines;
mod lists;
mod md_page;
mod pane;
mod path_filter;
mod pins;
mod review;
mod reviewed;
mod select;
mod stack;
mod zen;

use artoo_shell_api::{Frame, Parts};
use gpui::{App, AppContext, Window};
use std::rc::Rc;

/// The review's parts in every tab: the find bar's field and the line typed after `c` as its inputs; its item
/// page's middle among the tab's services (`item`).
pub fn install(cx: &mut App) {
    artoo_review_api::viewers::add(cx, md_page::viewer());
    artoo_review_api::viewers::add(cx, image_view::viewer());
    artoo_shell_api::add_parts(cx, parts);
}

/// The parts of a tab that offers its review; none in one that does not.
/// The parts of a tab that hosts the review (`artoo_review_api::Host`): the review made, offered to the tab and its
/// pages (`Review`, the file Code reads), its page and its inputs; none in another tab.
fn parts(frame: Rc<dyn Frame>, _: &mut Window, cx: &mut App) -> Parts {
    let services = frame.services();
    if services.get::<Rc<dyn artoo_review_api::Host>>().is_none() {
        return Parts::default();
    }
    let settings = artoo_core::storage::Settings::load();
    let review = cx.new(|_| review::Review::new(settings));
    review.update(cx, |rv, cx| {
        rv.frame = Some(Rc::downgrade(&frame));
        rv.viewers = artoo_review_api::viewers::all(cx);
    });
    services.set(file_view::file_view(&review));
    services.set(handle::handle(&review));
    services.set(item::view(review.clone(), &frame, cx));
    Parts {
        inputs: vec![
            find::input(review.clone()),
            goline::input(review.clone()),
            path_filter::input(review),
        ],
        ..Default::default()
    }
}
