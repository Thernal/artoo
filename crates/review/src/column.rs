//! The diff's column (#165 I5d): the file's pane with the diff's toolbar over it — which range is shown, a part's
//! piece, then how it is drawn (`render_options`) —, the find bar and the line typed after `c` over its rows, then
//! the parts' cards; zen's one file at a reading width (#55); while nothing is shown, lines shimmering as the diff is
//! read or what to pick. What the shell and the other features put on it comes drawn in a `ColumnLook`.

use crate::pane::PaneLook;
use crate::review::{Host, Review};
use crate::{find, goline};
pub use artoo_review_api::ColumnLook;
use artoo_shell_api::input_box;
use artoo_ui_kit::{island, motion, px, theme};
use gpui::{AnyElement, Context, Div, div, prelude::*};
use std::rc::Rc;

/// The most a file shown alone in zen is wide: a reading width (#55).
const READING_WIDTH: f32 = 1100.;

impl Review {
    /// The diff's column with what `look` brings: a press in its fields gives them the keyboard through the tab's
    /// `Frame`; its rows' pins and its clicks are `host`'s.
    pub fn render_column(
        &self,
        look: ColumnLook,
        host: Rc<dyn Host>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self.diff.is_none() {
            // Loading: a shimmer of lines where the diff will be (#56, 18).
            if self.diff_task.is_some() {
                return island()
                    .flex_1()
                    .bg(theme::editor())
                    .p(px(24.))
                    .child(motion::shimmer("diff-loading", 9))
                    .into_any_element();
            }
            return island()
                .flex_1()
                .items_center()
                .justify_center()
                .bg(theme::editor())
                .text_color(theme::muted())
                .child("Select a commit or a version.")
                .into_any_element();
        }
        let ColumnLook {
            border,
            mut top,
            bar,
            menu_open,
            open_in,
            cards,
            alone,
            zen_one,
            typing,
        } = look;
        top.push(
            self.render_toolbar(bar, menu_open, host.clone())
                .into_any_element(),
        );
        let this = cx.entity().downgrade();
        let frame = self.frame.as_ref().and_then(std::rc::Weak::upgrade);
        let boxed = |field: Div, id: &'static str| match &frame {
            Some(frame) => input_box(field, id, frame),
            None => field,
        };
        let find = self
            .render_find_bar(typing == Some(find::INPUT), this.clone())
            .map(|bar| boxed(bar, find::INPUT).into_any_element());
        // Painted last, over the rows that come after it.
        let goline = self
            .render_goline(typing == Some(goline::INPUT), this)
            .map(|prompt| {
                gpui::deferred(boxed(prompt, goline::INPUT))
                    .with_priority(1)
                    .into_any_element()
            });
        let cards = cards
            .into_iter()
            .map(|card| gpui::deferred(card).with_priority(1).into_any_element());
        let look = PaneLook {
            border,
            top,
            open_in,
            over: find.into_iter().chain(goline).chain(cards).collect(),
            alone,
        };
        let file = self.render_pane(look, host, cx);
        div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .child(div().relative().flex_1().min_h_0().flex().map(|s| {
                if zen_one {
                    // One file at a reading width, the next one sliding in as it comes.
                    s.justify_center().child(motion::appear(
                        div()
                            .relative()
                            .flex_1()
                            .min_w_0()
                            .max_w(px(READING_WIDTH))
                            .flex()
                            .child(file),
                        ("zen-file", self.file),
                        motion::BASE,
                        |d, v| d.top(px(16. * (1. - v))).opacity(v),
                    ))
                } else {
                    s.child(file)
                }
            }))
            .into_any_element()
    }

    /// The diff's toolbar, read left to right: what is shown (`bar`), then how it is drawn.
    fn render_toolbar(&self, bar: Option<AnyElement>, menu_open: bool, host: Rc<dyn Host>) -> Div {
        // A narrow diff wraps it: how it is drawn goes under what is shown, never out of view.
        div()
            .flex_none()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .min_h(px(40.))
            .px(px(12.))
            .py(px(6.))
            .border_b_1()
            .border_color(theme::island_border())
            .children(bar)
            .child(div().flex_1())
            .child(self.render_options(menu_open, host))
    }
}
