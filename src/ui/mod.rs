//! The Artoo window: branches, versions and commits on the left, the diff on the right.

use artoo_ui_kit::hover::TipOwned;
pub(crate) use artoo_ui_kit::{apply_ui_zoom, flat, px, ui_zoom};
mod hover;
use artoo_ui_kit::format::plural;
use artoo_ui_kit::{
    ISLAND_RADIUS, ROW_RADIUS, discussion, format, gap, icons, island, keys, markdown, room, row,
    theme, words,
};
mod agent;
mod agent_comments;
mod agent_events;
mod agent_notes;
mod annotate;
mod appearance;
mod bar;
mod beside;
mod bookmarks;
mod code;
mod compare;
mod complete;
mod composer;
mod create;
mod faces;
mod fetch;
mod file_view;
mod find;
mod findfiles;
mod fonts;
mod frame;
mod goline;
mod goto;
mod inbox;
mod input;
mod keyboard;
mod keymap;
mod labeling;
mod lists;
mod md_page;
mod menu;
mod merging;
mod motion;
mod narrow;
mod nav;
mod notifications;
mod open_in;
mod opening;
mod outdated;
mod pages;
mod palette;
mod parts;
mod pins;
mod rail;
mod reactions;
mod recents;
mod replies;
mod request_page;
mod requests;
mod reviewed;
mod roots;
mod routes;
mod select;
mod settings;
mod setup;
mod share;
mod shell;
mod shortcuts;
mod side;
mod spaces;
mod stack;
mod submit;
mod system_notes;
mod tabs;
mod theme_cards;
mod toast;
mod tools;
mod typing;
mod waiting;
mod watch;
mod welcome;
mod working;
mod zones;

use crate::git::{DiffSettings, Repo};
use crate::rows::{self, Row};
use crate::storage::{self, DiffMode, Layout, Settings, ViewOptions};
use artoo_branches::head;
use artoo_branches::history::History;
use artoo_branches_api::{Event as BranchesEvent, Selection};
use artoo_review::diff::{Diff, Header, Notice};
use artoo_review::diff_view::{self, CODE_SIZE};
use artoo_review::review::{self, Opt};
use artoo_shell_api::frame::{Drag, Split};
use artoo_shell_api::title;
use artoo_ui_kit::controls::tag;
use gpui::{
    App, Context, FocusHandle, KeyBinding, KeyDownEvent, ListAlignment, ListState, Menu, MenuItem,
    MouseButton, MouseDownEvent, MouseMoveEvent, PathPromptOptions, Point, SharedString, Task,
    TitlebarOptions, UniformListScrollHandle, Window, WindowBounds, WindowOptions, actions, div,
    font, prelude::*, size,
};
use lists::Field;
use menu::{Act, Entry};
use review::ReviewEvent;
use std::cell::Cell as Shared;
use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use artoo_hosts_api::Host;

/// The host `remote` is on, from the hosts the app was given (`main.rs`). A worker takes it on the window's
/// thread, before it leaves.
pub(crate) fn host_of(remote: &crate::git::WebRemote, cx: &App) -> Arc<dyn Host> {
    artoo_hosts_api::get(cx).of(remote)
}

actions!(
    artoo,
    [
        Open,
        SaveTheme,
        ToggleAgents,
        FoldMenu,
        GoToInbox,
        GoToCode,
        GoToBranches,
        GoToMergeRequests,
        Refresh,
        PreviousCommit,
        NextCommit,
        PreviousFile,
        NextFile,
        PreviousCommitOrFile,
        NextCommitOrFile,
        ToggleUnified,
        CycleMode,
        SetLines,
        SetWords,
        SetStructural,
        NextMatch,
        PreviousMatch,
        FindCommits,
        FilterFiles,
        GoBack,
        GoForward,
        ToggleSidebar,
        ToggleFiles,
        ToggleRequests,
        FocusDiff,
        ZenOneFile,
        SetUp,
        MergeRequest,
        CopySelection,
        SelectAll,
        NextChange,
        PreviousChange,
        OpenSettings,
        Find,
        ToggleWrap,
        ToggleFullContext,
        ToggleWhitespace,
        ToggleOneFile,
        ToggleExpandAll,
        ToggleRichComments,
        ToggleRendered,
        PickTheme,
        LeaveRequest,
        ToggleAllReviewed,
        NextRequest,
        PreviousRequest,
        CommentLine,
        SubmitReview,
        ApproveRequest,
        SwitchTo,
        NextDraft,
        PreviousDraft,
        CreateRequest,
        QuickCreateRequest,
        SendToAgent,
        OpenIn,
        RevealInFinder,
        CopyLink,
        PinTab,
        ShowShortcuts,
        NameProject,
        ToggleThread,
        ToggleComments,
        ShowOverview,
        NextSection,
        PreviousSection,
        NextOpenItem,
        PreviousOpenItem,
        MarkReviewed,
        OpenRecent,
        ZoomIn,
        ZoomOut,
        ZoomReset,
        ToggleBookmark,
        ShowBookmarks,
        ShowStructure,
        ToggleAnnotate,
        FindInFiles,
        ToggleReviewed,
        NextThread,
        PreviousThread,
        ReplyThread,
        ResolveThread,
        EditComment,
        ReactNote,
        NextZone,
        PreviousZone,
        Activate,
        PageUp,
        PageDown,
        DiffStart,
        DiffEnd,
        SwitchBranch,
        FocusCommits,
        FocusFiles,
        FocusDiffZone,
        NewTab,
        CloseItem,
        CloseTab,
        ReopenTab,
        NextTab,
        PreviousTab,
        Tab1,
        Tab2,
        Tab3,
        Tab4,
        Tab5,
        Tab6,
        Tab7,
        Tab8,
        Tab9,
        SearchEverywhere,
        SearchFiles,
        SearchCommits,
        SearchBranches,
        SearchRequests,
        SearchActions,
        Compare,
        PaletteUp,
        PaletteDown,
        Quit
    ]
);

/// How several projects show: `storage::Projects::ALL[n]`. With flat panes (`artoo_ui_kit::flat`), the
/// window-wide part of the settings: every tab and window reads it, set when settings load or change.
static PROJECTS: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

fn projects() -> storage::Projects {
    let n = PROJECTS.load(std::sync::atomic::Ordering::Relaxed) as usize;
    storage::Projects::ALL[n.min(storage::Projects::ALL.len() - 1)]
}

/// Makes `settings`' window-wide choices the ones every tab draws.
fn share_settings(settings: &Settings) {
    use std::sync::atomic::Ordering::Relaxed;
    artoo_ui_kit::set_flat(settings.flat);
    let n = storage::Projects::ALL
        .iter()
        .position(|p| *p == settings.projects)
        .unwrap_or(0);
    PROJECTS.store(n as u8, Relaxed);
}

/// The keys of the app as shipped; `keymap.txt` changes them (`keymap.rs`).
fn default_bindings() -> Vec<KeyBinding> {
    bindings(crate::platform::keys)
}

/// The shipped keys, written with macOS keys and put in this OS's terms by `k`.
/// The keys the commands features offer ship with (`artoo_shell_api::Action::key`), in the tab.
fn offered_bindings(k: fn(&str) -> String, cx: &App) -> Vec<KeyBinding> {
    let tab: Option<std::rc::Rc<gpui::KeyBindingContextPredicate>> =
        gpui::KeyBindingContextPredicate::parse("Workspace")
            .ok()
            .map(Into::into);
    artoo_shell_api::actions(cx)
        .iter()
        .filter_map(|a| {
            KeyBinding::load(
                &k(a.key?),
                a.act.boxed_clone(),
                tab.clone(),
                false,
                None,
                &gpui::DummyKeyboardMapper,
            )
            .ok()
        })
        .collect()
}

fn bindings(k: fn(&str) -> String) -> Vec<KeyBinding> {
    vec![
        KeyBinding::new(&k("cmd-o"), Open, None),
        KeyBinding::new(&k("cmd-r"), Refresh, None),
        KeyBinding::new(&k("cmd-q"), Quit, None),
        KeyBinding::new(&k("cmd-,"), OpenSettings, None),
        KeyBinding::new(&k("cmd-f"), Find, Some("Workspace")),
        KeyBinding::new(&k("cmd-s"), SaveTheme, Some("Workspace")),
        KeyBinding::new(&k("cmd-shift-f"), FindCommits, Some("Workspace")),
        KeyBinding::new(&k("cmd-alt-p"), FilterFiles, Some("Workspace")),
        KeyBinding::new(&k("cmd-["), GoBack, Some("Workspace")),
        KeyBinding::new(&k("cmd-]"), GoForward, Some("Workspace")),
        // ⌘J the side panel, as ⌘B the menu (the developer, 2026-10-23; ⌥⌘1 before).
        KeyBinding::new(&k("cmd-j"), ToggleSidebar, Some("Workspace")),
        KeyBinding::new(&k("cmd-alt-2"), ToggleFiles, Some("Workspace")),
        KeyBinding::new(&k("cmd-alt-3"), ToggleRequests, Some("Workspace")),
        KeyBinding::new(&k("c"), CommentLine, Some("Workspace && !Typing")),
        KeyBinding::new(
            &k("cmd-shift-enter"),
            SubmitReview,
            Some("Workspace && !Typing"),
        ),
        KeyBinding::new(&k("cmd-alt-m"), CreateRequest, Some("Workspace && !Typing")),
        // ⇧⌘A the Agents window as Design gives it, ⌥⌘A sends to an agent (D12, X3).
        KeyBinding::new(&k("cmd-alt-a"), SendToAgent, Some("Workspace && !Typing")),
        // Open in (#78): the page's file or folder in its app, or in Finder.
        KeyBinding::new(&k("cmd-alt-o"), OpenIn, Some("Workspace && !Typing")),
        KeyBinding::new(
            &k("cmd-alt-r"),
            RevealInFinder,
            Some("Workspace && !Typing"),
        ),
        KeyBinding::new(
            &k("cmd-shift-a"),
            ToggleAgents,
            Some("Workspace && !Typing"),
        ),
        // The menu folds with ⌘B (D2; ⌘0 stays zoom reset, the switch picker has no
        // key); in a field ⌘B is bold. ⌃⌘1…4 the spaces, Ctrl+Shift+1…4 on Windows and Linux.
        KeyBinding::new(&k("cmd-b"), FoldMenu, Some("Workspace && !Typing")),
        KeyBinding::new(&k("ctrl-cmd-1"), GoToInbox, Some("Workspace")),
        KeyBinding::new(&k("ctrl-cmd-2"), GoToCode, Some("Workspace")),
        KeyBinding::new(&k("ctrl-cmd-3"), GoToBranches, Some("Workspace")),
        KeyBinding::new(&k("ctrl-cmd-4"), GoToMergeRequests, Some("Workspace")),
        KeyBinding::new(&k("a"), ApproveRequest, Some("Workspace && !Typing")),
        // The Inbox's older key, from when the bell opened a window (#72).
        KeyBinding::new(&k("cmd-shift-n"), GoToInbox, Some("Workspace")),
        KeyBinding::new(&k("alt-n"), NextDraft, Some("Workspace && !Typing")),
        KeyBinding::new(&k("alt-p"), PreviousDraft, Some("Workspace && !Typing")),
        KeyBinding::new(&k("?"), ShowShortcuts, Some("Workspace && !Typing")),
        KeyBinding::new(&k("cmd-/"), ShowShortcuts, Some("Workspace")),
        KeyBinding::new(&k("shift-n"), NextThread, Some("Workspace && !Typing")),
        KeyBinding::new(&k("shift-p"), PreviousThread, Some("Workspace && !Typing")),
        KeyBinding::new(&k("r"), ReplyThread, Some("Workspace && !Typing")),
        KeyBinding::new(&k("x"), ResolveThread, Some("Workspace && !Typing")),
        KeyBinding::new(&k("e"), EditComment, Some("Workspace && !Typing")),
        KeyBinding::new(&k("shift-e"), ReactNote, Some("Workspace && !Typing")),
        KeyBinding::new(&k("o"), ToggleThread, Some("Workspace && !Typing")),
        KeyBinding::new(
            &k("cmd-shift-d"),
            ToggleComments,
            Some("Workspace && !Typing"),
        ),
        KeyBinding::new(&k("cmd-i"), ShowOverview, Some("Workspace && !Typing")),
        KeyBinding::new(&k("ctrl-tab"), NextSection, Some("Workspace")),
        KeyBinding::new(&k("ctrl-shift-tab"), PreviousSection, Some("Workspace")),
        // The items open under the space, as a browser's tabs: ⌥⌘→ ⌥⌘←, not ⌃→ (macOS's Spaces).
        KeyBinding::new(
            &k("cmd-alt-right"),
            NextOpenItem,
            Some("Workspace && !Typing"),
        ),
        KeyBinding::new(
            &k("cmd-alt-left"),
            PreviousOpenItem,
            Some("Workspace && !Typing"),
        ),
        KeyBinding::new(&k("v"), MarkReviewed, Some("Workspace && !Typing")),
        KeyBinding::new(&k("cmd-e"), OpenRecent, Some("Workspace")),
        KeyBinding::new(&k("cmd-="), ZoomIn, Some("Workspace")),
        KeyBinding::new(&k("cmd-+"), ZoomIn, Some("Workspace")),
        KeyBinding::new(&k("cmd-shift-="), ZoomIn, Some("Workspace")),
        KeyBinding::new(&k("cmd--"), ZoomOut, Some("Workspace")),
        KeyBinding::new(&k("cmd-_"), ZoomOut, Some("Workspace")),
        KeyBinding::new(&k("cmd-0"), ZoomReset, Some("Workspace")),
        KeyBinding::new(&k("f3"), ToggleBookmark, Some("Workspace && !Typing")),
        KeyBinding::new(&k("cmd-f3"), ShowBookmarks, Some("Workspace")),
        KeyBinding::new(&k("cmd-shift-o"), ShowStructure, Some("Workspace")),
        KeyBinding::new(
            &k("cmd-alt-b"),
            ToggleAnnotate,
            Some("Workspace && !Typing"),
        ),
        KeyBinding::new(&k("cmd-alt-f"), FindInFiles, Some("Workspace")),
        KeyBinding::new(&k("shift-v"), ToggleReviewed, Some("Workspace && !Typing")),
        KeyBinding::new(&k("tab"), NextZone, Some("Workspace && !Typing")),
        KeyBinding::new(&k("shift-tab"), PreviousZone, Some("Workspace && !Typing")),
        KeyBinding::new(&k("enter"), Activate, Some("Workspace && !Typing")),
        KeyBinding::new(&k("pagedown"), PageDown, Some("Workspace && !Typing")),
        KeyBinding::new(&k("space"), PageDown, Some("Workspace && !Typing")),
        KeyBinding::new(&k("pageup"), PageUp, Some("Workspace && !Typing")),
        KeyBinding::new(&k("shift-space"), PageUp, Some("Workspace && !Typing")),
        KeyBinding::new(&k("home"), DiffStart, Some("Workspace && !Typing")),
        KeyBinding::new(&k("end"), DiffEnd, Some("Workspace && !Typing")),
        KeyBinding::new(&k("h"), PreviousFile, Some("Workspace && !Typing")),
        KeyBinding::new(&k("l"), NextFile, Some("Workspace && !Typing")),
        // [ ] walk a commit's neighbours on its page, else the files as ← → do (the developer,
        // 2026-10-23).
        KeyBinding::new(&k("["), PreviousCommitOrFile, Some("Workspace && !Typing")),
        KeyBinding::new(&k("]"), NextCommitOrFile, Some("Workspace && !Typing")),
        KeyBinding::new(&k("ctrl-1"), SwitchBranch, Some("Workspace")),
        KeyBinding::new(&k("ctrl-2"), FocusCommits, Some("Workspace")),
        KeyBinding::new(&k("ctrl-3"), FocusFiles, Some("Workspace")),
        KeyBinding::new(&k("ctrl-4"), FocusDiffZone, Some("Workspace")),
        KeyBinding::new(&k("cmd-t"), NewTab, Some("Workspace")),
        // ⌘W closes what is open on screen; a project's tab is ⇧⌘W (#71).
        KeyBinding::new(&k("cmd-w"), CloseItem, Some("Workspace")),
        KeyBinding::new(&k("cmd-shift-w"), CloseTab, Some("Workspace")),
        KeyBinding::new(&k("cmd-shift-t"), ReopenTab, Some("Workspace")),
        KeyBinding::new(&k("cmd-shift-]"), NextTab, Some("Workspace")),
        KeyBinding::new(&k("cmd-shift-["), PreviousTab, Some("Workspace")),
        KeyBinding::new(&k("cmd-1"), Tab1, Some("Workspace")),
        KeyBinding::new(&k("cmd-2"), Tab2, Some("Workspace")),
        KeyBinding::new(&k("cmd-3"), Tab3, Some("Workspace")),
        KeyBinding::new(&k("cmd-4"), Tab4, Some("Workspace")),
        KeyBinding::new(&k("cmd-5"), Tab5, Some("Workspace")),
        KeyBinding::new(&k("cmd-6"), Tab6, Some("Workspace")),
        KeyBinding::new(&k("cmd-7"), Tab7, Some("Workspace")),
        KeyBinding::new(&k("cmd-8"), Tab8, Some("Workspace")),
        KeyBinding::new(&k("cmd-9"), Tab9, Some("Workspace")),
        KeyBinding::new(&k("cmd-."), FocusDiff, Some("Workspace")),
        KeyBinding::new(&k("cmd-alt-."), ZenOneFile, Some("Workspace")),
        KeyBinding::new(&k("cmd-c"), CopySelection, Some("Workspace")),
        KeyBinding::new(&k("cmd-a"), SelectAll, Some("Workspace && !Typing")),
        KeyBinding::new(&k("f7"), NextChange, Some("Workspace")),
        KeyBinding::new(&k("shift-f7"), PreviousChange, Some("Workspace")),
        KeyBinding::new(&k("n"), NextChange, Some("Workspace && !Typing")),
        KeyBinding::new(&k("p"), PreviousChange, Some("Workspace && !Typing")),
        KeyBinding::new(&k("cmd-g"), NextMatch, Some("Workspace")),
        KeyBinding::new(&k("cmd-shift-g"), PreviousMatch, Some("Workspace")),
        KeyBinding::new(
            &k("up"),
            PreviousCommit,
            Some("Workspace && !Palette && !Caret"),
        ),
        KeyBinding::new(
            &k("down"),
            NextCommit,
            Some("Workspace && !Palette && !Caret"),
        ),
        KeyBinding::new(&k("up"), PaletteUp, Some("Palette")),
        KeyBinding::new(&k("down"), PaletteDown, Some("Palette")),
        // Search everywhere (#52): one window, a tab per scope.
        KeyBinding::new(&k("cmd-k"), SearchEverywhere, Some("Workspace")),
        KeyBinding::new(&k("cmd-p"), SearchFiles, Some("Workspace")),
        KeyBinding::new(&k("cmd-shift-c"), SearchCommits, Some("Workspace")),
        KeyBinding::new(&k("cmd-shift-b"), SearchBranches, Some("Workspace")),
        KeyBinding::new(&k("cmd-shift-r"), SearchRequests, Some("Workspace")),
        // In a comment box ⌘⇧P previews the markdown.
        KeyBinding::new(
            &k("cmd-shift-p"),
            SearchActions,
            Some("Workspace && !Caret"),
        ),
        KeyBinding::new(&k("cmd-alt-c"), Compare, Some("Workspace")),
        // Android Studio's Copy Reference: the page on screen's link (#162).
        KeyBinding::new(&k("cmd-alt-shift-c"), CopyLink, Some("Workspace")),
        // Bare letters and arrows are typing while the search field is active.
        KeyBinding::new(&k("k"), PreviousCommit, Some("Workspace && !Typing")),
        KeyBinding::new(&k("j"), NextCommit, Some("Workspace && !Typing")),
        KeyBinding::new(&k("left"), PreviousFile, Some("Workspace && !Typing")),
        KeyBinding::new(&k("right"), NextFile, Some("Workspace && !Typing")),
        KeyBinding::new(&k("w"), CycleMode, Some("Workspace && !Typing")),
        KeyBinding::new(&k("m"), ToggleRendered, Some("Workspace && !Typing")),
        KeyBinding::new(&k("alt-u"), ToggleUnified, Some("Workspace")),
        KeyBinding::new(&k("alt-d"), CycleMode, Some("Workspace")),
        KeyBinding::new(&k("alt-z"), ToggleWrap, Some("Workspace")),
        KeyBinding::new(&k("alt-e"), ToggleFullContext, Some("Workspace")),
        KeyBinding::new(&k("alt-w"), ToggleWhitespace, Some("Workspace")),
        KeyBinding::new(&k("alt-a"), ToggleOneFile, Some("Workspace")),
        KeyBinding::new(&k("alt-shift-a"), ToggleExpandAll, Some("Workspace")),
        KeyBinding::new(
            &k("alt-m"),
            ToggleRichComments,
            Some("Workspace && !Typing"),
        ),
        KeyBinding::new(&k("cmd-alt-l"), LeaveRequest, Some("Workspace")),
        KeyBinding::new(
            &k("alt-shift-v"),
            ToggleAllReviewed,
            Some("Workspace && !Typing"),
        ),
        KeyBinding::new(&k("alt-down"), NextRequest, Some("Workspace && !Typing")),
        KeyBinding::new(&k("alt-up"), PreviousRequest, Some("Workspace && !Typing")),
    ]
}

/// The window, once `install` has wired the crates in (`main.rs`).
pub fn run(path: Option<PathBuf>, install: impl FnOnce(&mut App) + 'static) {
    gpui_platform::application()
        .with_assets(icons::Icons)
        .run(move |cx: &mut App| {
            install(cx);
            let mut bindings = default_bindings();
            bindings.extend(offered_bindings(crate::platform::keys, cx));
            keymap::install(bindings, cx);
            // Before any window: who Artoo is to the system's notifications (#26).
            system_notes::install(cx);
            theme::load();
            motion::set(Settings::load().animations, cx);
            // The themes folder is read again when it changes, with a repository open or not (X52).
            cx.spawn(async move |cx| {
                loop {
                    cx.background_executor()
                        .timer(std::time::Duration::from_secs(2))
                        .await;
                    // The icons folder too (#95).
                    if theme::reload_if_changed() | crate::file_icons::reload_if_changed() {
                        cx.update(|cx| cx.refresh_windows());
                    }
                    // Reduce motion may have been turned on or off in System Settings.
                    cx.update(motion::refresh);
                }
            })
            .detach();
            if !cfg!(feature = "snapshot") || std::env::var_os("ARTOO_SNAPSHOT").is_none() {
                crate::platform::set_icon();
            }
            cx.on_action(|_: &Quit, cx| cx.quit());
            cx.set_menus(menus(&ViewOptions::load(&Settings::load()), cx));

            // A snapshot renders offscreen: no window shown, no focus taken.
            let offscreen =
                cfg!(feature = "snapshot") && std::env::var_os("ARTOO_SNAPSHOT").is_some();
            let options = window_options(offscreen, cx);
            let window = cx
                .open_window(options, |window, cx| {
                    cx.new(|cx| shell::new(path, true, window, cx))
                })
                .expect("open the window");
            if !offscreen {
                cx.activate(true);
            }
            #[cfg(feature = "snapshot")]
            snapshot(window, cx);
            #[cfg(not(feature = "snapshot"))]
            let _ = window;
        });
}

/// A window of Artoo: its size, title bar and smallest size.
fn window_options(offscreen: bool, cx: &mut App) -> WindowOptions {
    let (width, height) = window_size();
    let bounds = gpui::Bounds::centered(None, size(px(width), px(height)), cx);
    WindowOptions {
        show: !offscreen,
        focus: !offscreen,
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions {
            title: Some("Artoo".into()),
            appears_transparent: crate::platform::TRANSPARENT_TITLEBAR,
            traffic_light_position: Some(Point::new(px(12.), px(12.))),
        }),
        window_min_size: Some(size(px(900.), px(500.))),
        app_id: Some("artoo".into()),
        ..Default::default()
    }
}

/// Opens `path` in a window of its own (the rail's "Open in a new window").
fn open_new_window(path: PathBuf, cx: &mut App) {
    let options = window_options(false, cx);
    let opened = cx.open_window(options, |window, cx| {
        cx.new(|cx| shell::new(Some(path), false, window, cx))
    });
    if opened.is_ok() {
        cx.activate(true);
    }
}

/// The window's size: 1480 × 920, or `ARTOO_SNAPSHOT_SIZE=WxH` in a snapshot (to check the
/// layout of a small window).
fn window_size() -> (f32, f32) {
    #[cfg(feature = "snapshot")]
    if let Some((w, h)) = std::env::var("ARTOO_SNAPSHOT_SIZE")
        .ok()
        .and_then(|v| v.split_once('x').map(|(w, h)| (w.to_owned(), h.to_owned())))
        .and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)))
    {
        return (w, h);
    }
    (1480., 920.)
}

/// The menu bar; View items carry a check mark for the options in effect. Artoo's menu opens with the
/// commands features put there (`artoo_shell_api::Menu::App`).
fn menus(options: &ViewOptions, cx: &App) -> Vec<Menu> {
    // The commands features offer, in the menu each names (`artoo_shell_api::Action::menu`).
    let offered = |menu: artoo_shell_api::Menu| {
        artoo_shell_api::actions(cx)
            .iter()
            .filter_map(|a| {
                let (to, words) = a.menu?;
                (to == menu).then(|| MenuItem::Action {
                    name: words.into(),
                    action: a.act.boxed_clone(),
                    os_action: None,
                    checked: false,
                    disabled: false,
                })
            })
            .collect::<Vec<_>>()
    };
    let help = offered(artoo_shell_api::Menu::Help);
    let offered = offered(artoo_shell_api::Menu::App);
    let separated = !offered.is_empty();
    vec![
        Menu::new("Artoo").items(
            offered
                .into_iter()
                .chain(separated.then(MenuItem::separator))
                .chain([
                    MenuItem::action("Settings…", OpenSettings),
                    MenuItem::separator(),
                    MenuItem::action("Quit Artoo", Quit),
                ]),
        ),
        Menu::new("File").items([
            MenuItem::action("New Tab", NewTab),
            MenuItem::action("Close", CloseItem),
            MenuItem::action("Close Tab", CloseTab),
            MenuItem::action("Reopen Closed Tab", ReopenTab),
            MenuItem::separator(),
            MenuItem::action("Open Repository…", Open),
            MenuItem::action("Create Merge Request…", CreateRequest),
            MenuItem::action("Refresh", Refresh),
            MenuItem::separator(),
            MenuItem::action("Find in Diff…", Find),
            MenuItem::action("Find Next", NextMatch),
            MenuItem::action("Find Previous", PreviousMatch),
            MenuItem::separator(),
            MenuItem::action("Find Commits…", FindCommits),
            MenuItem::action("Filter Files…", FilterFiles),
            MenuItem::separator(),
            MenuItem::action("Next Change", NextChange),
            MenuItem::action("Previous Change", PreviousChange),
        ]),
        Menu::new("Go").items([
            MenuItem::action("Inbox", GoToInbox),
            MenuItem::action("Code", GoToCode),
            MenuItem::action("Branches", GoToBranches),
            MenuItem::action("Merge Requests", GoToMergeRequests),
            MenuItem::separator(),
            MenuItem::action("Back", GoBack),
            MenuItem::action("Forward", GoForward),
            MenuItem::separator(),
            MenuItem::action("Next Tab", NextTab),
            MenuItem::action("Previous Tab", PreviousTab),
            MenuItem::separator(),
            MenuItem::action("Next Open Item", NextOpenItem),
            MenuItem::action("Previous Open Item", PreviousOpenItem),
        ]),
        Menu::new("View").items([
            MenuItem::action("Show the Side Panel", ToggleSidebar),
            MenuItem::action("Show Files", ToggleFiles),
            MenuItem::action("Focus on the Diff", FocusDiff),
            MenuItem::action("Fold the Menu to Icons", FoldMenu),
            MenuItem::action("Agents", ToggleAgents),
            MenuItem::separator(),
            MenuItem::action("Zoom In", ZoomIn),
            MenuItem::action("Zoom Out", ZoomOut),
            MenuItem::action("Actual Size", ZoomReset),
            MenuItem::separator(),
            MenuItem::action("Unified Diff", ToggleUnified).checked(options.unified),
            MenuItem::action("Line Diff", SetLines).checked(options.mode == DiffMode::Lines),
            MenuItem::action("Word Diff", SetWords).checked(options.mode == DiffMode::Words),
            MenuItem::action("Structural Diff (difftastic)", SetStructural)
                .checked(options.mode == DiffMode::Structural),
            MenuItem::separator(),
            MenuItem::separator(),
            MenuItem::action("Wrap Long Lines", ToggleWrap).checked(options.wrap),
            MenuItem::action("Show All Lines", ToggleFullContext).checked(options.full_context),
            MenuItem::action("Hide Whitespace Changes", ToggleWhitespace)
                .checked(options.ignore_whitespace),
            MenuItem::action("One File at a Time", ToggleOneFile).checked(options.one_file),
        ]),
        Menu::new("Help").items(
            [
                MenuItem::action("Set Up Artoo…", SetUp),
                MenuItem::action("Keyboard Shortcuts", ShowShortcuts),
            ]
            .into_iter()
            .chain(help),
        ),
    ]
}

/// Waits for loading (`ARTOO_SNAPSHOT_DELAY_MS`, 2500 by default), selects a commit by SHA
/// prefix (`ARTOO_SNAPSHOT_COMMIT`) and a file by path (`ARTOO_SNAPSHOT_FILE`), turns on view options
/// (`ARTOO_SNAPSHOT_VIEW=unified,structural,wrap,all-lines,whitespace`, not saved), optionally
/// compares the first and last versions (`ARTOO_SNAPSHOT_VERSIONS=1`), then writes the frame to
/// `ARTOO_SNAPSHOT` and quits.
#[cfg(feature = "snapshot")]
fn snapshot(window: gpui::WindowHandle<shell::Shell>, cx: &mut App) {
    use std::time::Duration;
    let Some(out) = std::env::var_os("ARTOO_SNAPSHOT").map(PathBuf::from) else {
        return;
    };
    let versions = std::env::var_os("ARTOO_SNAPSHOT_VERSIONS").is_some();
    let Ok(shell) = window.entity(cx) else {
        return;
    };
    let ws = shell.read(cx).first();
    cx.spawn(async move |cx| {
        let executor = cx.background_executor().clone();
        let wait = |ms| executor.timer(Duration::from_millis(ms));
        let delay = std::env::var("ARTOO_SNAPSHOT_DELAY_MS")
            .ok()
            .and_then(|d| d.parse().ok())
            .unwrap_or(2500);
        wait(delay).await;
        if let Ok(path) = std::env::var("ARTOO_SNAPSHOT_TABS") {
            // A second repository in a tab of its own.
            window
                .update(cx, |shell, window, cx| {
                    shell.open_tab(path.into(), window, cx)
                })
                .ok();
            wait(2500).await;
        }
        if let Ok(prefix) = std::env::var("ARTOO_SNAPSHOT_COMMIT") {
            ws.update(cx, |this, cx| {
                let found = this.branches().and_then(|b| b.commit_ix(&prefix, cx));
                if let Some(ix) = found {
                    this.select_commit(ix, cx);
                }
            });
            wait(1500).await;
        }
        if let Ok(path) = std::env::var("ARTOO_SNAPSHOT_FILE") {
            ws.update(cx, |this, cx| {
                let found = this
                    .review
                    .read(cx)
                    .diff
                    .as_ref()
                    .and_then(|d| d.files.iter().position(|f| f.path() == path));
                if let Some(ix) = found {
                    this.select_file(ix, cx);
                }
            });
            wait(1500).await;
        }
        if let Ok(view) = std::env::var("ARTOO_SNAPSHOT_VIEW") {
            ws.update(cx, |this, cx| {
                // Themes (#12), not kept: `theme:paper`, `code-theme:darcula`, `theme-edit:night`.
                let mut tried = theme::choice();
                for name in view.split(',') {
                    if let Some(id) = name.trim().strip_prefix("theme:") {
                        tried.window = id.to_owned();
                        theme::preview(&tried);
                        continue;
                    }
                    if let Some(id) = name.trim().strip_prefix("code-theme:") {
                        tried.code = Some(id.to_owned());
                        theme::preview(&tried);
                        continue;
                    }
                    if let Some(id) = name.trim().strip_prefix("theme-edit:") {
                        this.settings_open = true;
                        this.settings_section = settings::Section::Appearance;
                        this.start_theme_edit(id, cx);
                        continue;
                    }
                    // A token of the editor open, by its row: `theme-token:3`.
                    if let Some(row) = name.trim().strip_prefix("theme-token:")
                        && let Ok(row) = row.parse()
                    {
                        this.pick_token(row, cx);
                        continue;
                    }
                    // The editor's extends ▾ open.
                    if name.trim() == "theme-bases" {
                        this.open_theme_bases(Point::new(px(560.), px(118.)), cx);
                        continue;
                    }
                    if name.trim() == "theme-raise" {
                        this.raise_all(cx);
                        continue;
                    }
                    if name.trim() == "theme-palette" {
                        this.open_palette(palette::Kind::Theme, cx);
                        continue;
                    }
                    let opt = match name.trim() {
                        "requests" => {
                            this.show(spaces::Space::Requests, None, cx);
                            continue;
                        }
                        "unified" => Opt::Unified,
                        "split" => {
                            this.set_option_unsaved(Opt::Unified, false, cx);
                            continue;
                        }
                        "lines" => {
                            this.set_mode(DiffMode::Lines, cx);
                            continue;
                        }
                        "words" => {
                            this.set_mode(DiffMode::Words, cx);
                            continue;
                        }
                        "structural" => {
                            this.set_mode(DiffMode::Structural, cx);
                            continue;
                        }
                        "underlined" => {
                            // Not saved either: only this frame.
                            this.settings.mark_style = crate::storage::MarkStyle::Underlined;
                            this.review.update(cx, |rv, _| rv.settings = this.settings);
                            continue;
                        }
                        "comments" => {
                            // Unsaved sample comments on the first changed line, one being written.
                            let (Some(data), Some(path)) = (
                                &this.review.read(cx).data,
                                this.review.read(cx).current_path(),
                            ) else {
                                continue;
                            };
                            let target =
                                this.review.read(cx).rows.iter().find_map(|row| match row {
                                    Row::Split { right: Some(c), .. }
                                    | Row::Unified {
                                        cell: c,
                                        old: false,
                                        ..
                                    } if c.kind == crate::git::LineKind::Added => Some(c.line),
                                    _ => None,
                                });
                            if let Some(line) = target {
                                let code = data.side(false).line(line).0.to_owned();
                                let comment = crate::comment::Comment {
                                    id: 1,
                                    path: path.clone(),
                                    old: false,
                                    line,
                                    code: code.clone(),
                                    body: "Clamp this value — the API rejects anything else."
                                        .into(),
                                    at: "a10f3cf".into(),
                                    ..Default::default()
                                };
                                this.change_comments(cx, |list| list.push(comment));
                                this.start_comment(false, line + 1, cx);
                                if let Some(c) = this.compose.as_mut() {
                                    c.edit = crate::textedit::TextEdit::new(
                                        "Name this after what it holds.",
                                    );
                                }
                                this.refresh_rows(cx);
                            }
                            continue;
                        }
                        // An agent comment sent, and the agent asking back with a plan (#10).
                        "agent-form" => {
                            let (Some(data), Some(path)) = (
                                &this.review.read(cx).data,
                                this.review.read(cx).current_path(),
                            ) else {
                                continue;
                            };
                            let line = this.review.read(cx).rows.iter().find_map(|row| match row {
                                Row::Split { right: Some(c), .. }
                                | Row::Unified {
                                    cell: c,
                                    old: false,
                                    ..
                                } if c.kind == crate::git::LineKind::Added => Some(c.line),
                                _ => None,
                            });
                            if let Some(line) = line {
                                use crate::comment::{
                                    Answered, Form, FormKind, FormOption, Reply, Sent, State,
                                };
                                let step = |l: &str, on: bool| FormOption {
                                    label: l.into(),
                                    on,
                                    ..Default::default()
                                };
                                let comment = crate::comment::Comment {
                                    id: 7,
                                    path: path.clone(),
                                    line,
                                    code: data.side(false).line(line).0.to_owned(),
                                    body: "Keep the last form so Retry can resend it.".into(),
                                    at: "7c5758f".into(),
                                    state: State::Sent,
                                    sent: Some(Sent {
                                        session: "s".into(),
                                        title: "Claude · payments-service".into(),
                                        batch: "b".into(),
                                        delivered: true,
                                    }),
                                    // An earlier question, answered, before the one asked now.
                                    replies: vec![
                                        Reply {
                                            uid: "r0".into(),
                                            body: "Retry from the toast, or from the form?".into(),
                                            form: Some(Form {
                                                kind: FormKind::One,
                                                options: vec![
                                                    step("From the toast", false),
                                                    step("From the form", false),
                                                ],
                                            }),
                                            at: 0,
                                            answered: Some(Answered {
                                                picks: vec![0],
                                                note: "Keep the toast short.".into(),
                                                ..Default::default()
                                            }),
                                        },
                                        Reply {
                                        uid: "r".into(),
                                        body: "That touches three places. Which should I do?"
                                            .into(),
                                        form: Some(Form {
                                            kind: FormKind::Plan,
                                            options: vec![
                                                step("Set `lastForm` in `submit`", true),
                                                step("Clear it on success", true),
                                                step("Add a test: retry after a failure", true),
                                                step(
                                                    "Rename `Done` to `Signed` across the module",
                                                    false,
                                                ),
                                            ],
                                        }),
                                        at: 0,
                                        answered: None,
                                    },
                                    ],
                                    ..Default::default()
                                };
                                this.change_comments(cx, |list| list.push(comment));
                                this.refresh_rows(cx);
                            }
                            continue;
                        }
                        "notice" => {
                            this.review.update(cx, |rv, _| {
                                rv.notice = Some(Notice {
                                    text: "v3 arrived — this comparison now ends at v3.".into(),
                                    undo: None,
                                })
                            });
                            continue;
                        }
                        "viewmenu" => {
                            let groups = this.view_menu(cx);
                            this.open_titled(Point::new(px(1090.), px(78.)), groups, cx);
                            continue;
                        }
                        "linemenu" => {
                            this.review.update(cx, |rv, _| {
                                rv.sel = Some(artoo_review::select::Sel {
                                    old: false,
                                    anchor: artoo_review::select::Pos { line: 66, byte: 4 },
                                    head: artoo_review::select::Pos { line: 68, byte: 12 },
                                })
                            });
                            this.line_context(false, 67, Point::new(px(900.), px(520.)), cx);
                            continue;
                        }
                        "menu" => {
                            this.open_palette(palette::Kind::Projects, cx);
                            continue;
                        }
                        // The right clicks that copy a link (#162): the branch checked out, the
                        // newest commit, a request.
                        "branchmenu" => {
                            if let Some(ix) = this
                                .branches()
                                .and_then(|b| b.list(cx).iter().position(|b| b.is_head))
                            {
                                this.branch_context(ix, Point::new(px(420.), px(160.)), cx);
                            }
                            continue;
                        }
                        "commitmenu" => {
                            this.commit_context(0, Point::new(px(420.), px(160.)), cx);
                            continue;
                        }
                        // The two newest commits picked as one diff, as shift and a click do.
                        "commit-run" => {
                            this.load_commits(0, 1, None, cx);
                            continue;
                        }
                        "requestmenu" => {
                            this.request_context(398, Point::new(px(420.), px(160.)), cx);
                            continue;
                        }
                        "settings" => {
                            this.open_settings(cx);
                            continue;
                        }
                        "settings-general" => {
                            this.open_settings(cx);
                            this.settings_section = settings::Section::General;
                            continue;
                        }
                        // The token's source, and one made with the SSH key (#142).
                        "settings-requests" => {
                            this.open_requests_settings(cx);
                            continue;
                        }
                        // Fetch in the background's switch (#141).
                        "settings-repository" => {
                            this.open_settings(cx);
                            this.settings_section = settings::Section::Repository;
                            continue;
                        }
                        "swipe" | "onion" => {
                            this.review.update(cx, |rv, _| {
                                rv.images.mode = if name.trim() == "swipe" {
                                    artoo_review::image_view::Mode::Swipe
                                } else {
                                    artoo_review::image_view::Mode::Onion
                                }
                            });
                            continue;
                        }
                        "agents" => {
                            this.open_settings(cx);
                            this.settings_section = settings::Section::Agents;
                            continue;
                        }
                        "send" => {
                            this.open_agent_pick(cx);
                            continue;
                        }
                        // A comment on the rendered document's third block (#11).
                        "page-comment" => {
                            let ix = this.review.read(cx).file;
                            this.page_block_comment(ix, cx);
                            continue;
                        }
                        // The comments of the rendered document's third block, opened.
                        "page-notes" => {
                            let ix = this.review.read(cx).file;
                            this.page_block_notes(ix, cx);
                            continue;
                        }
                        "fold-places" => {
                            this.layout.fold_places = true;
                            continue;
                        }
                        // Merge requests' root narrowed to the ones you wrote (#72).
                        "mine-requests" => {
                            this.requests
                                .update(cx, |rq, _| rq.scope = requests::Scope::Mine);
                            this.go_space(spaces::Space::Requests, cx);
                            continue;
                        }
                        // Merge requests' root narrowed to the ones that ask for a review.
                        "review-requests" => {
                            this.requests
                                .update(cx, |rq, _| rq.scope = requests::Scope::ReviewMe);
                            this.go_space(spaces::Space::Requests, cx);
                            continue;
                        }
                        "rail-tabs" => {
                            this.settings.projects = crate::storage::Projects::Tabs;
                            share_settings(&this.settings);
                            this.review.update(cx, |rv, _| rv.settings = this.settings);
                            continue;
                        }
                        "rail-menu" => {
                            this.settings.projects = crate::storage::Projects::Menu;
                            share_settings(&this.settings);
                            this.review.update(cx, |rv, _| rv.settings = this.settings);
                            continue;
                        }
                        "flat" => {
                            this.settings.flat = true;
                            share_settings(&this.settings);
                            this.review.update(cx, |rv, _| rv.settings = this.settings);
                            continue;
                        }
                        "appearance" => {
                            this.open_settings(cx);
                            this.settings_section = settings::Section::Appearance;
                            continue;
                        }
                        // Set up on a part (#143).
                        "setup" | "setup-keys" | "setup-look" => {
                            this.open_setup(setup::Part::ALL.to_vec(), cx);
                            let at = ["setup", "setup-keys", "setup-look"]
                                .iter()
                                .position(|v| *v == name.trim())
                                .unwrap_or(0);
                            if let Some(s) = &mut this.setup {
                                s.at = at;
                            }
                            continue;
                        }
                        // Applied last, once the request's discussions are in (below).
                        "hover-note" | "reaction-grid" => continue,
                        "inbox" => {
                            this.go_space(spaces::Space::Inbox, cx);
                            continue;
                        }
                        // Every space's open rows shown, folded at first or not (#160).
                        "opens" => {
                            this.nav.folded = [false; 4];
                            continue;
                        }
                        // The new request for the open branch, and one of its lists open.
                        "create" => {
                            this.open_create(None, cx);
                            continue;
                        }
                        "create-quick" => {
                            this.open_create_quick(cx);
                            continue;
                        }
                        menu if menu.starts_with("create-") => {
                            this.open_create_menu(menu, cx);
                            continue;
                        }
                        // The welcome's cards (#54).
                        "welcome-connect" => {
                            this.open_welcome_card(welcome::Card::Connect, cx);
                            continue;
                        }
                        "welcome-clone" => {
                            this.open_welcome_card(welcome::Card::Clone, cx);
                            continue;
                        }
                        "all-requests" => {
                            this.requests
                                .update(cx, |rq, _| rq.scope = requests::Scope::All);
                            this.go_space(spaces::Space::Requests, cx);
                            continue;
                        }
                        // The merged or the closed listed (#114).
                        "merged-requests" | "closed-requests" => {
                            this.set_request_view(
                                if name.trim().starts_with("merged") {
                                    crate::mr::MrState::Merged
                                } else {
                                    crate::mr::MrState::Closed
                                },
                                cx,
                            );
                            this.go_space(spaces::Space::Requests, cx);
                            continue;
                        }
                        "fold-changes" => {
                            this.review.update(cx, |rv, _| rv.changes_open = false);
                            continue;
                        }
                        // A tool window on the right, without saving it as the open one.
                        tool if tool.starts_with("tool-") => {
                            if let Some(t) = this.tool_named(&tool[5..]) {
                                this.set_tool(Some(t), cx);
                            }
                            continue;
                        }
                        "keyboard" => {
                            this.open_settings(cx);
                            this.settings_section = settings::Section::Keyboard;
                            this.start_field(Field::Keys, cx);
                            continue;
                        }
                        // Settings → Keyboard waiting for keys to filter by (X55).
                        "keys-by" => {
                            this.open_settings(cx);
                            this.settings_section = settings::Section::Keyboard;
                            this.recording = Some(settings::BY_KEYS);
                            continue;
                        }
                        "wrap" => Opt::Wrap,
                        "all-lines" => Opt::FullContext,
                        "whitespace" => Opt::Whitespace,
                        "one-file" => Opt::OneFile,
                        // A view a feature's part opens (`artoo_shell_api::Parts::snapshots`).
                        other => {
                            this.open_part_snapshot(other, cx);
                            continue;
                        }
                    };
                    // Not saved: a snapshot must not change the user's settings.
                    this.set_option_unsaved(opt, true, cx);
                }
            });
            wait(1500).await;
        }
        if let Ok(text) = std::env::var("ARTOO_SNAPSHOT_FIND") {
            ws.update(cx, |this, cx| {
                this.start_dfind(cx);
                this.set_dfind(text, cx);
                this.step_change(true, cx);
            });
            wait(800).await;
        }
        if let Ok(text) = std::env::var("ARTOO_SNAPSHOT_SEARCH") {
            ws.update(cx, |this, cx| {
                if let Some(branches) = this.branches() {
                    branches.search(text, cx);
                }
                this.review.update(cx, |rv, _| rv.recompute_matches());
                cx.notify();
            });
            wait(800).await;
        }
        if versions {
            ws.update(cx, |this, cx| {
                let count = this.branches().map_or(0, |b| b.versions(cx).len());
                if count > 1 {
                    this.select_versions(0, count - 1, cx);
                }
            });
            wait(1500).await;
        }
        if std::env::var("ARTOO_SNAPSHOT_VIEW").is_ok_and(|v| v.contains("interdiff")) {
            ws.update(cx, |this, cx| this.select_interdiff(0, cx));
            wait(1500).await;
        }
        if std::env::var("ARTOO_SNAPSHOT_VIEW").is_ok_and(|v| v.contains("worktree")) {
            ws.update(cx, |this, cx| this.open_working_tree(cx));
            wait(2500).await;
        }
        if let Ok(spec) = std::env::var("ARTOO_SNAPSHOT_COMPARE") {
            ws.update(cx, |this, cx| {
                let repo = this.root.as_ref().and_then(|r| Repo::open(r).ok());
                let mut sides = spec
                    .split(',')
                    .filter_map(|s| Some((s.to_owned(), repo.as_ref()?.resolve(s).ok()?)));
                if let (Some(base), Some(head)) = (sides.next(), sides.next()) {
                    // As ⌥⌘C opens it: a comparison page of Branches.
                    this.arrive(nav::Page::Compare(base.clone(), head.clone(), true), cx);
                    this.run_compare(base, head, true, None, cx);
                }
            });
            wait(1500).await;
        }
        if let Ok(query) = std::env::var("ARTOO_SNAPSHOT_PALETTE") {
            ws.update(cx, |this, cx| {
                // `faces:` the interface's faces (#24).
                let kind = if query.starts_with("compare:") {
                    palette::Kind::Base
                } else if query.starts_with("faces:") {
                    palette::Kind::Face { code: false }
                } else {
                    palette::Kind::Search(palette::Scope::All)
                };
                this.open_palette(kind, cx);
                this.type_in_palette(
                    query
                        .trim_start_matches("compare:")
                        .trim_start_matches("faces:"),
                    cx,
                );
            });
            wait(500).await;
        }
        // Opening another repository while this one is open: the card asks, or Settings decides.
        if let Ok(path) = std::env::var("ARTOO_SNAPSHOT_OPEN") {
            ws.update(cx, |this, cx| this.request_open(path.into(), None, cx));
            wait(500).await;
        }
        if let Ok(iid) = std::env::var("ARTOO_SNAPSHOT_MR") {
            ws.update(cx, |this, cx| {
                if let Ok(iid) = iid.parse() {
                    this.open_request(iid, cx);
                }
            });
            wait(2000).await;
        }
        // Merge's card, or its list of what is in the way, before the keys (#164).
        if std::env::var("ARTOO_SNAPSHOT_VIEW").is_ok_and(|v| v.contains("merge")) {
            ws.update(cx, |this, cx| this.open_merge(cx));
            wait(500).await;
        }
        // Code's tabs: the project's pins, a file kept and one previewed, before the keys (#20).
        if std::env::var("ARTOO_SNAPSHOT_VIEW").is_ok_and(|v| v.contains("code-tabs")) {
            ws.update(cx, |this, cx| {
                this.open_code_file("README.md".into(), false, cx);
                this.open_code_file("src/main.rs".into(), true, cx);
            });
            wait(1500).await;
        }
        // What is beside a folder of Code's path, its list open under `src/ui` (#132).
        if std::env::var("ARTOO_SNAPSHOT_VIEW").is_ok_and(|v| v.contains("code-beside")) {
            ws.update(cx, |this, cx| {
                this.open_code_file("src/ui/bar.rs".into(), true, cx);
                this.open_beside("src/ui".into(), cx);
            });
            wait(1500).await;
        }
        if let Ok(keys) = std::env::var("ARTOO_SNAPSHOT_KEYS") {
            // Keystrokes sent to the window itself (not the keyboard), e.g. `right right cmd-t`.
            // `wait` gives a diff a second to load before the next key.
            // Showing motion (#56): a frame right after each key, as a shown window draws one,
            // and none of the usual pause after the last.
            let motion = std::env::var_os("ARTOO_SNAPSHOT_MOTION").is_some();
            if motion {
                // What the keys move must be on screen before them.
                cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
                    .ok();
            }
            let keys: Vec<&str> = keys.split_whitespace().collect();
            for (n, key) in keys.iter().enumerate() {
                if *key == "wait" {
                    wait(1000).await;
                    continue;
                }
                if let Ok(keystroke) = gpui::Keystroke::parse(key) {
                    cx.update_window(window.into(), |_, window, cx| {
                        window.dispatch_keystroke(keystroke, cx);
                        if motion {
                            window.draw(cx).clear(cx);
                        }
                    })
                    .ok();
                }
                if !(motion && n + 1 == keys.len()) {
                    wait(400).await;
                }
            }
        }
        // The newest toast's button pressed: where an agent's news leads (#150).
        if std::env::var("ARTOO_SNAPSHOT_VIEW").is_ok_and(|v| v.contains("toast-act")) {
            ws.update(cx, |this, cx| this.press_toast(cx));
            wait(2500).await;
        }
        // Changes sent on the request open, to see them waiting or refused (#139).
        if std::env::var("ARTOO_SNAPSHOT_VIEW").is_ok_and(|v| v.contains("changes")) {
            ws.update(cx, |this, cx| this.snapshot_changes(cx));
            wait(1500).await;
        }
        // Make the token pressed on the connect panel's offer, with ARTOO_SSH_FIXTURE (#142): hosts' part makes it
        // once the panel is drawn (`ssh-make`).
        if std::env::var("ARTOO_SNAPSHOT_VIEW").is_ok_and(|v| v.contains("ssh-make")) {
            wait(1500).await;
        }
        // Settings → Merge requests after the keys, the SSH key picked there by hosts' part (#156).
        if std::env::var("ARTOO_SNAPSHOT_VIEW").is_ok_and(|v| v.contains("ways-settings")) {
            ws.update(cx, |this, cx| this.open_requests_settings(cx));
            wait(1500).await;
        }
        // Answer on the first of an agent's notes on the file the keys reached (#151).
        if std::env::var("ARTOO_SNAPSHOT_VIEW").is_ok_and(|v| v.contains("note-answer")) {
            ws.update(cx, |this, cx| this.snapshot_answer_note(cx));
            wait(800).await;
        }
        // Open in's ▾ under its button, on the page the keys reached (#78).
        if std::env::var("ARTOO_SNAPSHOT_MENU").as_deref() == Ok("open-in") {
            ws.update(cx, |this, cx| {
                let at = gpui::point(gpui::px(this.win_width), gpui::px(84.));
                this.open_in_menu(at, cx)
            });
            wait(600).await;
        }
        // The interface's face menu, on Settings → Appearance (#24).
        if std::env::var("ARTOO_SNAPSHOT_MENU").as_deref() == Ok("faces") {
            ws.update(cx, |this, cx| {
                this.open_faces(false, gpui::point(gpui::px(760.), gpui::px(200.)), cx)
            });
            wait(600).await;
        }
        // The pointer resting at `x,y` (window points) long enough for a tooltip or a card (#53).
        if let Ok(at) = std::env::var("ARTOO_SNAPSHOT_HOVER")
            && let Some((x, y)) = at.split_once(',')
            && let (Ok(x), Ok(y)) = (x.trim().parse::<f32>(), y.trim().parse::<f32>())
        {
            // The keys' results drawn first: a hover reads the frame on screen.
            cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
                .ok();
            for dx in [0., 1.] {
                cx.update_window(window.into(), |_, window, cx| {
                    window.dispatch_event(
                        gpui::PlatformInput::MouseMove(gpui::MouseMoveEvent {
                            position: gpui::point(gpui::px(x + dx), gpui::px(y)),
                            pressed_button: None,
                            modifiers: Default::default(),
                        }),
                        cx,
                    );
                    // A hidden window draws only by hand: the hover is known after a frame.
                    window.draw(cx).clear(cx);
                })
                .ok();
                wait(200).await;
            }
            wait(1400).await;
            cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
                .ok();
            wait(300).await;
        }
        // The first discussion's first note hovered: its reaction bar, or ☺+ open (#9).
        if let Ok(view) = std::env::var("ARTOO_SNAPSHOT_VIEW")
            && (view.contains("hover-note") || view.contains("reaction-grid"))
        {
            ws.update(cx, |this, cx| {
                let note = this
                    .requests
                    .read(cx)
                    .threads
                    .iter()
                    .find(|t| t.path.is_some())
                    .and_then(|t| t.notes.first())
                    .map(|n| n.id);
                this.hover_note = note;
                if view.contains("reaction-grid")
                    && let Some(note) = note
                {
                    this.requests
                        .update(cx, |rq, _| rq.open_reaction_grid(note));
                }
                this.refresh_rows(cx);
                cx.notify();
            });
            wait(500).await;
        }
        // A hidden window gets no display-link frames: draw one by hand. Images start decoding
        // in the background at a draw, so a first draw, a pause, then the one that is saved.
        // `ARTOO_SNAPSHOT_MOTION=<ms>` keeps motion on (#56) and saves the frame that many
        // milliseconds after that first draw, midway through what the keys set moving.
        let midway = std::env::var("ARTOO_SNAPSHOT_MOTION")
            .ok()
            .and_then(|ms| ms.parse().ok());
        if midway.is_none() {
            cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
                .ok();
        }
        wait(midway.unwrap_or(800)).await;
        // `update_window` leaves the root view free for the draw to render.
        let saved = cx.update_window(window.into(), |_, window, cx| {
            window.draw(cx).clear(cx);
            window
                .render_to_image()
                .and_then(|image| Ok(image.save(&out)?))
        });
        if let Err(err) = saved.and_then(|r| r) {
            eprintln!("snapshot failed: {err:#}");
        }
        cx.update(|cx| cx.quit());
    })
    .detach();
}

pub struct Workspace {
    focus: FocusHandle,
    layout: Layout,
    drag: Option<Drag>,
    root: Option<PathBuf>,
    recent: Vec<PathBuf>,
    error: Option<SharedString>,
    /// Errors and news shown as toasts (`toast.rs`).
    toasts: gpui::Entity<artoo_ui_kit::toast::Toasts<toast::Act>>,
    /// A link's target waiting for its repository to load (`goto.rs`).
    pending_link: Option<crate::links::Target>,
    /// The comment or line a link pointed at, waiting for the request's diff and discussions.
    spot: Option<goto::Spot>,
    /// The diff on screen and how it is read: its files and rows, the find over it, the marks of files read, the
    /// annotations (#165 stage 4: the review's state, on its way out of the tab).
    review: gpui::Entity<review::Review>,
    /// The theme open in Settings → Appearance's editor (`appearance.rs`).
    theme_edit: Option<appearance::ThemeEdit>,
    /// The value of its selected token, as typed.
    theme_value: String,
    /// What Settings → Keyboard is filtered by.
    keys_filter: String,
    /// The filter is a key pressed (By keys): rows with exactly that key.
    keys_exact: bool,
    /// Settings → Keyboard lists only the keys the user changed.
    keys_changed_only: bool,
    /// The part of Settings shown.
    settings_section: settings::Section,
    /// A selection in the text of a comment (`markdown::TextSel`).
    note_sel: Option<markdown::TextSel>,
    /// The action whose new key Settings → Keyboard is waiting for.
    recording: Option<&'static str>,
    /// Zen (⌘.): what the branches and files islands were before it hid them.
    zen: Option<(bool, bool)>,
    /// Zen shows one file at a time, at a reading width (⌥⌘., #55).
    zen_one: bool,
    /// The window with no repository: the card one of its ways in opened (#54).
    welcome: welcome::Welcome,
    /// The pending comment ⌥N / ⌥P is on.
    draft_at: Option<usize>,
    title: String,
    /// The branch open, its commits and versions, what the diff shows of them (`artoo_branches`).
    history: gpui::Entity<History>,
    /// What the tab's features offer one another (`Frame::services`): Code among them (#165).
    services: Rc<artoo_shell_api::Services>,
    /// The project is a plain folder, not a git repository: Code is all it has (#75).
    plain: bool,
    /// The project's main working tree: where its notes for the agents are kept, whatever
    /// worktree is open (#77).
    project: Option<PathBuf>,
    palette: Option<palette::Palette>,
    zone: zones::Zone,
    fetch: fetch::Fetch,
    shell: Option<gpui::WeakEntity<shell::Shell>>,
    /// The window this workspace is in: a repository can open in a new tab of it from anywhere.
    window: gpui::AnyWindowHandle,
    /// The bell and its list (`notifications.rs`).
    notices: gpui::Entity<notifications::Notices>,
    /// The "this tab or a new tab?" card (`opening.rs`).
    open_ask: Option<opening::OpenAsk>,
    /// What is beside a folder of Code's path, listed under it (`beside.rs`, #132).
    beside: Option<beside::Beside>,
    /// Its agents (`artoo_agents_api::tab`): the chats of this repository, read on every beat, the
    /// review comments to them, their notes on the lines of their changes (#151), the "send to which
    /// chat?" card, Settings → Agents' connecting.
    agents: Rc<dyn artoo_agents_api::tab::Agents>,
    /// What agents left in this project, to tell it apart (`agent_events.rs`, #150), and when it
    /// was last read.
    agent_marks: Vec<crate::bus::Mark>,
    marks_stamp: Option<std::time::SystemTime>,
    /// Requests an agent gathered under Merge requests not opened since: the first open goes to
    /// its comments (#148).
    agent_fresh: HashSet<u64>,
    /// Codex's sessions looked at and handed their comments, in the background (#18).
    codex_beat: Option<Task<()>>,
    /// One's own public note asked about before it is deleted (#76).
    confirm_note: Option<u64>,
    /// The Inbox place is shown, and which of its notifications it lists.
    inbox: bool,
    inbox_view: gpui::Entity<inbox::InboxView>,
    /// The note under the pointer: its reaction bar shows (#9).
    hover_note: Option<u64>,
    requests: gpui::Entity<requests::Requests>,
    /// The menu's scroll (#146).
    places: spaces::Places,
    /// The card for a new merge request, while it is open.
    /// The keyboard card is open.
    shortcuts: bool,
    zoom_applied: bool,
    bookmarks: bookmarks::Store,
    /// Places opened lately, newest first.
    recents: Vec<recents::Entry>,
    /// A mention in comment text under the pointer, and its card (X62).
    mention_hover: Option<hover::MentionHover>,
    /// When panes last opened or closed, to draw them coming and going (#56).
    motion: motion::Marks,
    /// The tool window last open, drawn on its way out.
    last_tool: Option<tools::Tool>,
    /// The menu just closed, drawn fading out.
    /// Open in's choices kept per extension, and the app its button names (#78).
    open_with: std::collections::HashMap<String, open_in::App>,
    open_cache: open_in::Cache,
    /// The search over all files of the diff, while it is open.
    /// The window's width in logical pixels, as of the last frame.
    win_width: f32,
    /// Discussions whose open or folded state differs from the default (a resolved one is folded).
    toggled_mr: HashSet<usize>,
    /// Agent comments the reader folded.
    toggled_agent: HashSet<u64>,
    /// The discussion `r`, `x` and ⇧n / ⇧p act on (an index into the request's threads).
    thread_at: Option<usize>,
    /// The token being entered in the welcome's connect card; shown as dots.
    token_input: String,
    /// A notification opened on a comment: once its line's history is in, an outdated one opens
    /// on the diff since it (request, note).
    since_on_open: Option<(u64, u64)>,
    /// Agent comments whose line changed since they were written: where their line is in the diff
    /// shown (#46).
    agent_lines: std::collections::HashMap<u64, u32>,
    agent_task: Option<Task<()>>,
    /// Merge requests agents drafted (#152), with who did: the last one for each branch, until
    /// the reviewer creates it.
    agent_requests: Vec<(String, crate::bus::RequestDraft)>,
    /// The branch whose drafted request opens once its page is in (Open on its toast).
    create_waiting: Option<String>,
    /// Branches with an agent's drafted request whose card has not opened yet: it opens when the
    /// branch's page is in, however it is opened (#159).
    drafts_unseen: HashSet<String>,
    /// Rows an agent put something on for the reviewer, with what, until opened: an orange dot
    /// (#160).
    agent_calls: std::collections::HashMap<nav::Item, String>,
    tabs: shell::TabModel,
    settings: Settings,
    settings_open: bool,
    /// Set up Artoo, while it is open (#143).
    setup: Option<setup::Setup>,
    watch: watch::Watch,
    /// The review comment being written, and the Review island.
    compose: Option<composer::Compose>,
    /// The merge request the last comment went to: the next composer on its diff starts there.
    last_request_target: Option<u64>,
    /// Who is reviewing (git's `user.name`): the composer's avatar.
    me: String,
    /// A comment to scroll to once its file is laid out.
    jump: Option<u64>,
    /// "Copied ✓" on the Copy button until something changes.
    copied: bool,
    /// The caret, selection and undo of the field that has the keyboard (`input`).
    field_edit: crate::textedit::TextEdit,
    /// How far the one-line field with the keyboard is scrolled sideways.
    field_scroll: Rc<Shared<f32>>,
    /// The last layout of the composer's text and the x ↑ ↓ keep.
    field_lines: input::Lines,
    field_goal: Option<gpui::Pixels>,
    /// Where the window is: each space's page, back and forward, the open items (#71).
    nav: nav::Nav,
    /// Where the View button was painted, so its menu hangs from it.
    /// The branch filter, and which filter (if any) takes the keyboard.
    bfilter: String,
    field: Option<Field>,
    /// A context menu open at the pointer, and the repository's address on the web.
    ctx_menu: gpui::Entity<artoo_ui_kit::menu::Menu<Act>>,
    web: Option<crate::git::WebRemote>,
    /// The host `web` is on, set with it.
    host: Option<Arc<dyn Host>>,
    /// Each list's width as last drawn: its rows keep what it has room for (#79).
    widths: room::Widths,
    /// What folds, its open height (#110, `folding`).
    heights: room::Heights,
    /// What each fold was last drawn as (`Workspace::folding`).
    folds: motion::Folds,
    /// The card naming a project or a chat (`labeling.rs`), and the name being typed.
    label_edit: Option<labeling::LabelEdit>,
    label_name: String,
    /// What features draw in this tab (#165): title-bar items, cards over it, Settings rows.
    parts: Vec<artoo_shell_api::Parts>,
    /// The commands features offer (⌘K): their words and actions.
    contributed: Vec<(&'static str, Box<dyn gpui::Action>)>,
    /// The tree folder opened last, and a count of openings: its rows fade in (`tree_row`).
    tree_opened: Option<(String, usize)>,
    /// What a narrow window folded, for the session (#80).
    narrowed: narrow::Narrowed,
    /// Which input has the keyboard (#168).
    keyboard: typing::Keyboard,
    /// A file of Code being read and the line to show once it is in (a note from the Agents
    /// window whose file is not in the diff open).
    file_jump: Option<(String, u32)>,
    /// Whether this is the tab on screen: one behind another polls its repository seldom.
    on_screen: Rc<Shared<bool>>,
    /// The face `char_width` was measured in.
    char_face: SharedString,
    // Replacing a task drops, and so cancels, the one it replaces.
    repo_task: Option<Task<()>>,
}

impl Workspace {
    /// `open_recent`: with no `path`, open the most recent repository.
    fn new(
        path: Option<PathBuf>,
        open_recent: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        let recent = storage::recent();
        let mut settings = Settings::load();
        share_settings(&settings);
        // The keyboard card greets the very first launch, once (never a check's snapshot).
        let greet = !settings.keys_seen
            && std::env::var_os("ARTOO_SNAPSHOT").is_none()
            && !shortcuts::SHOWN.swap(true, std::sync::atomic::Ordering::Relaxed);
        if greet {
            settings.keys_seen = true;
            settings.save();
        }
        let start = path.or_else(|| open_recent.then(|| recent.first().cloned()).flatten());
        let review = cx.new(|_| review::Review {
            root: None,
            pfilter: String::new(),
            fcursor: None,
            settings,
            char_width: 0.,
            options: ViewOptions::load(&settings),
            notes: Vec::new(),
            images: Default::default(),
            view_button: Default::default(),
            page: Default::default(),
            annotate: false,
            annotations: Vec::new(),
            annot_task: None,
            blames: Default::default(),
            blame_tip: None,
            file_view: None,
            file_reading: None,
            reviewed: artoo_review::reviewed::Store::load(),
            done_card: false,
            review_scope: None,
            fps: Vec::new(),
            unfolded: HashSet::new(),
            narrow: false,
            tight: false,
            goline: None,
            goline_row: None,
            jump_line: None,
            jump_whole: false,
            diff: None,
            file: 0,
            collapsed: HashSet::new(),
            sel: None,
            selecting: false,
            dfind: None,
            dterms: Arc::default(),
            dfind_regex: false,
            dfind_error: None,
            find_area: Default::default(),
            goline_area: Default::default(),
            filter_area: Default::default(),
            filter_open: false,
            widths: Default::default(),
            ffind: None,
            changes: Vec::new(),
            change_at: None,
            matches: Vec::new(),
            match_at: 0,
            notice: None,
            restore_scroll: None,
            data: None,
            rows: Vec::new(),
            expanded: HashSet::new(),
            diff_list: ListState::new(0, ListAlignment::Top, px(400.)),
            stack: None,
            offset: 0.,
            body_width: Rc::new(Shared::new(0.)),
            file_scroll: UniformListScrollHandle::new(),
            diff_task: None,
            view_task: None,
            frame: None,
            changes_open: artoo_review::changes_view::changes_left_open(),
            changes_motion: Default::default(),
        });
        let services = Rc::new(artoo_shell_api::Services::default());
        let frame: Rc<dyn artoo_shell_api::Frame> =
            Rc::new(parts::TabFrame(cx.weak_entity(), services.clone()));
        let mut this = Self {
            focus,
            layout: Layout::load(),
            drag: None,
            root: None,
            recent,
            error: None,
            toasts: toast::toasts(cx),
            draft_at: None,
            zen: None,
            zen_one: false,
            welcome: Default::default(),
            recording: None,
            note_sel: None,
            settings_section: Default::default(),
            keys_filter: String::new(),
            keys_exact: false,
            keys_changed_only: false,
            review: review.clone(),
            theme_edit: None,
            theme_value: String::new(),
            pending_link: None,
            spot: None,
            window: window.window_handle(),
            open_ask: None,
            beside: None,
            agents: artoo_agents_api::tab::tab(cx),
            agent_marks: Vec::new(),
            marks_stamp: None,
            agent_fresh: HashSet::new(),
            codex_beat: None,
            confirm_note: None,
            inbox: false,
            inbox_view: cx.new(|_| Default::default()),
            hover_note: None,
            notices: cx.new(|_| Default::default()),
            title: String::new(),
            history: cx.new(|_| History::new()),
            services: services.clone(),
            plain: false,
            project: None,
            palette: None,
            zone: zones::Zone::default(),
            fetch: fetch::Fetch::default(),
            shell: None,
            requests: cx.new(|_| requests::Requests::default()),
            places: Default::default(),
            shortcuts: greet,
            zoom_applied: false,
            bookmarks: bookmarks::Store::load(),
            recents: Vec::new(),
            mention_hover: None,
            open_with: open_in::load(),
            open_cache: Default::default(),
            motion: Default::default(),
            last_tool: None,
            win_width: 1480.,
            toggled_mr: HashSet::new(),
            toggled_agent: HashSet::new(),
            thread_at: None,
            token_input: String::new(),
            since_on_open: None,
            agent_lines: Default::default(),
            agent_task: None,
            agent_requests: Vec::new(),
            create_waiting: None,
            drafts_unseen: HashSet::new(),
            agent_calls: Default::default(),
            tabs: Default::default(),
            settings,
            settings_open: false,
            setup: None,
            watch: watch::Watch::default(),
            compose: None,
            last_request_target: None,
            me: "You".to_owned(),
            jump: None,
            copied: false,
            field_edit: Default::default(),
            field_scroll: Rc::default(),
            field_lines: Rc::default(),
            field_goal: None,
            nav: nav::Nav::default(),
            bfilter: String::new(),
            field: None,
            ctx_menu: menu::menu(cx),
            web: None,
            host: None,
            widths: Default::default(),
            heights: Default::default(),
            folds: Default::default(),
            label_edit: None,
            label_name: String::new(),
            parts: Vec::new(),
            contributed: artoo_shell_api::actions(cx)
                .iter()
                .map(|a| (a.words, a.act.boxed_clone()))
                .collect(),
            tree_opened: None,
            narrowed: Default::default(),
            keyboard: Default::default(),
            file_jump: None,
            on_screen: Rc::new(Shared::new(true)),
            char_face: theme::code_font(),
            repo_task: None,
        };
        // The tab's review as its pages ask it, the tab as Code's host.
        services.set(artoo_review::file_view::file_view(&review));
        services.set(artoo_review::review::handle(&review));
        // The review itself, which its parts read and type in.
        services.set(review.clone());
        // Its history, which the branches' parts read and type in, its branches as the other parts read them,
        // and its answers to the history's acts.
        services.set(this.history.clone());
        services.set(artoo_branches::history::handle(&this.history, &services));
        services.set(artoo_branches::history::acts(
            &this.history,
            this.history_acts(cx),
        ));
        // Its requests, which the requests' parts read and draw, as the other features ask them, and its answers
        // to their acts.
        services.set(this.requests.clone());
        services.set(artoo_requests::requests::handle(&this.requests));
        services.set(requests::acts_of(cx.weak_entity()));
        // Its agents, which their parts draw — the Agents window, the card, the notes' summary,
        // Settings → Agents — the tab their host.
        services.set(this.agents.clone());
        services.set::<Rc<dyn artoo_agents_api::view::AgentsHost>>(Rc::new(agent::AgentsHost(
            cx.weak_entity(),
        )));
        services.set::<Rc<dyn artoo_agents_api::view::NotesHost>>(Rc::new(agent_notes::NotesHost(
            cx.weak_entity(),
        )));
        // Its notifications and what the Inbox selected, which the requests' Inbox draws, the tab its host.
        services.set(this.notices.clone());
        services.set(this.inbox_view.clone());
        services.set::<Rc<dyn artoo_requests::inbox_page::InboxHost>>(Rc::new(inbox::InboxHost(
            cx.weak_entity(),
        )));
        services.set::<Rc<dyn artoo_requests::root::RootHost>>(Rc::new(roots::RootHost(
            cx.weak_entity(),
        )));
        services.set::<Rc<dyn artoo_code_api::Host>>(Rc::new(code::CodeHost(cx.weak_entity())));
        // The review's host, which its item page draws the column with.
        services.set::<Rc<dyn artoo_review_api::Host>>(Rc::new(TabHost(cx.weak_entity())));
        this.parts = artoo_shell_api::make_parts(frame, window, cx);
        // A way in connected the host (hosts' connect panel): the requests read again, a toast saying with what.
        if let Some(connecting) = this.connecting() {
            let tab = cx.weak_entity();
            connecting.on_connected(
                Rc::new(move |said, cx| {
                    let tab = tab.clone();
                    cx.defer(move |cx| {
                        tab.update(cx, |this, cx| this.connected(said, cx)).ok();
                    });
                }),
                cx,
            );
        }
        if let Some(path) = start {
            this.open_repo(path, None, cx);
        }
        Self::start_watching(this.on_screen.clone(), cx);
        cx.subscribe(&this.review, Self::review_event).detach();
        cx.observe(&this.history, |_, _, cx| cx.notify()).detach();
        cx.observe(&this.review, |_, _, cx| cx.notify()).detach();
        // What the branches' reads come to.
        if let Some(branches) = this.branches() {
            let tab = cx.weak_entity();
            branches.on_event(
                Rc::new(move |event, cx| {
                    tab.update(cx, |this, cx| this.history_event(event, cx))
                        .ok();
                }),
                cx,
            );
        }
        cx.observe(&this.requests, |_, _, cx| cx.notify()).detach();
        // The notes' lines followed to the head shown: they are pinned again.
        let tab = cx.weak_entity();
        this.agents.on_followed(
            Rc::new(move |cx| {
                tab.update(cx, |this, cx| {
                    this.refresh_rows(cx);
                    cx.notify();
                })
                .ok();
            }),
            cx,
        );
        cx.subscribe(&this.requests, Self::requests_event).detach();
        // A connect ended: Set up looks whether the profiles have Artoo now.
        let tab = cx.weak_entity();
        this.agents.on_connecting(
            Rc::new(move |cx| {
                tab.update(cx, |this, cx| {
                    if !this.agents.connecting(cx).0 {
                        this.setup_look_again();
                    }
                    cx.notify();
                })
                .ok();
            }),
            cx,
        );
        cx.observe_window_activation(window, |this, window, cx| {
            if window.is_window_active() {
                this.window_activated(cx);
            }
        })
        .detach();
        this
    }

    // ---- loading -------------------------------------------------------------------------

    /// Opens `path`; `keep` re-selects a branch (by refname) and a commit after a refresh.
    fn open_repo(
        &mut self,
        path: PathBuf,
        keep: Option<(String, Option<git2::Oid>)>,
        cx: &mut Context<Self>,
    ) {
        self.error = None;
        self.repo_task = Some(cx.spawn(async move |this, cx| {
            let loaded = cx
                .background_executor()
                .spawn(async move {
                    let repo = match Repo::open(&path) {
                        Ok(repo) => repo,
                        // A folder that is not a git repository opens too: Code is what it has
                        // (#75).
                        Err(_) if path.is_dir() => {
                            return anyhow::Ok(Err(path.canonicalize().unwrap_or(path)));
                        }
                        Err(err) => return Err(err),
                    };
                    let git_dir = repo.git_dir();
                    let fingerprint = crate::git::fingerprint(&git_dir);
                    let web = repo.web_remote();
                    let decor = repo.decorations();
                    let me = repo.user_name();
                    anyhow::Ok(Ok((
                        repo.root(),
                        repo.branches()?,
                        git_dir,
                        fingerprint,
                        web,
                        (decor, me, repo.main_root()),
                    )))
                })
                .await;
            this.update(cx, |this, cx| {
                match loaded {
                    Ok(Err(folder)) => this.open_plain(folder, cx),
                    Ok(Ok((root, branches, git_dir, fingerprint, web, (decor, me, project)))) => {
                        this.plain = false;
                        this.project = Some(project.clone());
                        this.host = web.as_ref().map(|w| host_of(w, cx));
                        this.web = web;
                        this.me = me.unwrap_or_else(|| "You".to_owned());
                        this.recent = storage::remember(&root);
                        if this.root.as_ref() != Some(&root) {
                            this.agents.load_comments(project.clone(), cx);
                            this.agents.load_notes(project.clone(), cx);
                            this.marks_stamp = None;
                            this.compose = None;
                        }
                        this.watch.watch(git_dir, fingerprint);
                        this.start_working_poll(cx);
                        this.load_requests(cx);
                        // The documents this repository reads as Source (#11).
                        this.review
                            .update(cx, |rv, _| rv.page.load(&crate::comment::key(&root)));
                        // Its requests list's folds and the requests opened (#81).
                        let key = crate::comment::key(&root);
                        this.requests.update(cx, |rq, _| rq.rows.load(&key));
                        // Code's project, its pinned tabs (#20).
                        if let Some(code) = this.code() {
                            let fresh = this.root.as_ref() != Some(&root);
                            let key = crate::comment::key(&root);
                            code.set_project(root.clone(), false, fresh, &key, cx);
                        }
                        this.review.update(cx, |rv, _| rv.root = Some(root.clone()));
                        let fresh = this.root.as_ref() != Some(&root);
                        if let Some(branches_api) = this.branches() {
                            branches_api.opened(root.clone(), branches, Some(decor), fresh, cx);
                        }
                        this.root = Some(root);
                        // Once the root is this repository's: it reads whether fetch is off for it (#141).
                        this.start_fetching(cx);
                        let refresh = keep.is_some();
                        let (refname, commit) = keep.unzip();
                        let ix = this
                            .branches()
                            .and_then(|b| b.first_branch(refname.as_deref(), cx));
                        this.clear_branch(cx);
                        if !refresh {
                            // Opened: the space and pages it was left on (#71).
                            this.restore_nav(ix, cx);
                        } else if let Some(
                            page @ (nav::Page::Request(_)
                            | nav::Page::RequestCommit(..)
                            | nav::Page::RequestCommits(..)
                            | nav::Page::RequestVersions(..)),
                        ) = this.page_of(spaces::Space::Requests).cloned()
                            && this.nav.space == spaces::Space::Requests
                        {
                            // A request's page loads again as the request it is.
                            this.show(spaces::Space::Requests, Some(page), cx);
                        } else if let Some(ix) = ix {
                            this.select_branch(ix, commit.flatten(), cx);
                        }
                        this.follow_pending_link(cx);
                    }
                    Err(err) => this.error = Some(format!("{err:#}").into()),
                }
                cx.notify();
            })
            .ok();
        }));
    }

    /// A folder that is not a git repository: its files in Code, the other spaces greyed with
    /// what they need (#75).
    fn open_plain(&mut self, root: PathBuf, cx: &mut Context<Self>) {
        self.plain = true;
        self.web = None;
        self.host = None;
        self.recent = storage::remember(&root);
        self.project = Some(root.clone());
        let fresh = self.root.as_ref() != Some(&root);
        if fresh {
            self.agents.load_comments(root.clone(), cx);
            self.marks_stamp = None;
            self.compose = None;
        }
        // Nothing of git's to watch or fetch; the poll only brings the agents' answers.
        self.fetch = Default::default();
        self.start_working_poll(cx);
        self.load_requests(cx);
        self.review
            .update(cx, |rv, _| rv.page.load(&crate::comment::key(&root)));
        let key = crate::comment::key(&root);
        self.requests.update(cx, |rq, _| rq.rows.load(&key));
        if let Some(code) = self.code() {
            let fresh = self.root.as_ref() != Some(&root);
            code.set_project(root.clone(), true, fresh, &crate::comment::key(&root), cx);
        }
        self.review.update(cx, |rv, _| rv.root = Some(root.clone()));
        if let Some(branches) = self.branches() {
            branches.opened(root.clone(), Vec::new(), None, fresh, cx);
        }
        self.root = Some(root);
        self.clear_branch(cx);
        self.nav = nav::Nav::default();
        if let Some(branches) = self.branches() {
            branches.stop_waiting(cx);
        }
        self.show(spaces::Space::Code, None, cx);
    }

    fn clear_branch(&mut self, cx: &mut App) {
        self.forget_news(cx);
        if let Some(branches) = self.branches() {
            branches.forget(cx);
        }
    }

    fn clear_diff(&mut self, cx: &mut App) {
        if let Some(branches) = self.branches() {
            branches.clear_diff(cx);
        }
    }

    /// Branch `ix` opened at commit `keep` (`History::open_branch`).
    fn select_branch(&mut self, ix: usize, keep: Option<git2::Oid>, cx: &mut Context<Self>) {
        let read = self.branches().is_some_and(|b| b.open_branch(ix, keep, cx));
        self.branch_opened(ix, read, cx);
    }

    /// Branch `ix` opened, `read` anew: the watch starts over on it, and the request on it, if any,
    /// is the one open.
    pub(super) fn branch_opened(&mut self, ix: usize, read: bool, cx: &mut Context<Self>) {
        if read {
            self.forget_news(cx);
        }
        self.sync_request(ix, cx);
        cx.notify();
    }

    /// What the history said: a branch read is shown, more of its history searched, a failed read said.
    fn history_event(&mut self, event: &BranchesEvent, cx: &mut Context<Self>) {
        match event {
            BranchesEvent::Read => self.branch_read(cx),
            BranchesEvent::Older => {
                self.refilter(cx);
                self.index_paths(cx);
            }
            BranchesEvent::Failed(err) => self.error = Some(err.clone().into()),
            BranchesEvent::Picked(ix) => self.pick(Selection::Commit(*ix), cx),
            BranchesEvent::Working => self.working_changed(cx),
            // The beat redraws only what changed on it.
            BranchesEvent::Beat => return self.beat(cx),
            BranchesEvent::Reloaded(reloaded) => self.apply_reload(reloaded.clone(), cx),
        }
        cx.notify();
    }

    /// The branch open was read: its page — the branch against the main line, or the commit asked for —, or a
    /// request on it waiting for its commits (`History::show_read`). The page follows what it shows, its commits
    /// are kept among the places opened lately, and a link or a draft waiting for it goes on.
    fn branch_read(&mut self, cx: &mut Context<Self>) {
        let Some(settled) = self.branches().and_then(|b| b.show_read(cx)) else {
            return;
        };
        if settled.follow {
            self.record(cx);
        }
        for commit in &settled.commits {
            self.remember_commit(commit);
        }
        self.follow_pending_link(cx);
        self.open_waiting_draft(cx);
        cx.notify();
    }

    fn select_commit(&mut self, ix: usize, cx: &mut Context<Self>) {
        self.load_commit(ix, None, cx);
    }

    /// Shows commit `ix` (`History::show_commit`); `keep` re-selects a file by path. The page follows it.
    fn load_commit(&mut self, ix: usize, keep: Option<String>, cx: &mut Context<Self>) {
        if self.root.is_none() {
            return;
        }
        let Some(commit) = self.branches().and_then(|b| b.show_commit(ix, keep, cx)) else {
            return;
        };
        self.record(cx);
        self.remember_commit(&commit);
        cx.notify();
    }

    /// A run of the list's commits, `newest` to `oldest` (indexes, newest first), as one diff.
    pub(super) fn select_commits(&mut self, newest: usize, oldest: usize, cx: &mut Context<Self>) {
        self.load_commits(newest, oldest, None, cx);
    }

    /// Shows the run as one diff (`History::show_commits`); its marks are kept for that range. The page follows it.
    fn load_commits(
        &mut self,
        newest: usize,
        oldest: usize,
        keep: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if newest == oldest {
            return self.load_commit(newest, keep, cx);
        }
        if self.root.is_none() {
            return;
        }
        if !self
            .branches()
            .is_some_and(|b| b.show_commits(newest, oldest, keep, cx))
        {
            return;
        }
        self.record(cx);
        cx.notify();
    }

    fn select_versions(&mut self, from: usize, to: usize, cx: &mut Context<Self>) {
        self.load_versions(from, to, None, cx);
    }

    /// Shows versions `from` and `to` compared (`History::show_versions`). The page follows it.
    fn load_versions(
        &mut self,
        from: usize,
        to: usize,
        keep: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if self.root.is_none() {
            return;
        }
        if !self
            .branches()
            .is_some_and(|b| b.show_versions(from, to, keep, cx))
        {
            return;
        }
        self.record(cx);
        cx.notify();
    }

    /// The interdiff of one modified commit of the comparison shown (`History::show_interdiff`): a
    /// picture's; a click on a pair is the branches' (`artoo_branches::pairs`).
    #[cfg(feature = "snapshot")]
    fn select_interdiff(&mut self, pair: usize, cx: &mut Context<Self>) {
        if self.root.is_none() {
            return;
        }
        if self.branches().is_some_and(|b| b.show_interdiff(pair, cx)) {
            cx.notify();
        }
    }

    /// The tab's review as a page asks it (`artoo_review_api::Review`).
    pub(super) fn review_handle(&self) -> Rc<dyn artoo_review_api::Review> {
        artoo_review::review::handle(&self.review)
    }

    fn settings(&self, cx: &App) -> DiffSettings {
        DiffSettings {
            ignore_whitespace: self.review.read(cx).options.ignore_whitespace,
        }
    }

    /// What the review said: the tab follows a diff that came (the agents' lines on it, the pages' counts, its
    /// first file, the spot asked for), lays out what was built with what is pinned on it, and draws again.
    fn review_event(
        &mut self,
        _: gpui::Entity<review::Review>,
        event: &ReviewEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            ReviewEvent::Loaded(file) => {
                self.map_agent_comments(cx);
                self.map_agent_notes(cx);
                self.count_files(cx);
                if let Some(ix) = *file {
                    self.select_file(ix, cx);
                }
                self.resolve_spot(false, cx);
            }
            ReviewEvent::Failed(err) => self.error = Some(err.clone().into()),
            ReviewEvent::FileBuilt => {
                self.relayout(cx);
                self.refresh_annotations(cx);
                self.apply_jump(cx);
                self.review.update(cx, |rv, _| rv.restore_file_scroll());
            }
            ReviewEvent::StackBuilt(ix) => self.stack_built(*ix, cx),
            ReviewEvent::FileRead => self.file_read(cx),
            ReviewEvent::Changed => {}
            ReviewEvent::Found { file, row } => self.go_found(Some((*file, *row)), cx),
            ReviewEvent::GoLine(done) => self.goline_done(*done, cx),
            ReviewEvent::Page(event) => self.page_event(event.clone(), cx),
        }
        cx.notify();
    }

    fn select_file(&mut self, ix: usize, cx: &mut Context<Self>) {
        let Some(file) = self
            .review
            .read(cx)
            .diff
            .as_ref()
            .and_then(|d| d.files.get(ix))
            .cloned()
        else {
            return;
        };
        self.remember_place(
            recents::Key::File(file.path().to_owned()),
            file.path().to_owned(),
            "file".to_owned(),
        );
        if self.wants_stack(cx) {
            return self.stack_select(ix, cx);
        }
        self.review.update(cx, |rv, cx| rv.show_file(ix, file, cx));
        cx.notify();
    }

    /// Lays the selected file out again (`Review::relayout`) with what is pinned on its lines now.
    fn relayout(&mut self, cx: &mut App) {
        let pins = self.open_pins(cx);
        self.review.update(cx, |rv, _| rv.relayout(&pins));
    }

    /// Every file laid out again (`Review::relayout_all`) with what is pinned on their lines now.
    fn relayout_all(&mut self, cx: &mut App) {
        let pins = self.all_pins(cx);
        self.review.update(cx, |rv, _| rv.relayout_all(&pins));
    }

    /// Shows the hidden lines of an unchanged run in place (`Review::expand_gap`).
    fn expand_gap(&mut self, segment: usize, cx: &mut Context<Self>) {
        let pins = self.open_pins(cx);
        if self
            .review
            .update(cx, |rv, _| rv.expand_gap(segment, &pins))
        {
            cx.notify();
        }
    }

    fn set_option(&mut self, opt: Opt, value: bool, cx: &mut Context<Self>) {
        if self.review.read(cx).option(opt) != value {
            self.set_option_unsaved(opt, value, cx);
            self.review.read(cx).options.save();
        }
    }

    fn set_option_unsaved(&mut self, opt: Opt, value: bool, cx: &mut Context<Self>) {
        if !self.review.update(cx, |rv, _| rv.set_option(opt, value)) {
            return;
        }
        cx.set_menus(menus(&self.review.read(cx).options, cx));
        match opt {
            Opt::Unified | Opt::FullContext => self.relayout_all(cx),
            Opt::FoldReviewed => self.refold(cx),
            Opt::OneFile => self.show_files_anew(cx),
            // Same rows, new heights.
            Opt::Wrap => self.review.read(cx).diff_list.remeasure(),
            Opt::Whitespace => self.reload_diff(cx),
        }
        cx.notify();
    }

    /// Changes what a diff marks inside changed lines. Not remembered: Settings holds the default.
    fn set_mode(&mut self, mode: DiffMode, cx: &mut Context<Self>) {
        if !self.review.update(cx, |rv, _| rv.set_mode(mode)) {
            return;
        }
        cx.set_menus(menus(&self.review.read(cx).options, cx));
        if self.review.read(cx).stack.is_some() {
            self.reopen_stack(cx);
        } else {
            self.select_file(self.review.read(cx).file, cx);
        }
        cx.notify();
    }

    /// The View menu: the diff mode, then the options that are on or off.
    /// The View ▾ menu (#48 A): one flat list of what the toolbar does not show, in groups.
    fn view_menu(&self, cx: &App) -> Vec<(Option<&'static str>, Vec<Entry>)> {
        let o = self.review.read(cx).options;
        let tool = self.tool();
        vec![
            (
                Some("Changes"),
                DiffMode::ALL
                    .iter()
                    .map(|&mode| {
                        Entry::new(mode.label(), Act::SetMode(mode)).checked(o.mode == mode)
                    })
                    .collect(),
            ),
            (
                Some("Show"),
                vec![
                    Entry::new("Every line of the file", Act::Toggle(Opt::FullContext))
                        .key("⌥E")
                        .checked(o.full_context),
                    Entry::new("Hide whitespace changes", Act::Toggle(Opt::Whitespace))
                        .key("⌥W")
                        .checked(o.ignore_whitespace),
                    Entry::new("Wrap long lines", Act::Toggle(Opt::Wrap))
                        .key("⌥Z")
                        .checked(o.wrap),
                    Entry::new("Annotate (blame)", Act::Annotate)
                        .key("⌥⌘B")
                        .checked(self.review.read(cx).annotate),
                ],
            ),
            (
                Some("Files"),
                vec![
                    Entry::new("One file at a time", Act::Toggle(Opt::OneFile))
                        .key("⌥A")
                        .checked(o.one_file),
                    Entry::new("Fold reviewed files", Act::Toggle(Opt::FoldReviewed))
                        .checked(o.fold_reviewed),
                    Entry::new("Mark reviewed as I scroll past", Act::AutoReviewed)
                        .checked(self.settings.auto_reviewed),
                ],
            ),
            (
                Some("Panels"),
                vec![
                    Entry::new("Fold the menu to icons", Act::FoldPlaces)
                        .key("⌘B")
                        .checked(self.menu_folded()),
                    Entry::new("Discussions", Act::Tool(tools::Tool::Discussions))
                        .key("⌘⇧D")
                        .checked(tool == Some(tools::Tool::Discussions)),
                    Entry::new(
                        "Agents",
                        Act::Tool(tools::Tool::Part(artoo_agents_api::view::WINDOW)),
                    )
                    .key("⇧⌘A")
                    .checked(tool == Some(tools::Tool::Part(artoo_agents_api::view::WINDOW))),
                ],
            ),
        ]
    }

    // ---- selecting and context menus ---------------------------------------------------

    /// A right click on a diff line: copy it (or the selection), comment on it, open it.
    fn line_context(
        &mut self,
        old: bool,
        line: u32,
        at: Point<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) {
        let Some(data) = &self.review.read(cx).data else {
            return;
        };
        let in_selection = self.review.read(cx).sel.is_some_and(|s| {
            let (start, end) = s.ends();
            !s.is_empty() && s.old == old && (start.line..=end.line).contains(&line)
        });
        let copy = match self.review.read(cx).selected_text() {
            Some(text) if in_selection => Entry::new("Copy selection", Act::Copy(text)).key("⌘C"),
            _ => Entry::new(
                "Copy line",
                Act::Copy(data.side(old).line(line).0.to_owned()),
            ),
        };
        let mut second = vec![Entry::new(
            "Comment on this line",
            Act::Comment { old, line },
        )];
        if let (false, Some(path), Some(editor)) = (
            old,
            self.review.read(cx).current_path(),
            crate::editor::pick(self.settings.editor),
        ) {
            second.push(Entry::new(
                format!("Open in {} at line {line}", editor.label()),
                Act::OpenEditor { path, line },
            ));
        }
        // What a reviewer pastes into a chat: a link to the line, its place, the command.
        let mut share = Vec::new();
        if let (Some(path), Some(rev)) = (
            self.review.read(cx).current_path(),
            self.review.read(cx).viewed_rev(),
        ) {
            if let (false, Some(web)) = (old, &self.web) {
                share.push(
                    Entry::new(
                        format!("Copy link to line · {}", web.name()),
                        Act::Copy(web.blob(&rev.to_string(), &path, line)),
                    )
                    .key("↗"),
                );
            }
            share.push(Entry::new(
                "Copy path:line",
                Act::Copy(format!("{path}:{line}")),
            ));
            if matches!(
                self.branches().map(|b| b.selection(cx)),
                Some(Selection::Commit(_))
            ) {
                share.push(Entry::new(
                    format!("Copy git show {}", format::short(rev)),
                    Act::Copy(format!("git show {}", format::short(rev))),
                ));
            }
        }
        self.open_menu(at, vec![vec![copy], second, share], cx);
    }

    fn commit_context(&mut self, ix: usize, at: Point<gpui::Pixels>, cx: &mut Context<Self>) {
        let Some(commit) = self.branches().and_then(|b| b.commit(ix, cx)) else {
            return;
        };
        let mut groups = vec![vec![
            Entry::new("Copy SHA", Act::Copy(commit.id.to_string())),
            Entry::new("Copy message", Act::Copy(commit.message.clone())),
        ]];
        // Its page on the host; one only on this Mac says so instead (#162).
        let link = self.commit_link_entry(commit.id, cx);
        let on_host = link.as_ref().is_some_and(|e| e.act.is_some());
        groups[0].extend(link);
        if let Some(web) = self.web.as_ref().filter(|_| on_host) {
            groups.push(vec![Entry::new(
                format!("Open in {} ↗", web.name()),
                Act::OpenUrl(web.commit(&commit.id.to_string())),
            )]);
        }
        self.open_menu(at, groups, cx);
    }

    fn file_context(&mut self, ix: usize, at: Point<gpui::Pixels>, cx: &mut Context<Self>) {
        let Some(path) = self
            .review
            .read(cx)
            .diff
            .as_ref()
            .and_then(|d| d.files.get(ix))
            .map(|f| f.path().to_owned())
        else {
            return;
        };
        let name = path.rsplit('/').next().unwrap_or(&path).to_owned();
        let mut groups = vec![vec![
            Entry::new("Copy path", Act::Copy(path.clone())),
            Entry::new("Copy file name", Act::Copy(name)),
        ]];
        // Where the host anchors the diff's files: this one in it (#162).
        if let Some(link) = self.diff_file_link(&path, cx) {
            groups[0].push(Entry::new("Copy link", link.act()).key("↗"));
        }
        if let Some(editor) = crate::editor::pick(self.settings.editor) {
            groups.push(vec![Entry::new(
                format!("Open in {}", editor.label()),
                Act::OpenEditor { path, line: 1 },
            )]);
        }
        self.open_menu(at, groups, cx);
    }

    pub(super) fn branch_context(
        &mut self,
        ix: usize,
        at: Point<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) {
        let Some(branch) = self.branches().and_then(|b| b.list(cx).get(ix).cloned()) else {
            return;
        };
        let mut groups = vec![vec![Entry::new(
            "Copy branch name",
            Act::Copy(branch.name.clone()),
        )]];
        // Its page on the host; one only on this Mac says so instead (#162).
        let link = self.branch_link(&branch.name, cx).ok();
        groups[0].extend(self.branch_link_entry(&branch.name, cx));
        if self.requests.read(cx).available && branch.name != "main" && branch.name != "master" {
            groups.push(vec![
                Entry::new("Create merge request…", Act::CreateRequest(ix)).key("⌥⌘M"),
            ]);
        }
        if let (Some(web), Some(link)) = (&self.web, link) {
            groups.push(vec![Entry::new(
                format!("Open in {} ↗", web.name()),
                Act::OpenUrl(link.url),
            )]);
        }
        self.open_menu(at, groups, cx);
    }

    fn toggle(&mut self, opt: Opt, cx: &mut Context<Self>) {
        self.set_option(opt, !self.review.read(cx).option(opt), cx);
    }

    /// Recomputes the selected commit's or versions' diff, staying on the same file.
    fn reload_diff(&mut self, cx: &mut Context<Self>) {
        let keep = self
            .review
            .read(cx)
            .diff
            .as_ref()
            .and_then(|d| d.files.get(self.review.read(cx).file))
            .map(|f| f.path().to_owned());
        match self.branches().map_or(Selection::None, |b| b.selection(cx)) {
            Selection::Commit(ix) => self.load_commit(ix, keep, cx),
            Selection::Commits { newest, oldest } => self.load_commits(newest, oldest, keep, cx),
            Selection::Versions { from, to } => self.load_versions(from, to, keep, cx),
            Selection::WorkingTree => self.load_working_tree(keep, cx),
            Selection::None if self.narrowed_compare(cx).is_some() => {
                self.load_compare_pick(keep, cx)
            }
            Selection::None => {}
        }
    }

    /// ⌘+ and ⌘−: the whole interface one step bigger or smaller.
    pub(super) fn ui_zoom_by(&mut self, steps: i32, window: &mut Window, cx: &mut Context<Self>) {
        let next = i32::from(self.settings.ui_zoom) + steps * i32::from(Settings::ZOOM_STEP);
        self.set_ui_zoom(next.max(0) as u16, window, cx);
    }

    pub(super) fn set_ui_zoom(
        &mut self,
        percent: u16,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let zoom = percent.clamp(Settings::ZOOM_MIN, Settings::ZOOM_MAX);
        if zoom == self.settings.ui_zoom {
            return;
        }
        self.settings.ui_zoom = zoom;
        self.save_settings(cx);
        apply_ui_zoom(zoom, window);
        self.review.update(cx, |rv, _| rv.char_width = 0.);
        self.relayout_all(cx);
        cx.notify();
    }

    /// Code size only (Settings → Appearance, in points: `fonts.rs`): on top of the interface size.
    pub(super) fn set_zoom(&mut self, percent: u16, cx: &mut Context<Self>) {
        let zoom = percent.clamp(Settings::ZOOM_MIN, Settings::ZOOM_MAX);
        if zoom == self.settings.zoom {
            return;
        }
        self.settings.zoom = zoom;
        self.save_settings(cx);
        // The width of a character is measured again at the new size; the rows change height.
        self.review.update(cx, |rv, _| rv.char_width = 0.);
        self.relayout_all(cx);
        cx.notify();
    }

    /// The side panel's Changes is open and has the files as a zone.
    pub(super) fn files_zone(&self, cx: &App) -> bool {
        self.side_open()
            && self.review.read(cx).changes_open
            && self
                .review
                .read(cx)
                .diff
                .as_ref()
                .is_some_and(|d| d.files.len() > 1)
    }

    fn unfold(&mut self, path: &str, cx: &mut Context<Self>) {
        self.review.update(cx, |rv, cx| rv.unfold(path, cx));
        cx.notify();
    }

    // ---- actions -------------------------------------------------------------------------

    fn open(&mut self, _: &Open, _: &mut Window, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Open".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = picked.await else {
                return;
            };
            if let Some(path) = paths.into_iter().next() {
                this.update(cx, |this, cx| this.request_open(path, None, cx))
                    .ok();
            }
        })
        .detach();
    }

    fn refresh(&mut self, _: &Refresh, _: &mut Window, cx: &mut Context<Self>) {
        let Some(root) = self.root.clone() else {
            return;
        };
        let branches = self.branches();
        let branch = branches
            .as_ref()
            .and_then(|b| Some(b.list(cx).get(b.branch(cx)?)?.clone()));
        let commit = match branches.as_ref().map(|b| b.selection(cx)) {
            Some(Selection::Commit(ix)) => branches.and_then(|b| b.commit(ix, cx)).map(|c| c.id),
            _ => None,
        };
        let keep = branch.map(|b| (b.refname.clone(), commit));
        self.forget_news(cx);
        self.open_repo(root, keep, cx);
    }

    fn previous_commit(&mut self, _: &PreviousCommit, _: &mut Window, cx: &mut Context<Self>) {
        self.zone_vertical(false, cx);
    }

    fn next_commit(&mut self, _: &NextCommit, _: &mut Window, cx: &mut Context<Self>) {
        self.zone_vertical(true, cx);
    }

    fn previous_file(&mut self, _: &PreviousFile, _: &mut Window, cx: &mut Context<Self>) {
        self.zone_horizontal(false, cx);
    }

    fn previous_step(&mut self, _: &PreviousCommitOrFile, _: &mut Window, cx: &mut Context<Self>) {
        if !self.step_item_commit(false, cx) {
            self.zone_horizontal(false, cx);
        }
    }

    fn next_step(&mut self, _: &NextCommitOrFile, _: &mut Window, cx: &mut Context<Self>) {
        if !self.step_item_commit(true, cx) {
            self.zone_horizontal(true, cx);
        }
    }

    fn next_file(&mut self, _: &NextFile, _: &mut Window, cx: &mut Context<Self>) {
        self.zone_horizontal(true, cx);
    }

    fn filter_files_action(&mut self, _: &FilterFiles, _: &mut Window, cx: &mut Context<Self>) {
        // ⌥⌘P in Code pins the tab on screen (#20): Code has no changed files to filter.
        if self.nav.space == spaces::Space::Code && self.pin_shown_tab(cx) {
            return;
        }
        self.zone = zones::Zone::Files;
        let keep = self.zen.is_none();
        self.review.update(cx, |rv, cx| rv.open_filter(keep, cx));
        self.take_keyboard(typing::Input::Part(artoo_review::path_filter::INPUT), cx);
        cx.notify();
    }

    fn go_back_action(&mut self, _: &GoBack, _: &mut Window, cx: &mut Context<Self>) {
        self.go_back(cx);
    }

    fn go_forward_action(&mut self, _: &GoForward, _: &mut Window, cx: &mut Context<Self>) {
        self.go_forward(cx);
    }

    /// ⌘S: Save in the theme editor (X48); elsewhere it does nothing.
    fn save_theme_key(&mut self, _: &SaveTheme, _: &mut Window, cx: &mut Context<Self>) {
        if self.settings_open && self.theme_edit.is_some() {
            self.save_theme(cx);
        } else {
            cx.propagate();
        }
    }

    fn toggle_sidebar(&mut self, _: &ToggleSidebar, _: &mut Window, cx: &mut Context<Self>) {
        // A narrow window has it folded into the menu: it opens as the drawer (#80).
        if self.compact() && !self.side_icons(cx).is_empty() {
            return self.toggle_drawer(None, cx);
        }
        self.layout.show_sidebar = !self.layout.show_sidebar;
        self.save_layout();
        self.mark_motion("side", cx);
        cx.notify();
    }

    /// `key` opened or closed now: drawn on its way in or out, and the window drawn again once its
    /// motion is over, when what left is gone (#56).
    fn mark_motion(&mut self, key: &str, cx: &mut Context<Self>) {
        self.motion.mark(key);
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(motion::BASE + std::time::Duration::from_millis(120))
                .await;
            this.update(cx, |_, cx| cx.notify()).ok();
        })
        .detach();
    }

    /// ⌥⌘2: folds or opens the side panel's Changes.
    fn toggle_files(&mut self, _: &ToggleFiles, _: &mut Window, cx: &mut Context<Self>) {
        self.fold_changes(cx);
    }

    /// ⌥⌘3: the Merge requests space, as ⌃⌘4 (#71).
    fn toggle_requests(&mut self, _: &ToggleRequests, _: &mut Window, cx: &mut Context<Self>) {
        self.go_space(spaces::Space::Requests, cx);
    }

    /// Keeps the panels in `layout.txt` as they are outside zen: zen hides the side panel (and
    /// folds Changes, which the review keeps) only for a while, and saving that started the next
    /// launch with them hidden when the app was quit in zen (the developer, 2026-10-23).
    pub(super) fn save_layout(&self) {
        let mut layout = self.layout.clone();
        if let Some((sidebar, _)) = self.zen {
            layout.show_sidebar = sidebar;
        }
        layout.save();
    }

    /// ⌘.: zen — the diff alone, without the rail, with a line of the keys that move a review on;
    /// pressed again, the islands come back as they were.
    fn focus_diff(&mut self, _: &FocusDiff, _: &mut Window, cx: &mut Context<Self>) {
        match self.zen.take() {
            Some((sidebar, files)) => {
                self.layout.show_sidebar = sidebar;
                self.review
                    .update(cx, |rv, cx| rv.open_changes(files, true, false, cx));
                // One file at a time goes with it: every file in one scroll again.
                if std::mem::take(&mut self.zen_one) {
                    self.show_files_anew(cx);
                }
            }
            None => {
                self.zen = Some((self.layout.show_sidebar, self.review.read(cx).changes_open));
                self.layout.show_sidebar = false;
                self.review
                    .update(cx, |rv, cx| rv.open_changes(false, false, false, cx));
            }
        }
        self.save_layout();
        // The panels slide out and back in (#56, 16).
        self.mark_motion("side", cx);
        self.mark_motion("zen", cx);
        cx.notify();
    }

    /// Help → Set Up Artoo…, ⌘K (#143): every part again.
    fn set_up(&mut self, _: &SetUp, _: &mut Window, cx: &mut Context<Self>) {
        self.open_setup(setup::Part::ALL.to_vec(), cx);
    }

    /// ⌘K's Merge this merge request (#164): the card, or what is in the way.
    fn merge_request(&mut self, _: &MergeRequest, _: &mut Window, cx: &mut Context<Self>) {
        self.open_merge(cx);
    }

    /// ⌥⌘.: zen with one file at a time, at a reading width (#55); outside zen it enters zen so.
    /// Pressed again, every file in one scroll, still in zen.
    fn zen_one_file(&mut self, _: &ZenOneFile, window: &mut Window, cx: &mut Context<Self>) {
        if self.zen.is_none() {
            self.focus_diff(&FocusDiff, window, cx);
        }
        self.zen_one = !self.zen_one;
        self.show_files_anew(cx);
        cx.notify();
    }

    /// The files shown again as they now should be, one at a time or in one scroll, the file and
    /// the place in it kept.
    fn show_files_anew(&mut self, cx: &mut Context<Self>) {
        self.review
            .update(cx, |rv, _| rv.restore_scroll = rv.scroll_place());
        self.review.update(cx, |rv, _| rv.clear_file());
        self.select_file(self.review.read(cx).file, cx);
    }

    fn toggle_unified(&mut self, _: &ToggleUnified, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle(Opt::Unified, cx);
    }

    fn cycle_mode(&mut self, _: &CycleMode, _: &mut Window, cx: &mut Context<Self>) {
        self.set_mode(self.review.read(cx).options.mode.next(), cx);
    }

    fn set_lines(&mut self, _: &SetLines, _: &mut Window, cx: &mut Context<Self>) {
        self.set_mode(DiffMode::Lines, cx);
    }

    fn set_words(&mut self, _: &SetWords, _: &mut Window, cx: &mut Context<Self>) {
        self.set_mode(DiffMode::Words, cx);
    }

    fn set_structural(&mut self, _: &SetStructural, _: &mut Window, cx: &mut Context<Self>) {
        self.set_mode(DiffMode::Structural, cx);
    }

    fn open_settings_action(&mut self, _: &OpenSettings, _: &mut Window, cx: &mut Context<Self>) {
        self.open_settings(cx);
    }

    fn open_palette_action(
        &mut self,
        _: &SearchEverywhere,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.search(palette::Scope::All, cx);
    }

    /// ⌘K's Pin tab: Code's tab on screen pinned or unpinned (#20).
    fn pin_tab_action(&mut self, _: &PinTab, _: &mut Window, cx: &mut Context<Self>) {
        if !self.pin_shown_tab(cx) {
            self.toast_info("Pin tab: open a file or a folder in Code".into(), cx);
        }
    }

    fn copy_link_action(&mut self, _: &CopyLink, _: &mut Window, cx: &mut Context<Self>) {
        self.copy_page_link(cx);
    }

    fn compare_action(&mut self, _: &Compare, _: &mut Window, cx: &mut Context<Self>) {
        if self.root.is_some() {
            self.open_palette(palette::Kind::Base, cx);
        }
    }

    fn palette_up(&mut self, _: &PaletteUp, _: &mut Window, cx: &mut Context<Self>) {
        if self.review.read(cx).find_files_open() {
            return self.ffind_step(false, cx);
        }
        if self.beside.is_some() {
            return self.step_beside(false, cx);
        }
        self.palette_step(false, cx);
    }

    fn palette_down(&mut self, _: &PaletteDown, _: &mut Window, cx: &mut Context<Self>) {
        if self.review.read(cx).find_files_open() {
            return self.ffind_step(true, cx);
        }
        if self.beside.is_some() {
            return self.step_beside(true, cx);
        }
        self.palette_step(true, cx);
    }

    fn next_match(&mut self, _: &NextMatch, _: &mut Window, cx: &mut Context<Self>) {
        self.step_match(true, cx);
    }

    fn previous_match(&mut self, _: &PreviousMatch, _: &mut Window, cx: &mut Context<Self>) {
        self.step_match(false, cx);
    }

    /// ⌘F filters a space's list (#72), finds in the open diff, and with none open searches the
    /// commits.
    fn find_action(&mut self, _: &Find, _: &mut Window, cx: &mut Context<Self>) {
        if self.at_root_list() && !self.settings_open {
            self.start_field(lists::Field::Branches, cx);
        } else if self.review.read(cx).diff.is_some() && !self.settings_open {
            self.start_dfind(cx);
        } else {
            self.start_find(cx);
        }
    }

    fn find_commits_action(&mut self, _: &FindCommits, _: &mut Window, cx: &mut Context<Self>) {
        self.start_find(cx);
    }

    fn next_change(&mut self, _: &NextChange, _: &mut Window, cx: &mut Context<Self>) {
        self.step_change(true, cx);
    }

    fn previous_change(&mut self, _: &PreviousChange, _: &mut Window, cx: &mut Context<Self>) {
        self.step_change(false, cx);
    }

    fn toggle_wrap(&mut self, _: &ToggleWrap, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle(Opt::Wrap, cx);
    }

    fn toggle_full_context(
        &mut self,
        _: &ToggleFullContext,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle(Opt::FullContext, cx);
    }

    fn toggle_expand_all_action(
        &mut self,
        _: &ToggleExpandAll,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_expand_all(cx);
    }

    fn toggle_one_file(&mut self, _: &ToggleOneFile, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle(Opt::OneFile, cx);
    }

    fn toggle_whitespace(&mut self, _: &ToggleWhitespace, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle(Opt::Whitespace, cx);
    }

    fn toggle_rich_comments_action(
        &mut self,
        _: &ToggleRichComments,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_rich_comments(cx);
    }

    // ---- resizing ------------------------------------------------------------------------

    /// What the tab puts on the diff's column of an item page (`Frame::chrome`): the zone's border, the
    /// menu open, Open in, the file as a page with its pins where the review shows it so, zen's one
    /// file, the part's input with the keyboard.
    pub(super) fn chrome(&self, cx: &mut Context<Self>) -> artoo_shell_api::Chrome {
        let this = cx.entity().downgrade();
        let alone = {
            let rv = self.review.read(cx);
            let ix = rv.file;
            rv.diff
                .as_ref()
                .and_then(|d| d.files.get(ix))
                .filter(|f| rv.stack.is_none() && !rv.generated_folded(f) && rv.shows_page(f))
                .map(|_| ix)
        }
        .map(|ix| self.render_page_alone(ix, this, cx));
        artoo_shell_api::Chrome {
            border: self.zone_border(zones::Zone::Diff),
            menu_open: self.ctx_menu.read(cx).is_open(),
            open_in: self.open_in_chips(cx),
            alone,
            zen_one: self.zen_one && self.zen.is_some(),
            typing: self.part_typing(cx),
        }
    }

    /// What the screen shows, in a few words: zen's one line on top, a request's or what the diff is of.
    fn header_title(&self, diff: &Diff, cx: &App) -> String {
        if let Some(m) = self
            .requests
            .read(cx)
            .current
            .filter(|_| self.showing_request(cx))
            .and_then(|iid| self.requests.read(cx).find(iid))
        {
            return format!("{} {}", words::number(m.iid), m.title);
        }
        head::title(diff, self.compare_title(cx))
    }
}

/// The tab as the review's host (`artoo_review_api::Host`): it draws what its features pin on lines and the stack's
/// headers and notes, and follows what a line's events and the pane's clicks ask.
struct TabHost(gpui::WeakEntity<Workspace>);

impl artoo_review_api::Host for TabHost {
    fn pin(&self, row: Row, active: bool, indent: f32, cx: &App) -> gpui::AnyElement {
        let Some(workspace) = self.0.upgrade() else {
            return div().into_any_element();
        };
        let workspace = workspace.read(cx);
        let this = self.0.clone();
        match row {
            Row::Thread(id) => workspace.render_thread(id, indent, this, cx),
            Row::Request(ix) => workspace.render_request_thread(ix, indent, this, cx),
            Row::Draft(ix) => workspace.render_draft(ix, indent, this, cx),
            Row::Note(id) => workspace.render_agent_note(id, indent, this, cx),
            Row::Composer if workspace.compose.is_none() && active => {
                workspace.render_goline_row(indent, this, cx)
            }
            Row::Composer if workspace.compose.is_some() => {
                workspace.render_composer(indent, this, cx)
            }
            _ => div().into_any_element(),
        }
    }

    fn badge(&self, path: &str, cx: &App) -> Option<gpui::AnyElement> {
        let workspace = self.0.upgrade()?;
        let (name, letter, hue) = workspace.read(cx).changing(path, cx)?;
        Some(
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap(px(6.))
                .text_size(px(12.))
                .text_color(theme::muted())
                .child(artoo_agents_api::view::chat_badge(letter, hue, false, cx))
                .child(format!("{name} is changing it"))
                .into_any_element(),
        )
    }

    fn page(&self, file: usize, cx: &App) -> gpui::AnyElement {
        match self.0.upgrade() {
            Some(workspace) => workspace.read(cx).render_page(file, self.0.clone(), cx),
            None => div().into_any_element(),
        }
    }

    fn opening(&self, file: usize, cx: &App) -> bool {
        self.0.upgrade().is_some_and(|workspace| {
            let motion = &workspace.read(cx).motion;
            motion.recent(&format!("file-{file}")) || motion.recent("files")
        })
    }

    fn line(&self, file: usize, active: bool, event: artoo_review_api::LineEvent, cx: &mut App) {
        use artoo_review_api::LineEvent;
        self.0
            .update(cx, |this, cx| {
                // A press in another file makes it the active one first.
                if !active && !matches!(event, LineEvent::Number { .. } | LineEvent::Drag { .. }) {
                    this.activate_file(file, cx);
                }
                match event {
                    LineEvent::Comment { old, line } => this.start_comment(old, line, cx),
                    LineEvent::Toggle { old, line } => this.toggle_notes_at(old, line, cx),
                    LineEvent::Annotation { line } => this.open_annotated_commit(line, cx),
                    LineEvent::Press {
                        old,
                        line,
                        byte,
                        clicks,
                        shift,
                    } => this.select_press(old, line, byte, clicks, shift, cx),
                    LineEvent::Drag { old, line, byte } => this.select_drag(old, line, byte, cx),
                    LineEvent::Context { old, line, at } => this.line_context(old, line, at, cx),
                    LineEvent::Number { old, line, at } => {
                        this.hover_line_number(file, old, line, at, cx)
                    }
                    LineEvent::Expand { segment } => this.expand_gap(segment, cx),
                }
            })
            .ok();
    }

    fn pane(&self, event: artoo_review_api::PaneEvent, cx: &mut App) {
        use artoo_review_api::PaneEvent;
        self.0
            .update(cx, |this, cx| match event {
                PaneEvent::Pressed => this.set_zone(zones::Zone::Diff, cx),
                PaneEvent::Step { forward } => this.step_change(forward, cx),
                PaneEvent::Unfold(path) => this.unfold(&path, cx),
                PaneEvent::SvgSource(source) => {
                    this.review.update(cx, |rv, _| rv.images.source = source);
                    cx.notify();
                }
                PaneEvent::UndoNotice => this.undo_notice(cx),
                PaneEvent::DismissNotice => {
                    this.review.update(cx, |rv, _| rv.notice = None);
                    cx.notify();
                }
                PaneEvent::Unified(on) => this.set_option(Opt::Unified, on, cx),
                PaneEvent::ExpandAll { folded } => this.expand_all(folded, cx),
                PaneEvent::ViewMenu { under } => {
                    // The menu hangs from the button, its right edge on the button's.
                    let at = Point::new(under.right() - px(menu::WIDTH), under.bottom() + px(6.));
                    let groups = this.view_menu(cx);
                    this.open_titled(at, groups, cx);
                }
                PaneEvent::ImageMode(mode) => {
                    this.review.update(cx, |rv, _| rv.images.mode = mode);
                    cx.notify();
                }
                PaneEvent::ImageMix(mix) => {
                    this.review.update(cx, |rv, _| rv.images.mix = mix);
                    cx.notify();
                }
                PaneEvent::Review => {
                    let rv = this.review.read(cx);
                    if rv.review_state(rv.file) == Some(artoo_review::reviewed::State::Done) {
                        this.toggle_reviewed(cx)
                    } else {
                        this.mark_and_advance(cx)
                    }
                }
            })
            .ok();
    }

    fn file(&self, event: artoo_review_api::FileEvent, cx: &mut App) {
        use artoo_review_api::FileEvent;
        self.0
            .update(cx, |this, cx| match event {
                FileEvent::Fold(ix) => {
                    this.activate_file(ix, cx);
                    this.flip_fold(ix, cx);
                }
                FileEvent::Alone(ix) => this.view_alone(ix, cx),
                FileEvent::Note(line) => this.start_file_comment(line, cx),
            })
            .ok();
    }

    fn tree_row(&self, path: &str, row: gpui::AnyElement, cx: &App) -> gpui::AnyElement {
        match self.0.upgrade() {
            Some(workspace) => workspace.read(cx).tree_row(path, row),
            None => row,
        }
    }

    fn version_of(&self, tip: git2::Oid, cx: &App) -> Option<usize> {
        let branches = self.0.upgrade()?.read(cx).branches()?;
        branches
            .versions(cx)
            .into_iter()
            .find(|(_, t)| *t == tip)
            .map(|(number, _)| number)
    }

    fn changes(&self, event: artoo_review_api::ChangesEvent, cx: &mut App) {
        use artoo_review_api::ChangesEvent;
        use zones::Zone;
        self.0
            .update(cx, |this, cx| match event {
                ChangesEvent::ToggleReviewed(ix) => {
                    let done = this.review.read(cx).review_state(ix)
                        == Some(artoo_review::reviewed::State::Done);
                    this.set_reviewed(ix, !done, cx);
                }
                ChangesEvent::FolderReviewed { folder, done } => {
                    let ixs = this.review.read(cx).files_under(&folder);
                    this.set_reviewed_many(&ixs, done, cx);
                }
                ChangesEvent::ReviewAll => this.toggle_all_reviewed(cx),
                ChangesEvent::Pick(ix) => {
                    // Picked in the drawer, it goes and the file is on screen (#80).
                    this.close_drawer(cx);
                    this.select_file(ix, cx)
                }
                ChangesEvent::Menu { ix, at } => this.file_context(ix, at, cx),
                ChangesEvent::Fold(path) => this.toggle_dir(&path, cx),
                ChangesEvent::Section => this.fold_changes(cx),
                ChangesEvent::ListFiles(list) => this.set_file_view(list, cx),
                ChangesEvent::Pressed => {
                    if this.files_zone(cx) {
                        this.set_zone(Zone::Files, cx)
                    }
                }
            })
            .ok();
    }

    fn draft(&self, cx: &App) -> Option<String> {
        self.0.upgrade()?.read(cx).draft_path(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_has_a_default_size() {
        assert_eq!(window_size(), (1480., 920.));
    }
    /// The keys of `map` taken twice, as Settings → Keyboard shows them: (label, (key, the other action)).
    fn clashes(map: keymap::Keymap) -> Vec<(String, (String, String))> {
        map.entries()
            .into_iter()
            .filter_map(|e| e.clash.map(|c| (e.label, c)))
            .collect()
    }

    #[test]
    fn the_shipped_keys_do_not_clash() {
        let shipped = keymap::Keymap::with_preset(
            default_bindings(),
            crate::platform::MAC,
            keymap::Preset::Artoo,
        );
        let clashes = clashes(shipped);
        assert!(clashes.is_empty(), "{clashes:?}");
    }

    #[test]
    fn the_shipped_keys_do_not_clash_on_windows_and_linux() {
        let shipped = keymap::Keymap::with_preset(
            bindings(crate::platform::pc_keys),
            false,
            keymap::Preset::Artoo,
        );
        let clashes = clashes(shipped);
        assert!(clashes.is_empty(), "{clashes:?}");
    }

    #[test]
    fn no_preset_clashes_and_each_names_real_actions() {
        // macOS's keys and Windows and Linux's, whichever OS runs the test.
        for mac in [true, false] {
            let keys: fn(&str) -> String = if mac {
                str::to_owned
            } else {
                crate::platform::pc_keys
            };
            for preset in keymap::Preset::ALL {
                let map = keymap::Keymap::with_preset(bindings(keys), mac, preset);
                let entries = map.entries();
                for name in preset.actions() {
                    assert!(
                        entries.iter().any(|e| e.name == name),
                        "{preset:?}: no {name}"
                    );
                }
                let clashes = clashes(map);
                assert!(clashes.is_empty(), "{preset:?}, mac {mac}: {clashes:?}");
            }
        }
    }

    #[test]
    fn a_change_says_what_the_preset_had() {
        // alt, not cmd: cmd is written super- on Linux and win- on Windows.
        let map = keymap::Keymap::from_text(
            default_bindings(),
            crate::platform::MAC,
            "Preset = android-studio\nSearchFiles = alt-p\n",
        );
        assert_eq!(map.preset(), keymap::Preset::AndroidStudio);
        let files = map
            .entries()
            .into_iter()
            .find(|e| e.name == "SearchFiles")
            .unwrap();
        assert_eq!(files.keys, ["alt-p"]);
        assert_eq!(files.was, [crate::platform::keys("cmd-shift-o")]);
    }
}
