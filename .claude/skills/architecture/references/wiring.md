# Wiring

**Decided:** 2026-10-29 · **Applies to:** every implementation crate, `src/main.rs`

## Rule

1. **The app is the composition root.** `src/main.rs` reads the mode and the arguments, calls every
   implementation's `install(cx)`, and opens the window. It holds no view and no feature's logic.
2. **An implementation shows the app functions only.** Its `lib.rs` makes public `pub fn install(cx: &mut App)`
   — which provides its contracts' handles and registers its contributions
   ([contributions](contributions.md)) — and, when a windowless mode needs it, a constructor named for what it
   returns (`pub fn hosts() -> Arc<dyn Hosts>`) or the mode's entry (`pub fn mcp(…)`). Every module, type and
   other function is `pub(crate)`.

   This is the Rust form of a `wiring` module. Where an app must not see `impl`, a third module binds the
   implementation into the graph; here the compiler keeps a `pub(crate)` item from the app, so the binding is
   one function and there is no third crate.
3. **`install` registers, it reads nothing.** It never calls another contract's `get`; that happens when a
   view or a worker needs it. So the app calls the installs in any order, and adding one moves no other.
4. **A windowless mode is handed what it needs.** `artoo mcp` and `artoo agent-watch` run without a GPUI
   app, so there is no handle to read: main builds the contracts with the constructors and passes them in.
5. **A worker takes a clone.** `let hosts = hosts::get(cx);` on the window's thread, then the `Arc` moves into
   the background task; a worker never reaches for a handle itself.
6. **A contract's implementation is made by `install` or main, never by a `static`, a `OnceLock` or a lazy
   singleton inside a feature.** A cache inside an implementation is the implementation's own business.

## Why

- One place says what the app is made of; a feature added is one line there and nothing anywhere else.
- An install that reads another's handle makes the order of installs a hidden contract, broken by the next one
  added in the wrong place.
- A handle is per GPUI app, and GPUI's tests make an app per test; a `static` is one per process, shared by
  every test running beside it, so a test's fake would leak into the next.

## Example

```rust
// src/main.rs
fn main() {
    applog::catch_panics();
    match std::env::args().nth(1).as_deref() {
        Some("mcp") => return artoo_agents::mcp(artoo_hosts::hosts()),
        Some("agent-watch") => return artoo_agents::watch(),
        _ => {}
    }
    artoo_shell::run(path, |cx| {
        artoo_hosts::install(cx);
        artoo_agents::install(cx);
        artoo_requests::install(cx);
        artoo_updates::install(cx);
    });
}
```

```rust
// crates/hosts/src/lib.rs
mod github;
mod gitlab;
mod hosts;
mod pictures;

use artoo_hosts_api::Hosts;
use std::sync::Arc;

pub fn install(cx: &mut gpui::App) {
    for (order, kind) in hosts::kinds() {
        artoo_hosts_api::add_kind(cx, order, kind);
    }
    artoo_ui_kit::pictures::provide(cx, Arc::new(pictures::PicturesImpl::new()));
}

pub fn hosts() -> Arc<dyn Hosts> {
    Arc::new(artoo_hosts_api::Kinds::new(hosts::kinds().into_iter().map(|(_, k)| k).collect()))
}
```

## Checked by

`tests/crates.rs`: `an_implementation_shows_only_functions` reads each implementation's `lib.rs`;
`OPEN` lists the crates not there yet, and `an_open_surface_goes_once_it_is_narrow` fails on one that is.
Review for rules 3 to 6.
