//! Zen's head (#55): one line over the diff instead of the bar and the header — what is shown, the file, how far
//! the review is. One file at a time: the file's name and folder, where it is among the files ("file 3 of 8") and a
//! bar per file — reviewed, the one shown, the rest.

use crate::review::Review;
use crate::reviewed::State;
use artoo_ui_kit::{hover, island, px, theme};
use gpui::{AnyElement, FontWeight, IntoElement, div, prelude::*};

impl Review {
    /// Zen's head: `title` says what is shown (the tab's words for it); `one` file at a time or all.
    pub fn render_zen_head(&self, title: String, one: bool) -> AnyElement {
        let file = self
            .diff
            .as_ref()
            .and_then(|d| d.files.get(self.file))
            .map(|f| f.path().to_owned());
        if one && let Some(path) = &file {
            let order = self.file_order();
            let at = order.iter().position(|&ix| ix == self.file).unwrap_or(0);
            let (dir, name) = match path.rsplit_once('/') {
                Some((dir, name)) => (format!("{dir}/"), name.to_owned()),
                None => (String::new(), path.clone()),
            };
            let bars = order.iter().map(|&ix| {
                let color = if ix == self.file {
                    theme::accent()
                } else if self.review_state(ix) == Some(State::Done) {
                    theme::added()
                } else {
                    theme::hover()
                };
                div().w(px(18.)).h(px(4.)).rounded(px(2.)).bg(color)
            });
            return island()
                .flex_none()
                .flex_row()
                .items_center()
                .gap_3()
                .h(px(36.))
                .px_4()
                .child(
                    div()
                        .flex_none()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(name),
                )
                .child(
                    hover::cut("header-dir", dir)
                        .flex_1()
                        .text_size(px(12.))
                        .text_color(theme::muted()),
                )
                .child(
                    div()
                        .flex_none()
                        .text_size(px(12.))
                        .text_color(theme::muted())
                        .child(format!("file {} of {}", at + 1, order.len())),
                )
                // A bar per file while they fit; past 40 the count says it.
                .children(
                    (order.len() <= 40)
                        .then(|| div().flex_none().flex().gap(px(4.)).children(bars)),
                )
                .into_any_element();
        }
        island()
            .flex_none()
            .flex_row()
            .items_center()
            .gap_3()
            .h(px(36.))
            .px_4()
            .child(
                hover::cut("header-title", title)
                    .max_w(px(520.))
                    .font_weight(FontWeight::SEMIBOLD),
            )
            .children(file.map(|f| {
                hover::cut("header-file", f)
                    .flex_1()
                    .font_family(theme::code_font())
                    .text_size(px(12.))
                    .text_color(theme::muted())
            }))
            .children(self.review_progress().map(|(done, total)| {
                div()
                    .flex_none()
                    .text_size(px(12.))
                    .text_color(theme::muted())
                    .child(format!("{done} / {total} reviewed"))
            }))
            .into_any_element()
    }
}
