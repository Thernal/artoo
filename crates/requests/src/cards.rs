//! Overview's discussions (#241, Design rounds 45 and 46): a section after the description — a card per open
//! discussion and per pending comment, the line's composer at its end; with none, the section stays. A card: who
//! and when on top, its chevron folding it to two lines; the first note and the last with `N more` between; the
//! place last, with `Changed` or `Line gone` when the line moved on, and whose turn.

use crate::discussions::waiting_on;
use crate::pagefresh::mark;
use crate::requests::{Act, Acts, Requests};
use crate::waiting::FADED;
use artoo_hosts_api::{Draft, Note, Thread};
use artoo_ui_kit::format::{self, plural};
use artoo_ui_kit::hover::{self, Hover};
use artoo_ui_kit::markdown;
use artoo_ui_kit::{ROW_RADIUS, discussion, icons, px, theme};
use gpui::{AnyElement, App, ClickEvent, Div, FontWeight, div, prelude::*};
use std::rc::Rc;

/// What a click on a card does.
type Opener = Rc<dyn Fn(&mut App)>;

/// A card's words about where it is.
struct Place {
    /// `LoginViewModel.kt` and `:46`; none on the request as a whole.
    file: Option<(String, Option<u32>)>,
    /// `Changed`, `Line gone`: the line since it was written.
    since: Option<&'static str>,
    /// Whose move it is: an icon and its words.
    turn: Option<(&'static str, String)>,
}

/// A card to draw.
struct Card {
    id: String,
    /// The first note, and the last when there are more.
    first: (String, bool, Option<i64>, String),
    last: Option<(String, bool, Option<i64>, String)>,
    /// How many notes stand between the two.
    between: usize,
    place: Place,
    pending: bool,
    waits: bool,
    folded: bool,
    open: Opener,
}

impl Requests {
    /// The section: its head with the count of cards, the cards, the resolved ones as a line that opens the
    /// window, `composer` last.
    pub(crate) fn overview_discussions(
        &self,
        window: Opener,
        show: Acts,
        composer: Option<AnyElement>,
        base: Option<&str>,
        cx: &App,
    ) -> AnyElement {
        let resolved = self.threads.iter().filter(|t| t.resolved).count();
        let pending: Vec<&Draft> = (self.drafts.iter())
            .filter(|d| d.reply_to.is_none())
            .collect();
        let me = self.login.as_deref();
        let (open_shown, new) = crate::page::card_order(&self.threads, &self.page.new_notes);
        let mut cards = Vec::new();
        let mut new_cards = Vec::new();
        for (ix, t) in open_shown.iter().chain(&new).enumerate() {
            let Some(first) = t.notes.first() else {
                continue;
            };
            let at = self.threads.iter().position(|x| x.id == t.id);
            // A discussion on a line opens the diff at it; a general one opens the window (#242).
            let open: Opener = match at.filter(|_| t.path.is_some()) {
                Some(at) => {
                    let show = show.clone();
                    Rc::new(move |cx: &mut App| show(Act::ShowThread(at), cx))
                }
                None => window.clone(),
            };
            let said = |n: &Note| {
                let mine = me == Some(n.login.as_str());
                let (_, going) = self.note_time(n.id, n.created);
                (
                    if mine {
                        "You".to_owned()
                    } else {
                        n.author.clone()
                    },
                    mine,
                    (!going).then_some(n.created),
                    n.body.clone(),
                )
            };
            let card = Card {
                id: t.id.clone(),
                first: said(first),
                last: t.notes.last().filter(|_| t.notes.len() > 1).map(said),
                between: t.notes.len().saturating_sub(2),
                place: Place {
                    file: t.path.as_ref().map(|p| {
                        let name = p.rsplit('/').next().unwrap_or(p);
                        (name.to_owned(), t.new_line.or(t.old_line))
                    }),
                    since: self.line_since(t),
                    turn: waiting_on(t, me).map(|w| match w.as_str() {
                        "waiting on you" => ("reply", "Waiting on you".to_owned()),
                        _ => ("clock", w.replacen('w', "W", 1)),
                    }),
                },
                pending: (self.drafts.iter()).any(|d| d.reply_to.as_deref() == Some(t.id.as_str())),
                waits: self.thread_waits(&t.id).is_some()
                    || t.notes.iter().any(|n| self.note_time(n.id, n.created).1),
                folded: self.page.folded.contains(&t.id),
                open,
            };
            let el = self
                .draw_card(("overview-thread", ix), card, &show, base, cx)
                .relative()
                .child(mark(t.id.clone(), &self.page))
                .into_any_element();
            if ix < open_shown.len() {
                cards.push(el);
            } else {
                new_cards.push(el);
            }
        }
        for (ix, d) in pending.iter().enumerate() {
            let (_, target, old) = self.draft_where(d);
            let open: Opener = match target.clone() {
                Some((path, line)) => {
                    let show = show.clone();
                    Rc::new(move |cx: &mut App| {
                        let path = path.clone();
                        show(Act::JumpToLine { path, line, old }, cx)
                    })
                }
                None => window.clone(),
            };
            let key = format!("draft-{}", d.id);
            let card = Card {
                first: ("You".to_owned(), true, None, d.body.clone()),
                last: None,
                between: 0,
                place: Place {
                    file: self.draft_place(d).map(|(p, new, old)| {
                        let name = p.rsplit('/').next().unwrap_or(&p).to_owned();
                        (name, new.or(old))
                    }),
                    since: None,
                    turn: None,
                },
                pending: true,
                waits: self.draft_waits(d.id).is_some(),
                folded: self.page.folded.contains(&key),
                id: key,
                open,
            };
            cards.push(
                self.draw_card(("overview-pending", ix), card, &show, base, cx)
                    .into_any_element(),
            );
        }
        // What came in under the page stands at the section's end, until it is gone to.
        cards.append(&mut new_cards);
        let count = cards.len();
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Discussions")
                    .when(count > 0, |s| {
                        s.child(
                            div()
                                .text_size(px(12.))
                                .font_weight(FontWeight::NORMAL)
                                .text_color(theme::faint())
                                .child(count.to_string()),
                        )
                    }),
            )
            .when(count == 0 && resolved == 0, |s| {
                s.child(
                    div()
                        .text_size(px(12.))
                        .text_color(theme::faint())
                        .child("No discussions yet."),
                )
            })
            .children(cards)
            .children((resolved > 0).then(|| {
                div()
                    .id("overview-resolved")
                    .flex()
                    .items_center()
                    .py(px(2.))
                    .text_size(px(12.))
                    .text_color(theme::muted())
                    .cursor_pointer()
                    .hover(|s| s.text_color(theme::accent()))
                    .child(format!("{resolved} resolved"))
                    .child(div().flex_1())
                    .child("→")
                    .on_click(move |_: &ClickEvent, _, cx| window(cx))
            }))
            .children(composer.map(|c| div().pt(px(4.)).child(c)))
            .into_any_element()
    }

    /// `Changed` or `Line gone` for a discussion whose line is not as it was when written; nothing when it is, or
    /// before that is known.
    fn line_since(&self, t: &Thread) -> Option<&'static str> {
        t.path.as_ref()?;
        let h = self.histories.get(&t.id)?;
        match (h.changed(), h.line_now) {
            (_, None) => Some("Line gone"),
            (true, _) => Some("Changed"),
            (false, _) => None,
        }
    }

    fn draw_card(
        &self,
        id: (&'static str, usize),
        card: Card,
        acts: &Acts,
        base: Option<&str>,
        cx: &App,
    ) -> gpui::Stateful<Div> {
        let Card {
            id: key,
            first,
            last,
            between,
            place,
            pending,
            waits,
            folded,
            open,
        } = card;
        let fold = {
            let (acts, key) = (acts.clone(), key.clone());
            div()
                .id((id.0, id.1 * 2 + 1))
                .flex_none()
                .size(px(20.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(4.))
                .cursor_pointer()
                .hover(|s| s.bg(theme::hover()))
                .child(
                    icons::icon(if folded {
                        "chevron-right"
                    } else {
                        "chevron-down"
                    })
                    .size(px(14.))
                    .text_color(theme::faint()),
                )
                .tip(if folded { "Unfold" } else { "Fold" })
                .on_click(move |_: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    acts(Act::FoldCard(key.clone()), cx)
                })
        };
        // Who and when, the card's chevron after the time, its place in both states.
        let who = |(name, mine, when, _): &(String, bool, Option<i64>, String),
                   end: Vec<AnyElement>| {
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .text_size(px(12.))
                .child(discussion::avatar(name, *mine, 18.))
                .child(
                    hover::cut(("overview-who", usize::from(end.is_empty())), name.clone())
                        .min_w_0()
                        .font_weight(FontWeight::SEMIBOLD),
                )
                .child(div().flex_1())
                .children(when.map(|at| {
                    div()
                        .flex_none()
                        .text_size(px(11.))
                        .text_color(theme::faint())
                        .child(format::short_ago(at))
                }))
                .children(end)
        };
        let words = |note: usize, body: &str| {
            div().text_size(px(13.)).child(markdown::body(
                body,
                true,
                format!("overview-card-{key}-{note}"),
                base,
                None,
                cx,
            ))
        };
        let mut end: Vec<AnyElement> = Vec::new();
        if pending {
            end.push(
                div()
                    .id(("overview-pending-mark", id.1))
                    .flex_none()
                    .child(
                        icons::icon("circle-dashed")
                            .size(px(14.))
                            .text_color(theme::muted()),
                    )
                    .tip("Pending: only you see it until the review is submitted")
                    .into_any_element(),
            );
        }
        end.push(fold.into_any_element());
        let mut el = div()
            .id(id)
            .flex()
            .flex_col()
            .gap(px(6.))
            .px(px(12.))
            .py(px(10.))
            .rounded(px(ROW_RADIUS))
            .border_1()
            .border_color(theme::island_border())
            .bg(theme::panel())
            .cursor_pointer()
            .when(waits, |s| s.opacity(FADED))
            .child(who(&first, end));
        if folded {
            el = el.child(
                hover::cut(("overview-folded", id.1), format::one_line(&first.3, true))
                    .text_size(px(12.))
                    .text_color(theme::muted()),
            );
        } else {
            el = el.child(words(0, &first.3));
            if between > 0 {
                el = el.child(
                    div()
                        .text_size(px(11.))
                        .text_color(theme::faint())
                        .child(format!("{between} more")),
                );
            }
            if let Some(last) = &last {
                el = el.child(
                    div()
                        .pt(px(8.))
                        .border_t_1()
                        .border_color(theme::island_border())
                        .flex()
                        .flex_col()
                        .gap(px(6.))
                        .child(who(last, Vec::new()))
                        .child(words(1, &last.3)),
                );
            }
        }
        let notes = between + 1 + usize::from(last.is_some());
        let at = match &place.file {
            Some((name, line)) => div()
                .flex()
                .min_w_0()
                .font_family(theme::code_font())
                .child(hover::cut(("overview-place", id.1), name.clone()).min_w_0())
                .children(line.map(|n| div().text_color(theme::text()).child(format!(":{n}")))),
            None => div().child("General"),
        };
        el.child(
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .text_size(px(11.))
                .text_color(theme::faint())
                .child(
                    icons::icon(if place.file.is_some() {
                        "file"
                    } else {
                        "message"
                    })
                    .size(px(12.))
                    .text_color(theme::faint()),
                )
                .child(at)
                .children(place.since.map(|word| {
                    div()
                        .flex_none()
                        .px(px(5.))
                        .rounded(px(4.))
                        .bg(theme::selected())
                        .text_color(theme::warning())
                        .child(word)
                }))
                .child(div().flex_1())
                .children((notes > 1).then(|| {
                    div()
                        .id(("overview-count", id.1))
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap(px(3.))
                        .child(
                            icons::icon("message")
                                .size(px(12.))
                                .text_color(theme::faint()),
                        )
                        .child(notes.to_string())
                        .tip(plural(notes, "note"))
                }))
                .children(place.turn.map(|(icon, said)| {
                    let color = if icon == "reply" {
                        theme::warning()
                    } else {
                        theme::faint()
                    };
                    div()
                        .id(("overview-turn", id.1))
                        .flex_none()
                        .child(icons::icon(icon).size(px(13.)).text_color(color))
                        .tip(said)
                })),
        )
        .on_click(move |_: &ClickEvent, _, cx| open(cx))
    }
}
