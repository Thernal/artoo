//! Settings → Accounts: a row a host the projects use — the open one and those opened before — with the project it
//! is used by (pinned when this one), the way its token came in, whether it is in use and Test. One row opens at a
//! time, onto that host's ways in (`crate::panel`).

use crate::git::{Repo, WebRemote};
use crate::panel::{self, Fields, NO_TOKEN, Panel};
use artoo_hosts_api::Host;
use artoo_hosts_api::connect::Here;
use artoo_shell_api::Frame;
use artoo_ui_kit::{controls, hover, icons, px, theme};
use gpui::{
    AnyElement, App, AppContext, ClickEvent, Context, Entity, SharedString, Window, div, prelude::*,
};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

/// A project's repository on a host.
struct Used {
    root: PathBuf,
    project: String,
    web: WebRemote,
    /// `https://host`: what tells two hosts apart.
    key: String,
}

/// One row: a host and the projects that use it, the open one's first.
#[derive(Clone, Debug, PartialEq)]
struct Account {
    key: String,
    web: WebRemote,
    /// The first project's repository, where its ways in are looked for.
    root: PathBuf,
    projects: Vec<String>,
}

/// `used`, in its order, as a row a host.
fn grouped(used: Vec<Used>) -> Vec<Account> {
    let mut out: Vec<Account> = Vec::new();
    for u in used {
        match out.iter_mut().find(|a| a.key == u.key) {
            Some(a) if !a.projects.contains(&u.project) => a.projects.push(u.project),
            Some(_) => {}
            None => out.push(Account {
                key: u.key,
                web: u.web,
                root: u.root,
                projects: vec![u.project],
            }),
        }
    }
    out
}

/// A repository's project: the name of its main working tree.
fn project_name(root: &Path) -> String {
    root.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The hosts of the repositories at `paths`, those with no web remote left out. Reads each one: off the window's
/// thread.
fn listed(paths: Vec<PathBuf>) -> Vec<Account> {
    grouped(
        paths
            .into_iter()
            .filter_map(|root| {
                let repo = Repo::open(&root).ok()?;
                let web = repo.web_remote()?;
                Some(Used {
                    project: project_name(&repo.main_root()),
                    key: crate::hosts::of(&web).api_host(),
                    web,
                    root,
                })
            })
            .collect(),
    )
}

/// The account of what `me` answers: `@ada` of `Ada Byron (@ada)`, else the whole of it.
fn handle(who: &str) -> &str {
    who.rsplit_once('(')
        .and_then(|(_, rest)| rest.strip_suffix(')'))
        .filter(|h| h.starts_with('@'))
        .unwrap_or(who)
}

/// A tab's Settings → Accounts.
pub(crate) struct Accounts {
    frame: Rc<dyn Frame>,
    /// The tab's connect panel: what the tab asked to be told is told by its host's row too.
    tab: Entity<Panel>,
    fields: Fields,
    rows: Vec<Account>,
    /// The repositories `rows` were read from; `None`, to be read again.
    read: Option<Vec<PathBuf>>,
    /// The rows are in: an empty list says so.
    loaded: bool,
    /// The hosts whose token has been looked at, and who each said it is.
    looked: HashSet<String>,
    users: HashMap<String, String>,
    /// What Test said of a host.
    tested: HashMap<String, Result<String, String>>,
    open: Option<String>,
    panels: HashMap<String, Entity<Panel>>,
}

impl Accounts {
    pub(crate) fn new(frame: Rc<dyn Frame>, tab: Entity<Panel>, fields: Fields) -> Self {
        Self {
            frame,
            tab,
            fields,
            rows: Vec::new(),
            read: None,
            loaded: false,
            looked: HashSet::new(),
            users: HashMap::new(),
            tested: HashMap::new(),
            open: None,
            panels: HashMap::new(),
        }
    }

    /// The section opened: the hosts and their tokens are read anew when drawn.
    pub(crate) fn read_again(&mut self, cx: &mut Context<Self>) {
        self.read = None;
        self.tested.clear();
        cx.notify();
    }

    /// A snapshot's: the hosts read at once and the open repository's row opened, `pick` its way picked — before
    /// the one frame a snapshot draws.
    pub(crate) fn snapshot_open(&mut self, pick: Option<&'static str>, cx: &mut Context<Self>) {
        let this = cx.weak_entity();
        // Asked inside the tab's update: where the tab is, read once it is over.
        cx.defer(move |cx| {
            this.update(cx, |this, cx| {
                let paths = this.paths(cx);
                this.show(listed(paths.clone()), cx);
                this.read = Some(paths);
                let Some(key) = this.rows.first().map(|r| r.key.clone()) else {
                    return;
                };
                this.open = None;
                this.toggle(&key, cx);
                if let (Some(pick), Some(panel)) = (pick, this.panels.get(&key)) {
                    panel.update(cx, |p, _| p.pick(pick));
                }
            })
            .ok();
        });
    }

    /// The open repository first, then those opened before.
    fn paths(&self, cx: &App) -> Vec<PathBuf> {
        let places = self.frame.places(cx);
        let root = places.root;
        let before = places
            .recent
            .into_iter()
            .filter(|p| Some(p) != root.as_ref());
        root.iter().cloned().chain(before).collect()
    }

    fn show(&mut self, rows: Vec<Account>, cx: &mut Context<Self>) {
        for row in &rows {
            self.look_at(row, cx);
        }
        self.rows = rows;
        self.loaded = true;
        cx.notify();
    }

    /// Reads the hosts of `paths`, then each host's token and whose it is, off the window's thread.
    fn read(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        self.read = Some(paths.clone());
        cx.spawn(async move |this, cx| {
            let rows = cx
                .background_executor()
                .spawn(async move { listed(paths) })
                .await;
            this.update(cx, |this, cx| this.show(rows, cx)).ok();
        })
        .detach();
    }

    /// The row's token read — the way picked may borrow one, a process — and the host asked whose it is.
    fn look_at(&mut self, row: &Account, cx: &mut Context<Self>) {
        let (key, web, root) = (row.key.clone(), row.web.clone(), row.root.clone());
        cx.spawn(async move |this, cx| {
            let who = cx
                .background_executor()
                .spawn(async move {
                    let url = Repo::open(&root).ok().and_then(|r| r.web_remote_url());
                    crate::connect::borrow(&root, &url.unwrap_or_default(), &web);
                    let host = crate::hosts::of(&web);
                    host.token()?;
                    host.me().ok()
                })
                .await;
            this.update(cx, |this, cx| {
                if let Some(who) = who {
                    this.users.insert(key.clone(), who);
                }
                this.looked.insert(key);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Test: asks the host who the token belongs to.
    fn test(&mut self, key: String, host: Arc<dyn Host>, cx: &mut Context<Self>) {
        self.tested.remove(&key);
        cx.spawn(async move |this, cx| {
            let said = cx
                .background_executor()
                .spawn(async move { host.me() })
                .await;
            this.update(cx, |this, cx| {
                if let Ok(who) = &said {
                    this.users.insert(key.clone(), who.clone());
                }
                this.tested.insert(key, said.map_err(|e| format!("{e:#}")));
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Opens the row of `key` — the only one open — or closes it.
    fn toggle(&mut self, key: &str, cx: &mut Context<Self>) {
        let open = self.open.as_deref() != Some(key);
        self.open = open.then(|| key.to_owned());
        *self.fields.open.borrow_mut() = None;
        self.tested.clear();
        if open && let Some(row) = self.rows.iter().find(|r| r.key == key).cloned() {
            let panel = self.panel_of(key, cx);
            let here = Here {
                root: Some(row.root),
                web: Some(row.web),
                typing: false,
            };
            panel.update(cx, |p, cx| {
                p.reopen();
                p.at(here, cx);
                p.look_for_token(cx);
            });
            *self.fields.open.borrow_mut() = Some(panel);
        }
        cx.notify();
    }

    fn panel_of(&mut self, key: &str, cx: &mut Context<Self>) -> Entity<Panel> {
        if let Some(panel) = self.panels.get(key) {
            return panel.clone();
        }
        let frame = self.frame.clone();
        let panel = cx.new(|_| Panel::in_settings(frame));
        let of = key.to_owned();
        cx.observe(&panel, move |this, panel, cx| {
            // Whose the token is, as a way just connected says.
            if let Some(who) = panel.read(cx).who() {
                this.users.insert(of.clone(), who.to_owned());
            }
            this.frame.redraw(cx);
            cx.notify();
        })
        .detach();
        self.panels.insert(key.to_owned(), panel.clone());
        panel
    }

    /// The open row's ways in.
    fn ways(&mut self, row: &Account, cx: &mut Context<Self>) -> Option<AnyElement> {
        let panel = self.panel_of(&row.key, cx);
        let here = panel::here(&*self.frame, cx);
        // The tab is told when its own host connects: it reads the requests again and says so.
        let own = here
            .web
            .as_ref()
            .is_some_and(|web| crate::hosts::of(web).api_host() == row.key);
        let done = own.then(|| self.tab.read(cx).done.clone()).flatten();
        self.fields.settings_drawn.set(true);
        let here = Here {
            root: Some(row.root.clone()),
            web: Some(row.web.clone()),
            typing: here.typing,
        };
        panel.update(cx, |p, cx| {
            p.done = done;
            p.at(here, cx);
            p.render_ways_settings(cx)
        })
    }

    fn row(&mut self, i: usize, current: Option<&str>, cx: &mut Context<Self>) -> AnyElement {
        let row = self.rows[i].clone();
        let host = crate::hosts::of(&row.web);
        let open = self.open.as_deref() == Some(row.key.as_str());
        let source = host.token_source().map(|(_, from)| from);
        let who = self.users.get(&row.key).cloned();
        let here = current.is_some_and(|p| row.projects.iter().any(|n| n == p));
        let project = row.projects.first().cloned().unwrap_or_default();
        let all = row.projects.join(", ");
        let second = match source {
            Some(from) => Some(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .text_color(theme::faint())
                    .children(panel::way_name(&host, from))
                    .children(who.map(|w| handle(&w).to_owned()))
                    .into_any_element(),
            ),
            // Not said before the token has been looked for.
            None => self.looked.contains(&row.key).then(|| {
                panel::tipped(
                    div()
                        .id(("account-no-token", i))
                        .flex()
                        .items_center()
                        .gap(px(4.))
                        .text_color(theme::warning())
                        .child(
                            icons::icon("triangle-alert")
                                .size(px(12.))
                                .text_color(theme::warning()),
                        )
                        .child(NO_TOKEN),
                    "Merge requests stay hidden until a token is in use".to_owned(),
                )
                .into_any_element()
            }),
        };
        let none = source.is_none() && self.looked.contains(&row.key);
        let label = match (open, none) {
            (true, _) => "Close",
            (false, true) => "Sign in",
            (false, false) => "Change",
        };
        let said = self.tested.get(&row.key).cloned();
        let (key, test_key) = (row.key.clone(), row.key.clone());
        let head = div()
            .flex()
            .items_center()
            .gap(px(12.))
            .min_h(px(44.))
            .py(px(6.))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .min_w_0()
                            .child(hover::name(("account-host", i), host.host()).text_size(px(13.)))
                            .child(panel::tipped(
                                div()
                                    .id(("account-projects", i))
                                    .flex_none()
                                    .flex()
                                    .items_center()
                                    .gap(px(4.))
                                    .text_size(px(11.))
                                    .text_color(theme::faint())
                                    .children(here.then(|| {
                                        icons::icon("pin").size(px(12.)).text_color(theme::faint())
                                    }))
                                    .child(SharedString::from(project)),
                                if here {
                                    format!("This repository uses it · {all}")
                                } else {
                                    all
                                },
                            )),
                    )
                    .child(div().h(px(16.)).text_size(px(11.)).children(second)),
            )
            .when(source.is_some(), |d| {
                d.child(panel::in_use_mark(("account-in-use", i)))
                    .child(panel::tipped(
                        controls::row_icon_button(("account-test", i), "activity").on_click(
                            cx.listener(move |this, _: &ClickEvent, _, cx| {
                                let host = this.rows.iter().find(|r| r.key == test_key);
                                if let Some(host) = host.map(|r| crate::hosts::of(&r.web)) {
                                    this.test(test_key.clone(), host, cx);
                                }
                            }),
                        ),
                        "Test the token".to_owned(),
                    ))
            })
            .child(
                controls::row_button(("account-open", i), label).on_click(cx.listener(
                    move |this, _: &ClickEvent, _, cx| {
                        this.toggle(&key, cx);
                    },
                )),
            );
        let ways = open.then(|| self.ways(&row, cx)).flatten();
        div()
            .flex()
            .flex_col()
            .border_b_1()
            .border_color(theme::island_border())
            .child(head)
            .children(said.map(|said| {
                let (text, color) = match said {
                    Ok(who) => (format!("Connected as {who}"), theme::added()),
                    Err(why) => (why, theme::removed()),
                };
                div()
                    .pb(px(8.))
                    .text_size(px(12.))
                    .text_color(color)
                    .child(text)
            }))
            .children(ways.map(|ways| div().pb(px(10.)).child(ways)))
            .into_any_element()
    }
}

impl Render for Accounts {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let paths = self.paths(cx);
        if self.read.as_ref() != Some(&paths) {
            self.read(paths, cx);
        }
        let places = self.frame.places(cx);
        let current = places
            .project
            .or(places.root)
            .map(|root| project_name(&root));
        let empty = (self.loaded && self.rows.is_empty()).then(|| {
            div()
                .text_size(px(12.))
                .text_color(theme::muted())
                .child("No project here has a GitLab or GitHub remote, so there are no merge requests to set up.")
        });
        let rows: Vec<AnyElement> = (0..self.rows.len())
            .map(|i| self.row(i, current.as_deref(), cx))
            .collect();
        div()
            .w_full()
            .flex()
            .flex_col()
            .children(empty)
            .children(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn used(project: &str, host: &str) -> Used {
        Used {
            root: PathBuf::from(project),
            project: project.to_owned(),
            web: WebRemote {
                base: format!("{host}/group/{project}"),
                github: false,
            },
            key: host.to_owned(),
        }
    }

    #[test]
    fn a_host_is_one_row_with_the_open_projects_first() {
        let rows = grouped(vec![
            used("ledger-api", "https://gitlab.example.com"),
            used("partner-api", "https://gitlab.partners.example"),
            used("payments", "https://gitlab.example.com"),
            used("ledger-api", "https://gitlab.example.com"),
        ]);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].projects, ["ledger-api", "payments"]);
        assert_eq!(rows[0].root, PathBuf::from("ledger-api"));
        assert_eq!(rows[1].projects, ["partner-api"]);
    }

    #[test]
    fn the_row_says_the_account_alone() {
        assert_eq!(handle("Ada Byron (@ada)"), "@ada");
        assert_eq!(handle("@ada"), "@ada");
        assert_eq!(handle("Fixture"), "Fixture");
    }
}
