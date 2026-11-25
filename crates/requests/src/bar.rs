//! What the request open shows in the tab's bar and on its page's line (#165 R5d): its pipeline's pill, Approve or
//! who approved, Merge as it stands. A click asks the tab through `Acts`.

use crate::merge::Stand;
use crate::requests::{Act, Acts, Requests};
use crate::waiting::FADED;
use artoo_hosts_api::Pipeline;
use artoo_ui_kit::controls::{bar_button, glyph_button};
use artoo_ui_kit::format::plural;
use artoo_ui_kit::hover::{self, TipOwned};
use artoo_ui_kit::{ROW_RADIUS, icons, px, theme, words};
use gpui::{AnyElement, ClickEvent, Point, SharedString, div, prelude::*};

/// Merge's word while the host merges; its box is the same as the button's and `Merged`'s (round 41).
pub(crate) const MERGING: &str = "Merging…";
/// What the bar says while GitLab rebases the request (#244), asked here or elsewhere.
pub(crate) const REBASING: &str = "Rebasing…";
const MERGE_BOX: f32 = 118.;

impl Requests {
    /// The pipeline on the request's bar (#188): its glyph and the job that failed, `✗ lint`, or its state; when, and
    /// every job that failed, in its long hover; a click opens it.
    pub fn render_pipeline_chip(&self) -> Option<impl IntoElement + use<>> {
        let p = self.pipeline.clone()?;
        let (_, word, _) = pipeline_word(&p);
        let (glyph, short, color) = pipeline_short(&p);
        let url = pipeline_page(&p);
        let host = words::say("GitLab").into_owned();
        let when = if p.updated > 0 {
            format!(" · {}", hover::full_date(p.updated))
        } else {
            String::new()
        };
        let tip: SharedString = format!("Pipeline {word}{when}\nClick opens it on {host}").into();
        Some(
            div()
                .id("bar-pipeline")
                .flex_none()
                .flex()
                .items_center()
                .gap(px(4.))
                .px(px(8.))
                .h(px(22.))
                .rounded_full()
                .border_1()
                .border_color(color)
                .text_size(px(12.))
                .text_color(color)
                .cursor_pointer()
                .hover(|s| s.bg(theme::hover()))
                .child(format!("{glyph} {short}"))
                .tooltip_show_delay(hover::LONG)
                .tooltip(move |_, cx| cx.new(|_| TipOwned(tip.clone())).into())
                .on_click(move |_: &ClickEvent, _, cx| cx.open_url(&url)),
        )
    }

    /// Approve, or who approved: `a` toggles one's approval; `compact`, its key in the tip alone (#186).
    pub fn render_approval_chip(&self, compact: bool, acts: Acts) -> Option<AnyElement> {
        let waits = self.approval_waits();
        self.approvals.clone().map(|a| {
            let count = if a.required > 0 {
                format!(" {}/{}", a.required - a.left.min(a.required), a.required)
            } else if !a.by.is_empty() && !a.mine {
                format!(" {}", a.by.len())
            } else {
                String::new()
            };
            let action = match (a.mine, a.can) {
                (true, _) => "a takes yours back",
                (false, true) => "a approves",
                (false, false) => "",
            };
            // A long hover: who approved, who asked for changes, who is still asked (X59).
            let d = self.details.clone().unwrap_or_default();
            let waiting: Vec<&str> = d
                .reviewers
                .iter()
                .filter(|r| !a.by.contains(r) && !d.changes_requested.contains(r))
                .map(String::as_str)
                .collect();
            let mut lines = vec![if a.by.is_empty() {
                "No approvals yet".to_owned()
            } else {
                format!("Approved by {}", a.by.join(", "))
            }];
            if !d.changes_requested.is_empty() {
                lines.push(format!(
                    "Changes requested by {}",
                    d.changes_requested.join(", ")
                ));
            }
            if !waiting.is_empty() {
                lines.push(format!("Still asked: {}", waiting.join(", ")));
            }
            if a.left > 0 {
                lines.push(format!(
                    "{} more needed",
                    plural(a.left as usize, "approval")
                ));
            }
            if !action.is_empty() {
                lines.push(action.to_owned());
            }
            let tip: SharedString = words::say(&lines.join("\n")).into_owned().into();
            // Approve while you may is the filled primary button with its key (round 41, P1); the other
            // stands stay the quiet chip.
            if a.can && !a.mine && !compact {
                return bar_button("approval", "Approve", "a", true)
                    .when(waits, |s| s.opacity(FADED))
                    .tooltip_show_delay(hover::LONG)
                    .tooltip(move |_, cx| cx.new(|_| TipOwned(tip.clone())).into())
                    .on_click(move |_: &ClickEvent, _, cx| acts(Act::ToggleApproval, cx))
                    .into_any_element();
            }
            // Yours is a tick alone (Design round 21, E2): how many approved and who is the tip's.
            if a.mine {
                return div()
                    .id("approval")
                    .flex_none()
                    .size(px(28.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(ROW_RADIUS))
                    .cursor_pointer()
                    .hover(|s| s.bg(theme::hover()))
                    // Counted at once, faded until the host answers (#139).
                    .when(waits, |s| s.opacity(FADED))
                    .child(
                        icons::icon("check")
                            .size(px(14.))
                            .text_color(theme::muted()),
                    )
                    .tooltip_show_delay(hover::LONG)
                    .tooltip(move |_, cx| {
                        let tip = format!("You approved{count}\n{tip}");
                        cx.new(|_| TipOwned(tip.into())).into()
                    })
                    .on_click(move |_: &ClickEvent, _, cx| acts(Act::ToggleApproval, cx))
                    .into_any_element();
            }
            let (label, color) = if a.can {
                ("Approve", theme::text())
            } else {
                ("✓", theme::muted())
            };
            div()
                .id("approval")
                .flex_none()
                .flex()
                .items_center()
                .gap(px(6.))
                .px(px(8.))
                .h(px(22.))
                .rounded_full()
                .border_1()
                .border_color(theme::island_border())
                .text_size(px(12.))
                .text_color(color)
                .when(a.can, |s| {
                    s.cursor_pointer().hover(|s| s.bg(theme::hover()))
                })
                // Counted at once, faded until the host answers (#139).
                .when(waits, |s| s.opacity(FADED))
                .child(format!("{label}{count}"))
                .tooltip_show_delay(hover::LONG)
                .tooltip(move |_, cx| cx.new(|_| TipOwned(tip.clone())).into())
                .on_click(move |_: &ClickEvent, _, cx| acts(Act::ToggleApproval, cx))
                .into_any_element()
        })
    }

    /// Merge in the request's bar, as it stands; none while the host has not said. `compact` in a
    /// narrow window; `github` names the host.
    pub fn render_merge_control(
        &self,
        compact: bool,
        github: bool,
        acts: Acts,
    ) -> Option<AnyElement> {
        // Merging and merged stand in the button's own box.
        let sending =
            (self.merging.as_ref()).is_some_and(|c| c.busy && Some(c.iid) == self.current);
        if sending || self.merge_busy == Some(MERGING) {
            return Some(
                bar_button("bar-merge", MERGING, "", true)
                    .min_w(px(MERGE_BOX))
                    .justify_center()
                    .cursor_default()
                    .into_any_element(),
            );
        }
        if self.merged_on_page() {
            let color = theme::accent();
            return Some(
                bar_button("bar-merge", "Merged", "", true)
                    .min_w(px(MERGE_BOX))
                    .justify_center()
                    .cursor_default()
                    .bg(gpui::Rgba { a: 0.15, ..color })
                    .text_color(color)
                    .into_any_element(),
            );
        }
        let stand = self.merge_stand()?;
        let m = self.find(self.current?)?;
        let target = m.target.clone();
        let row = || div().flex_none().flex().items_center().gap(px(6.));
        let word = |text: String, color| {
            div()
                .flex_none()
                .text_size(px(12.))
                .text_color(color)
                .child(text)
        };
        let rebasing = (self.merge.as_ref()).is_some_and(|m| m.rebasing);
        if let Some(busy) = self.merge_busy.or(rebasing.then_some(REBASING)) {
            return Some(
                word(busy.to_owned(), theme::muted())
                    .opacity(FADED)
                    .into_any_element(),
            );
        }
        let merge_icon = |color| icons::icon("merge").size(px(14.)).text_color(color);
        Some(match stand {
            Stand::InTheWay(blocks) => {
                let n = blocks.len();
                let tip: SharedString = blocks
                    .iter()
                    .map(|b| format!("{}: {}", b.what, b.detail))
                    .collect::<Vec<_>>()
                    .join("\n")
                    .into();
                div()
                    .id("bar-merge")
                    .flex_none()
                    .h(px(28.))
                    .px(px(10.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .rounded(px(ROW_RADIUS))
                    .border_1()
                    .border_color(theme::island_border())
                    .cursor_pointer()
                    .hover(|s| s.bg(theme::hover()))
                    .text_size(px(13.))
                    .text_color(theme::muted())
                    .child(merge_icon(theme::muted()))
                    .child("Merge")
                    // What is in the way, counted: a glyph and its number; the sentences are the tooltip's (Design M1).
                    .child(
                        row()
                            .gap(px(3.))
                            .text_color(theme::warning())
                            .child(
                                icons::icon("circle-slash")
                                    .size(px(14.))
                                    .text_color(theme::warning()),
                            )
                            .child(n.to_string()),
                    )
                    .child(
                        icons::icon("chevron")
                            .size(px(13.))
                            .text_color(theme::faint()),
                    )
                    .tooltip_show_delay(hover::LONG)
                    .tooltip(move |_, cx| cx.new(|_| TipOwned(tip.clone())).into())
                    .on_click({
                        let acts = acts.clone();
                        move |e: &ClickEvent, _, cx| {
                            let at = e.position();
                            acts(Act::MergeReasons(Point::new(at.x - px(120.), at.y + px(18.))), cx)
                        }
                    })
                    .into_any_element()
            }
            Stand::Checking => word(
                format!(
                    "Merge · {} is checking…",
                    if github { "GitHub" } else { "GitLab" }
                ),
                theme::faint(),
            )
            .into_any_element(),
            Stand::Rebase(behind) => row()
                .child(
                    div()
                        .id("merge-behind")
                        .child(word(format!("{behind} behind {target}"), theme::muted()))
                        .tooltip_show_delay(hover::LONG)
                        .tooltip(move |_, cx| {
                            let tip = format!(
                                "{target} moved on since this branch left it. GitLab puts the commits on top of it; the pipeline runs again."
                            );
                            cx.new(|_| TipOwned(tip.into())).into()
                        }),
                )
                .child(
                    bar_button("bar-rebase", "Rebase", "", true)
                        .on_click(move |_: &ClickEvent, _, cx| acts(Act::Rebase, cx)),
                )
                .into_any_element(),
            Stand::Auto => row()
                .child(icons::icon("clock").size(px(14.)).text_color(theme::added()))
                .child(word(
                    "Merges when the pipeline succeeds".into(),
                    theme::added(),
                ))
                .child(
                    bar_button("bar-merge-cancel", "Cancel", "", false)
                        .on_click(move |_: &ClickEvent, _, cx| acts(Act::CancelAutoMerge, cx)),
                )
                .into_any_element(),
            Stand::Running | Stand::Ready => {
                let label = match stand {
                    Stand::Running if !compact => "Merge when the pipeline succeeds…",
                    _ => "Merge…",
                };
                glyph_button("bar-merge", "merge", label, true)
                    .min_w(px(MERGE_BOX))
                    .justify_center()
                    .on_click(move |_: &ClickEvent, _, cx| acts(Act::OpenMerge, cx))
                    .into_any_element()
            }
        })
    }
}

/// Where a click on a pipeline goes: the first job that failed, else the pipeline.
pub fn pipeline_page(p: &Pipeline) -> String {
    p.failed
        .first()
        .map(|(_, u)| u.clone())
        .filter(|u| !u.is_empty())
        .unwrap_or_else(|| p.url.clone())
}

/// A pipeline as a bar or a row says it (#188, #189): the job that failed, `✗ lint`, else its state, `✓ passed`.
pub fn pipeline_short(p: &Pipeline) -> (&'static str, String, gpui::Rgba) {
    let (glyph, word, color) = pipeline_word(p);
    let short = match (p.status.as_str(), p.failed.as_slice()) {
        ("failed", [(job, _)]) => job.clone(),
        ("failed", [(job, _), rest @ ..]) => format!("{job} +{}", rest.len()),
        _ => word,
    };
    (glyph, short, color)
}

/// A pipeline as a drawn glyph (a Lucide icon of the kit) and its colour: a request's rows and preview say its
/// state with it, the sentence in a tooltip.
pub fn pipeline_icon(p: &Pipeline) -> (&'static str, gpui::Rgba) {
    match p.status.as_str() {
        "success" => ("circle-check", theme::added()),
        "failed" => ("circle-x", theme::removed()),
        "running" => ("loader", theme::warning()),
        "pending" | "created" | "waiting_for_resource" | "preparing" => ("circle", theme::muted()),
        "canceled" => ("circle-slash", theme::muted()),
        _ => ("dot", theme::muted()),
    }
}

/// A pipeline as a glyph, a word and its colour: "✗ lint failed" in red.
pub fn pipeline_word(p: &Pipeline) -> (&'static str, String, gpui::Rgba) {
    match p.status.as_str() {
        "success" => ("✓", "passed".to_owned(), theme::added()),
        "failed" => (
            "✗",
            match p.failed.as_slice() {
                [] => "failed".to_owned(),
                [(job, _)] => format!("{job} failed"),
                [(job, _), rest @ ..] => format!("{job} +{} failed", rest.len()),
            },
            theme::removed(),
        ),
        "running" => ("●", "running".to_owned(), theme::warning()),
        "pending" | "created" | "waiting_for_resource" | "preparing" => {
            ("○", "pending".to_owned(), theme::muted())
        }
        "canceled" => ("⊘", "canceled".to_owned(), theme::muted()),
        other => ("·", other.replace('_', " "), theme::muted()),
    }
}
