//! Branches' contract (#165 stage 4): the routes of a branch's pages — the branch, a commit of it, a run of its
//! commits, two of its versions compared —, of two refs compared and of the working tree, which anyone opens through
//! the `Frame`; and the branches listed, as a part reads them (`Branches`).

use artoo_core::git::{BranchRef, CommitInfo, Deco, Version, WebRemote};
use artoo_core::route::{Place, Route};
use gpui::{AnyElement, AnyView, App, Window};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

/// What of the branch open the diff shows: a commit, a run of commits (indexes, newest first), two versions, the
/// working tree, or nothing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Selection {
    None,
    Commit(usize),
    /// A run of the list's commits as one diff: the newest's index and the oldest's.
    Commits {
        newest: usize,
        oldest: usize,
    },
    Versions {
        from: usize,
        to: usize,
    },
    WorkingTree,
}

/// What showing a route came to (`Branches::take_shown`), for what the tab does after.
pub enum Shows {
    /// Branch `ix` (of `Branches::list`) opened, `read` anew when there is one to read; what follows its read is the
    /// branches' own.
    Branch { ix: usize, read: bool },
    /// Request `number` to open: what it is read from waits for the requests (the tab's for now).
    Request(u64),
    /// Shown: the commit it is, kept among the places opened lately; `follow` when the page on screen follows
    /// what is shown.
    Shown {
        commit: Option<CommitInfo>,
        follow: bool,
    },
    /// Nothing changed.
    Stay,
}

/// What the side panel holds on a route (`Branches::panel_shows`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct PanelShows {
    pub versions: bool,
    pub commits: bool,
    /// A comparison's own commits.
    pub compare: bool,
}

/// A page waiting, shown (`Branches::finish_waiting`): the commit it shows, when it shows one.
pub struct Finished {
    pub commit: Option<CommitInfo>,
}

/// What showing a branch read came to (`Branches::show_read`).
pub struct Settled {
    /// The commits shown, in order: the tab keeps them among the places opened lately.
    pub commits: Vec<CommitInfo>,
    /// A page of the branch or of the request on it is shown, which the page on screen follows; not the branch's
    /// own Changes.
    pub follow: bool,
}

/// What the tab hears of the branches' reads (`Branches::on_event`).
#[derive(Clone)]
pub enum Event {
    /// The branch open was read: the tab shows it (`Branches::show_read`).
    Read,
    /// More of its history is listed.
    Older,
    /// ↵ in the commits' search: commit `ix`, the first found, shown.
    Picked(usize),
    /// A read failed: what to say.
    Failed(String),
    /// The working tree changed: what shows it reads it again.
    Working,
    /// The working tree's poll came round, changed or not: what rides on its beat looks (the agents' answers,
    /// pictures come in).
    Beat,
    /// The repository re-read after it changed under the window (`Branches::reload`): what it found.
    Reloaded(Reloaded),
}

/// What a re-read of the repository found.
#[derive(Clone)]
pub enum Reloaded {
    /// No branch is open: the branches are listed anew, nothing else.
    Listed,
    /// The branch open is gone: the checked-out one, or the first, to fall back to.
    Gone {
        refname: String,
        fallback: Option<usize>,
    },
    /// Nothing new on the branch open.
    Same,
    /// Only the reflog moved: a new version of the same history; `seen` versions were listed before.
    Versions { seen: usize },
    /// New commits, held back until the user asks for them.
    Held,
    /// The repository could not be read (mid-rebase, for a moment): the next change tries again.
    Unreadable,
}

/// What showing the commits held back leaves to do with the diff shown (`Branches::apply_pending`).
pub enum Applied {
    /// The selection still holds, at its new index.
    Kept,
    /// The commit shown is gone: the newest is shown instead.
    Newest,
    /// What was shown no longer is: the diff goes.
    Cleared,
}

/// A ref by its name, at its tip: a comparison's or a merge request's side.
pub type NamedTip = (String, git2::Oid);

/// A merge request on the branch open: its base and head, and its number.
pub type RequestSides = (NamedTip, NamedTip, u64);

/// What a merge request is read from (`Branches::request_branch`): its own fields, as its host says them.
pub struct RequestOf<'a> {
    pub number: u64,
    /// The branch it is from, and the one it goes into.
    pub source: &'a str,
    pub target: &'a str,
    /// Its head, as the host has it.
    pub head: &'a str,
    /// Open, not merged nor closed.
    pub open: bool,
    /// The commit it was merged with, once it is.
    pub merged_with: Option<&'a str>,
    /// On GitHub, where a fork's pull request is fetched to `pr/<n>`.
    pub github: bool,
}

/// A card a long hover shows (a commit's, a branch's), made when it shows.
pub type Card = std::rc::Rc<dyn Fn(&mut Window, &mut App) -> AnyView>;

/// What the version comparison shows: the files that changed, or the commits paired up.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum VersionTab {
    Files,
    Commits,
}

/// A section of the panel the history's.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fold {
    Versions,
    Commits,
}

/// What a click in the branches' views asks: the branches answer their own (a version clicked, the commits'
/// search's chip, a fold, the version comparison's tab, the message), the tab the rest (`TabActs`).
#[derive(Clone)]
pub enum Act {
    /// Version `ix` clicked, with ⌘ (`more`) or not: from the one picked to it, else to the latest.
    Version { ix: usize, more: bool },
    /// Commit `ix` clicked: its diff, the drawer gone.
    Commit(usize),
    /// A right click on commit `ix` at `at`: its menu.
    CommitMenu {
        ix: usize,
        at: gpui::Point<gpui::Pixels>,
    },
    /// A comparison's commit `ix` clicked, with ⇧ (`span`) or not (`Branches::pick_compare`).
    PickCompare { ix: usize, span: bool },
    /// The comparison whole again: the "only" pill's ×.
    WholeCompare,
    /// The working tree's chip: its diff against HEAD.
    WorkingTree,
    /// The search's chip: the commits' search, their section open.
    FindCommits,
    /// A click in the search's field: it takes the keyboard.
    FindField,
    /// A section of the side panel folded or opened.
    Fold(Fold),
    /// A press in the Commits section: its list takes the keyboard (↑ ↓).
    CommitsZone,
    /// A right click on the branch line at `at`: branch `ix`'s menu.
    BranchMenu {
        ix: usize,
        at: gpui::Point<gpui::Pixels>,
    },
    /// A row of Branches clicked, `clicks` times: picked, or opened.
    PickRow { row: BranchRow, clicks: usize },
    /// A row of Branches opened: the working tree, or the branch.
    OpenRow(BranchRow),
    /// A branch's request, by its number.
    ShowRequest(u64),
    /// A remote's branches folded or open.
    ToggleRemote(String),
    /// The working tree's halves: what is not staged only, or all of it.
    OnlyUnstaged(bool),
    /// What `head` changed against `base`, since their merge base or directly, a request's diff or not.
    Compare {
        base: (String, git2::Oid),
        head: (String, git2::Oid),
        since: bool,
        request: Option<u64>,
    },
    /// Versions `from` and `to` compared: an interdiff's way back to their commits.
    Versions { from: usize, to: usize },
    /// The version comparison's tab: its files, or its commits paired.
    VersionTab(VersionTab),
    /// A commit's message open under its title, or folded.
    Message,
    /// The pill's newer commits listed.
    ShowNew,
    /// Text to the clipboard: a commit's id.
    Copy(String),
    /// A page on the host: a commit's.
    OpenUrl(String),
}

/// The tab's answer to what the branches' views ask it (`Act`), in its services: the branches answer their own
/// first.
#[derive(Clone)]
pub struct TabActs(pub Acts);

/// How a view asks an `Act`.
pub type Acts = std::rc::Rc<dyn Fn(Act, &mut App)>;

/// Who hears what the branches' reads come to (`Branches::on_event`).
pub type Hears = std::rc::Rc<dyn Fn(&Event, &mut App)>;

/// The Branches space, by its id (`artoo_shell_api::spaces`): the branches offer it, their pages are in it.
pub const SPACE: &str = "branches";

/// The branches' scope of search everywhere, by its id (`artoo_shell_api::search`): ⌘⇧B opens ⌘K on it.
pub const SEARCH: &str = "branches";

/// The commits' search, as one of the tab's inputs.
pub const SEARCH_INPUT: &str = "branches.search";

/// A row of Branches' list: the working tree, or a branch (#72), by its place in `Branches::list`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BranchRow {
    WorkingTree,
    Branch(usize),
}

/// What a part reads of the tab's branches (`Rc<dyn Branches>` in its services).
pub trait Branches {
    /// The branches listed, local and remote.
    fn list(&self, cx: &App) -> Arc<Vec<BranchRef>>;

    /// How many commits of the branch open are listed.
    fn commit_count(&self, cx: &App) -> usize;

    /// The versions of the branch open, oldest first: each one's number and tip.
    fn versions(&self, cx: &App) -> Vec<(usize, git2::Oid)>;

    /// Its commits as a page of their own, the version each came with on it when `versions`. Asked from a view's
    /// own drawing.
    fn commits_page(&self, versions: bool, cx: &mut App) -> AnyElement;

    /// The side panel of a branch's or a request's page, at `width` (`Pages::side`).
    fn panel(&self, width: f32, cx: &mut App) -> Option<AnyElement>;

    /// A branch's, a comparison's, the working tree's or a request's route on screen (`Pages::show`): what it shows
    /// read, unless it is what is loaded.
    fn show(&self, route: &Route, cx: &mut App);

    /// The route of what the branch open shows now: a request's page while the request open is on it, a
    /// comparison `on_screen` kept while a run of its commits narrows it; `None` while no branch is open.
    fn loaded(&self, on_screen: Option<&Route>, cx: &App) -> Option<Route>;

    /// Whether branch `refname`'s page has Commits and Changes as tabs: it is the one open, off the main line.
    fn tabbed(&self, refname: &str, cx: &App) -> bool;

    /// A commit, or a run of them (newest, oldest), came on screen as a page: its Commits page has it picked.
    fn pick_commits(&self, newest: git2::Oid, oldest: Option<git2::Oid>, cx: &mut App);

    /// A request's diff waits for its branch to be read: what is shown does not move the page yet.
    fn settling(&self, cx: &App) -> bool;

    /// Another page came on screen: nothing waits to be shown.
    fn stop_waiting(&self, cx: &mut App);

    /// Esc from `route`'s page back to the list: its row is picked (the working tree's, a branch's).
    fn back_to_list(&self, route: &Route, cx: &mut App);

    /// The page `selection` of the branch open is once shown: a commit, a run or two versions, of the request open
    /// while it is on the branch. None for a selection that is not one of those.
    fn route_of(&self, selection: Selection, cx: &App) -> Option<Route>;

    /// What showing the last route came to, taken once (`Pages::show` said it).
    fn take_shown(&self, cx: &mut App) -> Option<Shows>;

    /// The page waiting shown, once what it asks for is in (a request's commits, a branch's versions); `None`
    /// while it waits, or when none does.
    fn finish_waiting(&self, cx: &mut App) -> Option<Finished>;

    /// A project opened at `root` with its `branches`, the refs' decorations when it is a repository; `fresh`
    /// when it is another than before (the commits' search starts over).
    fn opened(
        &self,
        root: PathBuf,
        branches: Vec<BranchRef>,
        decor: Option<HashMap<git2::Oid, Vec<Deco>>>,
        fresh: bool,
        cx: &mut App,
    );

    /// The branch to open first: the one named `refname`, else the checked-out one, else the first listed.
    fn first_branch(&self, refname: Option<&str>, cx: &App) -> Option<usize>;

    /// Branch `ix` (of `list`) opened at commit `keep`, what the one before showed forgotten; whether it is read
    /// anew.
    fn open_branch(&self, ix: usize, keep: Option<git2::Oid>, cx: &mut App) -> bool;

    /// What the branch open showed forgotten: no branch open.
    fn forget(&self, cx: &mut App);

    /// The diff of the branch open let go; the branch stays open.
    fn clear_diff(&self, cx: &mut App);

    /// Commit `ix` of the branch open shown, `keep` the file to open in it: the commit, when there is one.
    fn show_commit(&self, ix: usize, keep: Option<String>, cx: &mut App) -> Option<CommitInfo>;

    /// A run of its commits as one diff (indexes, newest first); whether it was shown.
    fn show_commits(
        &self,
        newest: usize,
        oldest: usize,
        keep: Option<String>,
        cx: &mut App,
    ) -> bool;

    /// Two of its versions compared; whether they were.
    fn show_versions(&self, from: usize, to: usize, keep: Option<String>, cx: &mut App) -> bool;

    /// The interdiff of one modified commit of the comparison shown; whether it was shown.
    fn show_interdiff(&self, pair: usize, cx: &mut App) -> bool;

    /// The branch read shown (`Event::Read`): its page, the commit asked for, or the request waiting for it.
    fn show_read(&self, cx: &mut App) -> Option<Settled>;

    /// The branch open, by its place in `list`.
    fn branch(&self, cx: &App) -> Option<usize>;

    /// What of the branch open the diff shows.
    fn selection(&self, cx: &App) -> Selection;

    /// Commit `ix` of those listed of the branch open.
    fn commit(&self, ix: usize, cx: &App) -> Option<CommitInfo>;

    /// The listed commit whose id starts with `prefix`.
    fn commit_ix(&self, prefix: &str, cx: &App) -> Option<usize>;

    /// The commits' search given `text`: the list filtered.
    fn search(&self, text: String, cx: &mut App);

    /// `hear` hears what the branches' reads come to, for as long as the tab lives.
    fn on_event(&self, hear: Hears, cx: &mut App);

    /// The repository changed under the window: the branches and the branch open read again in the background;
    /// what it found is said (`Event::Reloaded`).
    fn reload(&self, cx: &mut App);

    /// What was found on the branch open no longer applies (another one is open, or it is read anew), nor does a
    /// re-read under way.
    fn forget_news(&self, cx: &mut App);

    /// The commits held back shown, the selection kept where it still holds; with how many versions were listed
    /// before. `None` when nothing was held back.
    fn apply_pending(&self, cx: &mut App) -> Option<(Applied, usize)>;

    /// The branches a search lists, with their place in `list` (a request's fetched head is none of them).
    fn listed(&self, cx: &App) -> Vec<(usize, BranchRef)>;

    /// Branches' rows in the order the list shows them, `filter` typed.
    fn rows(&self, filter: &str, cx: &App) -> Vec<BranchRow>;

    /// The row picked in the list, else the one it picks when nothing is.
    fn picked_row(&self, cx: &App) -> Option<BranchRow>;

    /// `row` picked in the list (none: nothing); the working tree's files read when it is its row.
    fn pick_row(&self, row: Option<BranchRow>, cx: &mut App);

    /// "4 local · 6 on origin": the count after Branches' name in the bar.
    fn count_line(&self, cx: &App) -> String;

    /// A remote's branches folded or open in the list.
    fn toggle_remote(&self, remote: String, cx: &mut App);

    /// When each branch's tip was made and how far it is from the main line, read for the list's rows.
    fn read_facts(&self, cx: &mut App);

    /// The working tree's files read again when they changed since the last read.
    fn read_tree(&self, cx: &mut App);

    /// The commits' search opened (it is typed in the branches' own input).
    fn open_find(&self, cx: &mut App);

    /// The commits' search closed.
    fn close_find(&self, cx: &mut App);

    /// Something is typed in the commits' search.
    fn searching(&self, cx: &App) -> bool;

    /// The commits filtered again by what the search says.
    fn refilter(&self, cx: &mut App);

    /// The paths of every commit listed read, for the search's `path:`.
    fn index_paths(&self, cx: &mut App);

    /// The working tree looked at while the project is open (`plain` a folder without git, where only the beat
    /// runs; `shown` whether the tab is on screen).
    fn start_working_poll(
        &self,
        plain: bool,
        shown: std::rc::Rc<std::cell::Cell<bool>>,
        cx: &mut App,
    );

    /// The working tree shown against `HEAD`, `keep` the file to stay on (a reload finds a file partly staged at
    /// its place, #22).
    fn show_working_tree(&self, keep: Option<String>, cx: &mut App);

    /// The working tree's halves: what is not staged only, or all of it.
    fn set_only_unstaged(&self, on: bool, cx: &mut App);

    /// Commit `id`'s place in the list.
    fn commit_of(&self, id: git2::Oid, cx: &App) -> Option<usize>;

    /// The first `limit` commits of the list that `keep` keeps, newest first.
    fn commits_where(
        &self,
        keep: &dyn Fn(&CommitInfo) -> bool,
        limit: usize,
        cx: &App,
    ) -> Vec<CommitInfo>;

    /// Version `ix` of the request open (oldest first), and its latest.
    fn version(&self, ix: usize, cx: &App) -> Option<Version>;
    fn latest_version(&self, cx: &App) -> Option<Version>;

    /// The version comparison shows the files that changed (not its commits paired).
    fn show_version_files(&self, cx: &mut App);

    /// The Commits section folded; the commit's message open over its diff; newer commits or versions waiting
    /// to be shown; a branch being read.
    fn commits_folded(&self, cx: &App) -> bool;
    fn message_open(&self, cx: &App) -> bool;
    fn has_news(&self, cx: &App) -> bool;
    fn reading(&self, cx: &App) -> bool;

    /// The search's text goes, its list left as it is (another field took the keyboard).
    fn drop_find(&self, cx: &mut App);

    /// ↑ ↓ in the commit list: what the selection moves to (a commit, the working tree above the first), if it
    /// moves.
    fn step_row(&self, down: bool, cx: &App) -> Option<Selection>;

    /// ↑ ↓ in a Commits page: another commit picked, nothing opened; ↵ opens what is picked, a commit or a run.
    fn step_pick(&self, down: bool, cx: &mut App);
    fn picked(&self, cx: &App) -> Option<Selection>;

    /// What the side panel holds on `route`.
    fn panel_shows(&self, route: Option<&Route>, cx: &App) -> PanelShows;

    /// A diff arrived: the branch page learns how many files it has, when it is the page's own. The branch
    /// page's count.
    fn count_files(&self, cx: &mut App);
    fn branch_files(&self, cx: &App) -> Option<usize>;

    /// Commit `id`'s card (`web` its page on the host), branch `ix`'s (`request` the one riding on it).
    fn commit_card(&self, id: git2::Oid, web: Option<&WebRemote>, cx: &App) -> Option<Card>;
    fn branch_card(&self, ix: usize, request: Option<(u64, String)>, cx: &App) -> Option<Card>;

    /// What `head` changed against `base`, since their merge base or directly, a request's diff or not.
    fn compare(
        &self,
        base: NamedTip,
        head: NamedTip,
        since: bool,
        request: Option<u64>,
        cx: &mut App,
    );

    /// The comparison `route` is, its commits read; a run of them narrowing its diff.
    fn compare_listed(&self, route: Option<&Route>, cx: &App) -> bool;
    fn compare_narrowed(&self, route: Option<&Route>, cx: &App) -> bool;

    /// The commits of the comparison `route` is read, unless they are these already (esc back from a pick); none
    /// picked. Any other route has none.
    fn read_compare(&self, route: Option<&Route>, cx: &mut App);

    /// A click on a comparison's commit `ix`: its diff alone; with ⇧ (`span`) the run from the one clicked last
    /// to it; `keep` the file to stay on. Whether it is shown.
    fn pick_compare(&self, ix: usize, span: bool, keep: Option<String>, cx: &mut App) -> bool;

    /// The run picked shown under the comparison's own page. Whether a run is picked.
    fn show_compare_pick(&self, keep: Option<String>, cx: &mut App) -> bool;

    /// ↑ ↓ in the commits of the comparison `route` is: the one above or below the run picked, shown. Whether it
    /// moved.
    fn step_compare(
        &self,
        route: Option<&Route>,
        down: bool,
        keep: Option<String>,
        cx: &mut App,
    ) -> bool;

    /// The title of `route`'s comparison narrowed to one of its commits: the commit's subject.
    fn compare_title(&self, route: &Route, cx: &App) -> Option<String>;

    /// The merge request waiting for its branch to be read, and the one whose diff is open.
    fn request_sides(&self, cx: &App) -> (Option<RequestSides>, Option<RequestSides>);
    fn set_pending_request(&self, sides: Option<RequestSides>, cx: &mut App);
    fn set_open_request(&self, sides: Option<RequestSides>, cx: &mut App);

    /// The branch request `of` is read from, and its sides when they are not the branches' own.
    fn request_branch(&self, of: &RequestOf, cx: &App) -> Option<(usize, Option<RequestSides>)>;

    /// The branches read again (a fetch): the branch open stays open where it still is.
    fn set_list(&self, branches: Vec<BranchRef>, cx: &mut App);

    /// The open request's Commits read again from `base` to `head`; a commit or a run picked in it stays picked.
    fn show_request_commits(&self, base: &NamedTip, head: &NamedTip, cx: &mut App);

    /// The host's versions of the open request in the timeline, when there is more than one.
    fn show_request_versions(&self, versions: Option<&(u64, Vec<Version>)>, cx: &mut App);

    /// `hear` is told whenever the branches change, to draw again.
    fn on_change(&self, hear: std::rc::Rc<dyn Fn(&mut App)>, cx: &mut App);

    /// What the diff the review shows is of, in a few words (its title on `route`, a comparison narrowed to one
    /// commit that commit's subject).
    fn diff_title(&self, route: Option<&Route>, cx: &App) -> Option<String>;
}

/// A branch, by its refname.
#[derive(Serialize, Deserialize)]
pub struct BranchRoute {
    pub refname: String,
}

impl Place for BranchRoute {
    const KIND: &'static str = "branches.branch";
}

/// A commit of a branch.
#[derive(Serialize, Deserialize)]
pub struct BranchCommitRoute {
    pub refname: String,
    pub commit: String,
}

impl Place for BranchCommitRoute {
    const KIND: &'static str = "branches.commit";
}

/// A run of a branch's commits, newest and oldest.
#[derive(Serialize, Deserialize)]
pub struct BranchCommitsRoute {
    pub refname: String,
    pub newest: String,
    pub oldest: String,
}

impl Place for BranchCommitsRoute {
    const KIND: &'static str = "branches.commits";
}

/// Two versions of a branch compared.
#[derive(Serialize, Deserialize)]
pub struct VersionsRoute {
    pub refname: String,
    pub from: usize,
    pub to: usize,
}

impl Place for VersionsRoute {
    const KIND: &'static str = "branches.versions";
}

/// What is not committed.
#[derive(Serialize, Deserialize)]
pub struct WorkingTreeRoute {}

impl Place for WorkingTreeRoute {
    const KIND: &'static str = "branches.working-tree";
}

/// Two refs compared, each with the commit it was at; since their merge base or directly.
#[derive(Serialize, Deserialize)]
pub struct CompareRoute {
    pub base: String,
    pub base_commit: String,
    pub head: String,
    pub head_commit: String,
    pub since: bool,
}

impl Place for CompareRoute {
    const KIND: &'static str = "branches.compare";
}
