//! The files the notices keep between runs — what was seen (`seen-notes.txt`, `seen-resolved.txt`), what was read
//! (`seen-read.txt`) and what was told to the system (`seen-told.txt`) — and the only place that touches them
//! (audit step 11). Nothing here runs on the window thread: a read of them is a part of the notices read's job, and
//! a write is handed to one background writer that waits 500 ms, takes the newest of each file and writes it, so
//! marking ten notices read is one write, and a slow disk never holds a frame.

use crate::notices::{read_of, with_read};
use artoo_core::storage;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::Duration;

const NOTES_FILE: &str = "seen-notes.txt";
const RESOLVED_FILE: &str = "seen-resolved.txt";
const READ_FILE: &str = "seen-read.txt";
const TOLD_FILE: &str = "seen-told.txt";

/// How long a write waits for a newer one of the same file.
#[cfg(not(test))]
const DELAY: Duration = Duration::from_millis(500);
#[cfg(test)]
const DELAY: Duration = Duration::from_millis(30);

type Job = Box<dyn FnOnce() + Send>;

/// The writes waiting, by file (and project), and whether the writer is running.
struct Queue {
    jobs: Vec<(String, Job)>,
    running: bool,
}

static QUEUE: Mutex<Queue> = Mutex::new(Queue {
    jobs: Vec::new(),
    running: false,
});

/// `job` after `DELAY`, in the background; a newer job under the same `key` before then replaces it, since each one
/// writes the whole file. Jobs run one after another, so two tabs' read-modify-write of one file never interleave.
fn later(key: String, job: impl FnOnce() + Send + 'static) {
    let mut queue = QUEUE.lock().unwrap_or_else(|e| e.into_inner());
    queue.jobs.retain(|(k, _)| *k != key);
    queue.jobs.push((key, Box::new(job)));
    if queue.running {
        return;
    }
    queue.running = true;
    drop(queue);
    std::thread::spawn(|| {
        loop {
            std::thread::sleep(DELAY);
            let jobs = {
                let mut queue = QUEUE.lock().unwrap_or_else(|e| e.into_inner());
                if queue.jobs.is_empty() {
                    queue.running = false;
                    return;
                }
                std::mem::take(&mut queue.jobs)
            };
            for (_, job) in jobs {
                job();
            }
        }
    });
}

/// What the disk holds of what a read needs.
#[derive(Default)]
pub struct Loaded {
    /// The newest comment known of each request, and the resolved discussions known.
    pub seen: Option<(HashMap<String, u64>, HashSet<String>)>,
    /// A project's read notices and told ones.
    pub base: Option<(HashSet<String>, HashSet<String>)>,
}

/// Which of the files a read takes in: the seen ones once, a project's read and told ones once per project.
#[derive(Clone, Copy, Default)]
pub struct Wants {
    pub seen: bool,
    pub base: bool,
}

/// Reads what `wants` names; in the read job, never on the window thread.
pub fn load(base: &str, wants: Wants) -> Loaded {
    Loaded {
        seen: wants.seen.then(|| {
            let seen = storage::read(NOTES_FILE)
                .lines()
                .filter_map(|l| {
                    let (key, id) = l.split_once('\t')?;
                    Some((key.to_owned(), id.parse().ok()?))
                })
                .collect();
            let known = storage::read(RESOLVED_FILE)
                .lines()
                .filter(|l| !l.is_empty())
                .map(str::to_owned)
                .collect();
            (seen, known)
        }),
        base: wants.base.then(|| {
            (
                read_of(&storage::read(READ_FILE), base),
                read_of(&storage::read(TOLD_FILE), base),
            )
        }),
    }
}

/// Writes what was seen.
pub fn save_seen(seen: &HashMap<String, u64>, known: &HashSet<String>) {
    let (seen, known) = (seen.clone(), known.clone());
    later(NOTES_FILE.to_owned(), move || {
        let mut lines: Vec<String> = seen.iter().map(|(k, v)| format!("{k}\t{v}")).collect();
        lines.sort_unstable();
        storage::save(NOTES_FILE, lines.join("\n") + "\n");
        let mut known: Vec<&String> = known.iter().collect();
        known.sort_unstable();
        let known: Vec<&str> = known.into_iter().map(String::as_str).collect();
        storage::save(RESOLVED_FILE, known.join("\n") + "\n");
    });
}

/// Writes project `base`'s read notices; the other projects' lines (other tabs) stay as on disk.
pub fn save_read(base: &str, read: &HashSet<String>) {
    save_lines(READ_FILE, base, read);
}

/// Writes project `base`'s told notices.
pub fn save_told(base: &str, told: &HashSet<String>) {
    save_lines(TOLD_FILE, base, told);
}

fn save_lines(file: &'static str, base: &str, keys: &HashSet<String>) {
    let (base, keys) = (base.to_owned(), keys.clone());
    later(format!("{file}|{base}"), move || {
        storage::save(file, with_read(&storage::read(file), &base, &keys));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn writes_of_one_file_in_a_burst_are_one_write_of_the_newest() {
        let runs = Arc::new(AtomicUsize::new(0));
        let last = Arc::new(AtomicUsize::new(0));
        for n in 1..=5 {
            let (runs, last) = (runs.clone(), last.clone());
            later("burst-test".to_owned(), move || {
                runs.fetch_add(1, Ordering::SeqCst);
                last.store(n, Ordering::SeqCst);
            });
        }
        assert_eq!(runs.load(Ordering::SeqCst), 0, "nothing is written at once");
        std::thread::sleep(DELAY * 6);
        assert_eq!(runs.load(Ordering::SeqCst), 1);
        assert_eq!(last.load(Ordering::SeqCst), 5);
        // The writer stopped when idle and starts again for the next burst.
        let again = runs.clone();
        later("burst-test".to_owned(), move || {
            again.fetch_add(1, Ordering::SeqCst);
        });
        std::thread::sleep(DELAY * 6);
        assert_eq!(runs.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn files_of_other_projects_do_not_replace_each_other() {
        let runs = Arc::new(AtomicUsize::new(0));
        for base in ["a", "b"] {
            let runs = runs.clone();
            later(format!("{READ_FILE}|{base}-test"), move || {
                runs.fetch_add(1, Ordering::SeqCst);
            });
        }
        std::thread::sleep(DELAY * 6);
        assert_eq!(runs.load(Ordering::SeqCst), 2);
    }
}
