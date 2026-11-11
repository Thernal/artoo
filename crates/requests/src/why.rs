//! The why in reach from a request's Changes (#188, Design A5): `Description ▾` after its title in the page bar
//! opens the description over the toolbar, the issue it is for named above it (`PAY-123 ↗`); a request that says
//! nothing has no `Description ▾`.

use crate::requests::Requests;
use artoo_core::git::WebRemote;
use artoo_ui_kit::hover::{self, Tip};
use artoo_ui_kit::markdown::{self, Selecting};
use artoo_ui_kit::{ISLAND_RADIUS, ROW_RADIUS, icons, px, theme};
use gpui::{AnyElement, ClickEvent, Context, FontWeight, MouseButton, div, prelude::*};

/// The issue a request is for: its key or number, and its page when it is known.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Issue {
    pub label: String,
    pub url: Option<String>,
}

/// The issue a request is for, found in what its author wrote: a link to a Jira issue or a host's issue in the
/// description, a closing reference (`Closes #12`) on the host's own issues, else a key like `PAY-123` in the title,
/// the branch or the description, named without a page.
pub fn issue_of(
    title: &str,
    branch: &str,
    description: &str,
    web: Option<&WebRemote>,
) -> Option<Issue> {
    // A link written out: its key, or the host's issue number.
    for word in description.split(|c: char| c.is_whitespace() || "()<>[]".contains(c)) {
        let Some(start) = word.find("http") else {
            continue;
        };
        let url = word[start..].trim_end_matches(['.', ',', ';', ':']);
        if !url.contains("://") {
            continue;
        }
        let last = url.rsplit('/').next().unwrap_or_default();
        if url.contains("/browse/") && key(last).is_some() {
            return Some(Issue {
                label: last.to_owned(),
                url: Some(url.to_owned()),
            });
        }
        if url.contains("/issues/") && !last.is_empty() && last.bytes().all(|b| b.is_ascii_digit())
        {
            return Some(Issue {
                label: format!("#{last}"),
                url: Some(url.to_owned()),
            });
        }
    }
    // A closing reference: the host's own issue.
    let lower = description.to_lowercase();
    for verb in [
        "closes #",
        "fixes #",
        "resolves #",
        "close #",
        "fix #",
        "resolve #",
    ] {
        if let Some(at) = lower.find(verb) {
            let digits: String = description[at + verb.len()..]
                .chars()
                .take_while(char::is_ascii_digit)
                .collect();
            if !digits.is_empty() {
                let url = web.map(|w| match w.github {
                    true => format!("{}/issues/{digits}", w.base),
                    false => format!("{}/-/issues/{digits}", w.base),
                });
                return Some(Issue {
                    label: format!("#{digits}"),
                    url,
                });
            }
        }
    }
    // A key named: in the title first, then the branch, then the description.
    [title, branch, description]
        .iter()
        .flat_map(|text| text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-')))
        .find_map(|word| {
            // A branch's key comes first in it: `PAY-123-login-state`.
            let mut parts = word.splitn(3, '-');
            let (project, number) = (parts.next()?, parts.next()?);
            key(&format!("{project}-{number}")).map(|label| Issue { label, url: None })
        })
}

/// `PAY-123`: capitals (a digit may follow the first), a dash, a number.
fn key(word: &str) -> Option<String> {
    let (project, number) = word.split_once('-')?;
    let project_ok = project.len() >= 2
        && project.starts_with(|c: char| c.is_ascii_uppercase())
        && project
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit());
    let number_ok = !number.is_empty() && number.chars().all(|c| c.is_ascii_digit());
    (project_ok && number_ok).then(|| word.to_owned())
}

impl Requests {
    /// The open request's description: the details' once they are in, else the list's; none when it says nothing.
    pub fn description_text(&self) -> Option<String> {
        let m = self.find(self.current?)?;
        let text = match &self.details {
            Some(d) if !d.description.trim().is_empty() => &d.description,
            _ => &m.description,
        };
        (!text.trim().is_empty()).then(|| text.clone())
    }

    /// The issue the open request is for (`issue_of`).
    pub fn linked_issue(&self) -> Option<Issue> {
        let m = self.find(self.current?)?;
        let description = self.description_text().unwrap_or_default();
        issue_of(&m.title, &m.source, &description, self.web.as_ref())
    }

    /// `Description ▾`, its card hanging under it while open; none when the request says nothing.
    pub fn render_description(
        &self,
        selecting: Option<Selecting>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let text = self.description_text()?;
        let iid = self.current?;
        let open = self.description_open;
        let button = div()
            .id("bar-description")
            .relative()
            .flex_none()
            .h(px(28.))
            .px(px(8.))
            .flex()
            .items_center()
            .gap(px(4.))
            .rounded(px(ROW_RADIUS))
            .text_size(px(13.))
            .text_color(theme::accent())
            .cursor_pointer()
            .hover(|s| s.bg(theme::hover()))
            .when(open, |s| s.bg(theme::selected()))
            .child("Description")
            .child(
                icons::icon("chevron")
                    .size(px(13.))
                    .text_color(theme::accent()),
            )
            .tooltip_show_delay(hover::LONG)
            .tooltip(|_, cx| {
                cx.new(|_| Tip("What the request is for, over the diff  esc closes"))
                    .into()
            })
            .on_click(cx.listener(|rq, _: &ClickEvent, _, cx| {
                rq.description_open = !rq.description_open;
                cx.notify();
            }));
        if !open {
            return Some(button.into_any_element());
        }
        let issue = self.linked_issue();
        let base = self.web.as_ref().map(|w| w.base.clone());
        let body = markdown::body(
            &text,
            true,
            format!("description-{iid}"),
            base.as_deref(),
            selecting,
            cx,
        );
        let card = div()
            .id("description-card")
            .occlude()
            .w(px(560.))
            .max_h(px(420.))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(8.))
            .p(px(14.))
            .rounded(px(ISLAND_RADIUS))
            .border_1()
            .border_color(theme::island_border())
            .bg(theme::panel())
            .shadow_lg()
            .font_family(theme::ui_font())
            .text_size(px(13.))
            .text_color(theme::text())
            // The bar it hangs from keeps its words on one line; the description wraps.
            .whitespace_normal()
            .cursor_default()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(|_: &ClickEvent, _, cx| cx.stop_propagation())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .text_size(px(12.))
                    .text_color(theme::faint())
                    .children(issue.map(|issue| {
                        let link = issue.url.clone();
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child("Issue")
                            .child(
                                div()
                                    .id("description-issue")
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(if link.is_some() {
                                        theme::accent()
                                    } else {
                                        theme::muted()
                                    })
                                    .child(match link {
                                        Some(_) => format!("{} ↗", issue.label),
                                        None => issue.label.clone(),
                                    })
                                    .when_some(link, |s, url| {
                                        s.cursor_pointer()
                                            .hover(|s| s.text_color(theme::text()))
                                            .on_click(move |_: &ClickEvent, _, cx| {
                                                cx.open_url(&url)
                                            })
                                    }),
                            )
                    }))
                    .child(div().flex_1())
                    .child("esc"),
            )
            .child(body);
        Some(
            button
                .child(
                    div().absolute().left_0().bottom_0().size_0().child(
                        gpui::deferred(
                            gpui::anchored()
                                .offset(gpui::point(px(-8.), px(8.)))
                                .snap_to_window_with_margin(px(8.))
                                .child(card),
                        )
                        .with_priority(2),
                    ),
                )
                .into_any_element(),
        )
    }

    /// The description's card closed: `true` when it was open.
    pub fn close_description(&mut self, cx: &mut Context<Self>) -> bool {
        let was = std::mem::take(&mut self.description_open);
        if was {
            cx.notify();
        }
        was
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gitlab() -> WebRemote {
        WebRemote {
            base: "https://gitlab.example.com/acme/payments".into(),
            github: false,
        }
    }

    #[test]
    fn the_issue_is_found_where_the_author_named_it() {
        let issue = |title, branch, text| issue_of(title, branch, text, Some(&gitlab()));
        // A Jira link written out: its key, and the page.
        assert_eq!(
            issue(
                "Refactor",
                "feature/login",
                "See [PAY-123](https://jira.example.com/browse/PAY-123)."
            ),
            Some(Issue {
                label: "PAY-123".into(),
                url: Some("https://jira.example.com/browse/PAY-123".into())
            })
        );
        // A closing reference: the host's own issue.
        assert_eq!(
            issue(
                "Refactor",
                "feature/login",
                "Moves the state.\n\nCloses #42"
            ),
            Some(Issue {
                label: "#42".into(),
                url: Some("https://gitlab.example.com/acme/payments/-/issues/42".into())
            })
        );
        // A key in the title, else in the branch: named without a page.
        assert_eq!(
            issue("PAY-7 Refactor the login", "feature/login", ""),
            Some(Issue {
                label: "PAY-7".into(),
                url: None
            })
        );
        assert_eq!(
            issue("Refactor", "PAY-123-login-state", ""),
            Some(Issue {
                label: "PAY-123".into(),
                url: None
            })
        );
        // Nothing that reads as an issue: none. A word with a dash is not a key without capitals and a number.
        assert_eq!(
            issue("Refactor the reader", "feature/login", "An x-ray of PAY-x"),
            None
        );
    }
}
