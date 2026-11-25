//! The why in reach from a request's Changes (#188, Design A5): `Description ▾` after its title in the page bar
//! opens the description over the toolbar, the issue it is for named above it (`PAY-123 ↗`); a request that says
//! nothing has no `Description ▾`.

use crate::requests::Requests;
use artoo_core::git::WebRemote;
use artoo_ui_kit::hover::{self, Hover as _};
use artoo_ui_kit::markdown::{self, Selecting};
use artoo_ui_kit::{ISLAND_RADIUS, ROW_RADIUS, icons, px, theme};
use gpui::{AnyElement, ClickEvent, Context, FontWeight, MouseButton, div, prelude::*};

/// The issue a request is for: its key or number, and its page when it is known.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Issue {
    pub label: String,
    pub url: Option<String>,
}

/// What a click on a request's issue does (#217).
#[derive(Debug, PartialEq)]
pub enum IssueClick {
    /// The issue opens in the app, under its tracker's space.
    App,
    /// The browser opens this address.
    Browser(String),
    Nothing,
}

/// The choice: a plain click opens the issue in the app when a tracker knows the key (`known`), else the address the
/// request's text gave (`url`) in the browser; ⌘/Ctrl-click (`browser`) the browser, at `url` else the tracker's
/// address of the key (`tracker_url`), and the plain choice when it has none.
pub fn issue_click(
    known: bool,
    url: Option<&str>,
    tracker_url: Option<&str>,
    browser: bool,
) -> IssueClick {
    if browser && let Some(url) = url.or(tracker_url) {
        return IssueClick::Browser(url.to_owned());
    }
    match (known, url) {
        (true, _) => IssueClick::App,
        (false, Some(url)) => IssueClick::Browser(url.to_owned()),
        (false, None) => IssueClick::Nothing,
    }
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
        if url.contains("/browse/") && artoo_shell_api::issues::is_key(last) {
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
            let label = format!("{project}-{number}");
            artoo_shell_api::issues::is_key(&label).then_some(Issue { label, url: None })
        })
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
            .text_color(theme::muted())
            .cursor_pointer()
            .hover(|s| s.bg(theme::hover()).text_color(theme::text()))
            .when(open, |s| s.bg(theme::selected()).text_color(theme::text()))
            .child(icons::icon("text-align-start").size(px(16.)))
            .child(icons::icon("chevron").size(px(13.)))
            .tip_two(
                "Description",
                "What the request is for, over the diff  esc closes",
            )
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
                        // What a click does (#217): in the app when a tracker knows the key, else the browser; with
                        // ⌘ (Ctrl elsewhere) the browser.
                        let known = artoo_shell_api::issues::card(cx, &issue.label).is_some();
                        let tracker_url = artoo_shell_api::issues::url(cx, &issue.label);
                        let clickable = link.is_some() || known;
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
                                    .when(clickable, |s| {
                                        let label = issue.label.clone();
                                        let url = issue.url.clone();
                                        s.cursor_pointer()
                                            .hover(|s| s.text_color(theme::text()))
                                            .on_click(move |e: &ClickEvent, _, cx| {
                                                let click = issue_click(
                                                    known,
                                                    url.as_deref(),
                                                    tracker_url.as_deref(),
                                                    e.modifiers().secondary(),
                                                );
                                                match click {
                                                    IssueClick::App => {
                                                        artoo_shell_api::issues::open(cx, &label);
                                                    }
                                                    IssueClick::Browser(url) => cx.open_url(&url),
                                                    IssueClick::Nothing => {}
                                                }
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

    /// The title of the open request's Changes, its own line over the diff's toolbar, the description's button at its
    /// end (the title is not in the page's bar there).
    pub fn render_title_line(
        &mut self,
        selecting: Option<Selecting>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let title = self.find(self.current?)?.title.clone();
        let button = self.render_description(selecting, cx);
        Some(
            div()
                .flex_none()
                .h(px(44.))
                .px(px(12.))
                .flex()
                .items_center()
                .gap(px(8.))
                .border_b_1()
                .border_color(theme::island_border())
                .text_size(px(13.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::text())
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(hover::cut("changes-title", title)),
                )
                .children(button)
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

    #[test]
    fn an_issue_opens_in_the_app_when_known_else_the_browser_and_with_the_modifier_the_browser() {
        let jira = "https://jira.example/browse/PAY-1";
        let tracker = Some("https://tracker.example/PAY-1");
        // Plain: the app when a tracker knows it, else the written address, else nothing.
        assert_eq!(
            issue_click(true, Some(jira), tracker, false),
            IssueClick::App
        );
        assert_eq!(issue_click(true, None, None, false), IssueClick::App);
        assert_eq!(
            issue_click(false, Some(jira), None, false),
            IssueClick::Browser(jira.into())
        );
        assert_eq!(issue_click(false, None, None, false), IssueClick::Nothing);
        // With the modifier: the written address, else the tracker's, else the plain choice.
        assert_eq!(
            issue_click(true, Some(jira), tracker, true),
            IssueClick::Browser(jira.into())
        );
        assert_eq!(
            issue_click(true, None, tracker, true),
            IssueClick::Browser("https://tracker.example/PAY-1".into())
        );
        assert_eq!(issue_click(true, None, None, true), IssueClick::App);
        assert_eq!(issue_click(false, None, None, true), IssueClick::Nothing);
    }
}
