# Crates

**Decided:** 2026-10-29 · **Applies to:** every crate in the workspace

## Rule

1. **A crate's role is read from its name.** `artoo-core` and `artoo-ui-kit` are shared; a name ending in
   `-api` is a feature's contract; every other crate under `crates/` is a feature's implementation; the root
   package is the app.
2. **Names are flat:** `crates/<x>` is `artoo-<x>`, `crates/<x>-api` is `artoo-<x>-api`, so
   `members = ["crates/*"]` takes them all. Code imports a contract under its feature's name when the package
   name only adds noise: `use artoo_hosts_api as hosts;`.
3. **Direction:**
   - the core depends on nothing of Artoo's, and has no GPUI;
   - the kit depends on the core alone;
   - a contract depends on the core and the kit (and GPUI) only — never on another contract or an implementation.
     The kit is there for the shell's contract: a part draws with it, and the selection a frame hands a part is
     the kit's markdown's (2026-10-30, #165 stage 4);
   - an implementation depends on the core, the kit and any contract, its own and other features'. **Never on
     another implementation;**
   - the app depends on everything; it is the only crate that names an implementation.

   Dev-dependencies are outside the rule: a test may take an implementation (hosts' fixtures through
   `sandbox`, [tests](tests.md)).
4. **A feature with nothing to publish has no contract.** Updates and Report a problem are implementations the
   app installs; an `-api` is made the day another crate needs one of them.
5. **A crate is a unit of change.** Split along what changes together, not along what is large. Two crates that
   only ever change together are one; a crate holding two things that change apart is two.
6. **An exception is listed**, in `tests/crates.rs`'s `ALLOWED`, with the stage that removes it.

## Why

- Forbidding feature-to-feature dependencies is the rule that makes modules work; the rest follows from it. It is
  checkable from the manifests alone, so a test checks it and review does not have to.
- A contract that depends on another contract turns contracts into a graph of their own; an implementation
  that sees any contract costs nothing and keeps features independent of each other's insides. The type both
  contracts need belongs to neither, so it goes below both, into the core.
- Cargo rebuilds a crate's dependents when it changes: a change inside one implementation rebuilds it and the
  app, never another feature.

## Example

```toml
# crates/agents/Cargo.toml — the MCP server reads requests through hosts' contract
[dependencies]
artoo-core.workspace = true
artoo-ui-kit.workspace = true
artoo-hosts-api.workspace = true   # Host, Request — never artoo-hosts
```

## Checked by

`tests/crates.rs`: `a_crate_depends_only_on_what_its_role_allows` reads every manifest under `crates/`;
`an_allowed_edge_goes_once_it_is_gone` fails on an exception no longer needed.
