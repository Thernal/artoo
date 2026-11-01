//! Report a problem (#118) in a tab: a tool window on the right, in every space — a title and a
//! description, what goes with them (the version always; the system, the log's last lines and the
//! window's picture ticked to begin with), the issue exactly as it goes, and Send: to the relay,
//! which files it in the project's private reports, or GitHub's new-issue page, filled in. An
//! agent's chat that hit an error drafts one (`report_problem`); it opens here, purple, and goes
//! only when sent. Below, the reports sent from here and what became of them.

use crate::{Draft, ReportProblem, Scrub, Sent};
use artoo_core::textedit::TextEdit;
use artoo_core::{applog, platform};
use artoo_shell_api::{Frame, Parts, Tool};
use artoo_ui_kit::controls::{bar_button, tool_foot};
use artoo_ui_kit::input::{self, Edit, Lines, Look};
use artoo_ui_kit::{ROW_RADIUS, format, hover, icons, px, theme};
use gpui::{
    AnyElement, App, ClickEvent, Context, FontWeight, KeyDownEvent, MouseButton, SharedString,
    Window, div, prelude::*,
};
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

/// The tool window's id: the layout keeps it open by it.
const ID: &str = "report";

/// How often an agent's draft is looked for (the agents' beat).
const BEAT: Duration = Duration::from_secs(2);

/// What the feature draws in a tab: the tool window, and Report a problem as an action.
pub(crate) fn parts(frame: Rc<dyn Frame>, _: &mut Window, cx: &mut App) -> Parts {
    let typing = Rc::new(Cell::new(true));
    let report = cx.new(|cx| Report::new(frame, typing.clone(), cx));
    let (opened, key, run) = (report.clone(), report.clone(), report.clone());
    Parts {
        tool: Some(Tool {
            id: ID,
            icon: "flag",
            tip: "Report a problem  ⌥⌘/",
            title: "Report a problem",
            spaces: &[],
            badge: None,
            view: report.into(),
            // The window's picture is taken before the report's window is drawn over it.
            opened: Rc::new(move |cx| {
                opened.update(cx, |r, cx| {
                    r.prepare();
                    r.refresh_states(cx);
                })
            }),
            typing,
            key: Rc::new(move |event, _, cx| key.update(cx, |r, cx| r.key(event, cx))),
            list_key: None,
        }),
        actions: vec![(
            Box::new(ReportProblem),
            Rc::new(move |_, cx| run.update(cx, |r, cx| r.open(cx))),
        )],
        ..Default::default()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
enum Focus {
    #[default]
    Title,
    Description,
}

/// The report being written in a tab, and the reports sent from here.
pub(crate) struct Report {
    frame: Rc<dyn Frame>,
    draft: Draft,
    focus: Focus,
    /// The field with the keyboard: its caret, selection and undo; a wrapped one's layout, a one-line one's
    /// sideways offset.
    edit: TextEdit,
    lines: Lines,
    scroll: Rc<Cell<f32>>,
    /// The window as it was when the report was opened, before the report's window showed.
    picture: Option<Vec<u8>>,
    /// "macOS 15.1 · Mac15,3", read once.
    system: String,
    sending: bool,
    /// What the last send said: the id it was kept as, or why it was not.
    said: Option<Result<String, String>>,
    /// Whether it is being written (nothing sent yet): the shell gives it typed keys then.
    typing: Rc<Cell<bool>>,
    sent: Vec<Sent>,
    /// The extras have been ticked once: a later draft keeps what was unticked.
    ready: bool,
}

/// An extra of a report: its words, whether it goes, and why not when it can't.
struct Extra {
    id: &'static str,
    what: String,
    why: &'static str,
    on: bool,
    fixed: bool,
}

impl Report {
    /// The report of a tab; it looks for an agent's draft on the agents' beat.
    fn new(frame: Rc<dyn Frame>, typing: Rc<Cell<bool>>, cx: &mut Context<Self>) -> Self {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(BEAT).await;
                if this.update(cx, |r, cx| r.take_drafts(cx)).is_err() {
                    return;
                }
            }
        })
        .detach();
        Self {
            frame,
            draft: Draft::default(),
            focus: Focus::Title,
            edit: TextEdit::default(),
            lines: Rc::default(),
            scroll: Rc::default(),
            picture: None,
            system: String::new(),
            sending: false,
            said: None,
            typing,
            sent: Vec::new(),
            ready: false,
        }
    }

    /// What the last send said; one that went ends the writing.
    fn say(&mut self, said: Option<Result<String, String>>) {
        self.typing.set(!matches!(said, Some(Ok(_))));
        self.said = said;
    }

    /// The field with the keyboard starts over: its caret at the end of its text.
    fn reset_edit(&mut self) {
        self.edit = TextEdit::default();
        self.scroll.set(0.);
    }

    /// The report gets ready to be written: the extras ticked, the system read, the window's
    /// picture taken now — before the report's own window is drawn over it.
    fn prepare(&mut self) {
        if !self.ready {
            self.ready = true;
            self.draft.system = true;
            self.draft.log = true;
            self.draft.picture = true;
            self.sent = crate::sent();
        }
        if self.system.is_empty() {
            self.system = crate::system();
        }
        if !self.sending && !matches!(self.said, Some(Ok(_))) {
            self.picture = platform::window_picture();
        }
        self.focus = Focus::Title;
        self.reset_edit();
    }

    /// Help → Report a problem, ⌘K: the tool window; with no repository open, GitHub's page.
    fn open(&mut self, cx: &mut Context<Self>) {
        if self.frame.places(cx).root.is_none() {
            self.prepare();
            return self.in_browser(cx);
        }
        self.frame.show_tool(Some(ID), cx);
    }

    /// An agent's draft (`report_problem`): it opens to be checked.
    fn take_drafts(&mut self, cx: &mut Context<Self>) {
        let Some(d) = artoo_agents_api::get(cx).take().pop() else {
            return;
        };
        self.prepare();
        self.say(None);
        self.draft.title = d.title;
        self.draft.description = d.description;
        self.draft.by = Some(d.by.clone());
        self.reset_edit();
        self.frame.toast(
            format!("{} drafted a report — check it, then send it", d.by).into(),
            cx,
        );
        if self.frame.places(cx).root.is_some() {
            self.frame.show_tool(Some(ID), cx);
        }
        cx.notify();
    }

    /// What a report must not carry here: the hosts of the remotes and of the tokens kept, the
    /// projects' names and paths.
    fn scrub(&self, cx: &App) -> Scrub {
        let places = self.frame.places(cx);
        let mut hosts = Vec::new();
        let mut projects = Vec::new();
        if let Some(web) = &places.web {
            hosts.extend(places.host.clone());
            if let Some((_, path)) = web
                .base
                .split_once("://")
                .and_then(|(_, rest)| rest.split_once('/'))
            {
                projects.push(path.to_owned());
                projects.extend(path.rsplit('/').next().map(str::to_owned));
            }
        }
        if let Some(dir) = std::env::home_dir().map(|h| h.join(".config/artoo/tokens"))
            && let Ok(entries) = std::fs::read_dir(dir)
        {
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                let host = name
                    .strip_prefix("https___")
                    .or_else(|| name.strip_prefix("http___"))
                    .unwrap_or(&name);
                hosts.push(host.split('_').next().unwrap_or(host).to_owned());
            }
        }
        for path in places
            .root
            .iter()
            .chain(places.project.iter())
            .chain(&places.recent)
        {
            projects.extend(path.file_name().map(|n| n.to_string_lossy().into_owned()));
        }
        Scrub::new(hosts, projects)
    }

    /// The issue's body as it goes.
    fn body(&self, cx: &App) -> String {
        let log = if self.draft.log {
            applog::tail(crate::LOG_LINES)
        } else {
            Vec::new()
        };
        crate::body(&self.draft, &self.scrub(cx), &self.system, &log)
    }

    fn title(&self, cx: &App) -> String {
        self.scrub(cx).apply(self.draft.title.trim())
    }

    /// ⌘↵: to the relay, or GitHub's page when there is none.
    fn send(&mut self, cx: &mut Context<Self>) {
        let title = self.title(cx);
        if title.is_empty() {
            self.say(Some(Err("Give it a title.".to_owned())));
            self.focus = Focus::Title;
            return cx.notify();
        }
        let Some(relay) = crate::relay() else {
            return self.in_browser(cx);
        };
        if self.sending {
            return;
        }
        let body = self.body(cx);
        let picture = self.picture.clone().filter(|_| self.draft.picture);
        let agent = self.draft.by.is_some();
        self.sending = true;
        self.say(None);
        cx.notify();
        cx.spawn(async move |this, cx| {
            let t = title.clone();
            let result = cx
                .background_executor()
                .spawn(async move { crate::send(&relay, &t, &body, agent, picture.as_deref()) })
                .await;
            this.update(cx, |r, cx| {
                r.sending = false;
                match result {
                    Ok(id) => {
                        r.sent.insert(
                            0,
                            Sent {
                                id: id.clone(),
                                at: now(),
                                title,
                                state: "received".to_owned(),
                                url: String::new(),
                            },
                        );
                        crate::keep_sent(&r.sent);
                        r.say(Some(Ok(id)));
                        r.draft.title.clear();
                        r.draft.description.clear();
                        r.draft.by = None;
                        r.picture = None;
                    }
                    Err(e) => {
                        applog::line(&format!("report: {e:#}"));
                        r.say(Some(Err(format!("{e:#}"))));
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// GitHub's new-issue page of the project, filled in; the picture can't go that way, so it
    /// waits in Downloads to be dragged in.
    fn in_browser(&mut self, cx: &mut Context<Self>) {
        let mut body = self.body(cx);
        if self.draft.picture
            && let Some(png) = &self.picture
            && let Some(file) = std::env::home_dir().map(|h| h.join("Downloads/artoo-report.png"))
            && std::fs::write(&file, png).is_ok()
        {
            body.push_str(
                "\n\n_A picture of the window is in Downloads as artoo-report.png — drag it here._",
            );
        }
        let title = self.title(cx);
        cx.open_url(&crate::new_issue_url(&title, &body));
    }

    /// What became of the reports sent from here that are still on their way.
    fn refresh_states(&mut self, cx: &mut Context<Self>) {
        let Some(relay) = crate::relay() else {
            return;
        };
        let open: Vec<String> = self
            .sent
            .iter()
            .filter(|s| s.url.is_empty() && s.state != "closed")
            .map(|s| s.id.clone())
            .take(10)
            .collect();
        if open.is_empty() {
            return;
        }
        cx.spawn(async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move {
                    open.into_iter()
                        .filter_map(|id| crate::status(&relay, &id).ok().map(|s| (id, s)))
                        .collect::<Vec<_>>()
                })
                .await;
            this.update(cx, |r, cx| {
                for (id, (state, url)) in read {
                    if let Some(s) = r.sent.iter_mut().find(|s| s.id == id) {
                        s.state = state;
                        s.url = url;
                    }
                }
                crate::keep_sent(&r.sent);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Typing the report: ⇥ between title and description, ↵ from the title to the description
    /// and a new line in it, ⌘↵ sends, esc closes the window (what was written stays). The shell
    /// calls this from its own update: what asks the tab waits until it is over.
    fn key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let key = &event.keystroke;
        if key.key == "enter" && key.modifiers.secondary() {
            let this = cx.entity().downgrade();
            return cx.defer(move |cx| {
                this.update(cx, |r, cx| r.send(cx)).ok();
            });
        }
        if key.key == "escape" {
            return self.frame.show_tool(None, cx);
        }
        if key.key == "tab" {
            self.focus = match self.focus {
                Focus::Title => Focus::Description,
                Focus::Description => Focus::Title,
            };
            self.reset_edit();
            return cx.notify();
        }
        if key.key == "enter" && self.focus == Focus::Description {
            input::sync(&self.draft.description, &mut self.edit);
            self.edit.insert("\n");
            self.draft.description = self.edit.text().to_owned();
            return cx.notify();
        }
        let done = match self.focus {
            Focus::Title => input::edit(&mut self.draft.title, &mut self.edit, key, cx),
            Focus::Description => {
                input::edit_area(&mut self.draft.description, &mut self.edit, key, cx)
            }
        };
        if let Edit::Enter { .. } = done {
            self.focus = Focus::Description;
            self.reset_edit();
        }
        if matches!(self.said, Some(Err(_))) {
            self.say(None);
        }
        cx.notify();
    }

    fn set_focus(&mut self, focus: Focus, cx: &mut Context<Self>) {
        if self.focus != focus {
            self.focus = focus;
            self.reset_edit();
        }
        self.frame.show_tool(Some(ID), cx);
        cx.notify();
    }

    /// The extras a report can carry, as ticked.
    fn extras(&self) -> Vec<Extra> {
        let lines = applog::tail(crate::LOG_LINES).len();
        vec![
            Extra {
                id: "system",
                what: if self.system.is_empty() {
                    "The system and the machine".to_owned()
                } else {
                    self.system.clone()
                },
                why: "the system and the machine",
                on: self.draft.system,
                fixed: false,
            },
            Extra {
                id: "log",
                what: match lines {
                    0 => "The log (nothing in it yet)".to_owned(),
                    n if n < crate::LOG_LINES => format!("The log, its {n} lines"),
                    n => format!("The last {n} lines of the log"),
                },
                why: "what the app met just before; paths and hosts taken out",
                on: self.draft.log && lines > 0,
                fixed: lines == 0,
            },
            Extra {
                id: "picture",
                what: if self.picture.is_some() {
                    "This window as a picture".to_owned()
                } else {
                    "This window as a picture (can't be taken here)".to_owned()
                },
                why: "the window only, as it was when this opened",
                on: self.draft.picture && self.picture.is_some(),
                fixed: self.picture.is_none(),
            },
        ]
    }

    fn toggle_extra(&mut self, id: &str, cx: &mut Context<Self>) {
        let d = &mut self.draft;
        match id {
            "system" => d.system = !d.system,
            "log" => d.log = !d.log,
            "picture" => d.picture = !d.picture,
            _ => {}
        }
        cx.notify();
    }

    /// A field's inside: its text as edited while it has the keyboard, else as it stands.
    fn field_look(
        &self,
        text: &str,
        active: bool,
        placeholder: &'static str,
        cx: &Context<Self>,
    ) -> gpui::Div {
        let look = Look {
            wrap: true,
            ..Default::default()
        };
        input::field(text, active, placeholder, &look, || {
            let this = cx.entity().downgrade();
            let shown = text.to_owned();
            // A click in it places the caret, ⇧ or a drag selecting, a double click taking the word.
            let place: input::Place = Rc::new(move |at, shift, clicks, drag, lower, cx| {
                this.update(cx, |r, cx| {
                    input::sync(&shown, &mut r.edit);
                    r.edit.click(at, clicks, shift || drag);
                    if clicks == 1 && !shift && !drag {
                        r.edit.set_downstream(lower);
                    }
                    cx.notify();
                })
                .ok();
            });
            let look = Look {
                lines: Some(self.lines.clone()),
                ..look.clone()
            };
            input::text_body(text, &self.edit, look, self.scroll.clone(), Some(place))
        })
    }

    /// The issue as it goes: its title, then its body as text.
    fn preview(&self, cx: &App) -> impl IntoElement + use<> {
        let picture = self.draft.picture && self.picture.is_some();
        let mut body = self.body(cx);
        if picture {
            body.push_str("\n\n+ the window's picture");
        }
        div()
            .flex()
            .flex_col()
            .rounded(px(ROW_RADIUS))
            .border_1()
            .border_color(theme::island_border())
            .bg(theme::panel())
            .child(
                div()
                    .px(px(10.))
                    .py(px(6.))
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(6.))
                    .border_b_1()
                    .border_color(theme::island_border())
                    .text_size(px(11.))
                    .text_color(theme::muted())
                    .child(icons::icon("eye").size(px(13.)).text_color(theme::muted()))
                    .child("The issue as it goes")
                    .child(
                        div()
                            .text_color(theme::faint())
                            .child("· paths ~/…, hosts <host>, projects <project>"),
                    ),
            )
            .child(
                div()
                    .px(px(10.))
                    .pt(px(8.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(match self.title(cx) {
                        t if t.is_empty() => "(no title yet)".to_owned(),
                        t => t,
                    }),
            )
            .child(
                div()
                    .px(px(10.))
                    .pt(px(6.))
                    .pb(px(10.))
                    .font_family(theme::code_font())
                    .text_size(px(11.))
                    .text_color(theme::muted())
                    .children(
                        body.lines()
                            .map(|l| div().min_h(px(15.)).child(SharedString::from(l.to_owned()))),
                    ),
            )
    }

    /// The reports sent from here, newest first, with what became of each.
    fn render_sent(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        div()
            .flex()
            .flex_col()
            .gap(px(2.))
            .child(
                div()
                    .text_size(px(12.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme::muted())
                    .child("Your reports"),
            )
            .children(self.sent.iter().take(20).enumerate().map(|(n, s)| {
                let url = s.url.clone();
                let state = match s.state.as_str() {
                    "" => "sent".to_owned(),
                    other => other.to_owned(),
                };
                let public = !url.is_empty();
                div()
                    .id(("report-sent", n))
                    .py(px(5.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .border_b_1()
                    .border_color(theme::island_border())
                    .text_size(px(12.))
                    .child(
                        div()
                            .flex_none()
                            .font_family(theme::code_font())
                            .text_color(theme::faint())
                            .child(s.id.clone()),
                    )
                    .child(
                        hover::cut(
                            SharedString::from(format!("report-title-{}", s.id)),
                            s.title.clone(),
                        )
                        .flex_1(),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_size(px(11.))
                            .text_color(theme::faint())
                            .child(format::ago(s.at)),
                    )
                    .child(
                        div()
                            .flex_none()
                            .px(px(6.))
                            .rounded_full()
                            .text_size(px(11.))
                            .bg(if public {
                                theme::info_bg()
                            } else {
                                theme::hover()
                            })
                            .text_color(if public {
                                theme::added()
                            } else {
                                theme::muted()
                            })
                            .child(if public {
                                format!("{state} ↗")
                            } else {
                                state
                            }),
                    )
                    .when(public, |s| {
                        s.cursor_pointer().hover(|s| s.bg(theme::hover())).on_click(
                            cx.listener(move |_, _: &ClickEvent, _, cx| cx.open_url(&url)),
                        )
                    })
            }))
    }
}

impl Render for Report {
    /// The tool window under its title: the body, which scrolls, and the foot while it is written.
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let typing = self.frame.tool_typing(ID, cx);
        let label = |text: &'static str| {
            div()
                .text_size(px(12.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme::muted())
                .child(text)
        };
        let field =
            |id: &'static str, focus: Focus, text: &str, placeholder: &'static str, tall: bool| {
                let active = typing && self.focus == focus;
                div()
                    .id(id)
                    .px(px(10.))
                    .py(px(6.))
                    .when(tall, |s| s.min_h(px(120.)))
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
                    .on_click(
                        cx.listener(move |this, _: &ClickEvent, _, cx| this.set_focus(focus, cx)),
                    )
                    .child(self.field_look(text, active, placeholder, cx))
            };
        let mut body: Vec<AnyElement> = Vec::new();
        if let Some(Ok(id)) = &self.said {
            body.push(note("check", theme::added(), format!("Sent — {id}")).into_any_element());
            body.push(
                div()
                    .child(
                        bar_button("report-another", "Write another", "", false).on_click(
                            cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.say(None);
                                this.prepare();
                                this.set_focus(Focus::Title, cx);
                            }),
                        ),
                    )
                    .into_any_element(),
            );
        } else {
            if let Some(by) = &self.draft.by {
                body.push(
                    note(
                        "bot",
                        theme::agent(),
                        format!("{by} drafted this from an error it hit."),
                    )
                    .into_any_element(),
                );
            }
            body.push(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(label("Title"))
                    .child(field(
                        "report-title",
                        Focus::Title,
                        &self.draft.title,
                        "In a few words",
                        false,
                    ))
                    .into_any_element(),
            );
            body.push(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(label("Description"))
                    .child(field(
                        "report-description",
                        Focus::Description,
                        &self.draft.description,
                        "What happened, and what you did just before",
                        true,
                    ))
                    .into_any_element(),
            );
            body.push(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(label("Sent with it"))
                    // The build always goes: said, not a box to tick (the developer, 2026-10-23).
                    .child(
                        div()
                            .py(px(4.))
                            .text_size(px(12.))
                            .child(crate::version())
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(theme::faint())
                                    .child("always — which build it is"),
                            ),
                    )
                    .children(self.extras().into_iter().map(|e| extra_row(e, cx)))
                    .into_any_element(),
            );
            body.push(self.preview(cx).into_any_element());
            if let Some(Err(e)) = &self.said {
                body.push(
                    div()
                        .text_size(px(12.))
                        .text_color(theme::removed())
                        .child(e.clone())
                        .into_any_element(),
                );
            }
        }
        if !self.sent.is_empty() {
            body.push(self.render_sent(cx).into_any_element());
        }
        let writing = !matches!(self.said, Some(Ok(_)));
        let relay = crate::relay().is_some();
        let foot = writing.then(|| {
            div()
                .flex()
                .flex_col()
                .gap(px(8.))
                .px(px(12.))
                .py(px(10.))
                .border_t_1()
                .border_color(theme::island_border())
                .child(div().text_size(px(11.)).text_color(theme::faint()).child(if relay {
                    "To the project's private reports."
                } else {
                    "The reports' relay is not set up yet: it opens GitHub's page of the project, filled in."
                }))
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap_2()
                        .when(relay, |s| {
                            s.child(
                                bar_button("report-browser", "Open in the browser", "", false)
                                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                        this.in_browser(cx)
                                    })),
                            )
                        })
                        .child(
                            bar_button(
                                "report-send",
                                match (relay, self.sending) {
                                    (_, true) => "Sending…",
                                    (true, false) => "Send",
                                    (false, false) => "Open in the browser",
                                },
                                "⌘↵",
                                true,
                            )
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.send(cx))),
                        ),
                )
        });
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(
                div()
                    .id("report-body")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap(px(14.))
                    .p(px(12.))
                    .children(body),
            )
            .children(foot.map(|foot| tool_foot().child(foot)))
    }
}

/// Seconds since the epoch: when a report was sent.
fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// A line of what is said about the report: its icon in `tone`, the text.
fn note(icon: &'static str, tone: gpui::Rgba, text: String) -> gpui::Div {
    div()
        .flex()
        .items_start()
        .gap(px(8.))
        .px(px(10.))
        .py(px(8.))
        .rounded(px(ROW_RADIUS))
        .border_l(px(3.))
        .border_color(tone)
        .bg(gpui::Rgba { a: 0.12, ..tone })
        .text_size(px(12.))
        .child(icons::icon(icon).size(px(14.)).mt(px(1.)).text_color(tone))
        .child(div().flex_1().min_w_0().child(text))
}

/// One extra with its box; one that cannot go is greyed.
fn extra_row(e: Extra, cx: &mut Context<Report>) -> impl IntoElement + use<> {
    let id = e.id;
    div()
        .id(SharedString::from(format!("report-extra-{id}")))
        .py(px(4.))
        .flex()
        .items_start()
        .gap(px(8.))
        .text_size(px(12.))
        .when(!e.fixed, |s| {
            s.cursor_pointer()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_extra(id, cx)))
        })
        .child(
            div()
                .flex_none()
                .mt(px(1.))
                .size(px(16.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(3.))
                .border_2()
                .border_color(if e.on {
                    theme::accent()
                } else {
                    theme::faint()
                })
                .when(e.on, |s| s.bg(theme::accent()))
                .when(e.fixed, |s| s.opacity(0.55))
                .text_size(px(10.))
                .font_weight(FontWeight::BOLD)
                .text_color(theme::base())
                .child(if e.on { "✓" } else { "" }),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(
                    div()
                        .text_color(if e.fixed && !e.on {
                            theme::faint()
                        } else {
                            theme::text()
                        })
                        .child(e.what),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(theme::faint())
                        .child(e.why),
                ),
        )
}
