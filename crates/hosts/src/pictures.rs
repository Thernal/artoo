//! Pictures from the web, kept (the developer, 2026-10-29: "remote imagelar cachelənsin"): a
//! person's picture (`crate::avatars`) and an image a comment, a description or a document shows
//! are fetched once, through `curl` as the rest, into a shelf of the app's folder (named by the
//! address), and read from there from then on — after a restart too. An upload never changes
//! (GitLab's `/uploads/`, GitHub's attachments) and is kept as it is; another image (a badge) is
//! drawn as kept and asked again once a day. A GitLab upload written in a comment of the project
//! is asked through the API with the host's token (`crate::gitlab::upload`): a private project's
//! uploads need sign-in, and the token goes to that project's host only, never to an address a
//! comment names. One that could not be had is not asked for again for a week (`<name>.none`), and
//! after any failed fetch not again for a minute in the run; at most four are fetched at once;
//! images not written for 60 days are swept.

use crate::git::WebRemote;
use artoo_core::http;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex, Once};
use std::time::{Duration, Instant};

/// Where a picture is kept: people's pictures apart from the images pages show.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Shelf {
    Avatars,
    Images,
}

impl Shelf {
    fn dir(self) -> Option<PathBuf> {
        let name = match self {
            Self::Avatars => "avatars",
            Self::Images => "images",
        };
        Some(crate::platform::data_dir()?.join(name))
    }
}

pub use artoo_ui_kit::pictures::Picture;

/// The kit's `Pictures` port: what markdown draws by its address.
pub(crate) struct WebPictures;

impl artoo_ui_kit::pictures::Pictures for WebPictures {
    fn image(&self, url: &str) -> Option<Picture> {
        image(url)
    }

    fn upload(&self, project: &str, src: &str) -> Option<Picture> {
        upload(project, src)
    }

    fn take_fresh(&self) -> bool {
        FRESH.swap(false, Ordering::Relaxed)
    }
}

/// Each address's file once it is in; None while it is being fetched or could not be.
static FILES: LazyLock<Mutex<HashMap<String, Option<PathBuf>>>> = LazyLock::new(Default::default);
/// The pictures read, by file: read once, drawn as often as wanted; the least recently drawn go
/// past `DECODED`.
static IMAGES: LazyLock<Mutex<Lru<PathBuf, Option<Picture>>>> =
    LazyLock::new(|| Mutex::new(Lru::new(DECODED)));
/// Fetches waiting for one of the `WORKERS`, and how many of those run.
static QUEUE: LazyLock<Mutex<(VecDeque<Job>, usize)>> = LazyLock::new(Default::default);
/// Addresses whose fetch failed, and when they may be asked for again.
static BACKOFF: LazyLock<Mutex<HashMap<String, Instant>>> = LazyLock::new(Default::default);
/// A picture came in since the window last looked.
static FRESH: AtomicBool = AtomicBool::new(false);
static SWEPT: Once = Once::new();

/// Fetches that run at once.
const WORKERS: usize = 4;
/// Pictures kept decoded in memory.
const DECODED: usize = 128;
/// How long after a failed fetch (the host's no, a host not reached) the address is not asked again
/// in this run.
const BACKOFF_FOR: Duration = Duration::from_secs(60);
/// How long a picture that could not be had is not asked for again.
const RETRY: Duration = Duration::from_secs(7 * 24 * 3600);
/// How long an image that may change (not an upload) is drawn before it is asked for again.
const REFRESH: Duration = Duration::from_secs(24 * 3600);
/// How long an image is kept without being written again.
const KEEP: Duration = Duration::from_secs(60 * 24 * 3600);

/// The image at `url` (a web address) once it is in; the first ask fetches it.
pub fn image(url: &str) -> Option<Picture> {
    if !is_web(url) {
        return None;
    }
    want(url, Shelf::Images, None);
    get(url)
}

/// An upload of `project` (GitLab's `https://host/group/app`) as a comment there writes it,
/// `/uploads/<secret>/<file>`, once it is in; the first ask fetches it with the host's token.
pub fn upload(project: &str, src: &str) -> Option<Picture> {
    let url = format!("{project}{src}");
    let Some(path) = upload_path(src) else {
        return image(&url);
    };
    want(
        &url,
        Shelf::Images,
        Some((project.to_owned(), path.to_owned())),
    );
    get(&url)
}

/// The `<secret>/<file>` of a GitLab upload as a comment writes it: the API is asked for that
/// upload and nothing else.
fn upload_path(src: &str) -> Option<&str> {
    let path = src.strip_prefix("/uploads/")?;
    let (secret, file) = path.split_once('/')?;
    let plain = !secret.is_empty()
        && secret.chars().all(|c| c.is_ascii_hexdigit())
        && !file.is_empty()
        && !file.contains("..")
        && !file.contains(['?', '#']);
    plain.then_some(path)
}

/// Starts fetching `url` onto `shelf` unless it is known; a GitLab upload (`project`, the path after
/// `/uploads/`) is asked through the API first.
pub fn want(url: &str, shelf: Shelf, upload: Option<(String, String)>) {
    // A test fetches nothing: no folder made, no network asked.
    if crate::platform::sandboxed() {
        return;
    }
    let mut files = FILES.lock().unwrap_or_else(|e| e.into_inner());
    if files.contains_key(url) {
        return;
    }
    {
        let mut backoff = BACKOFF.lock().unwrap_or_else(|e| e.into_inner());
        match backoff.get(url) {
            Some(until) if Instant::now() < *until => return,
            Some(_) => {
                backoff.remove(url);
            }
            None => {}
        }
    }
    files.insert(url.to_owned(), None);
    let Some(dir) = shelf.dir() else {
        return;
    };
    if shelf == Shelf::Images {
        let dir = dir.clone();
        SWEPT.call_once(|| {
            std::thread::spawn(move || sweep(&dir));
        });
    }
    let stem = dir.join(sha1_smol::Sha1::from(url).digest().to_string());
    // Fetched before: it is there. One that may change is asked again once a day, and one about to
    // be swept, the kept one drawn meanwhile.
    if let Some(file) = kept(&stem) {
        let changes = shelf == Shelf::Images && !is_upload(url);
        let stale = age(&file) > Some(if changes { REFRESH } else { KEEP });
        files.insert(url.to_owned(), Some(file));
        if stale {
            spawn(url, stem, upload);
        }
        return;
    }
    // Not to be had a short while ago: not asked for again yet.
    if age(&stem.with_extension("none")).is_some_and(|a| a < RETRY) {
        return;
    }
    spawn(url, stem, upload);
}

/// One fetch waiting for a worker.
struct Job {
    url: String,
    stem: PathBuf,
    upload: Option<(String, String)>,
}

/// Queues the fetch; at most `WORKERS` threads run them, each taking the next until none is left.
fn spawn(url: &str, stem: PathBuf, upload: Option<(String, String)>) {
    let job = Job {
        url: url.to_owned(),
        stem,
        upload,
    };
    let start = {
        let mut queue = QUEUE.lock().unwrap_or_else(|e| e.into_inner());
        queue.0.push_back(job);
        let start = queue.1 < WORKERS;
        if start {
            queue.1 += 1;
        }
        start
    };
    if start {
        std::thread::spawn(work);
    }
}

fn work() {
    loop {
        let job = {
            let mut queue = QUEUE.lock().unwrap_or_else(|e| e.into_inner());
            match queue.0.pop_front() {
                Some(job) => job,
                None => {
                    queue.1 -= 1;
                    return;
                }
            }
        };
        run(job);
    }
}

fn run(Job { url, stem, upload }: Job) {
    match fetch(&url, &stem, upload) {
        Some(file) => {
            IMAGES
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&file);
            FILES
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(url, Some(file));
            FRESH.store(true, Ordering::Relaxed);
        }
        None => {
            // Not asked for again for a minute; then the disk and the host are looked at again.
            BACKOFF
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(url.clone(), Instant::now() + BACKOFF_FOR);
            FILES.lock().unwrap_or_else(|e| e.into_inner()).remove(&url);
        }
    }
}

/// The picture at `url`, once it is in.
pub fn get(url: &str) -> Option<Picture> {
    let file = FILES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(url)
        .cloned()
        .flatten()?;
    let mut images = IMAGES.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(picture) = images.get(&file) {
        return picture.clone();
    }
    let picture = read(&file);
    images.put(file, picture.clone());
    picture
}

/// A map that keeps the `cap` most recently used entries.
struct Lru<K, V> {
    cap: usize,
    map: HashMap<K, V>,
    order: VecDeque<K>,
}

impl<K: Clone + Eq + std::hash::Hash, V> Lru<K, V> {
    fn new(cap: usize) -> Self {
        Self {
            cap,
            map: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    fn touch(&mut self, key: &K) {
        if let Some(at) = self.order.iter().position(|k| k == key) {
            let k = self.order.remove(at).expect("position is in range");
            self.order.push_back(k);
        }
    }

    fn get(&mut self, key: &K) -> Option<&V> {
        if self.map.contains_key(key) {
            self.touch(key);
        }
        self.map.get(key)
    }

    fn put(&mut self, key: K, value: V) {
        if self.map.insert(key.clone(), value).is_some() {
            self.touch(&key);
            return;
        }
        self.order.push_back(key);
        while self.order.len() > self.cap {
            if let Some(old) = self.order.pop_front() {
                self.map.remove(&old);
            }
        }
    }

    fn remove(&mut self, key: &K) {
        if self.map.remove(key).is_some() {
            self.order.retain(|k| k != key);
        }
    }
}

fn is_web(url: &str) -> bool {
    url.starts_with("https://") || url.starts_with("http://")
}

/// An address whose image never changes: a GitLab or GitHub upload.
fn is_upload(url: &str) -> bool {
    url.contains("/uploads/")
        || url.contains("/user-attachments/")
        || url.contains("user-images.githubusercontent.com/")
}

/// How long ago `file` was written; None when it is not there.
fn age(file: &Path) -> Option<Duration> {
    std::fs::metadata(file)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
}

/// The kinds a picture is kept as, by what its bytes start with: the extension tells the image
/// reader how to read it.
const KINDS: [&str; 5] = ["png", "jpg", "gif", "webp", "svg"];

fn kind_of(head: &[u8]) -> Option<&'static str> {
    Some(match head {
        [0x89, b'P', b'N', b'G', ..] => "png",
        [0xff, 0xd8, ..] => "jpg",
        [b'G', b'I', b'F', ..] => "gif",
        [
            b'R',
            b'I',
            b'F',
            b'F',
            _,
            _,
            _,
            _,
            b'W',
            b'E',
            b'B',
            b'P',
            ..,
        ] => "webp",
        _ if String::from_utf8_lossy(head).contains("<svg") => "svg",
        _ => return None,
    })
}

/// The file kept beside `stem`, of whichever kind.
fn kept(stem: &Path) -> Option<PathBuf> {
    KINDS
        .iter()
        .map(|k| stem.with_extension(k))
        .find(|f| f.exists())
}

/// Fetches `url` beside `stem`, then names it by its kind, so a half-written one is never shown.
fn fetch(url: &str, stem: &Path, upload: Option<(String, String)>) -> Option<PathBuf> {
    std::fs::create_dir_all(stem.parent()?).ok()?;
    let part = stem.with_extension("part");
    let none = stem.with_extension("none");
    // Through the API first; a GitLab without the call (before 17.4) or a token it refused: the
    // address itself, as for a public project.
    let by_api = upload
        .and_then(|(project, path)| Some((WebRemote::parse(&project).filter(|r| !r.github)?, path)))
        .is_some_and(|(remote, path)| crate::gitlab::upload(&remote, &path, &part));
    if !by_api {
        let request = http::Request::get(url)
            .output(part.clone())
            .follow()
            .max_filesize(25_000_000)
            .timeout(20);
        // The host said no (404 for no Gravatar, 401 behind sign-in): not again for a while. A host
        // not reached (the VPN off) is asked again next time.
        match http::send(&request) {
            Ok(answer) if answer.ok() => {}
            Ok(_) => {
                let _ = std::fs::remove_file(&part);
                let _ = std::fs::write(&none, "");
                return None;
            }
            Err(_) => {
                let _ = std::fs::remove_file(&part);
                return None;
            }
        }
    }
    let head = std::fs::read(&part).ok()?;
    let Some(kind) = kind_of(&head[..head.len().min(512)]) else {
        let _ = std::fs::remove_file(&part);
        let _ = std::fs::write(&none, "");
        return None;
    };
    // Asked again, it may come back as another kind: the one kept before goes.
    if let Some(old) = kept(stem) {
        let _ = std::fs::remove_file(old);
    }
    let file = stem.with_extension(kind);
    std::fs::rename(&part, &file).ok()?;
    Some(file)
}

/// The picture in `file`, with its size.
fn read(file: &Path) -> Option<Picture> {
    use gpui::ImageFormat;
    let format = match file.extension()?.to_str()? {
        "png" => ImageFormat::Png,
        "jpg" => ImageFormat::Jpeg,
        "gif" => ImageFormat::Gif,
        "webp" => ImageFormat::Webp,
        "svg" => ImageFormat::Svg,
        _ => return None,
    };
    let bytes = std::fs::read(file).ok()?;
    let size = match format {
        ImageFormat::Svg => crate::git::svg_size(&String::from_utf8_lossy(&bytes)),
        _ => imagesize::blob_size(&bytes)
            .ok()
            .map(|s| (s.width as u32, s.height as u32)),
    };
    Some((Arc::new(gpui::Image::from_bytes(format, bytes)), size))
}

/// Removes what `dir` has kept without writing it again for `KEEP`.
fn sweep(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if age(&path).is_some_and(|a| a > KEEP) {
            let _ = std::fs::remove_file(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uploads_are_kept_and_other_images_asked_again() {
        assert!(is_upload(
            "https://gitlab.x.io/group/app/uploads/0123abcd/shot.png"
        ));
        assert!(is_upload(
            "https://github.com/user-attachments/assets/1f2e-3d4c"
        ));
        assert!(!is_upload(
            "https://img.shields.io/badge/build-passing-green"
        ));
        assert!(image("docs/logo.png").is_none());
    }

    #[test]
    fn the_least_recently_drawn_go_first() {
        let mut lru = Lru::new(2);
        lru.put("a", 1);
        lru.put("b", 2);
        assert_eq!(lru.get(&"a"), Some(&1));
        lru.put("c", 3);
        assert_eq!(lru.get(&"b"), None);
        assert_eq!(lru.get(&"a"), Some(&1));
        lru.remove(&"a");
        assert_eq!(lru.get(&"a"), None);
        assert_eq!(lru.get(&"c"), Some(&3));
    }

    #[test]
    fn only_an_upload_is_asked_of_the_api() {
        assert_eq!(
            upload_path("/uploads/0a1b2c/shot one.png"),
            Some("0a1b2c/shot one.png")
        );
        assert_eq!(upload_path("/uploads/zz/../../user"), None);
        assert_eq!(upload_path("/uploads/0a1b/../../user"), None);
        assert_eq!(upload_path("/uploads/0a1b/a.png?x=1"), None);
        assert_eq!(upload_path("/elsewhere/a.png"), None);
    }

    #[test]
    fn a_kind_is_read_from_the_bytes() {
        assert_eq!(kind_of(&[0x89, b'P', b'N', b'G', 0, 0]), Some("png"));
        assert_eq!(
            kind_of(b"<?xml version=\"1.0\"?><svg width=\"9\">"),
            Some("svg")
        );
        assert_eq!(kind_of(b"<!doctype html><title>Sign in</title>"), None);
    }
}
