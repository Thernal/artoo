//! The process's fatal faults, told to one function before the app ends as it would have: signals on Unix, exceptions
//! on Windows. Whatever handles them runs where almost nothing is safe, so what it writes goes through `write`.

use std::path::PathBuf;
use std::sync::OnceLock;

pub use os::{Target, target, write};

static ON_FAULT: OnceLock<fn(&[u8])> = OnceLock::new();

/// Tells `on_fault` what a fatal fault was, from then on. Only the first call counts.
pub fn listen(on_fault: fn(&[u8])) {
    if ON_FAULT.set(on_fault).is_ok() {
        os::listen();
    }
}

fn tell(what: &[u8]) {
    if let Some(on_fault) = ON_FAULT.get() {
        on_fault(what);
    }
}

#[cfg(unix)]
mod os {
    use super::{PathBuf, tell};
    use std::ffi::CString;
    use std::mem::MaybeUninit;
    use std::os::unix::ffi::OsStringExt;
    use std::sync::atomic::{AtomicBool, Ordering};

    pub type Target = CString;

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

    pub fn target(path: PathBuf) -> Target {
        CString::new(path.into_os_string().into_vec()).unwrap_or_default()
    }

    pub fn write(path: &Target, bytes: &[u8]) {
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
                tell(SIGNALS[at].1);
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
            // SAFETY: an action filled for this handler; `listen` runs once.
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
    use super::{PathBuf, tell};
    use windows_sys::Win32::Foundation::{
        EXCEPTION_ACCESS_VIOLATION, EXCEPTION_ILLEGAL_INSTRUCTION, EXCEPTION_INT_DIVIDE_BY_ZERO,
        EXCEPTION_PRIV_INSTRUCTION,
    };
    use windows_sys::Win32::System::Diagnostics::Debug::{
        AddVectoredExceptionHandler, EXCEPTION_POINTERS,
    };

    pub type Target = PathBuf;

    pub fn target(path: PathBuf) -> Target {
        path
    }

    pub fn write(path: &Target, bytes: &[u8]) {
        let _ = std::fs::write(path, bytes);
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
            tell(what);
        }
        CONTINUE_SEARCH
    }

    pub(super) fn listen() {
        // SAFETY: the handler lives as long as the process.
        unsafe { AddVectoredExceptionHandler(1, Some(on_exception)) };
    }
}
