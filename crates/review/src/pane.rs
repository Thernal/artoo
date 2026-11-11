//! The file's pane (#11, #22, #78): the island the diff's rows are drawn in — over them the file's line (its path,
//! the working tree's word, its note, its changes with ↑ ↓, how its rows were built), a notice the reviewer did not
//! ask for, and the rows or what stands in their place (nothing changed, a generated file folded, a document's page,
//! an image). What other features put on it comes drawn in a `PaneLook`; a click on it goes to the host
//! (`PaneEvent`).

use crate::diff_view::{change_color, file_tag};
use crate::review::{Host, PaneEvent, Review};
use crate::reviewed::State;
use artoo_review_api::viewers::Drawing;
use artoo_ui_kit::controls::{chip, group};
use artoo_ui_kit::format::plural;
use artoo_ui_kit::{ISLAND_RADIUS, ROW_RADIUS, hover, icons, island, keys, px, theme};
use gpui::{
    AnyElement, ClickEvent, Context, Div, FontWeight, MouseButton, Rgba, Stateful, canvas, div,
    prelude::*,
};
use std::cell::RefCell;
use std::rc::Rc;

/// What other features put on the file's pane, drawn by them.
pub struct PaneLook {
    /// The pane's border: whether the keyboard works in it.
    pub border: Rgba,
    /// Over the file's line, in order: a new request's switch, the since bar, the toolbar.
    pub top: Vec<AnyElement>,
    /// On the file's line after its changes: the app it opens in.
    pub open_in: Option<AnyElement>,
    /// Over the rows, in order: the find bar, the go-to-line prompt, the done card.
    pub over: Vec<AnyElement>,
    /// The file as a page, when it is shown so (`shows_page`).
    pub alone: Option<AnyElement>,
}

impl Review {
    /// The file's pane, with what `look` brings; its rows' pins and its clicks are `host`'s.
    pub fn render_pane(
        &self,
        look: PaneLook,
        host: Rc<dyn Host>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(diff) = self.diff.clone() else {
            return div().into_any_element();
        };
        let PaneLook {
            border,
            top,
            open_in,
            over,
            alone,
        } = look;
        let file = diff.files.get(self.file);
        let event = |event: fn() -> PaneEvent| {
            let host = host.clone();
            move |_: &ClickEvent, _: &mut gpui::Window, cx: &mut gpui::App| host.pane(event(), cx)
        };
        let body = match (file, &self.data) {
            _ if self.stack.is_some() => self.render_rows(host.clone(), cx).into_any_element(),
            (Some(f), _) if self.generated_folded(f) => {
                let path = f.path().to_owned();
                let unfold = host.clone();
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .text_color(theme::muted())
                    .child(format!(
                        "{} is generated: {} changed lines are folded.",
                        path.rsplit('/').next().unwrap_or(&path),
                        f.added + f.removed
                    ))
                    .child(
                        div()
                            .id("unfold-generated")
                            .px_3()
                            .h(px(26.))
                            .flex()
                            .items_center()
                            .rounded(px(ROW_RADIUS))
                            .bg(theme::hover())
                            .text_color(theme::text())
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::selected()))
                            .child("Show it anyway  ↵")
                            .on_click(move |_: &ClickEvent, _, cx| {
                                unfold.pane(PaneEvent::Unfold(path.clone()), cx)
                            }),
                    )
                    .into_any_element()
            }
            (None, _) => div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_color(theme::muted())
                .child("No changes")
                .into_any_element(),
            (Some(file), _) if let Some(viewer) = self.viewer_of(file) => (viewer.draw)(
                &Drawing {
                    file,
                    ix: self.file,
                    alone: true,
                    host: &host,
                    review: self,
                    ready: RefCell::new(alone),
                },
                cx,
            ),
            (Some(file), _) if file.hunks.is_empty() => div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_color(theme::muted())
                .child(
                    file.note
                        .clone()
                        .unwrap_or_else(|| "No content changes".to_owned()),
                )
                .into_any_element(),
            (Some(_), None) => div().flex_1().into_any_element(),
            (Some(_), Some(_)) => self.render_rows(host.clone(), cx).into_any_element(),
        };
        let mode_note = self.data.as_ref().and_then(|d| d.mode_note.clone());
        let pressed = host.clone();
        island()
            .relative()
            .flex_1()
            .min_w_0()
            // Never taller than the window: a page scrolls inside it (#11).
            .min_h_0()
            .bg(theme::editor())
            .border_2()
            .border_color(border)
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                pressed.pane(PaneEvent::Pressed, cx)
            })
            .children(top)
            .children(file.map(|f| {
                let path = match (&f.old_path, &f.new_path) {
                    (Some(old), Some(new)) if old != new => format!("{old} → {new}"),
                    _ => f.path().to_owned(),
                };
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_2()
                    .h(px(36.))
                    .px_4()
                    .border_b_1()
                    .border_color(theme::island_border())
                    .child(icons::file_icon(f.path(), 16.))
                    .child(
                        hover::path("file-path", path)
                            .flex_1()
                            .text_color(change_color(f, false)),
                    )
                    // The working tree's part it is (#22): staged, not staged, untracked; with
                    // every file in one scroll the card's own header has it.
                    .children(
                        self.stack
                            .is_none()
                            .then(|| file_tag(diff.tags.word(self.file)))
                            .flatten(),
                    )
                    // What the decorators say of it (its owners).
                    .children(self.badges(f, cx))
                    // An SVG read as its source can go back to the picture.
                    .children(
                        (self.page.is_source(f.path()) && crate::image_view::is_svg(f)).then(
                            || {
                                group()
                                    .child(
                                        chip("svg-image", "Image", false)
                                            .on_click(event(|| PaneEvent::SvgSource(false))),
                                    )
                                    .child(
                                        chip("svg-source", "Source", true)
                                            .on_click(event(|| PaneEvent::SvgSource(true))),
                                    )
                            },
                        ),
                    )
                    // With every file in one scroll the card's own header has the document's switch.
                    .children(
                        self.stack
                            .is_none()
                            .then(|| self.switch_of(self.file, false, cx.entity().downgrade()))
                            .flatten(),
                    )
                    .children(
                        self.review_chip(self.file, &host, cx)
                            .map(|chip| chip.on_click(event(|| PaneEvent::Review))),
                    )
                    .children(
                        (!f.hunks.is_empty())
                            .then_some(f.note.clone())
                            .flatten()
                            .map(|n| div().text_color(theme::warning()).child(n)),
                    )
                    .children((!self.changes.is_empty()).then(|| {
                        div()
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap_2()
                            // A diff under 600 px keeps the arrows (#80).
                            .when(!self.tight, |s| {
                                s.child(div().text_size(px(12.)).text_color(theme::muted()).child(
                                    match self.change_at {
                                        Some(at) => {
                                            format!("change {} of {}", at + 1, self.changes.len())
                                        }
                                        None => plural(self.changes.len(), "change"),
                                    },
                                ))
                            })
                            .child(
                                group()
                                    .child(
                                        chip("prev-change", "↑", false)
                                            .on_click(event(|| PaneEvent::Step { forward: false })),
                                    )
                                    .child(
                                        chip("next-change", "↓", false)
                                            .on_click(event(|| PaneEvent::Step { forward: true })),
                                    ),
                            )
                    }))
                    // The file in the app it belongs to, at the change (#78); ▾ the others.
                    .children(open_in)
                    .children(mode_note.clone().map(|n| {
                        let unavailable = n.starts_with("Structural diff");
                        div()
                            .flex_none()
                            .text_size(px(12.))
                            .text_color(if unavailable {
                                theme::warning()
                            } else {
                                theme::faint()
                            })
                            .child(n)
                    }))
            }))
            .children(over)
            .children(self.notice.as_ref().map(|notice| {
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_3()
                    .mx(px(8.))
                    .mt(px(6.))
                    .px_3()
                    .py(px(6.))
                    .rounded(px(ROW_RADIUS))
                    .bg(theme::warning_bg())
                    .text_size(px(12.))
                    .text_color(theme::warning())
                    .child(div().flex_1().child(notice.text.clone()))
                    .child(
                        div()
                            .id("notice-undo")
                            .px_2()
                            .rounded(px(ROW_RADIUS))
                            .font_weight(FontWeight::SEMIBOLD)
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::hover()))
                            .child("Undo")
                            .on_click(event(|| PaneEvent::UndoNotice)),
                    )
                    .child(
                        div()
                            .id("notice-dismiss")
                            .px_2()
                            .rounded(px(ROW_RADIUS))
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::hover()))
                            .child("✕")
                            .on_click(event(|| PaneEvent::DismissNotice)),
                    )
            }))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .when(self.stack.is_none(), |s| s.px(px(4.)).pt(px(4.)).pb(px(6.)))
                    .when(self.stack.is_some(), |s| {
                        s.bg(theme::base()).rounded_b(px(ISLAND_RADIUS))
                    })
                    .child(body),
            )
            .into_any_element()
    }

    /// How the diff is drawn, the toolbar's right: Split · Unified, Expand all or Collapse all with every file in
    /// one scroll, View ▾ (`menu_open` while its menu is); the same toggles live in the View menu.
    pub fn render_options(&self, menu_open: bool, host: Rc<dyn Host>) -> Div {
        let event = |event: PaneEvent| {
            let host = host.clone();
            move |_: &ClickEvent, _: &mut gpui::Window, cx: &mut gpui::App| {
                host.pane(event.clone(), cx)
            }
        };
        let o = self.options;
        let (bounds, menu) = (self.view_button.clone(), host.clone());
        div()
            .flex_none()
            .flex()
            .items_center()
            .gap_2()
            .child(
                group()
                    .child(
                        chip("split", "Split", !o.unified)
                            .on_click(event(PaneEvent::Unified(false))),
                    )
                    .child(
                        chip("unified", "Unified", o.unified)
                            .on_click(event(PaneEvent::Unified(true))),
                    ),
            )
            .children(self.stack.is_some().then(|| {
                let folded = self.any_folded();
                group().child(
                    chip(
                        "expand-all",
                        if folded { "Expand all" } else { "Collapse all" },
                        false,
                    )
                    .on_click(event(PaneEvent::ExpandAll { folded })),
                )
            }))
            .child(
                group().child(
                    chip("view", "View ▾", menu_open)
                        .relative()
                        .child({
                            let bounds = bounds.clone();
                            canvas(move |b, _, _| bounds.set(Some(b)), |_, _, _, _| {})
                                .absolute()
                                .size_full()
                        })
                        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                            // The menu hangs from the button.
                            if let Some(under) = bounds.get() {
                                menu.pane(PaneEvent::ViewMenu { under }, cx)
                            }
                        }),
                ),
            )
    }

    /// The chip on the file's line: mark file `ix` read, and the key; none where nothing is marked.
    fn review_chip(&self, ix: usize, host: &Rc<dyn Host>, cx: &gpui::App) -> Option<Stateful<Div>> {
        let state = self.review_state(ix)?;
        Some(
            div()
                .id("review-chip")
                .flex_none()
                .h(px(24.))
                .px(px(8.))
                .flex()
                .items_center()
                .gap(px(6.))
                .rounded(px(ROW_RADIUS))
                .text_size(px(12.))
                .text_color(if state == State::Done {
                    theme::added()
                } else {
                    theme::muted()
                })
                .cursor_pointer()
                .hover(|s| s.bg(theme::hover()).text_color(theme::text()))
                .child(match state {
                    State::Done => "✓ Reviewed".to_owned(),
                    State::Stale => self.stale_chip(ix, host, cx),
                    State::Open => "Mark reviewed".to_owned(),
                })
                .child(keys::caps(
                    if state == State::Done { "⇧V" } else { "V" },
                    keys::On::Plain,
                )),
        )
    }
}
