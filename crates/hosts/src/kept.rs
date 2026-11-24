//! What a host says about a project that hardly changes — who the token is, the members, the labels, the milestones,
//! the project itself — kept per host for 15 minutes (audit step 6), so the list, the page, the create form and the
//! notices read each once instead of each their own. Only an answer that came in is kept; a fixture is not.

use anyhow::Result;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How long an answer is served without asking again.
pub const TTL: Duration = Duration::from_secs(15 * 60);

type Key = (String, &'static str);

/// Answers by `(project base, what)`, with when each came in.
#[derive(Default)]
pub struct Kept(HashMap<Key, (Instant, Value)>);

impl Kept {
    fn fresh(&self, key: &Key, now: Instant) -> Option<Value> {
        self.0
            .get(key)
            .filter(|(at, _)| now.saturating_duration_since(*at) < TTL)
            .map(|(_, v)| v.clone())
    }

    fn put(&mut self, key: Key, now: Instant, value: Value) {
        self.0.insert(key, (now, value));
    }
}

static KEPT: Mutex<Option<Kept>> = Mutex::new(None);

fn with<R>(f: impl FnOnce(&mut Kept) -> R) -> R {
    let mut guard = KEPT.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(Kept::default))
}

/// `what` of the project at `base`: the kept answer while it is under 15 minutes old, else what `read` brings (kept
/// when it comes in, never when it fails). The lock is not held while `read` runs.
pub fn of(base: &str, what: &'static str, read: impl FnOnce() -> Result<Value>) -> Result<Value> {
    let key = (base.to_owned(), what);
    if let Some(value) = with(|k| k.fresh(&key, Instant::now())) {
        return Ok(value);
    }
    let value = read()?;
    with(|k| k.put(key, Instant::now(), value.clone()));
    Ok(value)
}

/// Drops everything kept: a token was saved or forgotten, so what it saw is no longer known to hold.
pub fn forget() {
    with(|k| k.0.clear());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_answer_is_kept_for_fifteen_minutes_only() {
        let mut k = Kept::default();
        let t0 = Instant::now();
        let key = ("https://h/p".to_owned(), "members");
        k.put(key.clone(), t0, Value::from(1));
        assert_eq!(
            k.fresh(&key, t0 + Duration::from_secs(899)),
            Some(Value::from(1))
        );
        assert_eq!(k.fresh(&key, t0 + TTL), None);
    }

    #[test]
    fn a_failed_read_is_not_kept_and_a_second_read_is_served() {
        forget();
        let base = "https://kept.test/a";
        assert!(of(base, "x", || Err(anyhow::anyhow!("down"))).is_err());
        let mut asked = 0;
        for _ in 0..3 {
            let v = of(base, "x", || {
                asked += 1;
                Ok(Value::from(7))
            })
            .unwrap();
            assert_eq!(v, Value::from(7));
        }
        assert_eq!(asked, 1);
        // Another project does not share it.
        assert_eq!(
            of("https://kept.test/b", "x", || Ok(Value::from(8))).unwrap(),
            Value::from(8)
        );
    }
}
