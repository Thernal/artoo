---
name: architecture
description: Artoo's crate architecture — which crate a piece of code belongs to (core, ui-kit, a feature's `-api` contract or its implementation, the shell, the app), what may depend on what, how a feature publishes a trait another feature calls and how it is wired in (`install`, the app as the composition root), how one feature opens another's page or adds an action, a settings section or a toast without naming it, and how tests replace a host. Use it before adding a crate or a module, moving code between crates, adding a dependency between crates, writing a trait or a type another crate uses, or when `tests/crates.rs` fails — even when architecture is not mentioned, and in any language ("where does this go", "new crate", "bu kod hara gedir", "yeni crate", "modul əlavə et"). Not for code that stays inside one module and crosses no crate boundary.
---

# Artoo's architecture

A Cargo workspace whose one rule underlies the rest: **a feature never sees another feature's
implementation.** Features share contracts (`-api` crates) and the two shared crates; the app is the one
crate that knows every implementation, and all it does with them is wire them together.

Each file under `references/` has the rule, why, an example and what checks it. Read the ones the task
touches; most tasks need one or two.

**Status:** written 2026-10-29 from the developer's decision (api / impl / wiring, implementations
independent of each other); the Rust form below waits for the developer's review.

## The crates

| Crate | Holds | Depends on |
|---|---|---|
| `artoo-core` (`crates/core`) | what draws nothing and more than one crate needs: git, highlighting, storage, the platform layer, the diff's rows, a route as data (`Route`, `Place`) | nothing of Artoo's; no GPUI |
| `artoo-ui-kit` (`crates/ui-kit`) | the design system — what draws and knows no feature: theme, icons, tooltips, the text field, markdown — and the ports its drawing needs (`Pictures`) | core |
| `artoo-<x>-api` (`crates/<x>-api`) | feature x's contract: traits, the plain types they speak, the handle that reads them | core, GPUI |
| `artoo-<x>` (`crates/<x>`) | feature x: its implementation, its views, its state; it shows the app functions only (`install`) | core, the kit, any `-api` |
| `artoo-shell-api` / `artoo-shell` | the registries every feature contributes to (routes, pages, actions, settings sections, toasts) / the frame that shows them: window, tabs, rail, sidebar | as a feature |
| `artoo` (the root package, `src/main.rs`) | the composition root: reads the mode, calls every `install`, opens the window | everything |

→ [crates](references/crates.md)

## Where does this go

| The code… | Goes to |
|---|---|
| draws nothing and two crates need it (a git read, a parser, the diff's rows) | core, as plain functions |
| draws and knows no feature (a control, a card's frame, markdown) | the kit |
| a kind of place a page shows (a request, a commit, a file at a line) | a route type in its owner's `-api` (`RequestRoute`), registered by the owner's `install`; a plugin's kinds at run time |
| something one feature asks another for while running (a request from its host, the chat a note goes to) | a trait in the owner's `-api`, implemented in the owner's crate |
| a service the kit's drawing needs (a picture by its address) | a port in the kit, implemented by the feature that has it |
| one feature's views, state and logic | that feature's crate, `pub(crate)` |
| an action, a settings section, a toast, the page for a route | contributed through `shell-api` by the feature that owns it |
| the frame: window, tabs, rail, sidebar, routing | the shell |
| choosing the mode, calling `install`, opening the window | the app |
| a few stable lines two features both need, where sharing would add an edge | written twice |

## Always

- **An implementation never depends on another implementation. A contract depends on no other contract and on
  no implementation.** A type two contracts need goes into core. → [crates](references/crates.md)
- **Structure only where something crosses.** A feature gets an `-api` only when another crate uses it; a
  trait only to cross a crate boundary, to have a second implementation, or to be replaced in a test. Pure
  code is plain functions. → [contracts](references/contracts.md)
- **An implementation shows the app functions only:** `install(cx)`, and what a windowless mode needs (a
  constructor returning `Arc<dyn …>`, the mode's entry). Everything else is `pub(crate)`.
  → [wiring](references/wiring.md)
- **`install` registers and reads nothing**, so the app calls them in any order; a worker takes a clone of
  the `Arc` it needs before it leaves the window's thread. → [wiring](references/wiring.md)
- **One feature reaches another only through the shell:** it opens a route, it raises a toast, it lists
  actions and settings sections it registered — never by naming the other feature. Every module registers its
  own routes, so a plugin adds places the app was built without. A feature's state is its
  own entity; nothing is added to the shell's struct. → [contributions](references/contributions.md)
- **A fake replaces the transport, never the contract**, so the real mapping runs; a consumer's test stubs
  the trait by hand. → [tests](references/tests.md)

## Which rule for which task

| Task | Read |
|---|---|
| a new crate; moving code between crates; a dependency between crates; `tests/crates.rs` fails | [crates](references/crates.md) |
| a trait or a type another crate uses; a port in the kit | [contracts](references/contracts.md) |
| `install`, `main.rs`, the `mcp` and `agent-watch` modes, a background task using a contract | [wiring](references/wiring.md) |
| opening another feature's page, an action, a keymap entry, a settings section, a toast, a new page | [contributions](references/contributions.md) |
| a test across crates, a fake host, a fixture, the sandbox | [tests](references/tests.md) |

## What checks what

- **`tests/crates.rs`**: every crate's dependencies against its role, the exceptions in `ALLOWED`; an
  implementation's `lib.rs` showing anything but functions, the exceptions in `OPEN`. An exception that is no
  longer needed fails too, so the lists only shrink.
- **Cargo**: the core has no GPUI, so it cannot draw; `pub(crate)` keeps an implementation's insides from the
  app; a crate cannot use what it does not depend on.
- **Review**: `install` reads nothing; a contract holds no logic; places, actions and toasts go through the
  shell.

## Today (#165, migrating)

The target above is reached stage by stage; until then:

- `src/ui/` is still the app's: the shell and every feature's views, as `impl Workspace` blocks in 68 files.
  Stage 4 moves them out, the shell first, then one feature at a time.
- Stage 3 (under way): `artoo-hosts-api` holds the hosts' contract and agents reads requests through it
  (`ALLOWED` is empty), and so do the app's views (`Workspace::host`, `ui::host_of`). Inside hosts, `mr.rs`
  still hands a GitHub remote to `github.rs`; the connect panel still calls `connect::` for the token's ways
  until it moves with hosts' views. The kit's `Pictures` port and markdown's move come next.
- `Page` (`src/ui/nav.rs`) is a closed enum the route registry replaces; `menu::Act` and `toast` know every
  feature until `shell-api`'s registries. Meanwhile a new page, action or toast is added there in the shape the
  registries will take: one variant, one id, one owner.
- Every implementation's surface is wide (`OPEN`) until its views move with it.
