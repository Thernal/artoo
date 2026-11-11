//! The window root's commands as data (#171): each with its id, its words in ⌘K and its place there, the keys it
//! ships with and where they apply, its item in the menu bar. They are offered to the shell's registry
//! (`artoo_shell_api::add_action`) beside the features' own, and ⌘K, the menu bar and the keys are made from what
//! was offered, nothing else. The tab answers these (`frame.rs`); a command moves to its feature once the feature
//! can run it alone, keeping its id, its GPUI name (`keymap.txt` keeps it) and its places.

use super::*;
use artoo_shell_api::when::{APP, LISTS, NO_CARET, NOT_TYPING, PALETTE, TAB};
use artoo_shell_api::{Action, Menu as Bar};
use gpui::{DummyKeyboardMapper, Global, KeyBindingContextPredicate};

/// The view options of the tab in front, which the View menu's choices say (`set_menus`).
#[derive(Default)]
struct Front(ViewOptions);

impl Global for Front {}

fn front(cx: &App) -> ViewOptions {
    cx.try_global::<Front>().map(|f| f.0).unwrap_or_default()
}

/// Offers the window root's commands, before the features offer theirs (`run`).
pub(super) fn install(cx: &mut App) {
    for action in own() {
        artoo_shell_api::add_action(cx, action);
    }
}

/// The keys `actions` ship with, as bindings, put in this OS's terms by `k`. A key that does not parse is left
/// out, and the log says so.
pub(super) fn bindings(actions: &[Action], k: fn(&str) -> String) -> Vec<KeyBinding> {
    actions
        .iter()
        .flat_map(|a| a.keys.iter().map(move |key| (a, key)))
        .filter_map(|(a, key)| {
            let when = match key.when {
                APP => None,
                when => Some(Rc::new(KeyBindingContextPredicate::parse(when).ok()?)),
            };
            let bound = KeyBinding::load(
                &k(key.keys),
                a.act.boxed_clone(),
                when,
                false,
                None,
                &DummyKeyboardMapper,
            );
            if bound.is_err() {
                crate::applog::line(&format!("{}: key {:?} not bound", a.id, key.keys));
            }
            bound.ok()
        })
        .collect()
}

/// The menu bar made again, its choices saying `options`: the tab in front's view.
pub(super) fn set_menus(options: &ViewOptions, cx: &mut App) {
    cx.set_global(Front(*options));
    let menus = menus(cx);
    cx.set_menus(menus);
}

/// The menu bar: each menu's items as offered (`artoo_shell_api::Action::menu`), by group and order, a rule
/// between groups.
fn menus(cx: &App) -> Vec<gpui::Menu> {
    let actions = artoo_shell_api::actions(cx);
    Bar::ALL
        .into_iter()
        .map(|bar| {
            let mut items: Vec<(&artoo_shell_api::InMenu, &Action)> = actions
                .iter()
                .filter_map(|a| Some((a.menu.as_ref().filter(|m| m.menu == bar)?, a)))
                .collect();
            items.sort_by_key(|(m, _)| (m.group, m.order));
            let mut out = Vec::new();
            let mut group = None;
            for (m, a) in items {
                if group.is_some_and(|g| g != m.group) {
                    out.push(MenuItem::separator());
                }
                group = Some(m.group);
                out.push(MenuItem::Action {
                    name: m.words.into(),
                    action: a.act.boxed_clone(),
                    os_action: None,
                    checked: m.checked.as_ref().is_some_and(|on| on(cx)),
                    disabled: false,
                });
            }
            gpui::Menu::new(bar.title()).items(out)
        })
        .collect()
}

/// ⌘K's place for the `n`th of Artoo's own commands there. A feature's command that was one keeps its place: the
/// Agents window's 100 (`at(10)`), the Discussions window's 150.
const fn at(n: u16) -> u16 {
    n * 10
}

/// The window root's commands, in the order their keys were bound.
pub(super) fn own() -> Vec<Action> {
    vec![
        Action::new("shell.open", Open)
            .words("Open a repository…")
            .order(at(31))
            .key("cmd-o", APP)
            .menu(Bar::File, "Open Repository…")
            .at(1, 0),
        Action::new("shell.refresh", Refresh)
            .words("Refresh")
            .order(at(30))
            .key("cmd-r", APP)
            .menu(Bar::File, "Refresh")
            .at(1, 2),
        Action::new("shell.quit", Quit)
            .key("cmd-q", APP)
            .menu(Bar::App, "Quit Artoo")
            .at(20, 0),
        Action::new("shell.settings", OpenSettings)
            .words("Settings")
            .order(at(57))
            .key("cmd-,", APP)
            .menu(Bar::App, "Settings…")
            .at(10, 0),
        Action::new("review.find", Find)
            .words("Find in the diff")
            .order(at(18))
            .key("cmd-f", TAB)
            .menu(Bar::File, "Find in Diff…")
            .at(2, 0),
        Action::new("branches.find-commits", FindCommits)
            .words("Search commits")
            .order(at(28))
            .key("cmd-shift-f", TAB)
            .menu(Bar::File, "Find Commits…")
            .at(3, 0),
        Action::new("review.filter-files", FilterFiles)
            .words("Filter files by path")
            .order(at(29))
            .key("cmd-alt-p", TAB)
            .menu(Bar::File, "Filter Files…")
            .at(3, 1),
        Action::new("shell.back", GoBack)
            .words("Back")
            .order(at(33))
            .key("cmd-[", TAB)
            .menu(Bar::Go, "Back")
            .at(1, 0),
        Action::new("shell.forward", GoForward)
            .words("Forward")
            .order(at(34))
            .key("cmd-]", TAB)
            .menu(Bar::Go, "Forward")
            .at(1, 1),
        // ⌘J the side panel, as ⌘B the menu (the developer, 2026-10-23; ⌥⌘1 before).
        Action::new("shell.side-panel", ToggleSidebar)
            .words("Hide or show the side panel")
            .order(at(49))
            .key("cmd-j", TAB)
            .menu(Bar::View, "Show the Side Panel")
            .at(0, 0),
        Action::new("review.changes", ToggleFiles)
            .words("Fold or open Changes")
            .order(at(50))
            .key("cmd-alt-2", TAB)
            .menu(Bar::View, "Show Files")
            .at(0, 1),
        Action::new("branches.or-requests", ToggleRequests)
            .words("Branches or merge requests")
            .order(at(51)),
        // The acts are in ⌘K too (#192); keys that only move stay out.
        Action::new("review.comment-line", CommentLine)
            .words("Comment on a line…")
            .order(at(58))
            .key("c", NOT_TYPING),
        Action::new("requests.submit", SubmitReview)
            .words("Submit review")
            .order(at(58) + 2)
            .key("cmd-shift-enter", NOT_TYPING),
        // ⌘K's Create merge request… opens the form, the one way in (#190: the quick way is gone).
        Action::new("requests.create", CreateRequest)
            .words("Create merge request…")
            .order(at(8))
            .key("cmd-shift-m", NOT_TYPING)
            .menu(Bar::File, "Create Merge Request…")
            .at(1, 1),
        // ⇧⌘A the Agents window as Design gives it, ⌥⌘A sends to an agent (D12, X3).
        Action::new("agents.send", SendToAgent)
            .words("Send the comments to an agent's chat…")
            .order(at(9))
            .key("cmd-alt-a", NOT_TYPING),
        // Open in (#78): the page's file or folder in its app, or in Finder.
        Action::new("shell.open-in", OpenIn)
            .words("Open in its app — the file or the folder")
            .order(at(11))
            .key("cmd-alt-o", NOT_TYPING),
        Action::new("shell.reveal", RevealInFinder)
            .words("Reveal in Finder")
            .order(at(12))
            .key("cmd-alt-r", NOT_TYPING),
        // The menu folds with ⌘B (D2; ⌘0 stays zoom reset, the switch picker has no key); in a field ⌘B is
        // bold. ⌃⌘1…4 the spaces, Ctrl+Shift+1…4 on Windows and Linux.
        Action::new("shell.fold-menu", FoldMenu)
            .words("Fold the menu to icons")
            .order(at(13))
            .key("cmd-b", NOT_TYPING)
            .menu(Bar::View, "Fold the Menu to Icons")
            .at(0, 3),
        Action::new("requests.approve", ApproveRequest)
            .words("Approve this merge request")
            .order(at(58) + 1)
            .key("a", NOT_TYPING),
        // A request's Changes: All and Since the version you read, in turn (#187).
        Action::new("requests.since-read", ToggleSince)
            .words("Since the version you read, or all changes")
            .order(at(59))
            .key("s", NOT_TYPING),
        Action::new("requests.next-draft", NextDraft).key("alt-n", NOT_TYPING),
        Action::new("requests.previous-draft", PreviousDraft).key("alt-p", NOT_TYPING),
        Action::new("shell.shortcuts", ShowShortcuts)
            .words("Keyboard shortcuts")
            .order(at(14))
            .key("?", NOT_TYPING)
            .key("cmd-/", TAB)
            .menu(Bar::Help, "Keyboard Shortcuts")
            .at(0, 1),
        Action::new("requests.next-thread", NextThread).key("shift-n", NOT_TYPING),
        Action::new("requests.previous-thread", PreviousThread).key("shift-p", NOT_TYPING),
        Action::new("requests.reply", ReplyThread)
            .words("Reply to the discussion")
            .order(at(59) + 1)
            .key("r", NOT_TYPING),
        Action::new("requests.resolve", ResolveThread)
            .words("Resolve the discussion, or reopen it")
            .order(at(59) + 2)
            .key("x", NOT_TYPING),
        Action::new("requests.edit", EditComment)
            .words("Edit your last comment in the discussion")
            .order(at(59) + 3)
            .key("e", NOT_TYPING),
        Action::new("requests.react", ReactNote)
            .words("React to the comment…")
            .order(at(59) + 4)
            .key("shift-e", NOT_TYPING),
        Action::new("requests.fold-thread", ToggleThread)
            .words("Fold the discussion, or open it")
            .order(at(59) + 5)
            .key("o", NOT_TYPING),
        Action::new("requests.overview", ShowOverview)
            .words("Show the request's overview")
            .order(at(16))
            .key("alt-1", NOT_TYPING)
            .key("cmd-i", NOT_TYPING),
        // A request's sections by number (#191).
        Action::new("requests.commits", ShowCommits)
            .words("Show the commits of the request or branch")
            .order(at(16) + 1)
            .key("alt-2", NOT_TYPING),
        Action::new("requests.changes", ShowChanges)
            .words("Show the changes of the request or branch")
            .order(at(16) + 2)
            .key("alt-3", NOT_TYPING),
        Action::new("shell.next-section", NextSection)
            .words("Next section of the request")
            .order(at(17))
            .key("ctrl-tab", TAB),
        Action::new("shell.previous-section", PreviousSection).key("ctrl-shift-tab", TAB),
        // The items open under the space, as a browser's tabs: ⌥⌘→ ⌥⌘←, not ⌃→ (macOS's Spaces).
        Action::new("shell.next-open-item", NextOpenItem)
            .key("cmd-alt-right", NOT_TYPING)
            .menu(Bar::Go, "Next Open Item")
            .at(3, 0),
        Action::new("shell.previous-open-item", PreviousOpenItem)
            .key("cmd-alt-left", NOT_TYPING)
            .menu(Bar::Go, "Previous Open Item")
            .at(3, 1),
        Action::new("review.mark-reviewed", MarkReviewed)
            .words("Mark the file reviewed, go to the next")
            .order(at(59) + 6)
            .key("v", NOT_TYPING),
        Action::new("shell.recent", OpenRecent)
            .words("Recent places")
            .order(at(21))
            .key("cmd-e", TAB),
        Action::new("shell.zoom-in", ZoomIn)
            .words("Zoom in — bigger interface")
            .order(at(24))
            .key("cmd-=", TAB)
            .key("cmd-+", TAB)
            .key("cmd-shift-=", TAB)
            .menu(Bar::View, "Zoom In")
            .at(1, 0),
        Action::new("shell.zoom-out", ZoomOut)
            .words("Zoom out — smaller interface")
            .order(at(25))
            .key("cmd--", TAB)
            .key("cmd-_", TAB)
            .menu(Bar::View, "Zoom Out")
            .at(1, 1),
        Action::new("shell.zoom-reset", ZoomReset)
            .words("Actual size")
            .order(at(26))
            .key("cmd-0", TAB)
            .menu(Bar::View, "Actual Size")
            .at(1, 2),
        // F3 and ⌘F3 are macOS's Mission Control (#183).
        Action::new("review.bookmark", ToggleBookmark)
            .words("Bookmark the line, or take it off")
            .order(at(59) + 7)
            .key("b", NOT_TYPING),
        Action::new("review.bookmarks", ShowBookmarks)
            .words("Bookmarks")
            .order(at(23))
            .key("shift-b", NOT_TYPING),
        Action::new("review.structure", ShowStructure)
            .words("Changes in this file — functions and types")
            .order(at(22))
            .key("cmd-shift-o", TAB),
        Action::new("shell.whole-text", ShowWhole)
            .words("Show the whole text — on the focused row")
            .order(at(38))
            .key("f1", TAB),
        Action::new("review.annotate", ToggleAnnotate)
            .words("Annotate — who wrote each line")
            .order(at(20))
            .key("cmd-alt-b", NOT_TYPING),
        Action::new("review.find-in-files", FindInFiles)
            .words("Search in all files of the diff")
            .order(at(19))
            .key("cmd-alt-f", TAB),
        Action::new("review.toggle-reviewed", ToggleReviewed).key("shift-v", NOT_TYPING),
        Action::new("shell.next-zone", NextZone).key("tab", NOT_TYPING),
        Action::new("shell.previous-zone", PreviousZone).key("shift-tab", NOT_TYPING),
        Action::new("shell.activate", Activate).key("enter", NOT_TYPING),
        Action::new("shell.page-down", PageDown)
            .key("pagedown", NOT_TYPING)
            .key("space", NOT_TYPING),
        Action::new("shell.page-up", PageUp)
            .key("pageup", NOT_TYPING)
            .key("shift-space", NOT_TYPING),
        Action::new("review.start", DiffStart).key("home", NOT_TYPING),
        Action::new("review.end", DiffEnd).key("end", NOT_TYPING),
        Action::new("review.previous-file", PreviousFile)
            .key("h", NOT_TYPING)
            .key("left", NOT_TYPING),
        Action::new("review.next-file", NextFile)
            .key("l", NOT_TYPING)
            .key("right", NOT_TYPING),
        // [ ] walk a commit's neighbours on its page, else the files as ← → do (the developer, 2026-10-23).
        Action::new("branches.previous-commit-or-file", PreviousCommitOrFile).key("[", NOT_TYPING),
        Action::new("branches.next-commit-or-file", NextCommitOrFile).key("]", NOT_TYPING),
        Action::new("shell.switch-branch", SwitchBranch).key("ctrl-1", TAB),
        Action::new("shell.focus-commits", FocusCommits).key("ctrl-2", TAB),
        Action::new("shell.focus-files", FocusFiles).key("ctrl-3", TAB),
        Action::new("shell.focus-diff", FocusDiffZone).key("ctrl-4", TAB),
        Action::new("shell.new-tab", NewTab)
            .words("New tab")
            .order(at(56))
            .key("cmd-t", TAB)
            .menu(Bar::File, "New Tab")
            .at(0, 0),
        // ⌘W closes what is open on screen; a project's tab is ⇧⌘W (#71).
        Action::new("shell.close", CloseItem)
            .key("cmd-w", TAB)
            .menu(Bar::File, "Close")
            .at(0, 1),
        Action::new("shell.close-tab", CloseTab)
            .key("cmd-shift-w", TAB)
            .menu(Bar::File, "Close Tab")
            .at(0, 2),
        Action::new("shell.reopen-tab", ReopenTab)
            .key("cmd-shift-t", TAB)
            .menu(Bar::File, "Reopen Closed Tab")
            .at(0, 3),
        Action::new("shell.next-tab", NextTab)
            .key("cmd-shift-]", TAB)
            .menu(Bar::Go, "Next Tab")
            .at(2, 0),
        Action::new("shell.previous-tab", PreviousTab)
            .key("cmd-shift-[", TAB)
            .menu(Bar::Go, "Previous Tab")
            .at(2, 1),
        Action::new("shell.tab-1", Tab1).key("cmd-1", TAB),
        Action::new("shell.tab-2", Tab2).key("cmd-2", TAB),
        Action::new("shell.tab-3", Tab3).key("cmd-3", TAB),
        Action::new("shell.tab-4", Tab4).key("cmd-4", TAB),
        Action::new("shell.tab-5", Tab5).key("cmd-5", TAB),
        Action::new("shell.tab-6", Tab6).key("cmd-6", TAB),
        Action::new("shell.tab-7", Tab7).key("cmd-7", TAB),
        Action::new("shell.tab-8", Tab8).key("cmd-8", TAB),
        Action::new("shell.tab-9", Tab9).key("cmd-9", TAB),
        Action::new("shell.diff-only", FocusDiff)
            .words("Diff only")
            .order(at(52))
            .key("cmd-.", TAB)
            .menu(Bar::View, "Focus on the Diff")
            .at(0, 2),
        Action::new("shell.diff-only-one-file", ZenOneFile)
            .words("Diff only, one file at a time")
            .order(at(53))
            .key("cmd-alt-.", TAB),
        Action::new("shell.copy", CopySelection).key("cmd-c", TAB),
        Action::new("shell.select-all", SelectAll).key("cmd-a", NOT_TYPING),
        Action::new("review.next-change", NextChange)
            .key("f7", TAB)
            .key("n", NOT_TYPING)
            .menu(Bar::File, "Next Change")
            .at(4, 0),
        Action::new("review.previous-change", PreviousChange)
            .key("shift-f7", TAB)
            .key("p", NOT_TYPING)
            .menu(Bar::File, "Previous Change")
            .at(4, 1),
        Action::new("review.next-match", NextMatch)
            .key("cmd-g", TAB)
            .menu(Bar::File, "Find Next")
            .at(2, 1),
        Action::new("review.previous-match", PreviousMatch)
            .key("cmd-shift-g", TAB)
            .menu(Bar::File, "Find Previous")
            .at(2, 2),
        // Bare letters and arrows are typing while a field is active.
        Action::new("branches.previous-commit", PreviousCommit)
            .key("up", LISTS)
            .key("k", NOT_TYPING),
        Action::new("branches.next-commit", NextCommit)
            .key("down", LISTS)
            .key("j", NOT_TYPING),
        Action::new("shell.palette-up", PaletteUp).key("up", PALETTE),
        Action::new("shell.palette-down", PaletteDown).key("down", PALETTE),
        // Search everywhere (#52): one window, a tab per scope.
        Action::new("shell.search", SearchEverywhere).key("cmd-k", TAB),
        Action::new("shell.search-files", SearchFiles).key("cmd-p", TAB),
        Action::new("shell.search-commits", SearchCommits).key("cmd-shift-c", TAB),
        Action::new("shell.search-branches", SearchBranches).key("cmd-shift-b", TAB),
        Action::new("shell.search-requests", SearchRequests).key("cmd-shift-r", TAB),
        // In a comment box ⌘⇧P previews the markdown.
        Action::new("shell.search-actions", SearchActions).key("cmd-shift-p", NO_CARET),
        Action::new("branches.compare", Compare)
            .words("Compare two refs…")
            .order(at(5))
            .key("cmd-alt-c", TAB),
        // Android Studio's Copy Reference: the page on screen's link (#162).
        Action::new("shell.copy-link", CopyLink)
            .words("Copy link to this page")
            .order(at(6))
            .key("cmd-alt-shift-c", TAB),
        // A toast's Undo from the keys (#191): a resolve, a deleted draft, toasts turned off.
        Action::new("shell.undo-toast", UndoToast)
            .words("Undo what the newest notification says")
            .order(at(15) + 1)
            .key("cmd-z", NOT_TYPING),
        // A toast's button from the keys (#198); a field with a caret keeps ⌥↵.
        Action::new("shell.open-toast", OpenToast)
            .words("Open the newest notification")
            .order(at(15))
            .key("alt-enter", NO_CARET),
        // ⌘K shows its first key: ⌥D, as it always did.
        Action::new("review.cycle-mode", CycleMode)
            .words("Cycle lines, words, structural")
            .order(at(36))
            .key("alt-d", NOT_TYPING)
            .key("w", NOT_TYPING),
        Action::new("review.rendered", ToggleRendered)
            .words("Markdown files rendered, or their source")
            .order(at(37))
            .key("m", NOT_TYPING),
        Action::new("review.unified", ToggleUnified)
            .words("Toggle split / unified")
            .order(at(35))
            .key("alt-u", NOT_TYPING)
            .menu(Bar::View, "Unified Diff")
            .at(2, 0)
            .checked(|cx| front(cx).unified),
        Action::new("review.lines", SetLines)
            .menu(Bar::View, "Line Diff")
            .at(2, 1)
            .checked(|cx| front(cx).mode == DiffMode::Lines),
        Action::new("review.words", SetWords)
            .menu(Bar::View, "Word Diff")
            .at(2, 2)
            .checked(|cx| front(cx).mode == DiffMode::Words),
        Action::new("review.structural", SetStructural)
            .menu(Bar::View, "Structural Diff (difftastic)")
            .at(2, 3)
            .checked(|cx| front(cx).mode == DiffMode::Structural),
        Action::new("review.wrap", ToggleWrap)
            .words("Toggle wrap")
            .order(at(39))
            .key("alt-z", NOT_TYPING)
            .menu(Bar::View, "Wrap Long Lines")
            .at(3, 0)
            .checked(|cx| front(cx).wrap),
        Action::new("review.full-context", ToggleFullContext)
            .words("Toggle full file")
            .order(at(40))
            .key("alt-e", NOT_TYPING)
            .menu(Bar::View, "Show All Lines")
            .at(3, 1)
            .checked(|cx| front(cx).full_context),
        Action::new("review.whitespace", ToggleWhitespace)
            .words("Toggle ignore whitespace")
            .order(at(41))
            .key("alt-w", NOT_TYPING)
            .menu(Bar::View, "Hide Whitespace Changes")
            .at(3, 2)
            .checked(|cx| front(cx).ignore_whitespace),
        Action::new("review.one-file", ToggleOneFile)
            .words("All files in one scroll, or one file at a time")
            .order(at(42))
            .key("alt-a", NOT_TYPING)
            .menu(Bar::View, "One File at a Time")
            .at(3, 3)
            .checked(|cx| front(cx).one_file),
        Action::new("review.expand-all", ToggleExpandAll)
            .words("Expand all files, or collapse them all")
            .order(at(43))
            .key("alt-shift-a", NOT_TYPING),
        Action::new("review.rich-comments", ToggleRichComments)
            .words("Comments as markdown or raw text")
            .order(at(44))
            .key("alt-m", NOT_TYPING),
        Action::new("requests.leave", LeaveRequest)
            .words("Leave the merge request — back to your branch")
            .order(at(45)),
        Action::new("review.all-reviewed", ToggleAllReviewed)
            .words("Mark all files reviewed, or unmark all")
            .order(at(46))
            .key("alt-shift-v", NOT_TYPING),
        Action::new("requests.next", NextRequest)
            .words("Next merge request not reviewed yet")
            .order(at(47))
            .key("alt-down", NOT_TYPING),
        Action::new("requests.previous", PreviousRequest)
            .words("Previous merge request")
            .order(at(48))
            .key("alt-up", NOT_TYPING),
        // In ⌘K, the menus or a toast alone.
        Action::new("shell.switch", SwitchTo)
            .words("Switch to a branch or a merge request…")
            .order(at(4)),
        Action::new("shell.pin-tab", PinTab)
            .words("Pin tab")
            .order(at(7)),
        Action::new("shell.name-project", NameProject)
            .words("Name and colour this project…")
            .order(at(32)),
        Action::new("shell.set-up", SetUp)
            .words("Set up Artoo")
            .order(at(54))
            .menu(Bar::Help, "Set Up Artoo…")
            .at(0, 0),
        Action::new("requests.merge", MergeRequest)
            .words("Merge this merge request…")
            .order(at(55)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_key_of_artoos_own_is_bound() {
        let own = own();
        let keys: usize = own.iter().map(|a| a.keys.len()).sum();
        assert_eq!(bindings(&own, str::to_owned).len(), keys);
        assert_eq!(bindings(&own, crate::platform::pc_keys).len(), keys);
    }

    #[test]
    fn artoos_own_are_offered_once() {
        let own = own();
        let mut ids = HashSet::new();
        let mut names = HashSet::new();
        for a in &own {
            assert!(ids.insert(a.id), "{} twice", a.id);
            let name = artoo_shell_api::short(a.act.name());
            assert!(names.insert(name), "{name} twice");
        }
    }

    #[test]
    fn a_command_keeps_its_place_in_palette() {
        let own = own();
        let order = |id: &str| own.iter().find(|a| a.id == id).unwrap().order;
        // Settings last, as the closed list had it; the spaces, the features' now, first
        // (`the_spaces_are_their_owners_in_their_places`).
        assert!(order("requests.merge") < order("shell.settings"));
        // Before what the features offer.
        assert!(
            own.iter()
                .filter(|a| a.words.is_some())
                .all(|a| a.order < artoo_shell_api::LATER)
        );
    }
}
