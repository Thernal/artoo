//! Clone from a URL, a way into the welcome (#54): the URL typed, the folder it goes in asked, `git clone` there with
//! the user's own git, then the clone opened in the tab (`Frame::open_project`). Its field is one of the tab's inputs.

use artoo_shell_api::welcome::{Card, Field, Tone, Way};
use artoo_shell_api::{Frame, Input, Parts, Typed};
use artoo_ui_kit::input::{self, Area, Edit, Look};
use gpui::{App, AppContext, Context, Entity, IntoElement, Keystroke, PathPromptOptions, Window};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::rc::Rc;

/// The URL's field, one of the tab's inputs.
const INPUT: &str = "shell.clone-url";

/// The card's state in one tab.
struct Cloning {
    frame: Rc<dyn Frame>,
    open: bool,
    url: String,
    area: Area,
    /// What the last try said: where it is cloning, or why not.
    said: Option<Result<String, String>>,
}

impl Cloning {
    /// The card opened, the URL's field given the keyboard, its caret at the end of what was typed before.
    fn open_card(&mut self, cx: &mut Context<Self>) {
        self.open = true;
        self.said = None;
        self.area = Area::default();
        self.frame.take_keyboard(INPUT, cx);
        cx.notify();
    }

    /// The ways again, the URL kept.
    fn close(&mut self, cx: &mut Context<Self>) {
        self.open = false;
        self.said = None;
        self.frame.leave_keyboard(INPUT, cx);
        cx.notify();
    }

    /// A key while the field has the keyboard: ↵ clones, esc goes back to the ways.
    fn key(&mut self, key: &Keystroke, cx: &mut Context<Self>) -> Typed {
        match input::edit(&mut self.url, &mut self.area.edit, key, cx) {
            Edit::Escape => {
                self.open = false;
                self.said = None;
                cx.notify();
                return Typed::Leave;
            }
            Edit::Enter { .. } => self.clone_it(cx),
            Edit::Changed | Edit::Selected | Edit::Ignored => {}
        }
        cx.notify();
        Typed::Kept
    }

    /// The folder the clone goes in asked, then git clones it there and it opens.
    fn clone_it(&mut self, cx: &mut Context<Self>) {
        let url = self.url.trim().to_owned();
        let Some(name) = clone_name(&url) else {
            self.said = Some(Err("That does not look like a repository's URL.".to_owned()));
            return cx.notify();
        };
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Clone Here".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = picked.await else {
                return;
            };
            let Some(parent) = paths.into_iter().next() else {
                return;
            };
            let into: PathBuf = parent.join(&name);
            if into.exists() {
                this.update(cx, |this, cx| {
                    this.said = Some(Err(format!("{} is already there.", into.display())));
                    cx.notify();
                })
                .ok();
                return;
            }
            this.update(cx, |this, cx| {
                this.said = Some(Ok(format!("Cloning into {}…", into.display())));
                this.frame.leave_keyboard(INPUT, cx);
                cx.notify();
            })
            .ok();
            let target = into.clone();
            let result = cx
                .background_executor()
                .spawn(async move { clone(&url, &target) })
                .await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(()) => {
                        this.open = false;
                        this.url.clear();
                        this.said = None;
                        this.frame.open_project(into, cx);
                    }
                    Err(e) => this.said = Some(Err(e)),
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
        let active = typing == Some(INPUT);
        let this = cx.entity().downgrade();
        let (back, go) = (this.clone(), this.clone());
        Some(Card {
            title: "Clone from a URL",
            text: "",
            fields: vec![Field {
                label: "URL",
                link: None,
                id: "welcome-url",
                input: INPUT,
                active,
                inside: self
                    .area
                    .field_look_of(
                        &self.url,
                        active,
                        "git@gitlab.com:group/project.git · https://github.com/owner/repo",
                        Look::default(),
                        |c: &mut Cloning| Some(&mut c.area),
                        this,
                    )
                    .into_any_element(),
            }],
            said: match &self.said {
                Some(Ok(text)) => Some((text.clone(), Tone::Muted)),
                Some(Err(e)) => Some((e.clone(), Tone::Bad)),
                None => None,
            },
            back: Rc::new(move |_, cx| {
                back.update(cx, |c, cx| c.close(cx)).ok();
            }),
            go: (
                "welcome-go",
                "Clone…",
                Rc::new(move |_, cx| {
                    go.update(cx, |c, cx| c.clone_it(cx)).ok();
                }),
            ),
        })
    }
}

/// The folder a clone of `url` goes into: its last part without `.git`.
fn clone_name(url: &str) -> Option<String> {
    let last = url.trim().trim_end_matches('/').rsplit(['/', ':']).next()?;
    let name = last.strip_suffix(".git").unwrap_or(last);
    (!name.is_empty() && name != "." && name != "..").then(|| name.to_owned())
}

/// `git clone url` into `into`, with the user's own git (SSH agent, credential helpers); the error in a sentence.
fn clone(url: &str, into: &Path) -> Result<(), String> {
    let out = Command::new("git")
        .arg("clone")
        .arg("--quiet")
        .arg("--")
        .arg(url)
        .arg(into)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|_| "git is not installed".to_owned())?;
    if out.status.success() {
        return Ok(());
    }
    let err = String::from_utf8_lossy(&out.stderr);
    Err(err
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("git clone failed")
        .trim()
        .trim_start_matches("fatal: ")
        .to_owned())
}

/// The URL's field as the tab's input.
fn input(cloning: &Entity<Cloning>) -> Input {
    let (open, key, selected) = (
        cloning.downgrade(),
        cloning.downgrade(),
        cloning.downgrade(),
    );
    Input {
        id: INPUT,
        open: Rc::new(move |cx| open.upgrade().is_some_and(|c| c.read(cx).open)),
        key: Rc::new(move |event, _, cx| {
            key.update(cx, |c, cx| c.key(&event.keystroke, cx))
                .unwrap_or(Typed::Kept)
        }),
        // Going elsewhere keeps the card open, as esc does not.
        left: Rc::new(|_| {}),
        selected: Rc::new(move |cx| {
            let c = selected.upgrade()?;
            c.read(cx).area.edit.selected_text().map(str::to_owned)
        }),
        caret: Rc::new(|_| true),
    }
}

/// A tab's card: its way into the welcome, its field, and what a snapshot opens of it (`welcome-clone`).
pub(crate) fn parts(frame: Rc<dyn Frame>, _: &mut Window, cx: &mut App) -> Parts {
    let cloning = cx.new(|_| Cloning {
        frame: frame.clone(),
        open: false,
        url: String::new(),
        area: Area::default(),
        said: None,
    });
    // Drawn inside the tab: a change draws the tab again.
    let redraw = frame.clone();
    cx.observe(&cloning, move |_, cx| redraw.redraw(cx))
        .detach();
    // The way's card holds it: it lives as long as the tab's parts.
    let (go, card, snap) = (cloning.downgrade(), cloning.clone(), cloning.downgrade());
    Parts {
        inputs: vec![input(&cloning)],
        ways: vec![Way {
            id: "welcome-clone",
            icon: "files",
            title: "Clone from a URL",
            text: "git clone, then open it",
            order: 2,
            go: Rc::new(move |_, cx| {
                go.update(cx, |c, cx| c.open_card(cx)).ok();
            }),
            card: Rc::new(move |typing, cx| card.update(cx, |c, cx| c.card(typing, cx))),
        }],
        snapshots: vec![(
            "welcome-clone",
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
    fn a_clone_is_named_after_its_url() {
        assert_eq!(
            clone_name("git@gitlab.com:group/payments.git").as_deref(),
            Some("payments")
        );
        assert_eq!(
            clone_name("https://github.com/owner/repo/").as_deref(),
            Some("repo")
        );
        assert_eq!(clone_name("git@host:repo.git").as_deref(), Some("repo"));
        assert_eq!(clone_name(""), None);
    }

    #[test]
    fn a_clone_lands_where_asked_and_a_bad_url_says_why() {
        let dir = std::env::temp_dir().join(format!("artoo-clone-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let origin = dir.join("origin.git");
        let made = Command::new("git")
            .args(["init", "--quiet", "--bare"])
            .arg(&origin)
            .status()
            .unwrap();
        assert!(made.success());
        let into = dir.join("work");
        clone(origin.to_str().unwrap(), &into).unwrap();
        assert!(into.join(".git").is_dir());
        let err = clone(dir.join("none").to_str().unwrap(), &dir.join("x"));
        std::fs::remove_dir_all(&dir).ok();
        assert!(err.is_err_and(|e| !e.is_empty() && !e.starts_with("fatal:")));
    }
}
