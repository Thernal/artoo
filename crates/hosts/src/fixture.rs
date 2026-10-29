//! The fixture that stands in for both hosts — `ARTOO_MR_FIXTURE=<dir>` for the UI's checks, a test's
//! `TEST_FIXTURE` — and the trail it keeps of what would have been sent.

use anyhow::Result;

/// `ARTOO_MR_FIXTURE=<dir>` reads `mrs.json` and `discussions-<iid>.json` from a directory
/// instead of the network (for UI checks); a test sets its own (`TEST_FIXTURE`).
pub(crate) fn fixture() -> Option<std::path::PathBuf> {
    #[cfg(any(test, feature = "sandbox"))]
    if let Some(dir) = TEST_FIXTURE.with(|f| f.borrow().clone()) {
        return Some(dir);
    }
    std::env::var_os("ARTOO_MR_FIXTURE").map(Into::into)
}

#[cfg(any(test, feature = "sandbox"))]
thread_local! {
    /// A test's fixture, on its thread only: tests run side by side in one process.
    pub static TEST_FIXTURE: std::cell::RefCell<Option<std::path::PathBuf>> =
        const { std::cell::RefCell::new(None) };
}

/// A line of the audit trail that UI checks read back: what the app would have sent. A fixture
/// with `slow.txt` answers after that many milliseconds, and one with `refuse.txt` refuses with its
/// text, as a host can (#139).
pub(crate) fn record(line: String) -> Result<bool> {
    let Some(dir) = fixture() else {
        return Ok(false);
    };
    if let Some(ms) = std::fs::read_to_string(dir.join("slow.txt"))
        .ok()
        .and_then(|t| t.trim().parse().ok())
    {
        std::thread::sleep(std::time::Duration::from_millis(ms));
    }
    if let Ok(why) = std::fs::read_to_string(dir.join("refuse.txt")) {
        anyhow::bail!("{}", why.trim());
    }
    // Appended in one write: changes sent at once each get their line.
    use std::io::Write;
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("posted.txt"))?
        .write_all(format!("{line}\n").as_bytes())?;
    Ok(true)
}
