//! A reviewer's marks: which files of a diff were read. A ✓ per file, a count in the files label,
//! `v` to mark the open file and go on to the next one not yet read. Kept per commit, version
//! range or merge request on this Mac; a file that changes in a newer version loses its mark.
//! Designed in `../Design/mockups/ide-ideas/a-ideas.html` (1).

use super::px;
use super::{Header, ROW_RADIUS, Workspace, theme};
use artoo_review::reviewed::State;
use gpui::{App, ClickEvent, Context, FontWeight, IntoElement, MouseButton, div, prelude::*};

impl Workspace {
    /// Called when a diff arrives (the review indexed it, `Review::index`): a request's and a branch's pages
    /// learn how many files it has.
    pub(super) fn count_files(&mut self, cx: &mut App) {
        if let Some(d) = &self.review.read(cx).diff
            && let Header::Compare {
                request: Some(iid), ..
            } = d.header
        {
            let files = Some((iid, d.files.len()));
            self.requests.update(cx, |rq, _| rq.files = files);
        }
        if self.branch_compare(cx)
            && let (Some(f), Some(d)) = (
                self.history.page.as_mut(),
                self.review.read(cx).diff.as_ref(),
            )
        {
            f.files = Some(d.files.len());
        }
    }

    pub(super) fn set_reviewed(&mut self, ix: usize, done: bool, cx: &mut Context<Self>) {
        if self.review.update(cx, |rv, _| rv.set_reviewed(ix, done)) {
            cx.notify();
        }
    }

    /// Marks or unmarks files `ixs` at once (all of them, a folder).
    pub(super) fn set_reviewed_many(&mut self, ixs: &[usize], done: bool, cx: &mut Context<Self>) {
        if self
            .review
            .update(cx, |rv, _| rv.set_reviewed_many(ixs, done))
        {
            cx.notify();
        }
    }

    /// The version of request `iid` your marks were last made at, by the versions loaded (#73).
    pub(super) fn marked_version(&self, iid: u64, cx: &App) -> Option<usize> {
        let root = self.root.as_ref()?.display().to_string();
        let heads = self
            .review
            .read(cx)
            .reviewed
            .marked_heads(&format!("{root}|mr:{iid}"));
        self.history
            .versions
            .iter()
            .filter(|v| heads.contains(&Some(v.tip)))
            .map(|v| v.number)
            .max()
    }

    /// ⌥⇧V and the buttons: every file the list shows marked, or (when all are) unmarked.
    pub(super) fn toggle_all_reviewed(&mut self, cx: &mut Context<Self>) {
        let shown = self.review.read(cx).file_order();
        let done = shown
            .iter()
            .all(|&ix| self.review.read(cx).review_state(ix) == Some(State::Done));
        self.set_reviewed_many(&shown, !done, cx);
    }

    /// The box of a folder: empty, part-marked or all marked; a click marks or unmarks the folder.
    pub(super) fn folder_box(
        &self,
        path: &str,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        self.review.read(cx).review_scope.as_ref()?;
        let ixs = self.review.read(cx).files_under(path);
        let done = ixs
            .iter()
            .filter(|&&ix| self.review.read(cx).review_state(ix) == Some(State::Done))
            .count();
        let all = done == ixs.len() && done > 0;
        let (border, fill, glyph) = if all {
            (theme::added(), Some(theme::added()), "✓")
        } else if done > 0 {
            (theme::added(), None, "–")
        } else {
            (theme::faint(), None, "")
        };
        let target = path.to_owned();
        Some(
            div()
                .id(gpui::ElementId::Name(format!("folder-box-{path}").into()))
                .flex_none()
                .size(px(16.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(3.))
                .border_2()
                .border_color(border)
                .when_some(fill, |s, c| s.bg(c))
                .text_size(px(10.))
                .font_weight(FontWeight::BOLD)
                .text_color(if all { theme::base() } else { theme::added() })
                .cursor_pointer()
                .hover(|s| s.border_color(theme::text()))
                .tooltip_show_delay(crate::ui::hover::LONG)
                .tooltip(move |_, cx| {
                    cx.new(|_| {
                        super::Tip(if all {
                            "Folder reviewed — click to unmark it"
                        } else {
                            "Mark the whole folder reviewed"
                        })
                    })
                    .into()
                })
                .child(glyph)
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    let ixs = this.review.read(cx).files_under(&target);
                    this.set_reviewed_many(&ixs, !all, cx);
                })),
        )
    }

    /// `⇧V`: marks or unmarks the open file.
    pub(super) fn toggle_reviewed(&mut self, cx: &mut Context<Self>) {
        let ix = self.review.read(cx).file;
        let done = self.review.read(cx).review_state(ix) == Some(State::Done);
        self.set_reviewed(ix, !done, cx);
    }

    /// `v`: marks the open file read and opens the next one that is not.
    pub(super) fn mark_and_advance(&mut self, cx: &mut Context<Self>) {
        if self.review.read(cx).review_scope.is_none() {
            return;
        }
        self.set_reviewed(self.review.read(cx).file, true, cx);
        self.next_unreviewed(cx);
    }

    /// Opens the next file, in the tree's order, that is not marked read (`Review::next_unreviewed`).
    pub(super) fn next_unreviewed(&mut self, cx: &mut Context<Self>) {
        if let Some(ix) = self.review.update(cx, |rv, _| rv.next_unreviewed()) {
            self.select_file(ix, cx);
            self.review.read(cx).scroll_file_into_view(ix);
        }
    }

    /// What a file's box says on a long hover: when it was marked, at which version (X59).
    fn review_said(&self, ix: usize, state: State, cx: &App) -> String {
        let when = self
            .review
            .read(cx)
            .review_scope
            .as_deref()
            .zip(
                self.review
                    .read(cx)
                    .diff
                    .as_ref()
                    .and_then(|d| d.files.get(ix)),
            )
            .and_then(|(scope, f)| self.review.read(cx).reviewed.when(scope, f.path()));
        let at = when.map(|(time, head)| {
            let version = head
                .and_then(|h| self.history.versions.iter().find(|v| v.tip == h))
                .map(|v| format!(" at v{},", v.number))
                .unwrap_or_default();
            format!("{version} {}", super::format::ago(time))
        });
        match (state, at) {
            (State::Open, _) => "Mark as reviewed  ⇧V".into(),
            (State::Done, Some(at)) => format!("Reviewed{at} — click to unmark  ⇧V"),
            (State::Done, None) => "Reviewed — click to unmark  ⇧V".into(),
            (State::Stale, Some(at)) => {
                format!("Changed since you reviewed it{at} — mark again  ⇧V")
            }
            (State::Stale, None) => "Changed since you marked it — mark again  ⇧V".into(),
        }
    }

    /// The box at the left of a file row; a click marks or unmarks without opening the file.
    pub(super) fn review_box(
        &self,
        ix: usize,
        cx: &Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let state = self.review.read(cx).review_state(ix)?;
        let said: gpui::SharedString = self.review_said(ix, state, cx).into();
        let (border, fill, glyph) = match state {
            State::Open => (theme::faint(), None, ""),
            State::Done => (theme::added(), Some(theme::added()), "✓"),
            State::Stale => (theme::warning(), None, "↻"),
        };
        Some(
            div()
                .id(("review-box", ix))
                .flex_none()
                .size(px(16.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(3.))
                .border_2()
                .border_color(border)
                .when_some(fill, |s, c| s.bg(c))
                .text_size(px(10.))
                .font_weight(FontWeight::BOLD)
                .text_color(if state == State::Done {
                    theme::base()
                } else {
                    theme::warning()
                })
                .cursor_pointer()
                .hover(|s| s.border_color(theme::text()))
                .tooltip_show_delay(crate::ui::hover::LONG)
                .tooltip(move |_, cx| cx.new(|_| super::TipOwned(said.clone())).into())
                // A new mark pops in (#56, 12).
                .child(super::motion::appear(
                    div().child(glyph),
                    ("review-tick", state as usize),
                    super::motion::FAST,
                    |tick, v| tick.opacity(v),
                ))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    let done = this.review.read(cx).review_state(ix) == Some(State::Done);
                    this.set_reviewed(ix, !done, cx);
                })),
        )
    }

    /// "7/17" with a bar under it, and Mark all / Unmark all.
    pub(super) fn render_review_progress(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let (done, total) = self.review.read(cx).review_progress()?;
        let filtered = !self.review.read(cx).pfilter.trim().is_empty();
        let shown = self.review.read(cx).file_order();
        let shown_done = shown
            .iter()
            .all(|&ix| self.review.read(cx).review_state(ix) == Some(State::Done));
        let action = |id: &'static str, label: String| {
            div()
                .id(id)
                .flex_none()
                .px(px(6.))
                .rounded(px(4.))
                .text_size(px(11.))
                .text_color(theme::muted())
                .cursor_pointer()
                .hover(|s| s.bg(theme::hover()).text_color(theme::text()))
                .child(label)
        };
        let toggle = action(
            "review-all",
            match (shown_done, filtered) {
                (true, false) => "Unmark all".to_owned(),
                (true, true) => format!("Unmark {} shown", shown.len()),
                (false, false) => "Mark all".to_owned(),
                (false, true) => format!("Mark {} shown", shown.len()),
            },
        )
        .tooltip_show_delay(crate::ui::hover::LONG)
        .tooltip(|_, cx| {
            cx.new(|_| super::Tip("Mark or unmark every file in the list  ⌥⇧V"))
                .into()
        })
        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_all_reviewed(cx)));
        Some(
            div()
                .flex_none()
                .px(px(14.))
                .pb(px(6.))
                .flex()
                .flex_col()
                .gap(px(4.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            // Only the numbers: the total over the files says what they count (#157).
                            super::hover::cut(
                                "progress-count",
                                if done == total {
                                    format!("{done}/{total} ✓")
                                } else {
                                    format!("{done}/{total}")
                                },
                            )
                            .flex_1()
                            .text_size(px(11.))
                            .text_color(if done == total {
                                theme::added()
                            } else {
                                theme::muted()
                            }),
                        )
                        .child(toggle),
                )
                .child(
                    div()
                        .h(px(4.))
                        .w_full()
                        .rounded(px(2.))
                        .bg(theme::hover())
                        // It grows to the new share (#56, 12).
                        .child(super::motion::toward_value(
                            div().h_full().rounded(px(2.)).bg(theme::added()),
                            "review-bar",
                            done as f32 / total.max(1) as f32,
                            super::motion::BASE,
                            |bar, share| bar.w(gpui::relative(share.clamp(0., 1.))),
                        )),
                ),
        )
    }

    /// The chip in the file header: mark it read, and the key.
    pub(super) fn render_review_chip(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let state = self
            .review
            .read(cx)
            .review_state(self.review.read(cx).file)?;
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
                    State::Done => "✓ Reviewed",
                    State::Stale => "↻ Changed — mark again",
                    State::Open => "Mark reviewed",
                })
                .child(super::keys::caps(
                    if state == State::Done { "⇧V" } else { "V" },
                    super::keys::On::Plain,
                ))
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    if this.review.read(cx).review_state(this.review.read(cx).file)
                        == Some(State::Done)
                    {
                        this.toggle_reviewed(cx)
                    } else {
                        this.mark_and_advance(cx)
                    }
                })),
        )
    }
}
