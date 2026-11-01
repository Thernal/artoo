//! An item page's middle (#165 I5d), the review's view: what the route on screen is of, the parts' pieces over the
//! diff, then the diff's column (`column`) or a part's piece in its place, each asked of the tab's `Frame` as the
//! view draws. The tab's services offer it (`ItemView`); a page puts it where it goes.

use crate::review::{Host, Review};
use artoo_review_api::{ColumnLook, ItemView};
use artoo_shell_api::{Frame, Slot};
use gpui::{AnyElement, App, Context, Entity, Render, Window, div, prelude::*};
use std::rc::{Rc, Weak};

/// The view.
pub struct ItemPage {
    review: Entity<Review>,
    /// Weak: the tab's services hold the page.
    frame: Weak<dyn Frame>,
}

/// The page over `review` in the tab `frame` is of.
pub fn view(review: Entity<Review>, frame: &Rc<dyn Frame>, cx: &mut App) -> ItemView {
    let frame = Rc::downgrade(frame);
    ItemView(cx.new(|_| ItemPage { review, frame }).into())
}

impl Render for ItemPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let page = div().flex_1().min_w_0().min_h_0().flex().flex_col();
        let Some(frame) = self.frame.upgrade() else {
            return page;
        };
        let identity = frame.identity(cx);
        let over = frame.pieces(Slot::OverDiff, cx);
        // A part's in the diff's place: a new merge request being written, two versions' commits paired.
        let diff = match frame.piece(Slot::Instead, cx) {
            Some(instead) => instead,
            None => self.column(&frame, cx),
        };
        page.children(identity).children(over).child(diff)
    }
}

impl ItemPage {
    /// The diff's column (`Review::render_column`) with what the shell and the parts put on it.
    fn column(&self, frame: &Rc<dyn Frame>, cx: &mut App) -> AnyElement {
        let Some(host) = frame.services().get::<Rc<dyn Host>>() else {
            return div().into_any_element();
        };
        let chrome = frame.chrome(cx);
        let look = ColumnLook {
            border: chrome.border,
            // The parts' (a new request's switch, an outdated discussion's stepper).
            top: frame.pieces(Slot::DiffTop, cx),
            bar: frame.piece(Slot::DiffBar, cx),
            menu_open: chrome.menu_open,
            open_in: chrome.open_in,
            // What comes next once the last file is marked.
            cards: frame.pieces(Slot::OverRows, cx),
            alone: chrome.alone,
            zen_one: chrome.zen_one,
            typing: chrome.typing,
        };
        self.review
            .update(cx, |rv, cx| rv.render_column(look, host, cx))
    }
}
