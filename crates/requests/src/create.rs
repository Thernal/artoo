//! Creating a merge request from a branch (Design #47 E, `docs/specs/new-request.md`, flow 08): the
//! request is written in place of the diff while the branch's commits and changes stay on the left.
//! From → Into, the target picked from the branch list; the description starts from the project's
//! template on the target, else the project's template setting, else a single commit's body, and
//! Templates ▾ offers the others; whether the pair merges cleanly is worked out in memory; the code
//! owners of the changed files start as the reviewers, and Reviewers searches the project's members for more (#190);
//! with nothing to merge, Create says so and cannot be pressed; labels and a milestone are picked from the
//! project's lists; Request | Changes (⌃⇥) shows the diff and keeps the draft. ⌘K's Create merge request… opens
//! this form, the one way in (#190: the quick way is gone). The one outward
//! action besides posting comments; it never pushes, so a branch only on this Mac is told to be
//! pushed first. The form keeps its fields' caret itself, and its field is one of the requests' inputs (`input`);
//! the tab opens it on a branch, gives it the keyboard, shows the pair's diff (`Event::Create`) and hears the
//! request was created (`Event::Created`).

use crate::requests::{Act, Acts, Event, Requests};
use artoo_branches_api::Branches;
use artoo_core::codeowners;
use artoo_core::git::{BranchRef, RefKind, Repo};
use artoo_hosts_api::{Host, Label, Milestone, NewOptions};
use artoo_shell_api::{Input, Services, Typed};
use artoo_ui_kit::complete::{list_key, render_list};
use artoo_ui_kit::controls::chip;
use artoo_ui_kit::format::plural;
use artoo_ui_kit::input::Area;
use artoo_ui_kit::input::{self, Edit};
use artoo_ui_kit::markdown::{self, Selecting};
use artoo_ui_kit::room::Widths;
use artoo_ui_kit::{ROW_RADIUS, discussion, hover, island, keys, motion, px, theme, words};
use git2::Oid;
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, FontWeight, KeyDownEvent, div, prelude::*,
};
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Focus {
    /// From: another branch can be picked (#159).
    Source,
    Target,
    Title,
    Description,
    /// Reviewers: what is typed searches the project's members (#190).
    Reviewers,
}

/// A list open under its field.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Menu {
    Templates,
    Labels,
    Milestone,
}

pub struct NewRequest {
    source: String,
    /// The branch is not on origin, so the host cannot open a request from it.
    local_only: bool,
    target: String,
    title: String,
    description: String,
    /// Where the description started from: "Default.md template", "the commit's message".
    started_from: Option<String>,
    /// The description as the form filled it: a template read later replaces it only while it
    /// is unchanged.
    started: String,
    /// Every template on offer (name, text), the default first.
    templates: Vec<(String, String)>,
    /// The labels chosen, and the milestone (its id).
    labels: Vec<String>,
    milestone: Option<u64>,
    /// The project's labels and milestones, read as the form opens; `lists_read` once they are.
    all_labels: Vec<Label>,
    milestones: Vec<Milestone>,
    lists_read: bool,
    menu: Option<Menu>,
    /// The files merging into the target would leave in conflict; `None` while not known.
    conflicts: Option<Vec<String>>,
    /// Code owners of the changed files; they start as the reviewers.
    suggested: Vec<String>,
    /// The reviewers were changed by hand: another target no longer replaces them.
    reviewers_touched: bool,
    draft: bool,
    delete_branch: bool,
    squash: bool,
    focus: Focus,
    /// The cursor of From's or Into's list while that field has the keyboard.
    target_at: usize,
    /// That list's scroll, so the cursor stays in sight.
    list_scroll: gpui::ScrollHandle,
    /// The request that already exists for this pair.
    existing: Option<u64>,
    commits: usize,
    /// The reviewers chosen, by username.
    reviewers: Vec<String>,
    /// What is typed under Reviewers, and the member picked in its list.
    reviewer_query: String,
    reviewer_at: usize,
    /// Both branches are known and the source has nothing the target lacks: Create cannot be pressed.
    nothing: bool,
    /// The description as it will read, instead of its text.
    preview: bool,
    /// Request | Changes: the diff of the pair is shown, the draft kept.
    changes: bool,
    error: Option<String>,
    posting: bool,
    /// The focused field's caret, its layout as drawn and its scroll.
    area: Area,
    /// The description's completion list: the row picked, and where esc closed it.
    pick: usize,
    closed: Option<usize>,
    /// From's and Into's widths, which their lists take.
    widths: Widths,
    /// The repository's folder and whether its host is GitHub, as it was opened with (`Here`).
    root: std::path::PathBuf,
    pub(crate) github: bool,
}

impl NewRequest {
    /// Another field has the keyboard: its caret starts at the end of its text.
    fn reset_field(&mut self) {
        self.area = Area::new(String::new());
        self.pick = 0;
        self.closed = None;
    }

    /// The description has the keyboard: ↑ ↓ move between its lines.
    fn typing_description(&self) -> bool {
        self.focus == Focus::Description && !self.changes && !self.preview
    }
}

/// What the form reads of the repository, which the tab knows: its folder, its branches, whether
/// the host is GitHub.
pub struct Here<'a> {
    pub root: &'a Path,
    pub branches: &'a [BranchRef],
    pub github: bool,
}

pub use artoo_requests_api::{CreateKey, Drafted, INPUT, Pair};

/// The repository the form is written in: its folder and whether its host is GitHub.
pub struct Repository {
    pub root: std::path::PathBuf,
    pub github: bool,
}

impl Repository {
    /// What the form reads, with the repository's `branches`.
    pub fn of<'a>(&'a self, branches: &'a [BranchRef]) -> Here<'a> {
        Here {
            root: &self.root,
            branches,
            github: self.github,
        }
    }
}

/// The pair's diff as the tab shows it: its files, the lines added and removed.
#[derive(Clone, Copy)]
pub struct Totals {
    pub files: usize,
    pub added: usize,
    pub removed: usize,
}

impl Totals {
    /// The totals of a comparison's diff; none of any other.
    pub fn of(d: &artoo_review_api::diff::Diff) -> Option<Totals> {
        if !matches!(d.header, artoo_review_api::diff::Header::Compare { .. }) {
            return None;
        }
        let (added, removed) = d
            .files
            .iter()
            .fold((0, 0), |(a, r), f| (a + f.added, r + f.removed));
        Some(Totals {
            files: d.files.len(),
            added,
            removed,
        })
    }
}

/// What the form is drawn with that the tab knows.
pub struct CreateLook<'a> {
    /// The fields show their caret only while the form has the keyboard (#168).
    pub typing: bool,
    pub github: bool,
    /// The project's web address, which relative links in the description's preview are relative to.
    pub base: Option<String>,
    /// The preview's text selection.
    pub selecting: Option<Selecting>,
    pub branches: &'a [BranchRef],
}

/// The form's field, found again when a click lands.
fn area_of(rq: &mut Requests) -> Option<&mut Area> {
    rq.creating.as_mut().map(|n| &mut n.area)
}

pub use artoo_core::mr::source_of;

/// The tip of `name` on origin (`origin/name`).
fn remote_tip(branches: &[BranchRef], name: &str) -> Option<Oid> {
    branches
        .iter()
        .find(|b| {
            b.kind == RefKind::Remote && b.name.split_once('/').is_some_and(|(_, s)| s == name)
        })
        .map(|b| b.tip)
}

/// The templates a new request can start from, read from the target with `file`/`files`, the
/// default first: GitLab's `.gitlab/merge_request_templates/*.md` (`Default.md` in any case is the
/// default); GitHub's `pull_request_template.md` in `.github/`, `docs/` or the root, then the files
/// of `.github/PULL_REQUEST_TEMPLATE/`. Each with its name and text.
pub fn templates(
    github: bool,
    file: impl Fn(&str) -> Option<String>,
    files: impl Fn(&str) -> Vec<String>,
) -> Vec<(String, String)> {
    let read = |dir: &str, name: String| {
        let path = if dir.is_empty() {
            name.clone()
        } else {
            format!("{dir}/{name}")
        };
        file(&path).map(|t| (name, t))
    };
    let markdown = |n: &String| n.to_lowercase().ends_with(".md");
    let mut out = Vec::new();
    if github {
        if let Some(found) = [".github", "docs", ""].into_iter().find_map(|dir| {
            files(dir)
                .into_iter()
                .find(|n| n.eq_ignore_ascii_case("pull_request_template.md"))
                .and_then(|name| read(dir, name))
        }) {
            out.push(found);
        }
        let dir = ".github/PULL_REQUEST_TEMPLATE";
        let mut names: Vec<String> = files(dir).into_iter().filter(markdown).collect();
        names.sort_by_key(|n| n.to_lowercase());
        out.extend(names.into_iter().filter_map(|n| read(dir, n)));
        return out;
    }
    let dir = ".gitlab/merge_request_templates";
    let mut names: Vec<String> = files(dir).into_iter().filter(markdown).collect();
    names.sort_by_key(|n| (!n.eq_ignore_ascii_case("default.md"), n.to_lowercase()));
    out.extend(names.into_iter().filter_map(|n| read(dir, n)));
    out
}

/// A template's name as the chip says it: `Default.md` without its extension.
fn template_name(name: &str) -> &str {
    name.strip_suffix(".md")
        .or_else(|| name.strip_suffix(".MD"))
        .unwrap_or(name)
}

impl Requests {
    /// Opens the form for `branch`, from what an agent drafted for it (#152) when it did; the
    /// pair whose diff the tab shows.
    pub fn open_create(
        &mut self,
        here: &Here,
        branch: &BranchRef,
        drafted: Option<Drafted>,
        host: Arc<dyn Host>,
        cx: &mut Context<Self>,
    ) -> Option<Pair> {
        let source = source_of(branch);
        let tip = branch.tip;
        let on_origin = here.branches.iter().any(|b| {
            b.kind == RefKind::Remote && b.name.split_once('/').is_some_and(|(_, n)| n == source)
        });
        let target = drafted
            .as_ref()
            .map(|d| d.target.clone())
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| {
                ["main", "master"]
                    .into_iter()
                    .find(|t| remote_tip(here.branches, t).is_some())
                    .unwrap_or("main")
                    .to_owned()
            });
        let repo = Repo::open(here.root).ok();
        let commit = repo.as_ref().and_then(|r| r.commit(tip).ok());
        let title = commit
            .as_ref()
            .map(|c| c.summary.clone())
            .unwrap_or_default();
        self.creating = Some(NewRequest {
            source,
            local_only: !on_origin,
            target,
            title,
            description: String::new(),
            started_from: None,
            started: String::new(),
            templates: Vec::new(),
            labels: Vec::new(),
            milestone: None,
            all_labels: Vec::new(),
            milestones: Vec::new(),
            lists_read: false,
            menu: None,
            conflicts: None,
            suggested: Vec::new(),
            reviewers_touched: false,
            draft: false,
            delete_branch: true,
            squash: false,
            focus: Focus::Title,
            target_at: 0,
            list_scroll: gpui::ScrollHandle::new(),
            existing: None,
            commits: 0,
            reviewers: Vec::new(),
            reviewer_query: String::new(),
            reviewer_at: 0,
            nothing: false,
            preview: false,
            changes: false,
            error: None,
            posting: false,
            area: Area::new(String::new()),
            pick: 0,
            closed: None,
            widths: Widths::default(),
            root: here.root.to_owned(),
            github: here.github,
        });
        let pair = self.refresh_create(here);
        // The description: the target's template, else the one commit's message body.
        let target_tip = self
            .creating
            .as_ref()
            .and_then(|n| remote_tip(here.branches, &n.target));
        if let Some(n) = self.creating.as_mut() {
            n.templates = match (&repo, target_tip) {
                (Some(r), Some(t)) => {
                    templates(here.github, |p| r.file_at(t, p), |d| r.files_at(t, d))
                }
                _ => Vec::new(),
            };
            if let Some((name, text)) = n.templates.first() {
                n.description = text.clone();
                n.started_from = Some(format!("from {} template", template_name(name)));
            } else if n.commits == 1
                && let Some(body) = commit
                    .as_ref()
                    .and_then(|c| c.message.split_once('\n'))
                    .map(|(_, b)| b.trim().to_owned())
                    .filter(|b| !b.is_empty())
            {
                n.description = body;
                n.started_from = Some("from the commit's message".to_owned());
            }
            n.started = n.description.clone();
            // The agent's words over the template's; `started` stays the template, so a template
            // read later leaves them.
            if let Some(d) = drafted {
                if !d.title.is_empty() {
                    n.title = d.title;
                }
                if !d.description.is_empty() {
                    n.description = d.description;
                    n.started_from = Some(format!("drafted by {}", d.agent));
                }
                n.labels = d.labels;
                if !d.reviewers.is_empty() {
                    n.reviewers = d.reviewers;
                    n.reviewers_touched = true;
                }
            }
        }
        self.read_create_lists(host, cx);
        cx.notify();
        pair
    }

    /// The project's labels, milestones and template setting, in the background. The template
    /// setting comes after a template file and before a commit's message.
    fn read_create_lists(&mut self, host: Arc<dyn Host>, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let (labels, milestones, template) = cx
                .background_executor()
                .spawn(async move {
                    (
                        host.labels().unwrap_or_default(),
                        host.milestones().unwrap_or_default(),
                        host.project_template().ok().flatten(),
                    )
                })
                .await;
            this.update(cx, |this, cx| {
                let Some(n) = this.creating.as_mut() else {
                    return;
                };
                n.all_labels = labels;
                n.milestones = milestones;
                n.lists_read = true;
                if let Some(text) = template {
                    let had_file = !n.templates.is_empty();
                    n.templates
                        .push(("Project settings".to_owned(), text.clone()));
                    if !had_file && n.description == n.started {
                        n.description = text;
                        n.started = n.description.clone();
                        n.started_from = Some("from the project's template".to_owned());
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Templates ▾: the description becomes template `ix`.
    /// What the form reads of the repository: its folder and its host (`Repository::of` adds the branches).
    pub fn here(&self) -> Option<Repository> {
        Some(Repository {
            root: self.root.clone()?,
            github: self.web.as_ref().is_some_and(|w| w.github),
        })
    }

    pub fn use_template(&mut self, ix: usize) {
        if let Some(n) = self.creating.as_mut()
            && let Some((name, text)) = n.templates.get(ix).cloned()
        {
            n.description = text;
            n.started = n.description.clone();
            n.started_from = Some(if name == "Project settings" {
                "from the project's template".to_owned()
            } else {
                format!("from {} template", template_name(&name))
            });
            n.menu = None;
            n.reset_field();
        }
    }

    /// A snapshot's way in: `create-templates`, `create-labels`, `create-milestone`.
    pub fn open_create_menu(&mut self, name: &str, cx: &mut Context<Self>) {
        let menu = match name {
            "create-templates" => Menu::Templates,
            "create-labels" => Menu::Labels,
            "create-milestone" => Menu::Milestone,
            _ => return,
        };
        self.toggle_create_menu(menu, cx);
    }

    fn toggle_create_menu(&mut self, menu: Menu, cx: &mut Context<Self>) {
        if let Some(n) = self.creating.as_mut() {
            n.menu = (n.menu != Some(menu)).then_some(menu);
        }
        cx.notify();
    }

    /// The request is written in the diff's place.
    pub fn create_in_place(&self) -> bool {
        self.creating.is_some()
    }

    /// Recomputes what the form says about the pair: commits, an existing request, the owners;
    /// the pair, whose diff the tab shows.
    fn refresh_create(&mut self, here: &Here) -> Option<Pair> {
        let n = self.creating.as_ref()?;
        let find = |name: &str| {
            here.branches
                .iter()
                .find(|b| {
                    b.kind == RefKind::Remote
                        && b.name.split_once('/').is_some_and(|(_, s)| s == name)
                })
                .or_else(|| {
                    here.branches
                        .iter()
                        .find(|b| b.kind == RefKind::Local && b.name == name)
                })
                .map(|b| (b.name.clone(), b.tip))
        };
        let (source, target) = (find(&n.source), find(&n.target));
        let existing = self
            .list
            .iter()
            .find(|m| m.source == n.source && m.target == n.target)
            .map(|m| m.iid);
        let repo = Repo::open(here.root).ok();
        let log = match (&repo, &source, &target) {
            (Some(r), Some(s), Some(t)) => r.range_log(t.1, s.1, true).unwrap_or_default(),
            _ => Vec::new(),
        };
        let commits = log.len();
        let conflicts = match (&repo, &source, &target) {
            (Some(r), Some(s), Some(t)) => r.conflicts(t.1, s.1),
            _ => None,
        };
        // The files the request changes, commit by commit: their code owners review it.
        let ids: Vec<Oid> = log.iter().map(|c| c.id).collect();
        let mut paths: Vec<String> = repo
            .as_ref()
            .map(|r| r.changed_paths(&ids))
            .unwrap_or_default()
            .into_iter()
            .flat_map(|(_, p)| p)
            .collect();
        paths.sort_unstable();
        paths.dedup();
        let places = if here.github {
            codeowners::PLACES_GITHUB
        } else {
            codeowners::PLACES_GITLAB
        };
        let owners = match (&repo, &target) {
            (Some(r), Some(t)) => places
                .iter()
                .find_map(|p| r.file_at(t.1, p))
                .map(|text| codeowners::parse(&text))
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        let suggested = codeowners::suggest(&owners, &paths, self.login.as_deref());
        let nothing = source.is_some() && target.is_some() && commits == 0;
        if let Some(n) = self.creating.as_mut() {
            n.existing = existing;
            n.commits = commits;
            n.nothing = nothing;
            n.conflicts = conflicts;
            if !n.reviewers_touched {
                n.reviewers = suggested.clone();
            }
            n.suggested = suggested;
        }
        // The diff the request would hold: under Changes, and the files on the left.
        target.zip(source)
    }

    /// The branches From (`source`) or Into offers for what is typed, the other side left out.
    /// Into: on origin first (the default first), then those only on this Mac; From: those here
    /// first, then those only on origin. Each with whether it is only local.
    fn branch_choices(&self, branches: &[BranchRef], source: bool) -> Vec<(String, bool)> {
        let Some(n) = self.creating.as_ref() else {
            return Vec::new();
        };
        let (typed, other) = if source {
            (&n.source, &n.target)
        } else {
            (&n.target, &n.source)
        };
        let typed = typed.to_lowercase();
        let mut remote: Vec<String> = branches
            .iter()
            .filter(|b| b.kind == RefKind::Remote)
            .filter_map(|b| b.name.split_once('/').map(|(_, s)| s.to_owned()))
            .filter(|s| s != "HEAD" && s != other)
            .collect();
        remote.sort_by_key(|s| (!["main", "master"].contains(&s.as_str()), s.clone()));
        remote.dedup();
        let here = branches
            .iter()
            .filter(|b| b.kind == RefKind::Local && &b.name != other);
        let mut all: Vec<(String, bool)> = if source {
            let mut all: Vec<(String, bool)> = here
                .map(|b| (b.name.clone(), !remote.contains(&b.name)))
                .collect();
            let only_origin: Vec<String> = remote
                .into_iter()
                .filter(|s| !all.iter().any(|(h, _)| h == s))
                .collect();
            all.extend(only_origin.into_iter().map(|s| (s, false)));
            all
        } else {
            let local: Vec<String> = here
                .filter(|b| !remote.contains(&b.name))
                .map(|b| b.name.clone())
                .collect();
            let mut all: Vec<(String, bool)> = remote.into_iter().map(|s| (s, false)).collect();
            all.extend(local.into_iter().map(|s| (s, true)));
            all
        };
        // An exact match is the choice already made: everything is offered again.
        if all.iter().any(|(s, _)| s.to_lowercase() == typed) {
            return all;
        }
        all.retain(|(s, _)| s.to_lowercase().contains(&typed));
        all
    }

    /// The description has the keyboard: ↑ ↓ move between its lines.
    pub fn typing_description(&self) -> bool {
        self.creating
            .as_ref()
            .is_some_and(NewRequest::typing_description)
    }

    pub fn close_create(&mut self, cx: &mut Context<Self>) {
        if self.creating.take().is_some() {
            cx.notify();
        }
    }

    /// From's list picked: the request is made from that branch, what is written kept (#159);
    /// the pair, whose diff the tab shows.
    pub fn pick_source(&mut self, here: &Here, name: String) -> Option<Pair> {
        let on_origin = remote_tip(here.branches, &name).is_some();
        let n = self.creating.as_mut()?;
        n.source = name;
        n.local_only = !on_origin;
        n.focus = Focus::Target;
        n.target_at = 0;
        n.error = None;
        n.reset_field();
        self.refresh_create(here)
    }

    /// Into's list picked; the pair, whose diff the tab shows.
    pub fn pick_target(&mut self, here: &Here, name: String) -> Option<Pair> {
        let n = self.creating.as_mut()?;
        n.target = name;
        n.focus = Focus::Title;
        n.target_at = 0;
        n.error = None;
        n.reset_field();
        self.refresh_create(here)
    }

    /// The list open under From or Into: whether it is From's.
    fn branch_list(&self) -> Option<bool> {
        match self.creating.as_ref()?.focus {
            Focus::Source => Some(true),
            Focus::Target => Some(false),
            _ => None,
        }
    }

    /// The project's members Reviewers' list offers for what is typed (#190): by username or name, you left out,
    /// eight at most; each with whether it is chosen already. None while nothing is typed.
    pub(crate) fn reviewer_choices(&self) -> Vec<(String, String, bool)> {
        let Some(n) = self
            .creating
            .as_ref()
            .filter(|n| n.focus == Focus::Reviewers)
        else {
            return Vec::new();
        };
        members_for(
            &self.members,
            self.login.as_deref(),
            &n.reviewer_query,
            &n.reviewers,
        )
    }

    /// A reviewer's name as the members give it, else the username.
    fn reviewer_name(&self, login: &str) -> String {
        self.members
            .iter()
            .find(|(l, name)| l == login && !name.is_empty())
            .map_or_else(|| login.to_owned(), |(_, name)| name.clone())
    }

    /// `login` asked to review, or no longer; what was typed is cleared once one is added.
    fn toggle_reviewer(&mut self, login: &str) {
        let Some(n) = self.creating.as_mut() else {
            return;
        };
        n.reviewers_touched = true;
        match n.reviewers.iter().position(|r| r == login) {
            Some(at) => {
                n.reviewers.remove(at);
            }
            None => {
                n.reviewers.push(login.to_owned());
                n.reviewer_query.clear();
                n.reviewer_at = 0;
                n.reset_field();
            }
        }
    }

    /// ↑ ↓ on From's or Into's list (#159), or Reviewers' (#190): they come as the commit keys while the field has
    /// no caret, so the zones hand them here.
    pub fn step_branch_list(&mut self, branches: &[BranchRef], down: bool) -> bool {
        let members = self.reviewer_choices().len();
        if members > 0 {
            let Some(n) = self.creating.as_mut() else {
                return false;
            };
            n.reviewer_at = if down {
                (n.reviewer_at + 1).min(members - 1)
            } else {
                n.reviewer_at.saturating_sub(1)
            };
            return true;
        }
        let Some(source) = self.branch_list() else {
            return false;
        };
        let count = self.branch_choices(branches, source).len();
        let Some(n) = self.creating.as_mut() else {
            return false;
        };
        n.target_at = if down {
            (n.target_at + 1).min(count.saturating_sub(1))
        } else {
            n.target_at.saturating_sub(1)
        };
        n.list_scroll.scroll_to_item(n.target_at);
        true
    }

    /// The form's keys, the repository's `branches` listed; what they ask of the tab.
    pub fn create_key(
        &mut self,
        event: &KeyDownEvent,
        branches: &[BranchRef],
        cx: &mut Context<Self>,
    ) -> Option<CreateKey> {
        let key = &event.keystroke;
        let n = self.creating.as_ref()?;
        let root = n.root.clone();
        let here = &Here {
            root: &root,
            branches,
            github: n.github,
        };
        if n.posting {
            return None;
        }
        if key.key == "enter" && key.modifiers.secondary() {
            return Some(CreateKey::Create);
        }
        // `@` and `:` complete in the description (#51, #9).
        let list = n
            .typing_description()
            .then(|| self.completion(&n.description, n.area.caret(&n.description), n.closed))
            .flatten();
        let n = self.creating.as_mut()?;
        if let Some((at, choices)) = list
            && list_key(
                key,
                &mut n.description,
                &mut n.area.edit,
                &mut n.pick,
                &mut n.closed,
                at,
                &choices,
            )
        {
            cx.notify();
            return None;
        }
        // ⌃⇥: Request | Changes, the draft kept.
        if key.key == "tab" && key.modifiers.control {
            n.changes = !n.changes;
            cx.notify();
            return None;
        }
        if key.key == "escape" && n.changes {
            n.changes = false;
            cx.notify();
            return None;
        }
        if key.key == "escape" && n.menu.is_some() {
            n.menu = None;
            cx.notify();
            return None;
        }
        if n.changes {
            return None;
        }
        if key.key == "d" && key.modifiers.secondary() {
            n.draft = !n.draft;
            cx.notify();
            return None;
        }
        if key.key == "p" && key.modifiers.secondary() && key.modifiers.shift {
            n.preview = !n.preview;
            cx.notify();
            return None;
        }
        // From's or Into's list: ↵ chooses (↑ ↓ come through `step_branch_list`); esc closes
        // the list, not the card.
        if let Some(source) = self.branch_list() {
            match key.key.as_str() {
                "enter" => {
                    let at = self.creating.as_ref().map_or(0, |n| n.target_at);
                    if let Some((name, _)) =
                        self.branch_choices(here.branches, source).get(at).cloned()
                    {
                        let pair = if source {
                            self.pick_source(here, name)
                        } else {
                            self.pick_target(here, name)
                        };
                        cx.notify();
                        return pair.map(CreateKey::Compare);
                    }
                }
                "escape" => {
                    if let Some(n) = self.creating.as_mut() {
                        n.focus = Focus::Title;
                        n.reset_field();
                    }
                    cx.notify();
                    return None;
                }
                _ => {}
            }
        }
        let n = self.creating.as_mut()?;
        // The description moves by its lines as drawn.
        if n.typing_description()
            && input::vertical(
                key,
                &n.description,
                &mut n.area.edit,
                &n.area.lines,
                &mut n.area.goal,
            )
        {
            cx.notify();
            return None;
        }
        // Reviewers: ↵ adds the member picked, or takes them off; ⌫ with nothing typed takes the last off; esc
        // clears what is typed before it closes the form.
        if n.focus == Focus::Reviewers {
            let typed = n.reviewer_query.is_empty();
            match key.key.as_str() {
                "enter" => {
                    let at = n.reviewer_at;
                    if let Some((login, _, _)) = self.reviewer_choices().get(at).cloned() {
                        self.toggle_reviewer(&login);
                    }
                    cx.notify();
                    return None;
                }
                "backspace" if typed => {
                    if n.reviewers.pop().is_some() {
                        n.reviewers_touched = true;
                    }
                    cx.notify();
                    return None;
                }
                "escape" if !typed => {
                    n.reviewer_query.clear();
                    n.reviewer_at = 0;
                    n.reset_field();
                    cx.notify();
                    return None;
                }
                _ => {}
            }
        }
        if key.key == "tab" {
            n.focus = match (n.focus, key.modifiers.shift) {
                (Focus::Source, false) | (Focus::Title, true) => Focus::Target,
                (Focus::Target, false) | (Focus::Description, true) => Focus::Title,
                (Focus::Title, false) | (Focus::Reviewers, true) => Focus::Description,
                (Focus::Description, false) | (Focus::Source, true) => Focus::Reviewers,
                (Focus::Reviewers, false) | (Focus::Target, true) => Focus::Source,
            };
            n.reset_field();
            cx.notify();
            return None;
        }
        if key.key == "enter" && n.focus == Focus::Description {
            input::sync(&n.description, &mut n.area.edit);
            n.area.edit.insert("\n");
            n.description = n.area.edit.text().to_owned();
            cx.notify();
            return None;
        }
        let text = match n.focus {
            Focus::Source => &mut n.source,
            Focus::Target => &mut n.target,
            Focus::Title => &mut n.title,
            Focus::Description => &mut n.description,
            Focus::Reviewers => &mut n.reviewer_query,
        };
        let edit = if n.focus == Focus::Description {
            input::edit_area
        } else {
            input::edit
        };
        match edit(text, &mut n.area.edit, key, cx) {
            Edit::Escape => {
                self.close_create(cx);
                return None;
            }
            Edit::Enter { .. } => {
                if n.focus == Focus::Title {
                    n.focus = Focus::Description;
                }
            }
            Edit::Changed => {
                n.error = None;
                if matches!(n.focus, Focus::Source | Focus::Target) {
                    n.target_at = 0;
                    n.list_scroll.scroll_to_item(0);
                }
                n.reviewer_at = 0;
            }
            Edit::Selected | Edit::Ignored => {}
        }
        cx.notify();
        None
    }

    /// Sends the request; `Event::Created` once the host made it. `Some` with the request
    /// already there for the pair: the tab opens it.
    pub fn submit_create(&mut self, host: Arc<dyn Host>, cx: &mut Context<Self>) -> Option<u64> {
        let n = self.creating.as_mut()?;
        if let Some(iid) = n.existing {
            self.creating = None;
            return Some(iid);
        }
        if n.local_only || n.posting || n.nothing {
            return None;
        }
        if n.title.trim().is_empty() {
            n.error = Some("A merge request needs a title.".to_owned());
            cx.notify();
            return None;
        }
        n.posting = true;
        n.error = None;
        let (source, target, title, description) = (
            n.source.clone(),
            n.target.trim().to_owned(),
            n.title.trim().to_owned(),
            n.description.trim().to_owned(),
        );
        let options = NewOptions {
            draft: n.draft,
            remove_source: n.delete_branch,
            squash: n.squash,
            reviewers: n.reviewers.clone(),
            labels: n.labels.clone(),
            milestone: n.milestone,
        };
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { host.create(&source, &target, &title, &description, &options) })
                .await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(request) => {
                        let (iid, source) = (request.iid, request.source.clone());
                        this.creating = None;
                        this.list.insert(0, request);
                        cx.emit(Event::Created { iid, source });
                    }
                    Err(e) => {
                        if let Some(n) = this.creating.as_mut() {
                            n.posting = false;
                            n.error = Some(format!("{e:#}"));
                        }
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        None
    }

    /// Request | Changes is on Changes: the diff of the pair stands in the pane.
    pub fn create_shows_changes(&self) -> bool {
        self.creating.as_ref().is_some_and(|n| n.changes)
    }

    /// The header row while a request is being written: its name, the pair, totals, Create.
    pub fn render_create_header(
        &self,
        totals: Option<Totals>,
        acts: Acts,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let n = self.creating.as_ref()?;
        // Nothing to merge: Create says why it cannot, and is not a button (#190).
        let nothing = n.nothing && n.existing.is_none();
        let blocked = n.local_only || n.posting;
        let go = if n.posting {
            "Creating…".to_owned()
        } else if let Some(iid) = n.existing {
            format!("Open {}", words::number(iid))
        } else if nothing {
            "Create · Nothing to merge".to_owned()
        } else {
            "Create".to_owned()
        };
        Some(
            island()
                .flex_none()
                .flex_row()
                .items_center()
                .gap_3()
                .px_4()
                .py(px(10.))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .child(
                            div()
                                .text_size(px(15.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(words::say("New merge request").into_owned()),
                        )
                        .child(
                            hover::branches("create-branches", &n.source, &n.target)
                                .font_family(theme::code_font())
                                .text_size(px(12.))
                                .text_color(theme::muted()),
                        ),
                )
                .children(self.render_create_totals(totals))
                .child(
                    div()
                        .id("create-cancel")
                        .flex_none()
                        .px(px(10.))
                        .h(px(28.))
                        .flex()
                        .items_center()
                        .gap_2()
                        .rounded(px(ROW_RADIUS))
                        .cursor_pointer()
                        .hover(|s| s.bg(theme::hover()))
                        .child("Cancel")
                        .child(keys::caps("esc", keys::On::Plain))
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.close_create(cx))),
                )
                .child(
                    div()
                        .id("create-go")
                        .flex_none()
                        .px(px(10.))
                        .h(px(28.))
                        .flex()
                        .items_center()
                        .gap_2()
                        .rounded(px(ROW_RADIUS))
                        .font_weight(FontWeight::SEMIBOLD)
                        .when(nothing, |s| {
                            s.border_1()
                                .border_color(theme::island_border())
                                .text_color(theme::faint())
                        })
                        .when(!nothing, |s| {
                            s.bg(theme::accent())
                                .text_color(theme::base())
                                .when(blocked, |s| s.opacity(0.45))
                                .when(!blocked, |s| s.cursor_pointer())
                        })
                        .child(go)
                        .when(!nothing, |s| {
                            s.child(keys::caps("⌘↵", keys::On::Filled))
                                .on_click(move |_: &ClickEvent, _, cx| acts(Act::CreateGo, cx))
                        }),
                )
                .into_any_element(),
        )
    }

    /// "3 commits · 8 files · +46 −12" from the diff of the pair.
    fn render_create_totals(&self, totals: Option<Totals>) -> Option<gpui::Div> {
        let n = self.creating.as_ref()?;
        let Totals {
            files,
            added: a,
            removed: r,
        } = totals?;
        Some(
            div()
                .flex_none()
                .flex()
                .gap_2()
                .text_size(px(12.))
                .text_color(theme::muted())
                .child(format!(
                    "{} · {} ·",
                    plural(n.commits, "commit"),
                    plural(files, "file")
                ))
                .child(div().text_color(theme::added()).child(format!("+{a}")))
                .child(div().text_color(theme::removed()).child(format!("−{r}"))),
        )
    }

    /// The Request | Changes switch at the top of the pane.
    pub fn render_create_switch(&self, files: usize, cx: &mut Context<Self>) -> Option<AnyElement> {
        let n = self.creating.as_ref()?;
        let set = |changes: bool| {
            cx.listener(move |this: &mut Self, _: &ClickEvent, _, cx| {
                if let Some(n) = this.creating.as_mut() {
                    n.changes = changes;
                }
                cx.notify();
            })
        };
        Some(
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap_3()
                .px(px(12.))
                .py(px(8.))
                .border_b_1()
                .border_color(theme::island_border())
                .child(
                    artoo_ui_kit::controls::group()
                        .child(chip("create-request", "Request", !n.changes).on_click(set(false)))
                        .child(
                            chip("create-changes", format!("Changes · {files}"), n.changes)
                                .on_click(set(true)),
                        ),
                )
                .children(n.changes.then(|| {
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .text_size(px(12.))
                        .text_color(theme::muted())
                        .child(div().size(px(7.)).rounded_full().bg(theme::warning()))
                        .child("Draft kept")
                }))
                .child(div().flex_1())
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .text_size(px(11.))
                        .text_color(theme::faint())
                        .child(keys::caps("⌃⇥", keys::On::Plain))
                        .child("switches"),
                )
                .into_any_element(),
        )
    }

    /// Beside From → Into: whether it merges cleanly, and how many commits are ahead.
    fn render_merge_state(&self, n: &NewRequest) -> gpui::Div {
        let ahead = if n.commits == 0 {
            "nothing ahead of the target".to_owned()
        } else {
            format!("{} ahead", plural(n.commits, "commit"))
        };
        let (text, color) = match &n.conflicts {
            _ if n.commits == 0 => (ahead, theme::muted()),
            Some(files) if files.is_empty() => {
                (format!("✓ merges cleanly · {ahead}"), theme::added())
            }
            Some(files) => (
                format!(
                    "⚠ {} · {ahead}",
                    match files.as_slice() {
                        [one] => format!("conflicts in {}", one.rsplit('/').next().unwrap_or(one)),
                        _ => format!("conflicts in {}", plural(files.len(), "file")),
                    }
                ),
                theme::warning(),
            ),
            None => (ahead, theme::muted()),
        };
        div()
            .flex_none()
            .text_size(px(12.))
            .text_color(color)
            .child(text)
    }

    /// A list under a field: rows of `(label, chosen, swatch)`, each a click.
    fn create_menu(
        &self,
        id: &'static str,
        rows: Vec<(String, bool, Option<u32>, Option<String>)>,
        pick: impl Fn(&mut Self, usize, &mut Context<Self>) + Clone + 'static,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        div()
            .id(id)
            .max_h(px(240.))
            .overflow_y_scroll()
            // The wheel stays with the list, not the form around it (#159).
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .p(px(4.))
            .rounded(px(ROW_RADIUS))
            .border_1()
            .border_color(theme::island_border())
            .bg(theme::panel())
            .children(
                rows.into_iter()
                    .enumerate()
                    .map(|(i, (label, on, swatch, end))| {
                        let pick = pick.clone();
                        div()
                            .id((id, i))
                            .px(px(8.))
                            .h(px(26.))
                            .flex()
                            .items_center()
                            .gap_2()
                            .rounded(px(ROW_RADIUS))
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::hover()))
                            .child(
                                div()
                                    .w(px(12.))
                                    .flex_none()
                                    .text_color(theme::accent())
                                    .child(if on { "✓" } else { "" }),
                            )
                            .children(swatch.map(|c| {
                                div()
                                    .size(px(10.))
                                    .flex_none()
                                    .rounded_full()
                                    .bg(gpui::rgb(c))
                            }))
                            .child(hover::cut("label", label).flex_1())
                            .children(end.map(|e| {
                                div()
                                    .flex_none()
                                    .text_size(px(11.))
                                    .text_color(theme::faint())
                                    .child(e)
                            }))
                            .on_click(
                                cx.listener(move |this, _: &ClickEvent, _, cx| pick(this, i, cx)),
                            )
                    }),
            )
    }

    /// Templates ▾ open: every template; the one the description started from is ticked.
    fn render_template_menu(
        &self,
        n: &NewRequest,
        acts: Acts,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let rows = n
            .templates
            .iter()
            .map(|(name, text)| {
                (
                    template_name(name).to_owned(),
                    *text == n.started,
                    None,
                    None,
                )
            })
            .collect();
        self.create_menu(
            "create-template-menu",
            rows,
            move |_, i, cx| acts(Act::UseTemplate(i), cx),
            cx,
        )
        .into_any_element()
    }

    /// Labels and Milestone side by side, each opening the project's list under it.
    fn render_labels_row(&self, n: &NewRequest, cx: &mut Context<Self>) -> AnyElement {
        let menu = match n.menu {
            Some(Menu::Labels) if !n.all_labels.is_empty() => Some(
                self.create_menu(
                    "create-label-menu",
                    n.all_labels
                        .iter()
                        .map(|l| (l.name.clone(), n.labels.contains(&l.name), l.color, None))
                        .collect(),
                    |this, i, cx| {
                        if let Some(n) = this.creating.as_mut()
                            && let Some(name) = n.all_labels.get(i).map(|l| l.name.clone())
                        {
                            match n.labels.iter().position(|l| *l == name) {
                                Some(at) => {
                                    n.labels.remove(at);
                                }
                                None => n.labels.push(name),
                            }
                        }
                        cx.notify();
                    },
                    cx,
                )
                .into_any_element(),
            ),
            Some(Menu::Milestone) if !n.milestones.is_empty() => Some(
                self.create_menu(
                    "create-milestone-menu",
                    std::iter::once(("No milestone".to_owned(), n.milestone.is_none(), None, None))
                        .chain(n.milestones.iter().map(|m| {
                            (
                                m.title.clone(),
                                n.milestone == Some(m.id),
                                None,
                                m.due.clone().map(|d| format!("due {d}")),
                            )
                        }))
                        .collect(),
                    |this, i, cx| {
                        if let Some(n) = this.creating.as_mut() {
                            n.milestone = i
                                .checked_sub(1)
                                .and_then(|i| n.milestones.get(i))
                                .map(|m| m.id);
                            n.menu = None;
                        }
                        cx.notify();
                    },
                    cx,
                )
                .into_any_element(),
            ),
            _ => None,
        };
        let field =
            |id: &'static str, menu: Menu, body: AnyElement| {
                let on = n.menu == Some(menu);
                div()
                    .id(id)
                    .flex_1()
                    .min_w_0()
                    .h(px(32.))
                    .px_2()
                    .flex()
                    .items_center()
                    .gap_2()
                    .rounded(px(ROW_RADIUS))
                    .border_2()
                    .border_color(if on {
                        theme::focus()
                    } else {
                        theme::island_border()
                    })
                    .bg(theme::base())
                    .cursor_pointer()
                    .child(div().flex_1().min_w_0().overflow_hidden().child(body))
                    .child(div().text_color(theme::faint()).child("▾"))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.toggle_create_menu(menu, cx)
                    }))
            };
        let faint = |text: &str| {
            div()
                .text_color(theme::faint())
                .child(text.to_owned())
                .into_any_element()
        };
        // Still being read: a bar where the value will be, not "No labels in this project".
        let reading = |id: &'static str| {
            motion::breathe(
                id,
                div()
                    .w(px(90.))
                    .h(px(10.))
                    .rounded(px(4.))
                    .bg(theme::hover()),
            )
        };
        let labels = if n.labels.is_empty() && !n.lists_read {
            reading("create-labels-loading")
        } else if n.labels.is_empty() {
            faint(if n.all_labels.is_empty() {
                "No labels in this project"
            } else {
                "None"
            })
        } else {
            div()
                .flex()
                .gap(px(6.))
                .children(n.labels.iter().map(|name| {
                    let color = n
                        .all_labels
                        .iter()
                        .find(|l| l.name == *name)
                        .and_then(|l| l.color);
                    div()
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap(px(5.))
                        .px(px(7.))
                        .h(px(20.))
                        .rounded_full()
                        .bg(theme::selected())
                        .text_size(px(12.))
                        .children(color.map(|c| div().size(px(8.)).rounded_full().bg(gpui::rgb(c))))
                        .child(name.clone())
                }))
                .into_any_element()
        };
        let milestone = match n
            .milestone
            .and_then(|id| n.milestones.iter().find(|m| m.id == id))
        {
            Some(m) => hover::cut("create-milestone-title", m.title.clone()).into_any_element(),
            None if !n.lists_read => reading("create-milestone-loading"),
            None => faint(if n.milestones.is_empty() {
                "No open milestones"
            } else {
                "None"
            }),
        };
        let label = |text: &'static str| {
            div()
                .text_size(px(11.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::faint())
                .child(text.to_uppercase())
        };
        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(
                div()
                    .flex()
                    .gap_3()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .child(label("Labels"))
                            .child(field("create-labels", Menu::Labels, labels)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .child(label("Milestone"))
                            .child(field("create-milestone", Menu::Milestone, milestone)),
                    ),
            )
            .children(menu)
            .into_any_element()
    }

    /// From's (`source`) or Into's list while its field has the keyboard: hung under the field,
    /// over the form, as wide as the field (#159).
    fn render_branch_list(
        &self,
        n: &NewRequest,
        source: bool,
        branches: &[BranchRef],
        acts: Acts,
    ) -> Option<AnyElement> {
        if self.branch_list() != Some(source) {
            return None;
        }
        let choices = self.branch_choices(branches, source);
        if choices.is_empty() {
            return None;
        }
        let width = n
            .widths
            .width(if source {
                "create-source"
            } else {
                "create-target"
            })
            .max(260.);
        let id = if source {
            "create-sources"
        } else {
            "create-targets"
        };
        let list = div()
            .id(id)
            .w(px(width))
            .max_h(px(260.))
            .overflow_y_scroll()
            .track_scroll(&n.list_scroll)
            // The wheel stays with the list, not the form under it.
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .p(px(4.))
            .rounded(px(ROW_RADIUS))
            .border_1()
            .border_color(theme::island_border())
            .bg(theme::panel())
            .shadow_lg()
            .text_size(px(13.))
            .children(choices.into_iter().enumerate().map(|(i, (name, local))| {
                let pick = name.clone();
                let default = !source && ["main", "master"].contains(&name.as_str());
                div()
                    .id((id, i))
                    .px(px(8.))
                    .h(px(26.))
                    .flex()
                    .items_center()
                    .gap_2()
                    .rounded(px(ROW_RADIUS))
                    .cursor_pointer()
                    .when(i == n.target_at, |s| s.bg(theme::selected()))
                    .hover(|s| s.bg(theme::hover()))
                    .child(
                        hover::name(("create-target-name", i), name.clone())
                            .flex_1()
                            .font_family(theme::code_font()),
                    )
                    .children(default.then(|| {
                        div()
                            .text_size(px(11.))
                            .text_color(theme::faint())
                            .child("default")
                    }))
                    .children(local.then(|| {
                        div()
                            .text_size(px(11.))
                            .text_color(theme::faint())
                            .child("not on origin")
                    }))
                    .on_click({
                        let acts = acts.clone();
                        move |_: &ClickEvent, _, cx| {
                            acts(
                                if source {
                                    Act::PickSource(pick.clone())
                                } else {
                                    Act::PickTarget(pick.clone())
                                },
                                cx,
                            )
                        }
                    })
            }));
        Some(
            div()
                .absolute()
                .left_0()
                .bottom_0()
                .size_0()
                .child(
                    gpui::deferred(
                        gpui::anchored()
                            .offset(gpui::point(px(0.), px(4.)))
                            .snap_to_window_with_margin(px(8.))
                            .child(list),
                    )
                    .with_priority(1),
                )
                .into_any_element(),
        )
    }

    /// The form, in place of the diff (`pieces::create_form`, which gives it the keyboard on a press).
    pub fn render_create_form(
        &self,
        look: &CreateLook,
        acts: Acts,
        cx: &mut Context<Self>,
    ) -> Option<gpui::Stateful<gpui::Div>> {
        let n = self.creating.as_ref()?;
        // The fields show their caret only while the form has the keyboard (#168).
        let typing = look.typing;
        let github = look.github;
        // Built first: they need the context to themselves.
        let template_menu = (n.menu == Some(Menu::Templates))
            .then(|| self.render_template_menu(n, acts.clone(), cx));
        let labels_row = self.render_labels_row(n, cx);
        // From's and Into's lists, each under its field while it has the keyboard (#159).
        let source_list = self.render_branch_list(n, true, look.branches, acts.clone());
        let target_list = self.render_branch_list(n, false, look.branches, acts);
        let completion = (n.focus == Focus::Description && !n.preview)
            .then(|| self.completion(&n.description, n.area.caret(&n.description), n.closed))
            .flatten()
            .map(|(_, choices)| render_list(&choices, n.pick));
        let label = |text: &'static str| {
            div()
                .text_size(px(11.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::faint())
                .child(text.to_uppercase())
        };
        let boxed = |id: &'static str, focus: Option<Focus>, body: AnyElement| {
            let on = typing && focus.is_some_and(|f| n.focus == f);
            div()
                .id(id)
                .px_2()
                .py(px(5.))
                .min_w_0()
                .rounded(px(ROW_RADIUS))
                .border_2()
                .border_color(if on {
                    theme::focus()
                } else {
                    theme::island_border()
                })
                .bg(theme::base())
                .when(focus.is_some(), |s| s.cursor_text())
                .child(body)
                .when_some(focus, |s, focus| {
                    s.on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        if let Some(n) = this.creating.as_mut()
                            && n.focus != focus
                        {
                            n.focus = focus;
                            n.preview = false;
                            n.reset_field();
                            cx.notify();
                        }
                    }))
                })
        };
        let check = |id: &'static str, on: bool, text: &'static str| {
            div()
                .id(id)
                .flex()
                .items_center()
                .gap_2()
                .cursor_pointer()
                .child(
                    div()
                        .size(px(14.))
                        .rounded(px(3.))
                        .border_2()
                        .border_color(if on { theme::accent() } else { theme::faint() })
                        .when(on, |s| s.bg(theme::accent()))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(10.))
                        .text_color(theme::base())
                        .child(if on { "✓" } else { "" }),
                )
                .child(keys::text(text).into_owned())
        };
        let note = if n.local_only {
            Some((
                words::say(&format!(
                    "{} is only on this Mac, and GitLab cannot open a request from it. Push it first: git push -u origin {}",
                    n.source, n.source
                ))
                .into_owned(),
                true,
            ))
        } else {
            n.existing.map(|iid| {
                (
                    keys::text(&format!(
                        "{} already exists for {} → {}. ⌘↵ opens it.",
                        words::number(iid),
                        n.source,
                        n.target
                    ))
                    .into_owned(),
                    false,
                )
            })
        };
        // Reviewers (#190): those chosen as pills, the code owners not chosen offered beside them, and a field that
        // searches the project's members, its list under it.
        let suggested = n.suggested.clone();
        let mut people: Vec<String> = n.reviewers.clone();
        for s in &suggested {
            if !people.contains(s) {
                people.push(s.clone());
            }
        }
        let pills: Vec<AnyElement> = people
            .into_iter()
            .enumerate()
            .map(|(i, who)| {
                let on = n.reviewers.contains(&who);
                let name = self.reviewer_name(&who);
                div()
                    .id(("create-reviewer", i))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .h(px(24.))
                    .pl(px(3.))
                    .pr(px(8.))
                    .rounded_full()
                    .cursor_pointer()
                    .when(on, |s| s.bg(theme::selected()))
                    .when(!on, |s| s.border_1().border_color(theme::island_border()))
                    .hover(|s| s.bg(theme::hover()))
                    .child(discussion::avatar(&name, false, 18.))
                    .child(name)
                    .child(
                        div()
                            .text_color(theme::faint())
                            .child(if on { "×" } else { "+" }),
                    )
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        this.toggle_reviewer(&who);
                        cx.notify();
                    }))
                    .into_any_element()
            })
            .collect();
        let searching = typing && n.focus == Focus::Reviewers;
        let choices = self.reviewer_choices();
        let typed = n.reviewer_query.trim().to_lowercase();
        let member_list =
            (!choices.is_empty()).then(|| {
                let list =
                    div()
                        .id("create-members")
                        .w(px(320.))
                        .max_h(px(260.))
                        .overflow_y_scroll()
                        .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                        .p(px(4.))
                        .rounded(px(ROW_RADIUS))
                        .border_1()
                        .border_color(theme::island_border())
                        .bg(theme::panel())
                        .shadow_lg()
                        .text_size(px(13.))
                        .children(choices.into_iter().enumerate().map(
                            |(i, (login, name, added))| {
                                let shown = if name.is_empty() { login.clone() } else { name };
                                let pick = login.clone();
                                div()
                                    .id(("create-member", i))
                                    .px(px(8.))
                                    .h(px(26.))
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .rounded(px(ROW_RADIUS))
                                    .cursor_pointer()
                                    .when(i == n.reviewer_at, |s| s.bg(theme::selected()))
                                    .hover(|s| s.bg(theme::hover()))
                                    .child(discussion::avatar(&shown, false, 18.))
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .flex()
                                            .children(matched(&shown, &typed)),
                                    )
                                    .child(
                                        div()
                                            .flex_none()
                                            .text_size(px(11.))
                                            .text_color(theme::faint())
                                            .font_family(if added {
                                                theme::ui_font()
                                            } else {
                                                theme::code_font()
                                            })
                                            .child(if added {
                                                "added".to_owned()
                                            } else {
                                                format!("@{login}")
                                            }),
                                    )
                                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                        this.toggle_reviewer(&pick);
                                        cx.notify();
                                    }))
                            },
                        ));
                div().absolute().left_0().bottom_0().size_0().child(
                    gpui::deferred(
                        gpui::anchored()
                            .offset(gpui::point(px(0.), px(4.)))
                            .snap_to_window_with_margin(px(8.))
                            .child(list),
                    )
                    .with_priority(1),
                )
            });
        let reviewers = div()
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(label("Reviewers"))
                    .child(div().text_size(px(11.)).text_color(theme::faint()).child(
                        if suggested.is_empty() {
                            "no code owners for these files"
                        } else {
                            "suggested: code owners of the changed files"
                        },
                    )),
            )
            .child(
                div()
                    .relative()
                    .child(boxed(
                        "create-reviewers",
                        Some(Focus::Reviewers),
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap(px(6.))
                            .children(pills)
                            .child(div().flex_1().min_w(px(140.)).child(n.area.field(
                                &n.reviewer_query,
                                searching,
                                "Search the project's members",
                                false,
                                area_of,
                                cx,
                            )))
                            .into_any_element(),
                    ))
                    .children(member_list),
            );
        let description: AnyElement = if n.preview {
            div()
                .min_h(px(200.))
                .child(if n.description.trim().is_empty() {
                    div()
                        .text_color(theme::faint())
                        .child("Nothing to preview")
                        .into_any_element()
                } else {
                    markdown::body(
                        &n.description,
                        true,
                        "create-preview",
                        look.base.as_deref(),
                        look.selecting.clone(),
                        cx,
                    )
                })
                .into_any_element()
        } else {
            div()
                .min_h(px(200.))
                .flex()
                .items_start()
                .child(n.area.field(
                    &n.description,
                    typing && n.focus == Focus::Description,
                    "What it changes and why",
                    true,
                    area_of,
                    cx,
                ))
                .into_any_element()
        };
        let set_preview = |preview: bool| {
            cx.listener(move |this: &mut Self, _: &ClickEvent, _, cx| {
                if let Some(n) = this.creating.as_mut() {
                    n.preview = preview;
                }
                cx.notify();
            })
        };
        let form =
            div()
                .id("create-form")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .px(px(16.))
                .py(px(14.))
                .flex()
                .flex_col()
                .gap_3()
                .max_w(px(920.))
                .text_size(px(13.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .relative()
                                .flex_1()
                                .min_w_0()
                                .child(n.widths.measure("create-source"))
                                .child(boxed(
                                    "create-source",
                                    Some(Focus::Source),
                                    // Typed, it scrolls with the caret; shown, its whole in a
                                    // long hover.
                                    if n.focus == Focus::Source {
                                        // clip: a field being typed in scrolls with its caret.
                                        div()
                                            .overflow_hidden()
                                            .whitespace_nowrap()
                                            .font_family(theme::code_font())
                                            .child(n.area.field(
                                                &n.source, typing, "branch", false, area_of, cx,
                                            ))
                                            .into_any_element()
                                    } else {
                                        hover::name("create-source-name", n.source.clone())
                                            .font_family(theme::code_font())
                                            .into_any_element()
                                    },
                                ))
                                .children(source_list),
                        )
                        .child(div().text_color(theme::faint()).child("→"))
                        .child(
                            div()
                                .relative()
                                .flex_1()
                                .min_w_0()
                                .child(n.widths.measure("create-target"))
                                .child(boxed(
                                    "create-target-field",
                                    Some(Focus::Target),
                                    div()
                                        .font_family(theme::code_font())
                                        .child(n.area.field(
                                            &n.target,
                                            typing && n.focus == Focus::Target,
                                            "main",
                                            false,
                                            area_of,
                                            cx,
                                        ))
                                        .into_any_element(),
                                ))
                                .children(target_list),
                        )
                        .child(self.render_merge_state(n)),
                )
                .children(note.map(|(text, warn)| {
                    div()
                        .px_2()
                        .py(px(5.))
                        .rounded(px(ROW_RADIUS))
                        .border_1()
                        .border_color(if warn {
                            theme::warning()
                        } else {
                            theme::island_border()
                        })
                        .text_size(px(12.))
                        .text_color(if warn {
                            theme::warning()
                        } else {
                            theme::muted()
                        })
                        .child(text)
                }))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .child(label("Title"))
                        .child(boxed(
                            "create-title",
                            Some(Focus::Title),
                            n.area
                                .field(
                                    &n.title,
                                    typing && n.focus == Focus::Title,
                                    "Title",
                                    true,
                                    area_of,
                                    cx,
                                )
                                .into_any_element(),
                        )),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(div().flex_1().child(label("Description")))
                                .child(
                                    artoo_ui_kit::controls::group()
                                        .child(
                                            chip("create-write", "Write", !n.preview)
                                                .on_click(set_preview(false)),
                                        )
                                        .child(
                                            chip("create-preview", "Preview", n.preview)
                                                .on_click(set_preview(true)),
                                        ),
                                )
                                .children(n.started_from.clone().map(|from| {
                                    div()
                                        .px(px(8.))
                                        .h(px(22.))
                                        .flex()
                                        .items_center()
                                        .rounded_full()
                                        .bg(theme::info_bg())
                                        .text_size(px(11.))
                                        .text_color(theme::muted())
                                        .child(from)
                                }))
                                .children((n.templates.len() > 1).then(|| {
                                    chip(
                                        "create-templates",
                                        "Templates ▾",
                                        n.menu == Some(Menu::Templates),
                                    )
                                    .on_click(cx.listener(
                                        |this, _: &ClickEvent, _, cx| {
                                            this.toggle_create_menu(Menu::Templates, cx)
                                        },
                                    ))
                                })),
                        )
                        .children(template_menu)
                        .child(boxed(
                            "create-description",
                            Some(Focus::Description),
                            description,
                        ))
                        .children(completion),
                )
                .child(reviewers)
                .child(labels_row)
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_x(px(20.))
                        .gap_y(px(8.))
                        .text_color(theme::muted())
                        .child(
                            check("create-draft", n.draft, "Draft  ⌘D").on_click(cx.listener(
                                |this, _: &ClickEvent, _, cx| {
                                    if let Some(n) = this.creating.as_mut() {
                                        n.draft = !n.draft;
                                    }
                                    cx.notify();
                                },
                            )),
                        )
                        .child(
                            check(
                                "create-delete",
                                n.delete_branch,
                                "Delete the branch after merge",
                            )
                            .on_click(cx.listener(
                                |this, _: &ClickEvent, _, cx| {
                                    if let Some(n) = this.creating.as_mut() {
                                        n.delete_branch = !n.delete_branch;
                                    }
                                    cx.notify();
                                },
                            )),
                        )
                        // GitHub squashes when merging, not when opening.
                        .children((!github).then(|| {
                            check("create-squash", n.squash, "Squash").on_click(cx.listener(
                                |this, _: &ClickEvent, _, cx| {
                                    if let Some(n) = this.creating.as_mut() {
                                        n.squash = !n.squash;
                                    }
                                    cx.notify();
                                },
                            ))
                        })),
                )
                .children(n.error.clone().map(|e| {
                    div()
                        .text_size(px(12.))
                        .text_color(theme::removed())
                        .child(e)
                }));
        Some(form)
    }
}

/// Of the project's `members` (username, name), those `typed` finds by username or name, `me` left out, eight at
/// most; each with whether it is `chosen` already. None while nothing is typed.
fn members_for(
    members: &[(String, String)],
    me: Option<&str>,
    typed: &str,
    chosen: &[String],
) -> Vec<(String, String, bool)> {
    let typed = typed.trim().to_lowercase();
    if typed.is_empty() {
        return Vec::new();
    }
    members
        .iter()
        .filter(|(login, _)| Some(login.as_str()) != me)
        .filter(|(login, name)| {
            login.to_lowercase().contains(&typed) || name.to_lowercase().contains(&typed)
        })
        .take(8)
        .map(|(login, name)| (login.clone(), name.clone(), chosen.contains(login)))
        .collect()
}

/// `text` with the part that matches `typed` (lower case) in bold: `Junip**er** Hale`.
fn matched(text: &str, typed: &str) -> Vec<AnyElement> {
    let lower = text.to_lowercase();
    // A name whose lower case changes its length is shown whole.
    let at = (lower.len() == text.len() && !typed.is_empty())
        .then(|| lower.find(typed))
        .flatten();
    match at {
        Some(at) => vec![
            div().child(text[..at].to_owned()).into_any_element(),
            div()
                .font_weight(FontWeight::BOLD)
                .text_color(theme::text())
                .child(text[at..at + typed.len()].to_owned())
                .into_any_element(),
            div()
                .child(text[at + typed.len()..].to_owned())
                .into_any_element(),
        ],
        None => vec![div().child(text.to_owned()).into_any_element()],
    }
}

/// The form's fields as one of the tab's inputs: its keys read the branches (`Branches` in `services`); what
/// they ask of the tab comes to it as `Event::Create`. ↑ ↓ are its own while the description types.
pub fn input(requests: Entity<Requests>, services: Rc<Services>) -> Input {
    let (open, key, selected, caret) = (
        requests.downgrade(),
        requests.downgrade(),
        requests.downgrade(),
        requests.downgrade(),
    );
    Input {
        id: INPUT,
        open: Rc::new(move |cx| {
            open.upgrade()
                .is_some_and(|rq| rq.read(cx).creating.is_some())
        }),
        key: Rc::new(move |event, _, cx| {
            let Some(branches) = services.get::<Rc<dyn Branches>>().map(|b| b.list(cx)) else {
                return Typed::Kept;
            };
            key.update(cx, |rq, cx| {
                if let Some(asked) = rq.create_key(event, &branches, cx) {
                    cx.emit(Event::Create(asked));
                }
            })
            .ok();
            Typed::Kept
        }),
        left: Rc::new(|_| {}),
        selected: Rc::new(move |cx| {
            let rq = selected.upgrade()?;
            let rq = rq.read(cx);
            let n = rq.creating.as_ref()?;
            n.area.edit.selected_text().map(str::to_owned)
        }),
        caret: Rc::new(move |cx: &App| {
            caret
                .upgrade()
                .is_some_and(|rq| rq.read(cx).typing_description())
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_templates_come_from_the_hosts_place_default_first() {
        let files = |dir: &str| match dir {
            ".gitlab/merge_request_templates" => vec![
                "Bug.md".to_owned(),
                "default.md".to_owned(),
                "notes.txt".to_owned(),
            ],
            "docs" => vec!["PULL_REQUEST_TEMPLATE.md".to_owned()],
            ".github/PULL_REQUEST_TEMPLATE" => vec!["release.md".to_owned()],
            _ => Vec::new(),
        };
        let file = |path: &str| Some(format!("text of {path}"));
        let names = |list: Vec<(String, String)>| -> Vec<String> {
            list.into_iter().map(|(n, _)| n).collect()
        };
        assert_eq!(
            names(templates(false, file, files)),
            ["default.md", "Bug.md"]
        );
        assert_eq!(
            templates(false, file, files)[0].1,
            "text of .gitlab/merge_request_templates/default.md"
        );
        assert_eq!(
            names(templates(true, file, files)),
            ["PULL_REQUEST_TEMPLATE.md", "release.md"]
        );
        assert!(templates(false, file, |_| Vec::new()).is_empty());
        assert_eq!(template_name("Default.md"), "Default");
    }

    #[test]
    fn reviewers_are_found_among_the_members_by_name_or_username() {
        let members: Vec<(String, String)> = [
            ("juniper", "Juniper Hale"),
            ("ember", "Ember Lind"),
            ("kestrel", "Kestrel Okafor"),
            ("you", "You"),
        ]
        .iter()
        .map(|(l, n)| (l.to_string(), n.to_string()))
        .collect();
        let found =
            |typed: &str, chosen: &[String]| members_for(&members, Some("you"), typed, chosen);
        // Nothing typed: no list.
        assert!(found("  ", &[]).is_empty());
        // By name, in any case; one already chosen says so; you are never offered.
        assert_eq!(
            found("ER", &["ember".to_owned()]),
            [
                ("juniper".to_owned(), "Juniper Hale".to_owned(), false),
                ("ember".to_owned(), "Ember Lind".to_owned(), true),
            ]
        );
        assert_eq!(found("kest", &[]).len(), 1);
        assert!(found("you", &[]).is_empty());
    }
}
