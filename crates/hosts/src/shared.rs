//! One read shared by the actions that ask for it at the same time (audit step 10): a GitHub page open wants
//! `pulls/{n}` for its details, checks and merge state and the review threads for its threads and drafts, and asks
//! for each once. An answer is served for 3 seconds, the actions asking together wait for the one in flight, and any
//! write drops all of it, so a read after a change is a fresh one. Only an answer that came in is kept.

use anyhow::Result;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How long an answer is served without asking again.
const TTL: Duration = Duration::from_secs(3);

type Slot = Arc<Mutex<Option<(Instant, Value)>>>;

static SLOTS: Mutex<Option<HashMap<String, Slot>>> = Mutex::new(None);

fn slot(key: &str) -> Slot {
    let mut guard = SLOTS.lock().unwrap_or_else(|e| e.into_inner());
    guard
        .get_or_insert_with(HashMap::new)
        .entry(key.to_owned())
        .or_default()
        .clone()
}

/// The answer under `key` while it is under 3 seconds old, else what `read` brings. A second caller of the same key
/// waits for the first one's read instead of making its own.
pub fn of(key: &str, read: impl FnOnce() -> Result<Value>) -> Result<Value> {
    let slot = slot(key);
    let mut held = slot.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((at, value)) = held.as_ref()
        && at.elapsed() < TTL
    {
        return Ok(value.clone());
    }
    let value = read()?;
    *held = Some((Instant::now(), value.clone()));
    Ok(value)
}

/// Drops every answer: something was written, so what was read may no longer hold.
pub fn forget() {
    let mut guard = SLOTS.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(map) = guard.as_mut() {
        map.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// `forget` empties the one shared store, so the tests that use it take turns.
    static TURN: Mutex<()> = Mutex::new(());

    #[test]
    fn readers_asking_together_make_one_read_and_a_write_drops_it() {
        let _turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
        forget();
        let asked = AtomicUsize::new(0);
        let read = || {
            asked.fetch_add(1, Ordering::SeqCst);
            Ok(Value::from(1))
        };
        std::thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(|| of("shared.test/a", read).unwrap());
            }
        });
        assert_eq!(asked.load(Ordering::SeqCst), 1);
        forget();
        of("shared.test/a", read).unwrap();
        assert_eq!(asked.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn a_failed_read_is_not_kept() {
        let _turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
        forget();
        assert!(of("shared.test/b", || Err(anyhow::anyhow!("down"))).is_err());
        assert_eq!(of("shared.test/b", || Ok(Value::from(2))).unwrap(), 2);
    }
}
