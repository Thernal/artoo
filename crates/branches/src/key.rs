//! A branch's name with its issue key as its own words (`feature/PAY-398-login-state`, #217): underlined, the issue's
//! card on a long hover, as the page bar's crumb draws it. With no tracker, or no key in the name, the name stays
//! one cut name (`hover::name`).

use artoo_shell_api::issues;
use artoo_shell_api::links::ShowLinkMenu;
use artoo_ui_kit::hover::{self, Hover};
use artoo_ui_kit::marked::Marked;
use gpui::{
    App, Context, IntoElement, ParentElement, Render, RenderOnce, SharedString, Styled, Window,
    div, prelude::*,
};

/// The name `text`, cut in its middle to its room, its key (if a tracker names one) apart.
#[derive(IntoElement)]
pub struct KeyedName {
    id: SharedString,
    text: SharedString,
}

/// A branch's name for a row or a fact; style it through its parent (the text style is inherited).
pub fn name(id: impl Into<SharedString>, text: impl Into<SharedString>) -> KeyedName {
    KeyedName {
        id: id.into(),
        text: text.into(),
    }
}

/// The issue's card, drawn by the kit.
struct Card(hover::IssueCard);

impl Render for Card {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        issues::drawn(&self.0)
    }
}

impl RenderOnce for KeyedName {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let key = issues::in_name(cx, &self.text)
            .into_iter()
            .next()
            .filter(|k| self.text.is_char_boundary(k.start) && self.text.is_char_boundary(k.end));
        let Some(k) = key else {
            return hover::name(self.id, self.text).into_any_element();
        };
        let (before, key, after) = (
            self.text[..k.start].to_owned(),
            self.text[k.clone()].to_owned(),
            self.text[k.end..].to_owned(),
        );
        let card_of = key.clone();
        let address = format!("{}{key}", issues::SCHEME);
        div()
            .min_w_0()
            .flex()
            .items_center()
            .children(
                (!before.is_empty())
                    .then(|| hover::name(SharedString::from(format!("{}-0", self.id)), before)),
            )
            .child(
                div()
                    .id(SharedString::from(format!("{}-1", self.id)))
                    .flex_none()
                    .whitespace_nowrap()
                    .underline()
                    .child(key.clone())
                    .link_mark(format!("{}-key", self.id), address.clone())
                    // ⌘/Ctrl-click: the issue in the browser; a plain click stays the row's.
                    .on_click(move |e: &gpui::ClickEvent, _, cx| {
                        if e.modifiers().secondary()
                            && let Some(url) = issues::url(cx, &key)
                        {
                            cx.stop_propagation();
                            cx.open_url(&url);
                        }
                    })
                    // Right-clicked: the key's menu.
                    .on_mouse_down(gpui::MouseButton::Right, move |e, window, cx| {
                        cx.stop_propagation();
                        let show = ShowLinkMenu {
                            address: address.clone(),
                            at: e.position,
                        };
                        window.dispatch_action(Box::new(show), cx);
                    })
                    .card(move |_, cx| match issues::card(cx, &card_of) {
                        Some(card) => cx.new(|_| Card(card)).into(),
                        None => cx.new(|_| gpui::Empty).into(),
                    }),
            )
            .children(
                (!after.is_empty())
                    .then(|| hover::name(SharedString::from(format!("{}-2", self.id)), after)),
            )
            .into_any_element()
    }
}
