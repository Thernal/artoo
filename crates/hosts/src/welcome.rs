//! Connect GitLab or GitHub, a way into the welcome (#54): a host typed as one likes, a token pasted, kept for that
//! host and asked who it is; a token the host refuses is taken back. Its two fields are the tab's inputs.

use artoo_core::git::WebRemote;
use artoo_shell_api::welcome::{Card, Field, Tone, Way};
use artoo_shell_api::{Frame, Input, Parts, Typed};
use artoo_ui_kit::input::{self, Area, Edit, Look};
use gpui::{App, AppContext, Context, Entity, IntoElement, Keystroke};
use std::rc::Rc;

/// The host's field, one of the tab's inputs.
const HOST: &str = "hosts.welcome-host";
/// The token's field.
const TOKEN: &str = "hosts.welcome-token";

/// The card's state in one tab.
struct Connecting {
    frame: Rc<dyn Frame>,
    open: bool,
    host: String,
    host_area: Area,
    token: String,
    token_area: Area,
    /// What the last check said: who the token is, or why not.
    said: Option<Result<String, String>>,
    busy: bool,
}

/// `host` as typed — `gitlab.example.com`, `https://gitlab.example.com/` — as a remote to check a token against:
/// GitHub's for `github.com`, else a GitLab host's.
fn remote_of(host: &str) -> Option<WebRemote> {
    let host = host.trim();
    let host = host
        .split_once("://")
        .map_or(host, |(_, rest)| rest)
        .trim_end_matches('/');
    let host = if host.is_empty() { "gitlab.com" } else { host };
    (!host.contains(char::is_whitespace)).then(|| WebRemote {
        base: format!("https://{host}/-/-"),
        github: host == "github.com",
    })
}

impl Connecting {
    /// The card opened, the host's field given the keyboard.
    fn open_card(&mut self, cx: &mut Context<Self>) {
        self.open = true;
        self.said = None;
        self.take(HOST, cx);
    }

    /// Field `input` given the keyboard, its caret at the end of its text.
    fn take(&mut self, input: &'static str, cx: &mut Context<Self>) {
        match input {
            HOST => self.host_area = Area::default(),
            _ => self.token_area = Area::default(),
        }
        self.frame.take_keyboard(input, cx);
        cx.notify();
    }

    /// The ways again, what was typed kept.
    fn close(&mut self, cx: &mut Context<Self>) {
        self.open = false;
        self.said = None;
        self.frame.leave_keyboard(HOST, cx);
        self.frame.leave_keyboard(TOKEN, cx);
        cx.notify();
    }

    /// A key in field `input`: ↵ in the host's goes to the token, in the token's connects; esc goes back to the
    /// ways.
    fn key(&mut self, input: &'static str, key: &Keystroke, cx: &mut Context<Self>) -> Typed {
        let (text, area) = match input {
            HOST => (&mut self.host, &mut self.host_area),
            _ => (&mut self.token, &mut self.token_area),
        };
        match input::edit(text, &mut area.edit, key, cx) {
            Edit::Escape => {
                self.open = false;
                self.said = None;
                cx.notify();
                return Typed::Leave;
            }
            Edit::Enter { .. } if input == HOST => self.take(TOKEN, cx),
            Edit::Enter { .. } => self.connect(cx),
            Edit::Changed | Edit::Selected | Edit::Ignored => {}
        }
        cx.notify();
        Typed::Kept
    }

    /// The token kept for the host, then asked who it is; a token the host refuses is taken back.
    fn connect(&mut self, cx: &mut Context<Self>) {
        let Some(remote) = remote_of(&self.host) else {
            self.said = Some(Err("That does not look like a host.".to_owned()));
            return cx.notify();
        };
        let host = artoo_hosts_api::get(cx).of(&remote);
        let token = self.token.trim().to_owned();
        if token.len() < 8 || token.contains(char::is_whitespace) {
            self.said = Some(Err("That does not look like a token.".to_owned()));
            return cx.notify();
        }
        if let Err(e) = host.save_token(&token) {
            self.said = Some(Err(format!("{e:#}")));
            return cx.notify();
        }
        self.said = None;
        self.busy = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let check = host.clone();
            let result = cx
                .background_executor()
                .spawn(async move {
                    let who = check.me()?;
                    Ok::<_, anyhow::Error>(match check.token_info() {
                        Some(info) => format!("{who} · {info}"),
                        None => who,
                    })
                })
                .await;
            this.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(who) => {
                        this.said = Some(Ok(who));
                        this.token.clear();
                        this.frame.leave_keyboard(TOKEN, cx);
                    }
                    Err(e) => {
                        host.forget_token();
                        this.said = Some(Err(format!("{e:#}")));
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// The card, while it is open.
    fn card(&self, typing: Option<&'static str>, cx: &mut Context<Self>) -> Option<Card> {
        if !self.open {
            return None;
        }
        let this = cx.entity().downgrade();
        let field = |input: &'static str| {
            let active = typing == Some(input);
            let (text, area, placeholder, mask, of): (
                _,
                _,
                _,
                _,
                fn(&mut Self) -> Option<&mut Area>,
            ) = match input {
                HOST => (
                    &self.host,
                    &self.host_area,
                    "gitlab.com · github.com · gitlab.example.com",
                    false,
                    |c| Some(&mut c.host_area),
                ),
                _ => (
                    &self.token,
                    &self.token_area,
                    "Paste your token",
                    true,
                    |c| Some(&mut c.token_area),
                ),
            };
            let look = Look {
                mask,
                ..Default::default()
            };
            (
                active,
                area.field_look_of(text, active, placeholder, look, of, this.clone())
                    .into_any_element(),
            )
        };
        let ((host_active, host), (token_active, token)) = (field(HOST), field(TOKEN));
        let page = remote_of(&self.host).map(|r| artoo_hosts_api::get(cx).of(&r).token_page());
        let done = matches!(self.said, Some(Ok(_)));
        let (back, go) = (this.clone(), this.clone());
        Some(Card {
            title: "Connect GitLab or GitHub",
            text: "For review requests, discussions and approvals.",
            fields: vec![
                Field {
                    label: "Host",
                    link: None,
                    id: "welcome-host",
                    input: HOST,
                    active: host_active,
                    inside: host,
                },
                Field {
                    label: "Personal access token",
                    link: page.map(|page| {
                        let open: artoo_shell_api::welcome::Press =
                            Rc::new(move |_, cx| cx.open_url(&page));
                        ("welcome-token-page", "Create one ↗", open)
                    }),
                    id: "welcome-token",
                    input: TOKEN,
                    active: token_active,
                    inside: token,
                },
            ],
            said: match (&self.said, self.busy) {
                (_, true) => Some(("Checking…".to_owned(), Tone::Muted)),
                (Some(Ok(text)), _) => Some((format!("✓ Signed in as {text}"), Tone::Good)),
                (Some(Err(e)), _) => Some((e.clone(), Tone::Bad)),
                _ => None,
            },
            back: Rc::new(move |_, cx| {
                back.update(cx, |c, cx| c.close(cx)).ok();
            }),
            go: if done {
                (
                    "welcome-done",
                    "Done",
                    Rc::new(move |_, cx| {
                        go.update(cx, |c, cx| c.close(cx)).ok();
                    }),
                )
            } else {
                (
                    "welcome-check",
                    "Connect",
                    Rc::new(move |_, cx| {
                        go.update(cx, |c, cx| c.connect(cx)).ok();
                    }),
                )
            },
        })
    }
}

/// Field `id` of the card as the tab's input.
fn input(connecting: &Entity<Connecting>, id: &'static str) -> Input {
    let (open, key, selected) = (
        connecting.downgrade(),
        connecting.downgrade(),
        connecting.downgrade(),
    );
    Input {
        id,
        open: Rc::new(move |cx| open.upgrade().is_some_and(|c| c.read(cx).open)),
        key: Rc::new(move |event, _, cx| {
            key.update(cx, |c, cx| c.key(id, &event.keystroke, cx))
                .unwrap_or(Typed::Kept)
        }),
        // Going elsewhere keeps the card open, as esc does not.
        left: Rc::new(|_| {}),
        // A token is not copied out of its field.
        selected: Rc::new(move |cx| {
            let c = selected.upgrade()?;
            let c = c.read(cx);
            (id == HOST)
                .then(|| c.host_area.edit.selected_text().map(str::to_owned))
                .flatten()
        }),
        caret: Rc::new(|_| true),
    }
}

/// A tab's card: its way into the welcome, its two fields, and what a snapshot opens of it (`welcome-connect`).
pub(crate) fn parts(frame: Rc<dyn Frame>, cx: &mut App) -> Parts {
    let connecting = cx.new(|_| Connecting {
        frame: frame.clone(),
        open: false,
        host: String::new(),
        host_area: Area::default(),
        token: String::new(),
        token_area: Area::default(),
        said: None,
        busy: false,
    });
    // Drawn inside the tab: a change draws the tab again.
    let redraw = frame.clone();
    cx.observe(&connecting, move |_, cx| redraw.redraw(cx))
        .detach();
    // The way's card holds it: it lives as long as the tab's parts.
    let (go, card, snap) = (
        connecting.downgrade(),
        connecting.clone(),
        connecting.downgrade(),
    );
    Parts {
        inputs: vec![input(&connecting, HOST), input(&connecting, TOKEN)],
        ways: vec![Way {
            id: "welcome-connect",
            icon: "pull-request",
            title: "Connect GitLab or GitHub",
            text: "your review requests and inbox, from a token",
            order: 1,
            go: Rc::new(move |_, cx| {
                go.update(cx, |c, cx| c.open_card(cx)).ok();
            }),
            card: Rc::new(move |typing, cx| card.update(cx, |c, cx| c.card(typing, cx))),
        }],
        snapshots: vec![(
            "welcome-connect",
            Rc::new(move |_, cx| {
                snap.update(cx, |c, cx| c.open_card(cx)).ok();
            }),
        )],
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_host_is_read_however_it_is_typed() {
        let base = |h: &str| remote_of(h).map(|r| (r.base, r.github));
        assert_eq!(
            base("https://gitlab.example.com/"),
            Some(("https://gitlab.example.com/-/-".to_owned(), false))
        );
        assert_eq!(
            base("github.com"),
            Some(("https://github.com/-/-".to_owned(), true))
        );
        assert_eq!(base(""), Some(("https://gitlab.com/-/-".to_owned(), false)));
        assert_eq!(base("a b"), None);
    }
}
