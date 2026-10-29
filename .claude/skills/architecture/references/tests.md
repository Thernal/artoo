# Fakes and tests across crates

**Decided:** 2026-10-29 · **Applies to:** every test that crosses a crate

## Rule

1. **A fake replaces the transport, never the contract.** A host's tests run its real mapping: the fixtures
   (`fixture::TEST_FIXTURE`, `connect::TEST_FIXTURE`) stand in for curl's answers, so a fake that returned finished
   `Request`s would test nothing of what breaks.
2. **A consumer's test stubs the contract by hand.** A test in agents that needs a host writes a small
   `struct Stub; impl Host for Stub` in its test module — no mocking crate — and provides it like the real one
   (`hosts::provide(cx, Arc::new(Stub))`) or passes it to the windowless mode.
3. **A test guard is `platform::sandboxed()`**, as `AGENTS.md` says: `cfg!(test)` is false in a crate another
   crate's tests call. The callers' dev-dependencies turn on its `sandbox` feature; what another crate's tests
   need is `#[cfg(any(test, feature = "sandbox"))]`.
4. **A test never reaches a live host or the real data folder**, and a merge is checked with fixtures only.

## Why

- The mapping from a host's answer to Artoo's types is where hosts break (a field renamed, a state added); a
  fake above it hides exactly that.
- A stub in the test says what the test assumes of the contract, in the test; a mocking crate would say it in
  a language of its own.

## Example

```rust
#[cfg(test)]
mod tests {
    struct Stub;
    impl artoo_hosts_api::Host for Stub {
        fn request(&self, _: u64) -> anyhow::Result<Request> {
            Ok(Request { number: 398, title: "Fix the login".into(), ..Default::default() })
        }
        fn post_pending(&self, _: u64, _: &PendingNote) -> anyhow::Result<()> {
            Ok(())
        }
    }
}
```

## Checked by

`the_cores_test_guard_holds_in_the_apps_tests` (the app's tests cannot write the data folder); review.
