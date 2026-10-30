//! Submitting a review of a merge request, as GitLab offers it: Comment, Approve or Request changes, with an
//! optional summary, publishing one's pending comments with it. Its card opens in the tool window on the right,
//! which stays while the diff scrolls: what goes out (the pending comments, each a jump to its line), the verdict
//! with what it does, the summary. The card keeps its summary and the summary's completion; the tab gives it the
//! keyboard, sends what it submits (`Submission`) and puts it in the window.

use crate::area::Area;
use crate::discussions::{draft_line, send_now_chip};
use crate::requests::{Act, Acts, Requests};
use crate::waiting::{Op, is_temp};
use artoo_hosts_api::{Host, Verdict};
use artoo_ui_kit::complete::{list_key, render_list};
use artoo_ui_kit::format::plural;
use artoo_ui_kit::hover::{self, Tip};
use artoo_ui_kit::input::{self, Edit};
use artoo_ui_kit::keys::{self, On};
use artoo_ui_kit::{ROW_RADIUS, px, theme, words};
use gpui::{ClickEvent, Context, FontWeight, KeyDownEvent, div, prelude::*};
use std::collections::HashMap;
use std::sync::Arc;

const VERDICTS: [Verdict; 3] = [Verdict::Comment, Verdict::Approve, Verdict::RequestChanges];

fn label(v: Verdict) -> &'static str {
    match v {
        Verdict::Comment => "Comment",
        Verdict::Approve => "Approve",
        Verdict::RequestChanges => "Request changes",
    }
}

fn hint(v: Verdict) -> &'static str {
    match v {
        Verdict::Comment => "no verdict",
        Verdict::Approve => "approves",
        Verdict::RequestChanges => "blocks merging",
    }
}

/// The card: what the review says besides its comments.
pub struct SubmitCard {
    summary: String,
    verdict: Verdict,
    error: Option<String>,
    /// The summary's caret, its layout as drawn and its scroll.
    area: Area,
    /// The completion list's row picked, and where esc closed it.
    pick: usize,
    closed: Option<usize>,
}

impl SubmitCard {
    fn new(summary: String, verdict: Verdict, error: Option<String>) -> Self {
        SubmitCard {
            area: Area::new(summary.clone()),
            summary,
            verdict,
            error,
            pick: 0,
            closed: None,
        }
    }

    /// The summary's caret; at its end until the field is edited.
    fn caret(&self) -> usize {
        self.area.caret(&self.summary)
    }
}

/// What the card is drawn with that the tab knows.
pub struct SubmitLook {
    /// The summary has the keyboard.
    pub active: bool,
    /// Comments are drawn rich: a pending one's line is without its markdown.
    pub rich: bool,
    /// The host is GitHub: a pending comment is not sent alone (#138).
    pub github: bool,
    /// The agent that wrote a pending comment, by its id (#150).
    pub agents: HashMap<u64, String>,
}

/// What a submit sends: the request, the changes shown until the host answers, the call, and the
/// card, given back if the host refuses (`Requests::submit_refused`).
pub struct Submission {
    pub iid: u64,
    pub ops: Vec<Op>,
    pub work: Box<dyn FnOnce(Arc<dyn Host>) -> anyhow::Result<()> + Send>,
    pub card: SubmitCard,
}

impl Requests {
    /// ⇧⌘↵ or "Submit review": the card, on Comment; `false` with no request open.
    pub fn open_submit_card(&mut self) -> bool {
        if self.current.is_none() {
            return false;
        }
        self.submitting = Some(SubmitCard::new(String::new(), Verdict::Comment, None));
        true
    }

    pub fn close_submit(&mut self, cx: &mut Context<Self>) {
        if self.submitting.take().is_some() {
            cx.notify();
        }
    }

    /// The card's keys: ⇥ the verdict, ↵ a new line, `@` and `:` completing in the summary (#51,
    /// #9), the rest the summary's. `true` for ⌘↵: the tab submits.
    pub fn submit_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> bool {
        let key = &event.keystroke;
        let list = match &self.submitting {
            Some(s) => self.completion(&s.summary, s.caret(), s.closed),
            None => return false,
        };
        if key.key == "enter" && key.modifiers.secondary() {
            return true;
        }
        let Some(s) = self.submitting.as_mut() else {
            return false;
        };
        if let Some((at, choices)) = list
            && list_key(
                key,
                &mut s.summary,
                &mut s.area.edit,
                &mut s.pick,
                &mut s.closed,
                at,
                &choices,
            )
        {
            cx.notify();
            return false;
        }
        if key.key == "tab" {
            let at = VERDICTS.iter().position(|v| *v == s.verdict).unwrap_or(0);
            let step = if key.modifiers.shift { 2 } else { 1 };
            s.verdict = VERDICTS[(at + step) % VERDICTS.len()];
            s.error = None;
        } else if key.key == "enter" {
            input::sync(&s.summary, &mut s.area.edit);
            s.area.edit.insert("\n");
            s.summary = s.area.edit.text().to_owned();
        } else if !input::vertical(
            key,
            &s.summary,
            &mut s.area.edit,
            &s.area.lines,
            &mut s.area.goal,
        ) {
            // ↑ ↓ ⌘← ⌘→ moved by the lines as drawn; the rest edit the summary.
            match input::edit_area(&mut s.summary, &mut s.area.edit, key, cx) {
                Edit::Escape => self.submitting = None,
                Edit::Changed => s.error = None,
                Edit::Selected | Edit::Enter { .. } | Edit::Ignored => {}
            }
        }
        cx.notify();
        false
    }

    /// What ⌘↵ or the card's button sends: the pending comments, then the summary, then the
    /// verdict. The card closes at once and the review shows, faded, until the host answers: the
    /// pending comments as discussions, the summary, the approval (#139). `None`, the card saying
    /// why, when there is nothing to submit.
    pub fn submission(&mut self, cx: &mut Context<Self>) -> Option<Submission> {
        let iid = self.current?;
        let head = self.find(iid).map(|m| m.head.clone()).unwrap_or_default();
        // One still on its way is not on the host to publish yet.
        let sent: Vec<u64> = self
            .drafts
            .iter()
            .map(|d| d.id)
            .filter(|id| !is_temp(*id) && self.draft_waits(*id).is_none())
            .collect();
        let drafts = !sent.is_empty();
        let approve_already = self.approved_already();
        let s = self.submitting.as_mut()?;
        let summary = s.summary.trim().to_owned();
        let verdict = s.verdict;
        if !drafts && summary.is_empty() && verdict == Verdict::Comment {
            s.error =
                Some("Nothing to submit: write a summary, or approve or ask for changes.".into());
            cx.notify();
            return None;
        }
        let card = self.submitting.take()?;
        let w = &mut self.waiting;
        let mut ops = vec![Op::Publish(
            sent.into_iter()
                .map(|id| (id, w.temp_thread(), w.temp()))
                .collect(),
        )];
        if !summary.is_empty() {
            ops.push(Op::Discussion {
                thread: w.temp_thread(),
                note: w.temp(),
                body: summary.clone(),
                at: None,
            });
        }
        if verdict == Verdict::Approve && !approve_already {
            ops.push(Op::Approve { mine: true });
        }
        cx.notify();
        Some(Submission {
            iid,
            ops,
            work: Box::new(move |host: Arc<dyn Host>| {
                host.submit(iid, &head, drafts, &summary, verdict, approve_already)
            }),
            card,
        })
    }

    /// The host refused what `card` submitted on request `iid`: the card comes back saying why,
    /// unless another is open or the page shows another request (`false`: the tab says it).
    pub fn submit_refused(&mut self, iid: u64, card: SubmitCard, why: String) -> bool {
        if self.submitting.is_some() || self.current != Some(iid) {
            return false;
        }
        self.submitting = Some(SubmitCard::new(card.summary, card.verdict, Some(why)));
        true
    }

    /// The card's body and foot, for the tool window while it is open; what a click asks goes to
    /// the tab (`Act`).
    pub fn submit_card(
        &self,
        look: SubmitLook,
        acts: Acts,
        cx: &Context<Self>,
    ) -> Option<(gpui::Stateful<gpui::Div>, gpui::Div)> {
        let s = self.submitting.as_ref()?;
        let iid = self.current?;
        let title = self.find(iid).map(|m| m.title.clone()).unwrap_or_default();
        // What Approve does to the approvals: "1 / 2 → 2 / 2" where rules count them.
        let approve = match &self.approvals {
            Some(a) if a.mine => "approved already".to_owned(),
            Some(a) if a.required > 0 => {
                let have = a.required.saturating_sub(a.left);
                format!(
                    "{have} / {} → {} / {}",
                    a.required,
                    (have + 1).min(a.required),
                    a.required
                )
            }
            _ => hint(Verdict::Approve).to_owned(),
        };
        let option = |v: Verdict| {
            let on = s.verdict == v;
            let hint = if v == Verdict::Approve {
                approve.clone()
            } else {
                hint(v).to_owned()
            };
            div()
                .id(("verdict", v as usize))
                .flex()
                .items_center()
                .gap(px(8.))
                .px(px(8.))
                .py(px(6.))
                .rounded(px(ROW_RADIUS))
                .cursor_pointer()
                .when(on, |s| s.bg(theme::selected()))
                .hover(|s| s.bg(theme::hover()))
                .child(
                    div()
                        .size(px(14.))
                        .flex_none()
                        .rounded_full()
                        .border_2()
                        .border_color(if on { theme::accent() } else { theme::faint() })
                        .flex()
                        .items_center()
                        .justify_center()
                        .children(
                            on.then(|| div().size(px(6.)).rounded_full().bg(theme::accent())),
                        ),
                )
                .child(div().flex_1().child(label(v)))
                .child(
                    div()
                        .flex_none()
                        .text_size(px(11.))
                        .text_color(theme::faint())
                        .child(hint),
                )
                .on_click(cx.listener(move |rq, _: &ClickEvent, _, cx| {
                    if let Some(s) = rq.submitting.as_mut() {
                        s.verdict = v;
                        s.error = None;
                        cx.notify();
                    }
                }))
        };
        let pending = self.drafts.iter().enumerate().map(|(ix, d)| {
            let (place, target, old) = self.draft_where(d);
            div()
                .id(("submit-draft", ix))
                .flex()
                .items_baseline()
                .gap_2()
                .px(px(6.))
                .py(px(2.))
                .rounded(px(ROW_RADIUS))
                .cursor_pointer()
                .hover(|s| s.bg(theme::hover()))
                .child(
                    div()
                        .flex_none()
                        .font_family(theme::code_font())
                        .text_size(px(11.))
                        .text_color(theme::faint())
                        .child(place),
                )
                .child(
                    hover::cut(
                        "draft-line",
                        draft_line(d, look.rich, look.agents.get(&d.id).map(String::as_str)),
                    )
                    .flex_1()
                    .text_size(px(12.))
                    .text_color(theme::muted()),
                )
                .relative()
                .group("submit-pending-row")
                .children(
                    send_now_chip(d.id, "submit-pending-row", look.github, acts.clone())
                        .map(|chip| chip.absolute().top(px(1.)).right(px(4.))),
                )
                // The window stays: the comment is checked in the diff beside it.
                .on_click({
                    let acts = acts.clone();
                    move |_: &ClickEvent, _, cx| {
                        if let Some((path, line)) = target.clone() {
                            acts(Act::JumpToLine { path, line, old }, cx);
                        }
                    }
                })
        });
        let drafts = self.drafts.len();
        let go = match s.verdict {
            Verdict::Comment => "Submit review",
            Verdict::Approve => "Submit and approve",
            Verdict::RequestChanges => "Submit, request changes",
        };
        let active = look.active;
        let field = s.area.field(
            &s.summary,
            active,
            "A summary, if you want one…",
            true,
            |rq| rq.submitting.as_mut().map(|s| &mut s.area),
            cx,
        );
        let body = div()
            .id("submit-body")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px(px(14.))
            .py(px(12.))
            .flex()
            .flex_col()
            .gap_3()
            .text_size(px(13.))
            .child(
                hover::cut("submit-title", format!("{} {title}", words::number(iid)))
                    .text_size(px(12.))
                    .text_color(theme::faint()),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .px(px(4.))
                    .py(px(8.))
                    .rounded(px(ROW_RADIUS))
                    .bg(theme::base())
                    .child(
                        div()
                            .px(px(6.))
                            .text_size(px(12.))
                            .text_color(theme::muted())
                            .child(if drafts == 0 {
                                "No pending comments".to_owned()
                            } else {
                                plural(drafts, "pending comment")
                            }),
                    )
                    .children(pending),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .children(VERDICTS.map(option)),
            )
            .child(
                div()
                    .px_2()
                    .py(px(5.))
                    .min_h(px(64.))
                    .flex()
                    .items_start()
                    .rounded(px(ROW_RADIUS))
                    .border_2()
                    .border_color(if active {
                        theme::focus()
                    } else {
                        theme::island_border()
                    })
                    .bg(theme::base())
                    .cursor_text()
                    .child(field),
            )
            .children(
                self.completion(&s.summary, s.caret(), s.closed)
                    .map(|(_, choices)| render_list(&choices, s.pick)),
            )
            .children(s.error.clone().map(|e| {
                div()
                    .px_3()
                    .py_2()
                    .rounded(px(ROW_RADIUS))
                    .border_1()
                    .border_color(theme::removed())
                    .text_size(px(12.))
                    .text_color(theme::removed())
                    .child(e)
            }));
        let foot = div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .gap_1()
                    .text_size(px(11.))
                    .text_color(theme::faint())
                    .child(keys::caps("⇥", On::Plain))
                    .child("verdict"),
            )
            .child(
                div()
                    .id("submit-cancel")
                    .px_2()
                    .h(px(28.))
                    .flex()
                    .items_center()
                    .rounded(px(ROW_RADIUS))
                    .cursor_pointer()
                    .hover(|s| s.bg(theme::hover()))
                    .child("Cancel")
                    .tooltip_show_delay(hover::LONG)
                    .tooltip(|_, cx| cx.new(|_| Tip("esc")).into())
                    .on_click(cx.listener(|rq, _: &ClickEvent, _, cx| rq.close_submit(cx))),
            )
            .child(
                div()
                    .id("submit-go")
                    .h(px(28.))
                    .px(px(10.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .rounded(px(ROW_RADIUS))
                    .bg(match s.verdict {
                        Verdict::Approve => theme::added(),
                        Verdict::RequestChanges => theme::removed(),
                        Verdict::Comment => theme::warning(),
                    })
                    .text_color(theme::base())
                    .font_weight(FontWeight::SEMIBOLD)
                    .cursor_pointer()
                    .hover(|s| s.opacity(0.85))
                    .child(go)
                    .child(keys::caps("⌘↵", On::Filled))
                    .on_click(move |_: &ClickEvent, _, cx| acts(Act::RunSubmit, cx)),
            );
        Some((body, foot))
    }
}
