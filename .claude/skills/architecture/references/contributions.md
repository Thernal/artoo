# Contributions: routes, pages, actions, settings, toasts

**Decided:** 2026-10-29 · **Applies to:** every feature with a view · **In force:** with the shell's crates
(stage 4). Until then `Page` (`src/ui/nav.rs`), `menu::Act` and `toast` are what these replace, and a new
page, action or toast is added there in the shape below — one variant, one id, one owner.

## Rule

1. **A place is a `Route`**, a plain enum in the core: a file, a folder, a branch, a commit, a comparison, the
   working tree, a request and its parts (today's `Page`). It is data — kept in `nav.txt`, compared, sent in an
   agent's event — so it is one closed enum the compiler matches exhaustively, not a type per feature.
2. **One feature opens another's place through the shell:** `shell::open(cx, Route::Request(398))`. The feature
   that draws a kind of route registers for it in `install`; exactly one feature owns each kind.
3. **A page is the feature's own entity.** What a tab shows is the page the owning feature made for its route,
   behind `shell_api::Page` (its title, its view, where the keys go, what back does). A feature never adds a
   field to the shell's struct or an `impl` block to it.
4. **Actions are contributed.** A feature registers its actions in `install` — an id, the words, the keys, when
   it applies, what it does — and the menus, ⌘K and Settings → Keys list what was registered. An id is a
   stable string (`requests.merge`), because a keymap keeps it.
5. **Settings sections are contributed** the same way — a title, the rows' view, the words search finds — and
   Set up draws the same sections, never a copy.
6. **A toast is raised through the shell:** `shell::toast(cx, Toast { text, act })`, its button a route to open
   or an action's id.
7. **A rule that spans features belongs to the feature that owns the subject.** What a toast about an agent's
   pending comments says, and when it gives way to one about the requests it gathered, is agents'; the shell
   only shows toasts.

## Why

- `menu::Act`'s variants and the toasts are the edges that tie the UI together today: one enum holding every
  feature's actions makes every feature depend on the crate that holds it, and that crate on every feature's
  code to run them. A registry turns both edges around: features know the shell's contract, the shell knows
  no feature.
- A route is persisted and compared, so a closed enum in the core costs less than a registry of route types
  and lets `match` say when a kind is missing. The enum knows the kinds of places, not the code that draws
  them.
- A field on the shell's struct is the same edge as an import: the shell would change with every feature.

## Example

```rust
// crates/requests/src/lib.rs
pub fn install(cx: &mut App) {
    shell_api::pages(cx).register(RouteKind::Request, |route, window, cx| {
        RequestPage::new(route, window, cx).into()
    });
    shell_api::actions(cx).register(Action {
        id: "requests.merge",
        words: "Merge this merge request",
        keys: None,
        applies: |page, cx| page.is::<RequestPage>() && /* … */ true,
        run: |page, window, cx| /* … */ (),
    });
}

// crates/agents/src — says what an agent did, opens nothing itself
shell_api::toast(cx, Toast::new(text).act("Review", Route::Request(number)));
```

The registries' exact signatures are set when the shell moves (stage 4); the rules above hold from then.

## Checked by

With the shell's crates: a test that every `RouteKind` has one owner and every action id is unique. Review
until then.
