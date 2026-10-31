//! A commit on a long hover (#53): its card — its whole message's start, who and when, what it changed, its
//! labels, its id to copy, its page on the host —, and the labels on commits as a row shows them.

use artoo_core::git::{CommitInfo, Deco, DecoKind, Repo};
use artoo_ui_kit::controls::tag;
use artoo_ui_kit::format::plural;
use artoo_ui_kit::hover::{CutInTip, card_action, card_frame, cut, full_date};
use artoo_ui_kit::{discussion, motion, px, theme};
use gpui::{
    AnyView, App, ClickEvent, Context, FontWeight, IntoElement, Render, Rgba, Window, div,
    prelude::*,
};

/// A label's words and colour: "HEAD → main", a branch, a remote's, a tag.
fn label(d: &Deco) -> (String, Rgba) {
    match d.kind {
        DecoKind::Head => (format!("HEAD → {}", d.name), theme::accent()),
        DecoKind::Local => (d.name.clone(), theme::accent()),
        DecoKind::Remote => (d.name.clone(), theme::faint()),
        DecoKind::Tag => (d.name.clone(), theme::warning()),
    }
}

/// "HEAD → main", "origin/main", "v1.2": the first label and a count of the rest.
pub fn deco_tags(decos: &[Deco]) -> Vec<gpui::AnyElement> {
    let pill = |text: String, color: Rgba| {
        // The pill gives way (its text is cut with …) before the time beside it does.
        div()
            .flex_shrink(1.)
            .min_w_0()
            .px(px(6.))
            .rounded_full()
            .border_1()
            .border_color(color)
            .text_size(px(10.))
            .text_color(color)
            .max_w(px(120.))
            .cut_in_tip()
            .child(text)
            .into_any_element()
    };
    let mut out: Vec<_> = decos
        .iter()
        .take(1)
        .map(|d| {
            let (text, color) = label(d);
            pill(text, color)
        })
        .collect();
    // The count never gives way: it says there is more.
    if decos.len() > 1 {
        out.push(
            div()
                .flex_none()
                .px(px(6.))
                .rounded_full()
                .border_1()
                .border_color(theme::faint())
                .text_size(px(10.))
                .text_color(theme::faint())
                .child(format!("+{}", decos.len() - 1))
                .into_any_element(),
        );
    }
    out
}

/// A commit: its whole message's start, who and when, the full id, every branch and tag on it.
pub struct CommitCard {
    pub info: CommitInfo,
    pub refs: Vec<Deco>,
    /// The commit on GitLab or GitHub.
    pub url: Option<String>,
    /// The repository, to read what it changed when the card shows.
    pub root: Option<std::path::PathBuf>,
    /// (files, lines added, lines removed), once read (X58).
    pub stat: Option<(usize, usize, usize)>,
}

impl CommitCard {
    pub fn clone_of(other: &CommitCard) -> CommitCard {
        CommitCard {
            info: other.info.clone(),
            refs: other.refs.clone(),
            url: other.url.clone(),
            root: other.root.clone(),
            stat: other.stat,
        }
    }

    /// The card, shown: what the commit changed is read in the background and drawn when known.
    pub fn show(other: &CommitCard, cx: &mut App) -> AnyView {
        let card = cx.new(|_| CommitCard::clone_of(other));
        if let Some(root) = other.root.clone() {
            let (id, view) = (other.info.id, card.downgrade());
            cx.spawn(async move |cx| {
                let stat = cx
                    .background_executor()
                    .spawn(async move { Repo::open(&root)?.commit_stat(id) })
                    .await;
                view.update(cx, |card, cx| {
                    card.stat = stat.ok();
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }
        card.into()
    }
}

impl Render for CommitCard {
    // It fades in after the long rest (#56, 7).
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        motion::appear(self.body(), "card-in", motion::FAST, |c, v| c.opacity(v))
    }
}

impl CommitCard {
    fn body(&mut self) -> gpui::Div {
        let c = &self.info;
        let body: Vec<String> = c
            .message
            .lines()
            .skip(1)
            .skip_while(|l| l.trim().is_empty())
            .take(4)
            .map(str::to_owned)
            .collect();
        let sha = c.id.to_string();
        let url = self.url.clone();
        card_frame()
            .child(
                div()
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(c.summary.clone()),
            )
            .children((!body.is_empty()).then(|| {
                div()
                    .text_color(theme::muted())
                    .children(body.into_iter().map(|l| div().child(l)))
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(discussion::avatar(&c.author, false, 20.))
                    .child(
                        div().flex().flex_col().child(c.author.clone()).child(
                            div()
                                .text_size(px(11.))
                                .text_color(theme::faint())
                                .child(format!("{} · {}", c.email, full_date(c.time))),
                        ),
                    ),
            )
            .children(self.stat.map(|(files, added, removed)| {
                div()
                    .flex()
                    .gap(px(6.))
                    .text_size(px(11.))
                    .child(
                        div()
                            .text_color(theme::muted())
                            .child(plural(files, "file")),
                    )
                    .child(div().text_color(theme::added()).child(format!("+{added}")))
                    .child(
                        div()
                            .text_color(theme::removed())
                            .child(format!("−{removed}")),
                    )
            }))
            .children((!self.refs.is_empty()).then(|| {
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(4.))
                    .children(self.refs.iter().map(|d| {
                        let (text, color) = label(d);
                        tag(text, color)
                    }))
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(
                        cut("card-sha", sha.clone())
                            .flex_1()
                            .font_family(theme::code_font())
                            .text_size(px(11.))
                            .text_color(theme::muted()),
                    )
                    .child(card_action("card-copy-sha", "Copy").on_click(
                        move |_: &ClickEvent, _, cx| {
                            cx.write_to_clipboard(gpui::ClipboardItem::new_string(sha.clone()))
                        },
                    ))
                    .children(url.map(|url| {
                        card_action("card-open-commit", "Open ↗")
                            .on_click(move |_: &ClickEvent, _, cx| cx.open_url(&url))
                    })),
            )
    }
}
