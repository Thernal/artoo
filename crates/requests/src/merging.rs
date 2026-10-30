//! Merge's card (#164 B): From → Into, what the host allows, the message, before a merge goes. What goes is
//! the head the page shows, so a push nobody has seen is refused (`merge`). The card keeps its message's caret
//! itself; the tab gives it the keyboard and answers what it asks (`Act`), and hears the host merged
//! (`Event::Merged`).

use crate::area::Area;
use crate::merge::{self, Merge, Method, Squash};
use crate::requests::{Act, Acts, Event, Requests};
use crate::waiting::FADED;
use artoo_hosts_api::Host;
use artoo_ui_kit::discussion::{Kind, button};
use artoo_ui_kit::format::plural;
use artoo_ui_kit::input::{self, Edit};
use artoo_ui_kit::{ISLAND_RADIUS, ROW_RADIUS, icons, keys, px, theme};
use gpui::{
    ClickEvent, Context, FontWeight, KeyDownEvent, MouseButton, SharedString, div, prelude::*,
};
use std::sync::Arc;

/// The card before a merge goes.
pub struct MergeCard {
    /// The request it merges: never another one that came on the page meanwhile.
    pub iid: u64,
    pub merge: Merge,
    pub error: Option<String>,
    /// Refused for a push after the version on the page: the card offers the new one.
    pub moved: bool,
    /// Sent, and the host has not answered yet.
    pub busy: bool,
    /// The message's caret, selection and undo, its layout as drawn and its scroll.
    area: Area,
}

/// What the card is drawn with that the tab knows.
pub struct CardLook {
    /// The commits of the request's branch.
    pub commits: usize,
    /// The host is GitHub.
    pub github: bool,
    /// The message has the keyboard.
    pub active: bool,
}

/// A request's number as its host writes it: `!12` on GitLab, `#12` on GitHub.
pub fn reference(iid: u64, github: bool) -> String {
    format!("{}{iid}", if github { '#' } else { '!' })
}

impl Requests {
    /// Opens Merge's card on the request open, the project's defaults filled in; `when` merges
    /// when the pipeline succeeds. `false` when the host has not said how it merges.
    pub fn open_merge_card(&mut self, when: bool, github: bool) -> bool {
        let (Some(iid), Some(host)) = (self.current, self.merge.as_ref()) else {
            return false;
        };
        let Some(m) = self.find(iid) else {
            return false;
        };
        let message = format!("{} ({})", m.title, reference(iid, github));
        let merge = Merge::defaults(host, &m.head, message.clone(), when);
        self.merging = Some(MergeCard {
            iid,
            merge,
            error: None,
            moved: false,
            busy: false,
            area: Area::new(message),
        });
        true
    }

    /// Esc, Cancel or ×; not while the merge is on its way.
    pub fn close_merge(&mut self, cx: &mut Context<Self>) {
        if self.merging.as_ref().is_some_and(|c| !c.busy) {
            self.merging = None;
            cx.notify();
        }
    }

    /// The card's keys: the message takes them, ↵ a new line. `true` for ⌘↵: the tab sends the
    /// merge.
    pub fn merge_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> bool {
        let key = &event.keystroke;
        let Some(card) = self.merging.as_mut().filter(|c| !c.busy) else {
            return false;
        };
        if key.key == "enter" && key.modifiers.secondary() {
            return true;
        }
        if key.key == "enter" {
            input::sync(&card.merge.message, &mut card.area.edit);
            card.area.edit.insert("\n");
            card.merge.message = card.area.edit.text().to_owned();
        } else {
            match input::edit_area(&mut card.merge.message, &mut card.area.edit, key, cx) {
                Edit::Escape => self.close_merge(cx),
                Edit::Changed => card.error = None,
                Edit::Selected | Edit::Enter { .. } | Edit::Ignored => {}
            }
        }
        cx.notify();
        false
    }

    /// Sends the merge; the card stays until the host answers, and says why it refused.
    /// `Event::Merged` tells the tab it went.
    pub fn run_merge(&mut self, host: Arc<dyn Host>, cx: &mut Context<Self>) {
        let Some(card) = self.merging.as_mut().filter(|c| !c.busy) else {
            return;
        };
        let iid = card.iid;
        card.busy = true;
        card.error = None;
        let merge = card.merge.clone();
        self.merge_task = Some(cx.spawn(async move |this, cx| {
            let sent = merge.clone();
            let result = cx
                .background_executor()
                .spawn(async move { host.merge(iid, &sent) })
                .await;
            this.update(cx, |rq, cx| {
                match result {
                    Ok(()) => {
                        rq.merging = None;
                        cx.emit(Event::Merged { iid, merge });
                    }
                    Err(e) => {
                        if let Some(card) = rq.merging.as_mut() {
                            let e = format!("{e:#}");
                            card.busy = false;
                            card.moved = merge::moved_on(&e);
                            card.error = Some(merge::refusal(&e));
                        }
                    }
                }
                cx.notify();
            })
            .ok();
        }));
        cx.notify();
    }

    /// The card before a merge goes, over everything; a click on Merge or Show the new version
    /// asks the tab.
    pub fn render_merge_card(
        &self,
        look: CardLook,
        acts: Acts,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let card = self.merging.as_ref()?;
        let iid = card.iid;
        let m = self.find(iid)?;
        let host = self.merge.clone().unwrap_or_default();
        let merge = &card.merge;
        let reference = reference(iid, look.github);
        let version = self
            .versions
            .as_ref()
            .filter(|(of, _)| *of == iid)
            .and_then(|(_, v)| v.iter().find(|v| v.tip.to_string() == m.head))
            .map(|v| format!("v{}", v.number));
        let commits = look.commits;
        let seen = match &version {
            Some(v) => format!(" · {v}, the latest you have"),
            None => " · the latest you have".to_owned(),
        };
        let head = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .px(px(16.))
            .py(px(12.))
            .border_b_1()
            .border_color(theme::island_border())
            .child(icons::icon("merge").size(px(16.)).text_color(theme::text()))
            .child(
                div()
                    .flex_1()
                    .text_size(px(15.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(if merge.when_pipeline_succeeds {
                        format!("Merge {reference} when the pipeline succeeds")
                    } else {
                        format!("Merge {reference}")
                    }),
            )
            .child(
                div()
                    .id("merge-close")
                    .size(px(24.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(ROW_RADIUS))
                    .cursor_pointer()
                    .hover(|s| s.bg(theme::hover()))
                    .child(icons::icon("x").size(px(14.)).text_color(theme::muted()))
                    .on_click(cx.listener(|rq, _: &ClickEvent, _, cx| rq.close_merge(cx))),
            );
        let route = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(6.))
            .child(mono(&m.source).text_color(theme::accent()))
            .child(
                icons::icon("forward")
                    .size(px(14.))
                    .text_color(theme::faint()),
            )
            .child(mono(&m.target))
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(theme::faint())
                    .child(format!("{}{seen}", plural(commits, "commit"))),
            );
        let label = |text: &'static str| {
            div()
                .w(px(112.))
                .flex_none()
                .pt(px(2.))
                .text_size(px(12.))
                .text_color(theme::muted())
                .child(text)
        };
        let field_row = || div().flex().items_start().gap(px(8.));
        let mut options = Vec::new();
        if host.github {
            let default = host.methods.first().copied();
            let chips = host.methods.iter().map(|&way| {
                let on = merge.method == way;
                div()
                    .id(SharedString::from(format!("merge-way-{}", way.key())))
                    .px(px(10.))
                    .h(px(24.))
                    .flex()
                    .items_center()
                    .rounded(px(ROW_RADIUS))
                    .cursor_pointer()
                    .text_size(px(12.))
                    .when(on, |s| {
                        s.bg(theme::selected())
                            .text_color(theme::text())
                            .font_weight(FontWeight::MEDIUM)
                    })
                    .when(!on, |s| {
                        s.text_color(theme::muted()).hover(|s| s.bg(theme::hover()))
                    })
                    .child(way.label())
                    .on_click(cx.listener(move |rq, _: &ClickEvent, _, cx| {
                        if let Some(card) = rq.merging.as_mut() {
                            card.merge.method = way;
                            cx.notify();
                        }
                    }))
            });
            options.push(
                field_row()
                    .child(label("How"))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(6.))
                            .child(
                                div()
                                    .flex()
                                    .gap(px(2.))
                                    .p(px(2.))
                                    .rounded(px(ROW_RADIUS + 2.))
                                    .border_1()
                                    .border_color(theme::island_border())
                                    .children(chips),
                            )
                            .children(default.map(|way| {
                                div()
                                    .text_size(px(11.))
                                    .text_color(theme::faint())
                                    .child(format!(
                                        "The ways this repository allows; {} is first",
                                        way.label()
                                    ))
                            })),
                    )
                    .into_any_element(),
            );
        }
        let mut checks = Vec::new();
        if !host.github {
            let n = plural(commits.max(1), "commit");
            let squash = match host.squash {
                Squash::Never => None,
                Squash::Always => Some((true, true, "the project always squashes".to_owned())),
                Squash::Choose(default) => Some((
                    merge.squash,
                    false,
                    if merge.squash == default {
                        "the project's default".to_owned()
                    } else {
                        String::new()
                    },
                )),
            };
            if let Some((on, fixed, sub)) = squash {
                checks.push(
                    check(
                        "merge-squash",
                        on,
                        fixed,
                        format!("Squash the {n} into one"),
                        sub,
                    )
                    .when(!fixed, |s| {
                        s.on_click(cx.listener(|rq, _: &ClickEvent, _, cx| {
                            if let Some(card) = rq.merging.as_mut() {
                                card.merge.squash = !card.merge.squash;
                                cx.notify();
                            }
                        }))
                    })
                    .into_any_element(),
                );
            }
        }
        // GitHub's auto-merge deletes nothing unless the repository does.
        if !(host.github && merge.when_pipeline_succeeds && !host.deletes_itself) {
            let fixed = host.deletes_itself;
            let on = merge.delete_branch || fixed;
            let sub = if fixed {
                "the repository deletes it".to_owned()
            } else if !host.github && merge.delete_branch == host.delete_branch {
                "the project's default".to_owned()
            } else {
                String::new()
            };
            checks.push(
                check(
                    "merge-delete",
                    on,
                    fixed,
                    format!("Delete {} after", m.source),
                    sub,
                )
                .when(!fixed, |s| {
                    s.on_click(cx.listener(|rq, _: &ClickEvent, _, cx| {
                        if let Some(card) = rq.merging.as_mut() {
                            card.merge.delete_branch = !card.merge.delete_branch;
                            cx.notify();
                        }
                    }))
                })
                .into_any_element(),
            );
        }
        if !checks.is_empty() {
            options.push(
                field_row()
                    .child(label(if host.github { "" } else { "Options" }))
                    .child(div().flex().flex_col().gap(px(8.)).children(checks))
                    .into_any_element(),
            );
        }
        let active = look.active;
        let message = if host.github && merge.method == Method::Rebase {
            field_row().child(label("Commit message")).child(
                div()
                    .flex_1()
                    .pt(px(2.))
                    .text_size(px(12.))
                    .text_color(theme::faint())
                    .child("A rebase keeps each commit and its message: none to write."),
            )
        } else {
            let field = card.area.field(
                &card.merge.message,
                active,
                "Empty: the project's own message",
                |rq| rq.merging.as_mut().map(|c| &mut c.area),
                cx,
            );
            field_row().child(label("Commit message")).child(
                div()
                    .flex_1()
                    .min_w_0()
                    .px_2()
                    .py(px(5.))
                    .min_h(px(84.))
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
        };
        let body = div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .px(px(16.))
            .py(px(14.))
            .child(route)
            .children(options)
            .child(message)
            .children(card.error.clone().map(|e| {
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .px_3()
                    .py_2()
                    .rounded(px(ROW_RADIUS))
                    .border_1()
                    .border_color(theme::removed())
                    .text_size(px(12.))
                    .text_color(theme::removed())
                    .child(div().flex_1().min_w_0().child(e))
                    .when(card.moved, |s| {
                        s.child(
                            button(
                                "merge-new-version",
                                "Show the new version",
                                None,
                                Kind::Plain,
                            )
                            .on_click({
                                let acts = acts.clone();
                                move |_: &ClickEvent, _, cx| acts(Act::ShowNewVersion, cx)
                            }),
                        )
                    })
            }));
        let after = version.unwrap_or_else(|| "this version".to_owned());
        let go = match (card.busy, merge.when_pipeline_succeeds) {
            (true, _) => "Merging…",
            (false, true) => "Merge when it succeeds",
            (false, false) => "Merge",
        };
        let foot = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .px(px(16.))
            .py(px(12.))
            .border_t_1()
            .border_color(theme::island_border())
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(12.))
                    .text_color(theme::faint())
                    .child(format!(
                        "Everyone sees it, and it cannot be undone. A push after {after} makes it refuse."
                    )),
            )
            .child(
                button("merge-cancel", "Cancel", Some("esc"), Kind::Quiet)
                    .on_click(cx.listener(|rq, _: &ClickEvent, _, cx| rq.close_merge(cx))),
            )
            .child(
                button(
                    "merge-go",
                    go,
                    (!card.busy).then_some("⌘↵"),
                    Kind::Primary(theme::accent()),
                )
                .when(card.busy, |s| s.opacity(FADED))
                .on_click(move |_: &ClickEvent, _, cx| acts(Act::RunMerge, cx)),
            );
        Some(
            div()
                .id("merge-backdrop")
                .absolute()
                .size_full()
                .flex()
                .justify_center()
                .items_start()
                .pt(px(72.))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|rq, _, _, cx| rq.close_merge(cx)),
                )
                .child(
                    div()
                        .id("merge-card")
                        .occlude()
                        .w(px(600.))
                        .max_h(gpui::relative(0.86))
                        .flex()
                        .flex_col()
                        .rounded(px(ISLAND_RADIUS))
                        .border_1()
                        .border_color(theme::island_border())
                        .bg(theme::panel())
                        .shadow_lg()
                        .text_size(px(13.))
                        .child(head)
                        .child(
                            div()
                                .id("merge-body")
                                .flex_1()
                                .min_h_0()
                                .overflow_y_scroll()
                                .child(body),
                        )
                        .child(foot),
                ),
        )
    }
}

/// A checkbox with its label and, faint, what it is (`the project's default`); `fixed` the host's
/// rule, taking no click.
fn check(
    id: &'static str,
    on: bool,
    fixed: bool,
    text: String,
    sub: String,
) -> gpui::Stateful<gpui::Div> {
    let ink = if fixed {
        theme::faint()
    } else {
        theme::accent()
    };
    div()
        .id(id)
        .flex()
        .items_center()
        .gap_2()
        .when(!fixed, |s| s.cursor_pointer())
        .child(
            div()
                .flex_none()
                .size(px(14.))
                .rounded(px(3.))
                .border_2()
                .border_color(if on { ink } else { theme::faint() })
                .when(on, |s| s.bg(ink))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(10.))
                .text_color(theme::base())
                .child(if on { "✓" } else { "" }),
        )
        .child(div().child(keys::text(&text).into_owned()))
        .when(!sub.is_empty(), |s| {
            s.child(
                div()
                    .text_size(px(12.))
                    .text_color(theme::faint())
                    .child(format!("· {sub}")),
            )
        })
}

fn mono(text: &str) -> gpui::Div {
    div()
        .font_family(theme::code_font())
        .text_size(px(12.))
        .child(text.to_owned())
}
