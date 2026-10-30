# Contracts

**Decided:** 2026-10-29 · **Applies to:** every `-api` crate, every trait that crosses a crate

## Rule

1. **An `-api` crate holds a contract and nothing else:** traits, the plain types they take and return, the
   routes others may open ([contributions](contributions.md)), and the handle (`provide` / `get`). No logic past a constructor, a `Display` or a `matches`.
2. **A trait exists only to cross a crate boundary, to have a second implementation (GitLab and GitHub), or to
   be replaced in a test.** Pure code that two crates need — the diff's rows, a parser — is plain functions in
   the core, not a trait.
3. **A trait is `Send + Sync` and object-safe, used as `Arc<dyn Trait>`.** Its methods block (they run curl and
   git, as today) and are called off the window's thread. A contract of what lives in one tab (a space, its
   review: `artoo-code-api`) is the exception: used as `Rc<dyn Trait>` on the window's thread, its methods take
   `cx` and return at once.
4. **Split a trait by who calls it.** The MCP server reads a request and writes a pending comment; it does not
   merge. A consumer depends on what it calls, so a trait that only one consumer uses half of is two.
5. **Names:** a trait is named for what it offers (`Hosts`, `Host`); an implementation for what it is
   (`GitLab`, `GitHub`); the only implementation of a trait is `<Trait>Impl`, never `Default…`.
6. **One handle per contract:** a private GPUI global with `provide(cx, …)`, set by the implementation's
   `install`, and `get(cx)`, read by anyone. Six lines, repeated in each contract rather than shared — a shared
   helper would be a contract every contract depends on. A tab's contract has one handle per tab and no global:
   the feature offers it in the tab's `Services` as its parts are made (`Frame::services`), and the others read
   it there when they use it.
7. **A shared crate that needs a service declares the port itself.** The kit's markdown draws pictures by their
   address, so the kit holds `Pictures` and its handle and hosts implements it; the kit stays on the core
   alone and nothing it draws knows a host.
8. **A contract's `lib.rs` opens with a `//!`** that says what it offers, task by task. Its items' docs say
   what each promises — what it returns when the host does not answer, whether it reads a cache.

## Why

- A contract carrying logic carries that logic's dependencies to every consumer, and its changes rebuild them
  all; a contract that is only shapes changes rarely.
- `Arc<dyn …>` because one instance serves every window and every worker; a generic parameter would carry the
  implementation's type into every consumer's signatures, which is the dependency the contract exists to cut.
- A port declared by the crate that needs it keeps the dependency pointing at the shared crate; the other
  way, the kit would depend on hosts and stop being a leaf.

## Example

```rust
//! GitLab and GitHub behind one contract: the host a repository's remote is on, its merge requests (or
//! pull requests), their discussions, and the pending comments written to them.

use artoo_core::git::WebRemote;
use gpui::{App, Global};
use std::sync::Arc;

pub trait Hosts: Send + Sync {
    /// The host `remote` is on: GitHub's for github.com, GitLab's for any other.
    fn of(&self, remote: &WebRemote) -> Arc<dyn Host>;
}

/// One remote's host, every side of it; a function that needs one side takes it alone (`&dyn Discussions`).
pub trait Host: Account + Requests + Discussions + Reviews + Merging {
    fn remote(&self) -> &WebRemote;
}

/// A request's discussions: their notes and reactions, the pending comments.
pub trait Discussions: Send + Sync {
    fn threads(&self, iid: u64) -> anyhow::Result<Vec<Thread>>;
    /// Sends pending comment `id` alone; an error where the host sends one only with its review (GitHub).
    fn publish_draft(&self, iid: u64, id: u64) -> anyhow::Result<()>;
}

struct Installed(Arc<dyn Hosts>);
impl Global for Installed {}

pub fn provide(cx: &mut App, hosts: Arc<dyn Hosts>) {
    cx.set_global(Installed(hosts));
}

pub fn get(cx: &App) -> Arc<dyn Hosts> {
    cx.global::<Installed>().0.clone()
}
```

## Checked by

Cargo for the direction (a contract cannot reach what it does not depend on, and `tests/crates.rs` keeps it
from depending on one); review for rules 1, 2 and 4.
