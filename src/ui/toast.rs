//! Errors as desktop notifications, the tab's side: the toasts are the kit's (`artoo_ui_kit::toast`). Code still
//! reports an error by setting `Workspace::error`; each frame moves it there. A deleted draft's toast says so and
//! offers Undo instead (#76). A toast with a button (`artoo_shell_api::Toast`) is raised by what it is about —
//! an agent's news offering to go and see it (#150) —, its button an action the tab answers (`press`).

use super::Workspace;
use artoo_shell_api::Toast;
use artoo_ui_kit::menu::WIDTH;
use artoo_ui_kit::toast::{More, Toasts, Words};
use gpui::{Action, App, Context, Entity, SharedString, prelude::*};
use std::cell::Cell;
use std::rc::Rc;

/// A toast's button as the stack keeps it: its words and the action it runs. Two are the same button when
/// their actions are the same, fields and all.
#[derive(Debug)]
pub(super) struct Press {
    words: &'static str,
    pub(super) act: Box<dyn Action>,
}

impl Clone for Press {
    fn clone(&self) -> Self {
        Self {
            words: self.words,
            act: self.act.boxed_clone(),
        }
    }
}

impl PartialEq for Press {
    fn eq(&self, other: &Self) -> bool {
        self.act.partial_eq(&*other.act)
    }
}

/// Stop listening on the toast of a chat that began: its listening ends (`Agents::stop_listening`).
#[derive(Clone, PartialEq, Debug, gpui::Action)]
#[action(namespace = agents, no_json, no_register)]
pub(super) struct StopListening {
    session: String,
}

/// Show on the toast of a chat that began: the Agents window opens. It names the chat so two chats' toasts
/// stay two (a toast with the same button is joined to the older one).
#[derive(Clone, PartialEq, Debug, gpui::Action)]
#[action(namespace = agents, no_json, no_register)]
pub(super) struct ShowChat {
    session: String,
}

/// The toast of a chat that began listening or went: what happened, the branch it works on under it, and for one
/// that began Stop listening.
pub(super) fn listening(said: &artoo_agents_api::tab::Said, mark: (char, gpui::Rgba)) -> Toast {
    // The chat's name is bold: what comes before ` started` or ` stopped`.
    let bold = said
        .text
        .find(" started")
        .or_else(|| said.text.find(" stopped"))
        .unwrap_or(0);
    let toast = Toast::new(said.text.clone())
        .kind(artoo_agents_api::notices::PRESENCE)
        .flat(artoo_ui_kit::toast::Flat {
            mark,
            bold,
            branch: said.branch.clone().into(),
        });
    match said.began {
        // Show opens the Agents window; Stop listening ends the chat's listening.
        true => toast
            .button(
                "Show",
                ShowChat {
                    session: said.session.clone(),
                },
            )
            .second_button(
                "Stop listening",
                StopListening {
                    session: said.session.clone(),
                },
            ),
        false => toast,
    }
}

/// An error as a title and a body: its first sentence, then the reason (`Couldn't save the draft: the disk is
/// full`); one sentence is the title alone.
pub(super) fn split_error(text: &str) -> (String, Option<String>) {
    let text = text.trim();
    let cut = text
        .find(": ")
        .map(|at| (at, 2))
        .or_else(|| text.find(". ").map(|at| (at, 2)))
        .or_else(|| text.find('\n').map(|at| (at, 1)));
    let Some((at, skip)) = cut else {
        return (text.trim_end_matches('.').to_owned(), None);
    };
    let (title, rest) = (&text[..at], text[at + skip..].trim());
    if rest.is_empty() {
        return (title.to_owned(), None);
    }
    let mut chars = rest.chars();
    let first = chars.next().map(|c| c.to_uppercase().collect::<String>());
    let body = format!("{}{}", first.unwrap_or_default(), chars.as_str());
    (title.to_owned(), Some(body))
}

/// A tab's toasts, their buttons answered by the tab.
pub(super) fn toasts(cx: &mut Context<Workspace>) -> Entity<Toasts<Press>> {
    let tab = cx.weak_entity();
    cx.new(|_| {
        Toasts::new(
            |press: &Press| press.words,
            Rc::new(move |press: Press, cx| {
                tab.update(cx, |this, cx| this.press(&*press.act, cx)).ok();
            }),
        )
    })
}

impl Workspace {
    /// Moves a reported error into a toast.
    pub(super) fn take_error(&mut self, cx: &mut Context<Self>) {
        if let Some(text) = self.error.take() {
            let (title, body) = split_error(&text);
            let words = match body {
                Some(body) => Words::new(title).about(body),
                None => Words::new(title),
            };
            self.toasts.update(cx, |t, cx| t.show(words, false, cx));
        }
    }

    /// News that is not an error: a chat began or stopped listening (#77).
    pub(super) fn toast_info(&mut self, text: SharedString, cx: &mut Context<Self>) {
        self.toasts.update(cx, |t, cx| t.show(text, true, cx));
    }

    /// A toast: news alone, or with a button, about `count` things — a toast with the same button
    /// still showing goes, its count added to this one's, and the toast words the sum (#150).
    ///
    /// A notice (a toast with a kind, #196) shows and goes to the system as its kind's switches say,
    /// its words after its source's name, `⋯` turning its kind's toasts off (#198); a kind no source
    /// offered is refused.
    pub(super) fn show_toast(&mut self, toast: Toast, cx: &mut Context<Self>) {
        self.raise(toast, true, cx);
    }

    /// A notice's toast alone: what goes to the system is said apart (the Inbox's news, the agents'
    /// answers, grouped).
    pub(super) fn show_notice_toast(&mut self, toast: Toast, cx: &mut Context<Self>) {
        self.raise(toast, false, cx);
    }

    fn raise(&mut self, toast: Toast, send: bool, cx: &mut Context<Self>) {
        use artoo_shell_api::notices::{self, Channel};
        let (shows, sends, guest, words) = match toast.kind {
            Some(kind) => {
                let Some((declared, source)) = notices::kind(cx, kind) else {
                    crate::applog::line(&format!(
                        "a notice of kind {kind}, offered by none: refused"
                    ));
                    return;
                };
                let sends = send && notices::on(cx, kind, Channel::System);
                let shows = notices::on(cx, kind, Channel::Toast);
                // A plugin's kind is named with its source: `Jira · Assigned to you`.
                let name = match source.guest {
                    true => format!("{} · {}", source.name, declared.name),
                    false => declared.name.to_owned(),
                };
                let tip = format!("Turn off toasts for {name}");
                let words = Words::default()
                    .source(self.source_name(&source))
                    .more_tip(tip)
                    .more(self.toast_more(kind, name, cx));
                (shows, sends, source.guest.then_some(source.name), words)
            }
            None => (true, send && toast.system.is_some(), None, Words::default()),
        };
        // A flat toast has no source and no ⋯: the chat's face, its words and branch are its whole line.
        let words = match toast.flat.clone() {
            Some(flat) => Words::default().flat(flat),
            None => Words {
                about: toast.about.clone().map(Into::into),
                source: toast
                    .source
                    .clone()
                    .map(Into::into)
                    .or(words.source.clone()),
                tone: toast.tone,
                facts: toast.facts.clone(),
                ..words
            },
        };
        let say = toast.say.clone();
        // The system's notification names a plugin first, as Artoo's own need not.
        let said = move |n: usize| match guest {
            Some(name) => format!("{name} · {}", say(n)),
            None => say(n),
        };
        let tag = toast.system.clone().or(toast.kind.map(str::to_owned));
        let Some((words_of_button, act)) = toast.button else {
            if sends && let Some(tag) = tag {
                let project = self.tab_title();
                let open = super::system_notes::Open::Here;
                self.notify_system(tag, said(toast.count), project, open, cx);
            }
            if shows {
                let words = Words {
                    text: (toast.say)(toast.count).into(),
                    ..words
                };
                self.toasts.update(cx, |t, cx| t.show(words, true, cx));
            }
            return;
        };
        let press = Press {
            words: words_of_button,
            act,
        };
        let second = toast.second.map(|(words, act)| Press { words, act });
        let count = toast.count + self.toasts.update(cx, |t, _| t.take_count(&press));
        // An agent's news goes to the system's notifications too, the newer replacing the older
        // (#26).
        if sends && let Some(tag) = tag {
            let open = super::system_notes::Open::Act(press.clone());
            let project = self.tab_title();
            self.notify_system(tag, said(count), project, open, cx);
        }
        if shows {
            let words = Words {
                text: (toast.say)(count).into(),
                ..words
            };
            self.toasts.update(cx, |t, cx| match second {
                Some(second) => t.show_acts(words, press, second, count, cx),
                None => t.show_act(words, press, count, cx),
            });
        }
        cx.notify();
    }

    /// A source's name on its toasts: the requests' is their host's (`GitLab`).
    fn source_name(&self, source: &artoo_shell_api::notices::Source) -> String {
        match &self.web {
            Some(web) if source.id == artoo_requests_api::NOTICES => web.name().to_owned(),
            _ => source.name.to_owned(),
        }
    }

    /// `⋯` on a notice's toast: its kind's toasts turned off, and Settings → Notifications; the
    /// menu as wide as the kind's whole name.
    fn toast_more(&self, kind: &'static str, name: String, cx: &mut Context<Self>) -> More {
        use super::menu::{Act, Entry};
        let tab = cx.weak_entity();
        Rc::new(move |at, _, cx| {
            let name = name.clone();
            tab.update(cx, |this, cx| {
                let off = format!("Turn off toasts for {name}");
                let width = (off.chars().count() as f32 * 7.5 + 56.).clamp(WIDTH, 420.);
                let entries = vec![
                    Entry::new(
                        off,
                        Act::new(move |this, _, cx| this.toasts_off(kind, name.clone(), cx)),
                    ),
                    Entry::new(
                        "Notification settings…",
                        Act::new(|this, _, cx| {
                            this.open_settings(cx);
                            this.open_settings_section(super::notice_settings::NOTIFICATIONS, cx);
                        }),
                    ),
                ];
                this.ctx_menu
                    .update(cx, |m, cx| m.open(at, vec![(None, entries)], width, cx));
                cx.notify();
            })
            .ok();
        })
    }

    /// Kind `kind`'s toasts off, said with Undo.
    fn toasts_off(&mut self, kind: &'static str, name: String, cx: &mut Context<Self>) {
        use artoo_shell_api::notices::{self, Channel};
        notices::set(cx, kind, Channel::Toast, false);
        let undo = Rc::new(move |cx: &mut App| notices::set(cx, kind, Channel::Toast, true));
        let text = format!("Toasts for {name} are off");
        self.toasts
            .update(cx, |t, cx| t.show_with(text, "Undo", undo, cx));
        cx.notify();
    }

    /// The toasts whose button runs an action `over` picks go now: news another has taken in.
    pub(super) fn drop_toasts(
        &mut self,
        over: impl Fn(&dyn Action) -> bool,
        cx: &mut Context<Self>,
    ) {
        self.toasts
            .update(cx, |t, cx| t.drop_acts(|press| over(&*press.act), cx));
    }

    /// The news of `text` is over: its toast goes now.
    pub(super) fn toast_over(&mut self, text: &str, cx: &mut Context<Self>) {
        self.toasts.update(cx, |t, cx| t.drop_text(text, cx));
    }

    /// "Draft deleted", `Money.kt:23` under it, with Undo, which puts `comment` back.
    pub(super) fn toast_deleted(
        &mut self,
        comment: crate::comment::Comment,
        cx: &mut Context<Self>,
    ) {
        let words = Words::new("Draft deleted").about(comment.reads());
        let (tab, comment) = (cx.weak_entity(), Cell::new(Some(comment)));
        let undo = Rc::new(move |cx: &mut App| {
            if let Some(comment) = comment.take() {
                tab.update(cx, |this, cx| this.undo(comment, cx)).ok();
            }
        });
        self.toasts
            .update(cx, |t, cx| t.show_with(words, "Undo", undo, cx));
        cx.notify();
    }

    /// Undo: the deleted draft is back where it was.
    fn undo(&mut self, comment: crate::comment::Comment, cx: &mut Context<Self>) {
        self.agents.restore_comment(comment, cx);
        self.refresh_rows(cx);
        cx.notify();
    }

    /// A toast's button, pressed, or its system notification clicked (#26): the tab's own answer
    /// to its action — the agents' while their code is the tab's, Settings' —, else the part's that
    /// runs it.
    pub(super) fn press(&mut self, act: &dyn Action, cx: &mut Context<Self>) {
        if let Some(a) = act.as_any().downcast_ref::<StopListening>() {
            self.agents.stop_listening(&a.session, cx);
            self.refresh_chats(cx);
            cx.notify();
            return;
        }
        if act.as_any().is::<ShowChat>() {
            return self.press(&artoo_agents::ToggleAgents, cx);
        }
        if self.answer_agents(act, cx)
            || self.answer_settings(act, cx)
            || self.answer_notice(act, cx)
        {
            return;
        }
        let run = self
            .part_actions()
            .into_iter()
            .find(|(a, _)| a.partial_eq(act))
            .map(|(_, run)| run);
        if let Some(run) = run {
            let window = self.window;
            cx.defer(move |cx| {
                window.update(cx, |_, window, cx| run(window, cx)).ok();
            });
        }
    }

    /// ⌥↵: the newest toast's button, pressed (#198); the snapshot checks press it too.
    pub(super) fn press_toast(&mut self, cx: &mut Context<Self>) -> bool {
        self.toasts.update(cx, |t, cx| t.press_newest(cx))
    }

    /// `esc`: the newest toast dismissed (round 40); false when none shows.
    pub(super) fn dismiss_toast(&mut self, cx: &mut Context<Self>) -> bool {
        self.toasts.update(cx, |t, cx| t.dismiss_newest(cx))
    }

    /// ⌘Z: the newest toast's Undo (#191).
    pub(super) fn undo_toast(&mut self, cx: &mut Context<Self>) -> bool {
        self.toasts.update(cx, |t, cx| t.press_undo(cx))
    }

    /// The stack, over everything else, under the page bar's right end.
    pub(super) fn render_toasts(&self, cx: &App) -> Option<impl IntoElement + use<>> {
        (!self.toasts.read(cx).is_empty()).then(|| self.toasts.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use artoo_agents_api::tab::Said;

    fn said(began: bool) -> Said {
        Said {
            text: "Codex started listening".into(),
            session: "s1".into(),
            branch: "feature/ledger-cache".into(),
            began,
        }
    }

    #[test]
    fn the_listening_toast_names_the_branch_and_offers_stop_listening() {
        let toast = listening(&said(true), ('X', gpui::rgb(0x112233)));
        assert_eq!((toast.say)(1), "Codex started listening");
        let flat = toast.flat.as_ref().expect("a flat line");
        assert_eq!(flat.branch, "feature/ledger-cache");
        assert_eq!(&"Codex started listening"[..flat.bold], "Codex");
        let (words, _) = toast.button.expect("a button");
        assert_eq!(words, "Show");
        let (words, act) = toast.second.expect("a second button");
        assert_eq!(words, "Stop listening");
        assert!(act.partial_eq(&StopListening {
            session: "s1".into()
        }));
    }

    #[test]
    fn an_error_is_a_title_and_a_reason() {
        assert_eq!(
            split_error("Couldn't save the draft: the disk is full"),
            (
                "Couldn't save the draft".into(),
                Some("The disk is full".into())
            )
        );
        assert_eq!(
            split_error("GitLab does not let you approve this merge request."),
            (
                "GitLab does not let you approve this merge request".into(),
                None
            )
        );
        assert_eq!(
            split_error("Nothing to edit. Open a thread first."),
            (
                "Nothing to edit".into(),
                Some("Open a thread first.".into())
            )
        );
    }

    #[test]
    fn a_chat_that_went_has_no_button() {
        assert!(
            listening(&said(false), ('X', gpui::rgb(0x112233)))
                .button
                .is_none()
        );
    }
}
