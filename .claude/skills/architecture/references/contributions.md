# Contributions: routes, pages, actions, settings, toasts

**Decided:** 2026-10-29 · **Applies to:** every feature with a view · **In force:** with the shell's crates
(stage 4). Until then `Page` (`src/ui/nav.rs`), `menu::Act` and `toast` are what these replace, and a new
page, action or toast is added there in the shape below — one variant, one id, one owner.

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
8. **A toast is raised through the shell:** `shell::toast(cx, Toast { text, act })`, its button a route to open
   or an action's id.
9. **A rule that spans features belongs to the feature that owns the subject.** What a toast about an agent's
   pending comments says, and when it gives way to one about the requests it gathered, is agents'; the shell
   only shows toasts.

## Why

- `menu::Act`'s variants and the toasts are the edges that tie the UI together today: one enum holding every
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
