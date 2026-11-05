# Contributions: routes, pages, actions, settings, toasts

**Decided:** 2026-10-29 · **Applies to:** every feature with a view · **In force:** actions, parts and the
`Frame` since 2026-10-30 (`artoo-shell-api`, updates the first feature on it); routes as data since 2026-10-30
(`artoo_core::route`: `nav.txt` keeps them, the app's kinds in `src/ui/routes.rs`); the page registry (`Pages`)
since 2026-10-30, Code's pages the first on it, still drawn in the app until Code is a crate; toasts since 2026-11-04
(`Toast`, the closed `toast::Act` gone, #171); tool windows and editors since 2026-11-04 (`Tool`, the tab's
`tools::Tool` gone; `add_editor`, the core's `Editor` enum data, #171); commands since 2026-11-05 (every one offered, the
window root's as data in `src/ui/commands.rs`; ⌘K, the menu bar and the keys made from the registry alone, `palette::Cmd`
and the hand-written lists gone; a context menu's entry a closure where the menu is made, the closed `menu::Act` gone,
#171); Settings' sections since 2026-11-05 (`add_settings_section`: an id, a title, a place in the left column; the
window root's five drawn by the tab, the agents' and Appearance (`artoo-appearance`) their parts' rows — any owner's
rows in a section placed by their order, the window root's faces and panes inside Appearance; the closed
`settings::Section` gone, #171 phase 4); the kinds of link a text has unwritten since 2026-11-05 (`Parts::links`, the
requests' `!398` / `#12` the first, #171 D11); ⌘K's tabs since 2026-11-05, the window root's All and Actions with the
features' among them (`Parts::searches`, `artoo_shell_api::search`: a scope's id, label, placeholder, the action that opens it, its place, what the
All tab takes of it and its character there, its finds for a text, each a route opened as the tab opens one, or for what is no place the owner's
doing (`Opens::Run`, run once ⌘K has closed: Code's files, picked through its `Host`) — the requests', the branches', the
commits' and Code's files first, #171 7b), and the lists the same window shows alone (`Parts::lists`, `search::List`,
opened by `Frame::search`: a comparison's refs the branches', the themes to try the appearance's, the tab's places opened
lately, bookmarks, Switch to, Open a repository and faces its own — every find and doing run outside the tab's update,
so they may read it); the spaces of the menu since 2026-11-05 (`spaces::add`: an id, a label,
an icon, a place in the menu, the action that goes there — the requests' Inbox and Merge requests, Code's, the
branches'; the closed `Space` enum gone, a `Space` its id, #171 phase 6); the tab's places as routes since 2026-11-05
(the page on screen, its history, esc's way up, the items open under a space and `nav.txt` all routes, the page one up
and the item a route belongs to its pages' — `Pages::parent`, `Pages::item` —, the page bar's path and name too
(`Pages::path`), what the tab frames it as and so its keyboard zones (`Pages::screen`: a page of its own, a list, an item's
screen), and its row in the menu while it is open (`Pages::row`); a route of any kind its pages draw opens, one no page draws dropped where `nav.txt` is read; the closed `nav::Item` gone, #171 phase 7). A new page is a
route kind its owner's `Pages` draw, no variant anywhere else; `Page` (`src/ui/nav.rs`) is only the tab's reading of
the kinds it still coordinates — the item screen's bar, sections and links — and shrinks as their owners take those.
A new command is offered: the window root's in `commands.rs` while the tab runs it, a feature's in its
`install`.

**What `artoo-shell-api` holds now:** `add_settings_section` (a section of Settings: its id, its title, its place in
the left column; a second id refused), `add_action` (an `Action`: its id, its GPUI action, its words for ⌘K and its
place there, the keys it ships with each with the key context it applies in (`when::TAB`, `when::NOT_TYPING`, …), its
item in the menu bar — `Menu`'s App, File, Go, View or Help, a group and an order in it, a check asked as the menus are
set —; a second id, or a second GPUI name's last part as `keymap.txt` keeps it, refused), `add_editor` (an editor Open in opens a file in at a line, by its
key, a key offered twice refused), `spaces::add` (a space of the menu: its id, its label and icon, its order there
until dragged, the id of the action that goes to it, whose key the row's hover shows; a second id refused; `layout.txt`
keeps the order by id, one nobody offers passed over) and `add_parts` (a maker each tab calls with its `Frame`: a
title-bar item with when it shows, an `Overlay` — a card that is drawn and has the keys while open, closed by
Escape, polled each frame the tab is not busy —, a `Tool` — a tool window in the spaces it names, on their pages
only or not, in its `order`, readied as it opens, given typed keys while it has the keyboard and says it is typing;
every tool window is one (#171) —, Settings rows (`settings::Rows`: a section's by its id, their order among its
rows, what Set up draws of them, and whether they fill the section now, take esc, end as Settings shuts),
the kinds of link it finds in the tab's texts (`links::Pattern`: an id, an order, its finds — bytes and an address;
every text the kit's markdown draws with the tab's `Selecting` shows them, an earlier pattern's kept where two meet,
what is written as a link or code winning; the requests' numbers the first, #171 D11),
the tab's handlers of the feature's actions, the snapshot views it opens (a name ending in `:` given what follows), a space's `Pages` — the kinds of route
they draw, the page one up from each and what stays open of it (`parent`, `item`), what the page bar says of each — the path above it, each part a
route up or the space's own page, its name, the name's long hover (a tip or a card) and the faint count after it (`path`, a `PagePath`) —, what the tab frames each as (`screen`, a `Screen`: a page of its own with the tool window the only
keyboard zone beside it; a list of the tab's, ↑ ↓ ↵ its rows and ⌃1; an item's screen, the side panel's commits, the review's files and diff, its
sections in the bar, zen's keys), the menu's row of what is open of theirs under the space (`row`, an `ItemRow`: its icon,
words, hover and whether something of it is unread — an item open there given the page it was left on, or the page the space keeps when it
belongs to no item, Code's file —; the tab adds whether it is on screen, the agents' marks and the comments waiting), their view, the panel beside them, and `show`, which the shell calls with the route on screen, or none
for the space's own page, and whether it was just opened or gone back to). What the `Frame` changes (a toast, the
error line, a redraw, a tool window shown, a menu at the pointer — `MenuItem`s, each running its own closure, its groups titled or not —, the
narrow tab's drawer closed, a route opened — a kind no pages draw says so in a toast —, a space's own page opened, a space gone to as its row
goes (`go_space`: the page it was left on, or what the project lacks said),
a route kept among the places opened lately) waits until the update under way is over, so a part may ask it from
its own update and from a hook. Its `Services` — what the tab's features offer one another, a contract's handle by
its type — are set and read at once. What it reads (the selection, the `Places` the tab has open, whether a tool window types) needs the tab
free: the shell calls a part's key, close, poll, opened and snapshot hooks from inside its own update, so a hook
defers what reads it (`cx.defer`); the part's views, listeners and tasks read it as they like.

## Rule

1. **A place is a route, and a route is data:** who draws it (`kind`, a stable id such as `requests.request`) and
   what it is (`args`, its fields as JSON) — the core's `Route`. It is what `nav.txt` keeps, what history
   compares, what an agent's event and a link carry, so none of them needs the type of the feature that
   draws it.
2. **Each module registers its own routes, in `install`; a plugin registers its kinds the same way while
   running.** The shell's registry maps a kind to what draws it — one owner per kind, a second registration of a
   kind refused. Nothing lists every kind.
3. **A route others may open is a type in its owner's `-api`:** `pub struct RequestRoute { number: u64 }`,
   implementing the core's `Place` (its `KIND`, its fields through serde), so opening one is typed —
   `shell::open(cx, RequestRoute { number: 398 })` — and needs only the owner's contract. A route only its own
   feature opens is `pub(crate)` in the implementation. A plugin's routes have no Rust type: it registers,
   opens and draws plain `Route`s.
4. **A route nobody draws is not a crash.** A stored one (its plugin removed, its kind renamed) is dropped when
   `nav.txt` is read; one opened says so in a toast and the log. A kind is renamed only with a migration, like
   an action's id.
5. **A page is the feature's own entity.** What a tab shows is the page the owning feature made for its route,
   behind `shell_api::Page` (its title, its view, where the keys go, what back does). A feature never adds a
   field to the shell's struct or an `impl` block to it.
6. **Actions are contributed.** A feature registers its actions in `install` — an id, the words, the keys, when
   it applies, what it does — and the menus, ⌘K and Settings → Keys list what was registered. An id is a
   stable string (`requests.merge`), because a keymap keeps it.
7. **Settings sections are contributed** the same way — a title, the rows' view, the words search finds — and
   Set up draws the same sections, never a copy.
8. **A toast is raised through the shell:** `frame.toast(Toast::new(text).button("Review", act), cx)`, its button
   an action — its id and its fields, as a route is a kind and its fields — that the tab answers: its own, or a part's
   in `Parts::actions` (one without fields, as `Run` is not handed the action). A route alone could not say what a button does on the page it opens (an
   agent's first pending comment, the New merge request card), so the button is an action.
9. **A rule that spans features belongs to the feature that owns the subject.** What a toast about an agent's
   pending comments says, and when it gives way to one about the requests it gathered, is agents'; the shell
   only shows toasts.

## Why

- `menu::Act`'s variants and the toasts were the edges that tied the UI together: one enum holding every
  feature's actions makes every feature depend on the crate that holds it, and that crate on every feature's
  code to run them. A registry turns both edges around: features know the shell's contract, the shell knows
  no feature.
- The set of places is open: a plugin adds kinds the app was built without. A closed enum would compile every
  kind into one crate, which every feature then depends on, and a plugin could add none. What the enum gave —
  `match` saying a kind is missing — the registry gives back as checks: one owner per kind, each typed route
  surviving its own encoding.
- A route kept as data outlives the code that drew it: `nav.txt` written with a plugin installed is read
  without it.
- A field on the shell's struct is the same edge as an import: the shell would change with every feature.

## Example

```rust
// crates/core/src/route.rs
#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct Route {
    pub kind: String,
    pub args: String,
}

/// A route a crate knows the type of; `Route::from(place)` and `P::from_route(&route)` convert.
pub trait Place: Serialize + DeserializeOwned {
    const KIND: &'static str;
}

// crates/requests-api/src/lib.rs
#[derive(Serialize, Deserialize)]
pub struct RequestRoute {
    pub number: u64,
}

impl Place for RequestRoute {
    const KIND: &'static str = "requests.request";
}

// crates/requests/src/lib.rs
pub fn install(cx: &mut App) {
    shell_api::pages(cx).register::<RequestRoute>(|route, window, cx| {
        RequestPage::new(route.number, window, cx).into()
    });
    shell_api::actions(cx).register(Action {
        id: "requests.merge",
        words: "Merge this merge request",
        keys: None,
        applies: |page, cx| page.is::<RequestPage>() && /* … */ true,
        run: |page, window, cx| /* … */ (),
    });
}

// crates/agents/src — says what an agent did, opens nothing itself; needs requests' contract only
shell_api::toast(cx, Toast::new(text).act("Review", RequestRoute { number }));
```

The registries' exact signatures are set when the shell moves (stage 4); the rules above hold from then.

## Checked by

With the shell's crates: a test that installs every feature and sees no kind or action id registered twice,
and that each registered typed route comes back from its `Route` unchanged. Review until then.
