//! Merge requests' contract (#165 stage 4): the routes of a request's pages — the request, a commit of it, a run of
//! its commits, two of its versions compared —, which anyone opens through the `Frame`; and what other features ask
//! of the requests read (`Requests`, a tab's service).

use artoo_core::git::{BranchRef, DiffLine, Version, WebRemote};
use artoo_core::mr::{Approvals, Draft, Merge, Mr, MrState, Thread};
use artoo_core::route::Place;
use artoo_ui_kit::complete::Choice;
use artoo_ui_kit::markdown::Selecting;
use gpui::{AnyElement, App, KeyDownEvent, Pixels, Point};
use std::path::PathBuf;
use std::rc::Rc;
use serde::{Deserialize, Serialize};

/// Which requests the list shows: All · To review · Mine in Merge requests' bar (#70, #72).
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum Scope {
    #[default]
    All,
    /// The token's user is asked to review.
    ReviewMe,
    /// The token's user wrote it.
    Mine,
}

/// Where a reason it cannot merge is fixed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fix {
    /// Nothing to go to: someone has to act (another approval).
    Nowhere,
    /// The request's discussions, on its Overview.
    Discussions,
    /// A page on the host: the pipeline, the request itself.
    Page(String),
}

/// One thing in the way: what, the words, where it is fixed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub what: &'static str,
    pub detail: String,
    pub fix: Fix,
}

/// Where merging stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stand {
    /// Set to merge when the pipeline succeeds.
    Auto,
    InTheWay(Vec<Block>),
    /// The host is still working it out.
    Checking,
    /// Behind its target by this many commits, and the host can rebase it (GitLab).
    Rebase(u32),
    /// The pipeline runs: merging can be set for when it succeeds.
    Running,
    Ready,
}

/// What Merge's card is drawn with that the tab knows.
pub struct CardLook {
    /// The commits of the request's branch.
    pub commits: usize,
    /// The host is GitHub.
    pub github: bool,
    /// The message has the keyboard.
    pub active: bool,
}

/// What a key in ☺+ asks (`Requests::reaction_key`).
pub enum GridKey {
    /// ↵: react with the one picked (none when nothing matches), and close.
    React(Option<String>),
    /// Esc.
    Close,
}

/// A change sent to a request's host (#139), as data: the requests show it at once, faded, and make the host's call.
pub enum Change {
    /// A reply in discussion `thread`.
    Reply { thread: String, body: String },
    /// A comment on the request.
    Comment { body: String },
    /// A comment on a line of the diff at `head` (`range` its first and last line when it spans several), or a
    /// pending one (`draft`).
    OnLine {
        head: String,
        path: String,
        old_path: String,
        new_line: Option<u32>,
        old_line: Option<u32>,
        range: Option<(DiffLine, DiffLine)>,
        body: String,
        draft: bool,
    },
    /// Note `note` of discussion `thread` written again, or deleted.
    Edit { thread: String, note: u64, body: String },
    Delete { thread: String, note: u64 },
    Resolve { thread: String, resolved: bool },
    /// One's approval given (on `head`), or taken back.
    Approve { head: String },
    Unapprove,
    /// A pending comment deleted, or sent alone.
    DropDraft(u64),
    PublishDraft(u64),
}

/// What follows a change sent once its host answered: `Ok`, taken; `Err(Some(why))`, refused; `Err(None)`, refused
/// and said already (Submit review's card back).
pub type Answered = Box<dyn FnOnce(Result<(), Option<String>>, &mut App)>;

/// Where the open request's discussions and pending comments went when what shows of it was made again: by their
/// place before, their place now (`None`: gone).
#[derive(Default)]
pub struct Moved {
    pub threads: Vec<Option<usize>>,
    pub drafts: Vec<Option<usize>>,
}

/// Which notifications the Inbox lists.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum InboxFilter {
    #[default]
    All,
    Mentions,
    /// What your agents answered (#116).
    Agents,
    Reviews,
}

/// A row of the Inbox: a notice from the host, or a note of yours an agent answered (#116), by its comment's id.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InboxRow {
    Notice(usize),
    Agent(u64),
}

/// Your notes an agent answered that wait for you (#116): each note's id, when the agent last wrote, and the key that
/// is read once looked at (a new answer is new again).
pub type Answers<'a> = &'a [(u64, i64, String)];

/// How a row of the Inbox looks.
pub struct RowLook {
    /// Not read yet: bold, its dot blue.
    pub fresh: bool,
    /// Selected.
    pub on: bool,
    /// The list has 260 px (#79): when, and the start of what was written.
    pub wide: bool,
}

/// What a click in the Inbox asks the tab.
pub enum InboxEvent {
    /// A filter chosen: its first row is selected.
    Filter(InboxFilter),
    /// Notice `0`'s row clicked: selected, or opened where the tab is narrow (#80).
    Pick(usize),
    /// Notice `0`'s request opened.
    Open(usize),
    /// Done with notice `0`.
    Done(usize),
    /// The composer under the notice selected.
    Reply,
    /// Discussion `id` of request `iid` resolved, or opened again.
    Resolve { iid: u64, id: String, resolved: bool },
    /// Request `iid`'s menu, at the pointer.
    Menu { iid: u64, at: Point<Pixels> },
}

/// How the tab answers the Inbox's clicks.
pub type OnInbox = Rc<dyn Fn(InboxEvent, &mut App)>;

/// What the Inbox asks of the tab (`Rc<dyn InboxHost>` in its services).
pub trait InboxHost {
    /// Your notes an agent answered that wait for you: each one's id, when the agent last wrote, and the key read
    /// once it is looked at (a new answer is new again).
    fn answers(&self, cx: &App) -> Vec<(u64, i64, String)>;

    /// The row of the answer to note `id`.
    fn answer_row(&self, id: u64, look: RowLook, cx: &App) -> AnyElement;

    /// The answer to note `id` previewed; `None` once it is gone.
    fn answer_preview(&self, id: u64, cx: &App) -> Option<AnyElement>;

    /// The agent says it is done with note `id`: E resolves it.
    fn answer_done(&self, id: u64, cx: &App) -> bool;

    /// The composer, while it writes a reply to the notice selected.
    fn writing(&self, cx: &App) -> Option<AnyElement>;

    /// Selecting in comments.
    fn selecting(&self, cx: &App) -> Option<Selecting>;

    /// Comments drawn as markdown.
    fn rich(&self, cx: &App) -> bool;

    /// The tab narrow: no preview beside the list, and a click opens a notice (#80).
    fn narrow(&self, cx: &App) -> bool;

    /// A click in the Inbox.
    fn act(&self, event: InboxEvent, cx: &mut App);
}

/// What a notice is answered with: its request, its discussion (none: a comment on the request), and its path and
/// line when the discussion is on code.
pub type ReplyTo = (u64, Option<String>, Option<(String, u32)>);

/// A notification new since the last read, said to the system (#26): its key, its request and the request's title,
/// who did it and what ("mentioned you").
pub struct News {
    pub key: String,
    pub iid: u64,
    pub title: String,
    pub who: String,
    pub verb: &'static str,
}

/// What other features ask of a tab's merge requests, from its services (`Rc<dyn Requests>`).
pub trait Requests {
    /// The request on branch `branch` (a local or remote one by its name, a request's own ref by its number): its
    /// number and title.
    fn of_branch(&self, branch: &BranchRef, cx: &App) -> Option<(u64, String)>;

    /// The request open, by its number: the one whose branch is read.
    fn current(&self, cx: &App) -> Option<u64>;

    /// The list is being read.
    fn listing(&self, cx: &App) -> bool;

    /// A project without a token: connecting is offered instead of the list.
    fn connectable(&self, cx: &App) -> bool;

    /// Of `numbers`, those the list read lacks (merged, closed, gone); none while the list failed.
    fn unlisted(&self, numbers: &[u64], cx: &App) -> Vec<u64>;

    /// Esc from request `number`'s page back to the list: its row is picked and brought into view.
    fn back_to_list(&self, number: u64, cx: &mut App);

    /// The versions the host keeps of the request open (its number with them), once read.
    fn versions(&self, cx: &App) -> Option<(u64, Vec<Version>)>;

    /// Request `number` wherever it was read: the open list, the merged or closed, those that left the list.
    fn find(&self, number: u64, cx: &App) -> Option<Mr>;

    /// The open list.
    fn list(&self, cx: &App) -> Vec<Mr>;

    /// The host has a token: the requests are read.
    fn available(&self, cx: &App) -> bool;

    /// The token's username: one's own notes are the ones it wrote.
    fn login(&self, cx: &App) -> Option<String>;

    /// The project's members, (username, name).
    fn members(&self, cx: &App) -> Vec<(String, String)>;

    /// Who approved the request open, and where you stand.
    fn approvals(&self, cx: &App) -> Option<Approvals>;

    /// The request open's discussions, discussion `ix`, and your pending comments on it.
    fn threads(&self, cx: &App) -> Vec<Thread>;
    fn thread(&self, ix: usize, cx: &App) -> Option<Thread>;
    fn drafts(&self, cx: &App) -> Vec<Draft>;

    /// Where pending comment `ix` is: its own place, or for a reply its discussion's — (path, new line, old line).
    fn draft_place(&self, ix: usize, cx: &App) -> Option<(String, Option<u32>, Option<u32>)>;

    /// The discussions on lines of file `path`: (index, old side, line).
    fn threads_in(&self, path: &str, cx: &App) -> Vec<(usize, bool, u32)>;

    /// What a long hover on request `number`'s title says.
    fn request_tip(&self, number: u64, cx: &App) -> Option<String>;

    /// The host's id of request `number`'s version at `head` (a link to two of them compared names the newer by it).
    fn version_id(&self, number: u64, head: git2::Oid, cx: &App) -> Option<u64>;

    /// Request `number` was read alone already (gone from the list, a link's): one try each.
    fn tried(&self, number: u64, cx: &App) -> bool;

    /// How many of the open list `scope` lists.
    fn count_in(&self, scope: Scope, cx: &App) -> usize;

    /// ☺+ is open; Merge's card is open.
    fn grid_open(&self, cx: &App) -> bool;
    fn merge_card_open(&self, cx: &App) -> bool;

    /// The list completing the token before `caret` in `text`, unless it was closed with esc at `closed`.
    fn completion(
        &self,
        text: &str,
        caret: usize,
        closed: Option<usize>,
        cx: &App,
    ) -> Option<(usize, Vec<Choice>)>;

    /// The files of request `number`'s whole diff when it was last loaded (its Changes tab's count); a diff of a
    /// request arrived.
    fn files(&self, number: u64, cx: &App) -> Option<usize>;
    fn set_files(&self, files: Option<(u64, usize)>, cx: &mut App);

    /// The row picked in the list.
    fn picked_row(&self, cx: &App) -> Option<u64>;

    /// The request open's approval chip and Merge's control, on its bar.
    fn approval_chip(&self, cx: &mut App) -> Option<AnyElement>;
    fn merge_control(&self, compact: bool, github: bool, cx: &mut App) -> Option<AnyElement>;

    /// The repository open and its remote's web address: what the requests read and ask their host.
    fn set_repository(&self, root: Option<PathBuf>, web: Option<WebRemote>, cx: &mut App);

    /// The list: nothing read (a repository without a remote); whether a token is there, connecting is offered or
    /// a token is being read; cleared (no token); read.
    fn reset(&self, cx: &mut App);
    fn set_token(&self, available: bool, connectable: bool, loading: bool, cx: &mut App);
    fn clear_list(&self, cx: &mut App);
    fn read_list(&self, cx: &mut App);

    /// The list shows the merged or the closed requests, or the open ones again (#114); the scope it shows.
    fn set_view(&self, state: MrState, cx: &mut App);
    fn scope(&self, cx: &App) -> Scope;

    /// The next page of `scope`'s merged or closed requests; the page that could not be read, again; what became
    /// of requests `numbers`, gone from the open list or a link's.
    fn load_past_page(&self, scope: Scope, cx: &mut App);
    fn retry_past_page(&self, cx: &mut App);
    fn read_left(&self, numbers: Vec<u64>, cx: &mut App);

    /// The open request's discussions, pending comments and approvals read again; request `number` read as the
    /// one opened; its versions read (what its host compares it from forgotten first).
    fn reload_threads(&self, cx: &mut App);
    fn read_open(&self, number: u64, cx: &mut App);
    fn load_versions(&self, number: u64, cx: &mut App);

    /// The lines of the discussions written on an earlier version followed to the latest; the reactions on the
    /// open request's notes read; your reaction `name` on note `note` added or taken back.
    fn compute_histories(&self, cx: &mut App);
    fn load_awards(&self, cx: &mut App);
    fn toggle_reaction(&self, note: u64, name: &str, cx: &mut App);

    /// The request picked in the list: its pipeline and approvals, and with `viewed` (the head you viewed, the one
    /// now) what changed since.
    fn read_preview(&self, request: &Mr, viewed: Option<(git2::Oid, git2::Oid)>, cx: &mut App);

    /// Where merging the open request stands, read again; the merge sent; the host's rebase; the merge when the
    /// pipeline succeeds cancelled.
    fn reload_merge(&self, cx: &mut App);
    fn run_merge(&self, cx: &mut App);
    fn rebase(&self, cx: &mut App);
    fn cancel_auto_merge(&self, cx: &mut App);

    /// The host merged request `number` (`merge` what was sent), or set it to merge when the pipeline succeeds:
    /// what a toast says of it, the open one's merge state following.
    fn merged(&self, number: u64, merge: &Merge, github: bool, cx: &mut App) -> Option<String>;

    /// Merge's card: opened (`when` the pipeline succeeds), its keys (whether one merges), gone with the merge's
    /// state (a push came after the version on the page).
    fn open_merge_card(&self, when: bool, github: bool, cx: &mut App) -> bool;
    fn merge_key(&self, event: &KeyDownEvent, cx: &mut App) -> bool;
    fn drop_merge(&self, cx: &mut App);

    /// The New merge request form sends its request; the number of the one created, or of the one there is.
    fn submit_create(&self, cx: &mut App) -> Option<u64>;

    /// Where merging the open request stands; its pipeline's page.
    fn merge_stand(&self, cx: &App) -> Option<Stand>;
    fn pipeline_url(&self, cx: &App) -> Option<String>;

    /// Merge's card, while it is open.
    fn merge_card(&self, look: CardLook, cx: &mut App) -> Option<AnyElement>;

    /// ☺+ opened on note `note`, closed; the note it is open on; a key in it (`github` the host's emoji).
    fn open_grid(&self, note: u64, cx: &mut App);
    fn close_grid(&self, cx: &mut App);
    fn grid_note(&self, cx: &App) -> Option<u64>;
    fn reaction_key(&self, event: &KeyDownEvent, github: bool, cx: &mut App) -> Option<GridKey>;

    /// `change` sent on request `number`: it shows at once, faded, the request's notices read again on their next
    /// look; `answered` follows the host's answer. `false`: there is no host.
    fn send(&self, number: u64, change: Change, answered: Answered, cx: &mut App) -> bool;
    /// What Submit review's card submits, sent (`answered` as `send`'s); `false` when nothing went (the card says
    /// why).
    fn submit(&self, answered: Answered, cx: &mut App) -> bool;
    /// What shows of request `number` made again with the changes waiting on it, `me` the name they show by:
    /// where the discussions and pending comments went, when it is the one open.
    fn show_waiting(&self, number: u64, me: &str, cx: &mut App) -> Option<Moved>;
    /// The changes the request open and the Inbox both hold now are let go.
    fn settle_waiting(&self, cx: &mut App);
    /// An id given to what is not on the host yet: a note or a pending comment, a discussion.
    fn is_temp(&self, id: u64) -> bool;
    fn is_temp_thread(&self, id: &str) -> bool;

    /// The Inbox's notifications read again in the background, and every few minutes after (`me` the name the
    /// changes waiting show by); what is new is told as an event. `refresh_notices`: everything read afresh, not
    /// only what reports a change (the Inbox opened).
    fn read_notices(&self, me: &str, cx: &mut App);
    fn refresh_notices(&self, me: &str, cx: &mut App);
    /// Notice `ix` opened, read from now on: its request and its note.
    fn open_notice(&self, ix: usize, cx: &mut App) -> Option<(u64, Option<u64>)>;
    /// `e`: done with notice `ix` — a to-do marked done on the host (refused, it comes back and the tab hears why),
    /// a comment read from now on.
    fn dismiss_notice(&self, ix: usize, cx: &mut App);
    /// Everything listed read, and `others` (the agents' answers' keys) with it; key `key` read.
    fn read_all_notices(&self, others: Vec<String>, cx: &mut App);
    fn mark_read(&self, key: String, cx: &mut App);
    /// The notice keyed `key`, by its place; the request of notice `ix`; the request of the notice selected.
    fn notice_by_key(&self, key: &str, cx: &App) -> Option<usize>;
    fn notice_request(&self, ix: usize, cx: &App) -> Option<u64>;
    fn selected_notice(&self, cx: &App) -> Option<u64>;
    /// Request `number` has notices not read.
    fn unread_of(&self, number: u64, cx: &App) -> bool;
    /// The Inbox's count: notices not read, and the agents' `answers` not looked at.
    fn unread_with(&self, answers: Answers, cx: &App) -> usize;
    /// The discussions of request `number` waiting for its author, as the notifications read them.
    fn waiting_threads(&self, number: u64, cx: &App) -> Vec<Thread>;

    /// What the Inbox lists under its filter, newest first, with the agents' `answers`; the row selected; the row
    /// after it or before it (j k).
    fn inbox_rows(&self, answers: Answers, cx: &App) -> Vec<InboxRow>;
    fn inbox_current(&self, answers: Answers, cx: &App) -> Option<InboxRow>;
    fn inbox_step(&self, answers: Answers, down: bool, cx: &App) -> Option<InboxRow>;
    /// Row `row` selected (none: the first notice).
    fn inbox_pick(&self, row: Option<InboxRow>, cx: &mut App);
    /// The code the selected notice's discussion is on, read once, in the background.
    fn read_excerpt(&self, cx: &mut App);
    fn set_inbox_filter(&self, filter: InboxFilter, cx: &mut App);
    /// The Inbox's filters, in its page bar.
    fn inbox_filters(&self, on: OnInbox, cx: &App) -> AnyElement;
    /// What the notice selected is answered with: its request, its discussion (none: a comment on the request)
    /// and where that is on the code; none for a review request or a to-do.
    fn inbox_reply_to(&self, cx: &App) -> Option<ReplyTo>;
    /// What the Inbox's composer writes to: a request, and its discussion.
    fn inbox_reply(&self, cx: &App) -> Option<(u64, Option<String>)>;
    fn set_inbox_reply(&self, reply: Option<(u64, Option<String>)>, cx: &mut App);
}

/// A merge request, by its number.
#[derive(Serialize, Deserialize)]
pub struct RequestRoute {
    pub number: u64,
}

impl Place for RequestRoute {
    const KIND: &'static str = "requests.request";
}

/// A commit of a merge request.
#[derive(Serialize, Deserialize)]
pub struct RequestCommitRoute {
    pub number: u64,
    pub commit: String,
}

impl Place for RequestCommitRoute {
    const KIND: &'static str = "requests.commit";
}

/// A run of a merge request's commits, newest and oldest.
#[derive(Serialize, Deserialize)]
pub struct RequestCommitsRoute {
    pub number: u64,
    pub newest: String,
    pub oldest: String,
}

impl Place for RequestCommitsRoute {
    const KIND: &'static str = "requests.commits";
}

/// Two versions of a merge request compared.
#[derive(Serialize, Deserialize)]
pub struct RequestVersionsRoute {
    pub number: u64,
    pub from: usize,
    pub to: usize,
}

impl Place for RequestVersionsRoute {
    const KIND: &'static str = "requests.versions";
}
