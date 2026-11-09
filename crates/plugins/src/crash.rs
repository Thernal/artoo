//! A plugin that ends the app (#178): which one it was, known at the next start.
//!
//! Most of what goes wrong in a plugin can't be caught: a panic in its view, a click or a task can't unwind through
//! the system's frames, and a bad write is a signal. So the cause is written down as the app ends: a panic hook and a
//! fatal signal (Windows: an exception) walk the stack, and the first frame in a plugin's code names it, in
//! `last-crash.txt`. Nothing is written when no plugin's code is on the stack: the crash is Artoo's own, and the system
//! reports it as before. None of this is set up in a run with no plugin to load.
//!
//! `plugins-starting.txt` catches what a handler can't — a library's constructors, the process killed while it hangs:
//! the plugin about to install is written down and struck off once it has; one left there ended that start.

use artoo_core::{platform, storage};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};

/// What the app said about the plugin that ended it.
const FILE: &str = "last-crash.txt";
/// The starts ended by a plugin's install, one id a line; the last may be the one installing.
const STARTING: &str = "plugins-starting.txt";
/// The stack as far as it is looked at.
const FRAMES: usize = 48;
/// A report's bytes: its frames fit, a panic's message is cut.
const REPORT: usize = 4_096;

/// A plugin's code in memory.
#[derive(Clone)]
struct Image {
    start: usize,
    end: usize,
    id: String,
    file: String,
}

/// Every plugin loaded so far. A list replaced whole, the old one left: a handler may be reading it, and a run loads
/// a handful once.
static IMAGES: AtomicPtr<Vec<Image>> = AtomicPtr::new(std::ptr::null_mut());
static PATH: OnceLock<Target> = OnceLock::new();
/// One report for one end: a panic's abort comes back as a signal.
static REPORTED: AtomicBool = AtomicBool::new(false);

fn images() -> &'static [Image] {
    let list = IMAGES.load(Ordering::Acquire);
    if list.is_null() {
        return &[];
    }
    // SAFETY: only ever a leaked list (`add_image`), never freed.
    unsafe { &*list }
}

/// `id`'s code, from `start` to `end`: a frame there is that plugin's.
pub(crate) fn add_image(id: &str, file: &str, (start, end): (usize, usize)) {
    let mut list = images().to_vec();
    list.push(Image {
        start,
        end,
        id: id.to_owned(),
        file: file.to_owned(),
    });
    IMAGES.store(Box::into_raw(Box::new(list)), Ordering::Release);
}

fn image_at(images: &[Image], ip: usize) -> Option<&Image> {
    images.iter().find(|i| (i.start..i.end).contains(&ip))
}

/// A fixed buffer, as a signal handler may not allocate.
struct Text {
    bytes: [u8; REPORT],
    len: usize,
}

impl Text {
    fn put(&mut self, more: &[u8]) {
        let n = more.len().min(REPORT - self.len);
        self.bytes[self.len..self.len + n].copy_from_slice(&more[..n]);
        self.len += n;
    }

    fn hex(&mut self, mut value: usize) {
        let mut digits = [0u8; 16];
        let mut at = digits.len();
        loop {
            at -= 1;
            digits[at] = b"0123456789abcdef"[value & 0xf];
            value >>= 4;
            if value == 0 {
                break;
            }
        }
        self.put(&digits[at..]);
    }
}

/// Writes down the plugin whose code is on this stack, if one is: its id, `what` ended it, its frames.
fn report(what: &[u8]) {
    let mut ips = [0usize; FRAMES];
    let mut n = 0;
    // SAFETY: the app is ending and no other thread's walk is waited for.
    unsafe {
        backtrace::trace_unsynchronized(|frame| {
            ips[n] = frame.ip() as usize;
            n += 1;
            n < FRAMES
        });
    }
    let images = images();
    let Some(culprit) = ips[..n].iter().find_map(|ip| image_at(images, *ip)) else {
        return;
    };
    if REPORTED.swap(true, Ordering::SeqCst) {
        return;
    }
    let mut text = Text {
        bytes: [0; REPORT],
        len: 0,
    };
    text.put(culprit.id.as_bytes());
    text.put(b"\n");
    text.put(what);
    text.put(b"\n");
    for ip in &ips[..n] {
        if let Some(image) = image_at(images, *ip) {
            text.put(image.file.as_bytes());
            text.put(b"+0x");
            text.hex(ip - image.start);
            text.put(b"\n");
        }
    }
    os::write(&text.bytes[..text.len]);
}

/// Starts listening, once: the app's end is told as a plugin's when one is on the stack.
pub(crate) fn watch() {
    let Some(path) = platform::data_dir().map(|d| d.join(FILE)) else {
        return;
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if PATH.set(os::target(path)).is_err() {
        return;
    }
    let before = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let said = info.to_string().replace('\n', " ");
        report(format!("panic: {said}").as_bytes());
        before(info);
    }));
    os::listen();
}

/// A panic in a plugin's install was caught, and the app goes on: there is nothing to tell.
pub(crate) fn forget() {
    REPORTED.store(false, Ordering::SeqCst);
    if let Some(path) = platform::data_dir().map(|d| d.join(FILE)) {
        let _ = std::fs::remove_file(path);
    }
}

/// What ended the app last time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Crash {
    pub id: String,
    pub what: String,
    pub frames: Vec<String>,
}

fn parse(text: &str) -> Option<Crash> {
    let mut lines = text.lines();
    let id = lines.next().filter(|id| !id.is_empty())?.to_owned();
    let what = lines.next()?.to_owned();
    Some(Crash {
        id,
        what,
        frames: lines.map(str::to_owned).collect(),
    })
}

/// What ended the app last time, said once: gone from the disk unless this run leaves the user's files alone.
pub(crate) fn take(quiet: bool) -> Option<Crash> {
    // A test reads none of the user's files.
    if platform::sandboxed() {
        return None;
    }
    let crash = parse(&storage::read(FILE));
    if crash.is_some() && !quiet {
        forget();
    }
    crash
}

/// The ids of the plugins that ended two starts as they installed, and what is left of the list without them.
fn twice(lines: Vec<String>) -> (Vec<String>, Vec<String>) {
    let mut out: Vec<String> = Vec::new();
    for line in &lines {
        if !out.contains(line) && lines.iter().filter(|l| *l == line).count() >= 2 {
            out.push(line.clone());
        }
    }
    let left = lines.into_iter().filter(|l| !out.contains(l)).collect();
    (out, left)
}

/// The starts a plugin ended, kept while plugins install.
pub(crate) struct Starting {
    lines: Vec<String>,
}

impl Starting {
    /// As the last starts left it, with the plugins to turn off: those that ended two.
    pub(crate) fn read() -> (Self, Vec<String>) {
        let text = storage::read(STARTING);
        let lines = text.lines().map(str::to_owned).filter(|l| !l.is_empty());
        let (off, lines) = twice(lines.collect());
        let starting = Self { lines };
        if !off.is_empty() {
            starting.save(None);
        }
        (starting, off)
    }

    fn save(&self, also: Option<&str>) {
        let lines = self.lines.iter().map(String::as_str).chain(also);
        let body: String = lines.map(|l| format!("{l}\n")).collect();
        storage::save(STARTING, body);
    }

    /// `id` is about to install.
    pub(crate) fn begin(&self, id: &str) {
        self.save(Some(id));
    }

    /// `id` installed, or failed and the app went on: it has ended no start, this one or those before.
    pub(crate) fn done(&mut self, id: &str) {
        self.lines.retain(|l| l != id);
        self.save(None);
    }
}

/// What Artoo's report of the crash says, as the plugin's repository's new issue.
pub(crate) fn issue_body(crash: &Crash, version: &str) -> String {
    let what = match std::env::home_dir() {
        Some(home) => crash.what.replace(&home.display().to_string(), "~"),
        None => crash.what.clone(),
    };
    format!(
        "{} stopped Artoo.\n\n- Plugin: {} {version}\n- Artoo: {}\n- System: {} {}\n\n```\n{what}\n{}\n```\n",
        crash.id,
        crash.id,
        artoo_core::version(),
        std::env::consts::OS,
        std::env::consts::ARCH,
        crash.frames.join("\n"),
    )
}

/// A plugin repository's new issue, filled in; a repository not on GitHub opens as it is.
pub(crate) fn issue_url(repository: &str, title: &str, body: &str) -> String {
    /// GitHub refuses longer addresses.
    const MAX: usize = 7_500;
    if !repository.contains("github.com/") {
        return repository.to_owned();
    }
    let encode = |text: &str| {
        let mut out = String::new();
        for byte in text.bytes() {
            match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    out.push(byte as char)
                }
                _ => out.push_str(&format!("%{byte:02X}")),
            }
        }
        out
    };
    let mut url = format!(
        "{}/issues/new?title={}&body={}",
        repository.trim_end_matches('/'),
        encode(title),
        encode(body)
    );
    if url.len() > MAX {
        // Not inside a `%XX`.
        let mut cut = MAX;
        while url.as_bytes()[cut - 1] == b'%' || url.as_bytes()[cut - 2] == b'%' {
            cut -= 1;
        }
        url.truncate(cut);
    }
    url
}

type Target = os::Target;

#[cfg(unix)]
mod os {
    use super::{PATH, PathBuf, report};
    use std::ffi::CString;
    use std::mem::MaybeUninit;
    use std::os::unix::ffi::OsStringExt;
    use std::sync::atomic::{AtomicBool, Ordering};

    pub(super) type Target = CString;

    const SIGNALS: [(libc::c_int, &[u8]); 6] = [
        (libc::SIGSEGV, b"signal SIGSEGV"),
        (libc::SIGBUS, b"signal SIGBUS"),
        (libc::SIGILL, b"signal SIGILL"),
        (libc::SIGABRT, b"signal SIGABRT"),
        (libc::SIGFPE, b"signal SIGFPE"),
        (libc::SIGTRAP, b"signal SIGTRAP"),
    ];

    /// What each signal did before: what it goes back to, so the app ends as it would have.
    static mut BEFORE: [MaybeUninit<libc::sigaction>; 6] = [const { MaybeUninit::zeroed() }; 6];
    static IN: AtomicBool = AtomicBool::new(false);

    pub(super) fn target(path: PathBuf) -> Target {
        CString::new(path.into_os_string().into_vec()).unwrap_or_default()
    }

    pub(super) fn write(bytes: &[u8]) {
        let Some(path) = PATH.get() else { return };
        // SAFETY: `open`, `write` and `close` are safe in a signal handler, and the buffers outlive the calls.
        unsafe {
            let flags = libc::O_WRONLY | libc::O_CREAT | libc::O_TRUNC;
            let fd = libc::open(path.as_ptr(), flags, 0o644 as libc::c_uint);
            if fd >= 0 {
                libc::write(fd, bytes.as_ptr().cast(), bytes.len());
                libc::close(fd);
            }
        }
    }

    extern "C" fn on_signal(signal: libc::c_int, _: *mut libc::siginfo_t, _: *mut libc::c_void) {
        let at = SIGNALS.iter().position(|(s, _)| *s == signal);
        if !IN.swap(true, Ordering::SeqCst) {
            // Walking a broken stack may hang: the alarm ends the app all the same.
            // SAFETY: `alarm` is safe in a signal handler.
            unsafe { libc::alarm(3) };
            if let Some(at) = at {
                report(SIGNALS[at].1);
            }
        }
        if let Some(at) = at {
            // SAFETY: `BEFORE[at]` was filled before this handler was set, and is not written again. Back as it
            // was, the signal ends the app: a fault faults again on return, an abort is raised again.
            unsafe {
                let before = (*std::ptr::addr_of!(BEFORE))[at].as_ptr();
                libc::sigaction(signal, before, std::ptr::null_mut());
                if signal == libc::SIGABRT {
                    libc::raise(signal);
                }
            }
        }
    }

    pub(super) fn listen() {
        for (at, (signal, _)) in SIGNALS.iter().enumerate() {
            // SAFETY: an action filled for this handler; `listen` runs once (`watch`), on the window's thread.
            unsafe {
                let mut action: libc::sigaction = std::mem::zeroed();
                action.sa_sigaction = on_signal as *const () as usize;
                // On the alternate stack std sets up: a stack overflow has no room on its own.
                action.sa_flags = libc::SA_SIGINFO | libc::SA_ONSTACK;
                libc::sigemptyset(&mut action.sa_mask);
                let before = (*std::ptr::addr_of_mut!(BEFORE))[at].as_mut_ptr();
                libc::sigaction(*signal, &action, before);
            }
        }
    }
}

#[cfg(windows)]
mod os {
    use super::{PATH, PathBuf, report};
    use windows_sys::Win32::Foundation::{
        EXCEPTION_ACCESS_VIOLATION, EXCEPTION_ILLEGAL_INSTRUCTION, EXCEPTION_INT_DIVIDE_BY_ZERO,
        EXCEPTION_PRIV_INSTRUCTION,
    };
    use windows_sys::Win32::System::Diagnostics::Debug::{
        AddVectoredExceptionHandler, EXCEPTION_POINTERS,
    };

    pub(super) type Target = PathBuf;

    pub(super) fn target(path: PathBuf) -> Target {
        path
    }

    pub(super) fn write(bytes: &[u8]) {
        if let Some(path) = PATH.get() {
            let _ = std::fs::write(path, bytes);
        }
    }

    /// Windows has no abort to catch (`__fastfail` reaches no handler): a panic is told by the panic hook.
    unsafe extern "system" fn on_exception(info: *mut EXCEPTION_POINTERS) -> i32 {
        /// Look further: whoever would have handled it still does.
        const CONTINUE_SEARCH: i32 = 0;
        // SAFETY: the system gives a valid record for the exception being raised.
        let code = unsafe { (*(*info).ExceptionRecord).ExceptionCode };
        let what: Option<&[u8]> = match code {
            EXCEPTION_ACCESS_VIOLATION => Some(b"access violation"),
            EXCEPTION_ILLEGAL_INSTRUCTION | EXCEPTION_PRIV_INSTRUCTION => {
                Some(b"illegal instruction")
            }
            EXCEPTION_INT_DIVIDE_BY_ZERO => Some(b"division by zero"),
            _ => None,
        };
        if let Some(what) = what {
            report(what);
        }
        CONTINUE_SEARCH
    }

    pub(super) fn listen() {
        // SAFETY: the handler lives as long as the process.
        unsafe { AddVectoredExceptionHandler(1, Some(on_exception)) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_report_reads_back() {
        let crash =
            parse("hello\nsignal SIGSEGV\nlibhello.dylib+0x1f0\nlibhello.dylib+0x20\n").unwrap();
        assert_eq!(crash.id, "hello");
        assert_eq!(crash.what, "signal SIGSEGV");
        assert_eq!(crash.frames.len(), 2);
        assert_eq!(parse(""), None);
        assert_eq!(parse("only-an-id"), None);
    }

    #[test]
    fn a_frame_belongs_to_the_image_it_is_in() {
        let image = |id: &str, start, end| Image {
            start,
            end,
            id: id.to_owned(),
            file: format!("lib{id}.dylib"),
        };
        let images = [image("a", 100, 200), image("b", 300, 400)];
        assert_eq!(image_at(&images, 150).map(|i| i.id.as_str()), Some("a"));
        assert_eq!(image_at(&images, 300).map(|i| i.id.as_str()), Some("b"));
        assert!(image_at(&images, 200).is_none() && image_at(&images, 50).is_none());
    }

    #[test]
    fn numbers_are_written_without_allocating_and_a_report_stays_in_its_buffer() {
        let mut text = Text {
            bytes: [0; REPORT],
            len: 0,
        };
        text.hex(0);
        text.put(b" ");
        text.hex(0x1f0a);
        assert_eq!(&text.bytes[..text.len], b"0 1f0a");
        text.put(&[b'x'; REPORT * 2]);
        assert_eq!(text.len, REPORT);
    }

    #[test]
    fn two_starts_ended_turn_a_plugin_off_and_one_does_not() {
        let lines = |l: &[&str]| l.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(twice(lines(&["a", "b"])), (Vec::new(), lines(&["a", "b"])));
        assert_eq!(
            twice(lines(&["a", "b", "a"])),
            (lines(&["a"]), lines(&["b"]))
        );
    }

    #[test]
    fn a_new_issue_is_filled_in_and_stays_short_enough() {
        let url = issue_url("https://github.com/me/hello/", "It stopped", "a b&c");
        assert_eq!(
            url,
            "https://github.com/me/hello/issues/new?title=It%20stopped&body=a%20b%26c"
        );
        assert_eq!(
            issue_url("https://gitlab.com/me/x", "t", "b"),
            "https://gitlab.com/me/x"
        );
        let long = issue_url("https://github.com/me/x", "t", &"é".repeat(5_000));
        assert!(long.len() <= 7_500 && !long.ends_with('%'));
    }
}
