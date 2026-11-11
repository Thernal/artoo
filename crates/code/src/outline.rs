//! The Outline tool window (#76): the declarations of the file read in Code, each at its depth with its kind and
//! its line; a click brings the line into view. The file and what it declares are the review's
//! (`artoo_review_api::Review`), found in the tab's services as the window draws; the click goes to the tab
//! (`Host::reveal_line`), which knows the notes pinned on the file's lines.

use artoo_code_api::Host;
use artoo_review_api::Review;
use artoo_shell_api::{Frame, Tool};
use artoo_ui_kit::{hover, px, row, theme};
use gpui::{App, ClickEvent, Context, SharedString, Window, div, prelude::*};
use std::cell::Cell;
use std::rc::Rc;

/// The window's id: the layout keeps it open by it.
const ID: &str = "outline";

/// The Outline tool window in a tab: in Code, its count after the title, nothing on its icon.
pub(crate) fn tool(frame: Rc<dyn Frame>, host: Rc<dyn Host>, cx: &mut App) -> Tool {
    let counting = frame.clone();
    let view = cx.new(|_| Outline { frame, host });
    Tool {
        id: ID,
        icon: "list",
        tip: "Outline",
        title: "Outline",
        spaces: &[artoo_code_api::SPACE],
        on_pages: false,
        stripe: true,
        // About the page: before the windows that are in every space.
        order: 10,
        badge: None,
        count: Some(Rc::new(move |cx| {
            let n = review(&*counting).map_or(0, |r| r.file_outline(cx).len());
            (n > 0).then(|| n.to_string())
        })),
        view: view.into(),
        opened: Rc::new(|_| {}),
        typing: Rc::new(Cell::new(false)),
        key: Rc::new(|_, _, _| {}),
        list_key: None,
    }
}

/// The tab's review, which reads Code's file.
fn review(frame: &dyn Frame) -> Option<Rc<dyn Review>> {
    frame.services().get()
}

struct Outline {
    frame: Rc<dyn Frame>,
    host: Rc<dyn Host>,
}

impl Render for Outline {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let review = review(&*self.frame);
        let outline = review
            .as_ref()
            .map(|r| r.file_outline(cx))
            .unwrap_or_default();
        let n = outline.len();
        let rows = outline.into_iter().map(|(line, depth, kind, name)| {
            let host = self.host.clone();
            row(SharedString::from(format!("outline-{line}")), false)
                .h(px(26.))
                .pl(px(10. + depth as f32 * 14.))
                .gap(px(8.))
                .text_size(px(12.))
                .child(
                    div()
                        .flex_none()
                        .font_family(theme::code_font())
                        .text_color(theme::faint())
                        .child(kind),
                )
                .child(hover::cut("name", name).flex_1())
                .child(
                    div()
                        .flex_none()
                        .text_size(px(11.))
                        .text_color(theme::faint())
                        .child(line.to_string()),
                )
                .on_click(move |_: &ClickEvent, _, cx| host.reveal_line(line, cx))
        });
        let empty = match review.as_ref().and_then(|r| r.file_read(cx)) {
            None => "Open a file to see what it declares.",
            Some(_) => "Nothing declared here that Artoo reads.",
        };
        div()
            .id("outline-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p(px(6.))
            .children(rows)
            .children((n == 0).then(|| {
                div()
                    .px(px(8.))
                    .py(px(6.))
                    .text_size(px(12.))
                    .text_color(theme::muted())
                    .child(empty)
            }))
    }
}
