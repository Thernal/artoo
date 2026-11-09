//! The process's shared libraries: loading one, and which of them an address is in.

use std::path::{Path, PathBuf};

/// The file of the image an address is in, and where that image starts.
#[cfg(unix)]
pub fn image_at(at: *const ()) -> Option<(PathBuf, usize)> {
    use std::os::unix::ffi::OsStrExt;
    // SAFETY: `dladdr` fills `info` for an address of a loaded image; the name it points at lives with the image.
    let mut info: libc::Dl_info = unsafe { std::mem::zeroed() };
    if unsafe { libc::dladdr(at.cast(), &mut info) } == 0 || info.dli_fname.is_null() {
        return None;
    }
    let name = unsafe { std::ffi::CStr::from_ptr(info.dli_fname) };
    let path = PathBuf::from(std::ffi::OsStr::from_bytes(name.to_bytes()));
    Some((path, info.dli_fbase as usize))
}

#[cfg(windows)]
pub fn image_at(at: *const ()) -> Option<(PathBuf, usize)> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::System::LibraryLoader::{
        GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
        GetModuleFileNameW, GetModuleHandleExW,
    };
    let flags =
        GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT;
    let mut module = std::ptr::null_mut();
    // SAFETY: the address is in a loaded image; its handle is not counted (UNCHANGED_REFCOUNT), the name copied.
    if unsafe { GetModuleHandleExW(flags, at.cast(), &mut module) } == 0 {
        return None;
    }
    let mut name = vec![0u16; 32_768];
    let len = unsafe { GetModuleFileNameW(module, name.as_mut_ptr(), name.len() as u32) } as usize;
    (len > 0).then(|| {
        (
            PathBuf::from(std::ffi::OsString::from_wide(&name[..len])),
            module as usize,
        )
    })
}

/// Loads a library with every symbol bound now, so one that is missing refuses it rather than stopping the app at its
/// first call, and its symbols private to it.
///
/// # Safety
///
/// Loading runs the library's own initialisers: the caller vouches for what it runs.
pub unsafe fn open(path: &Path) -> Result<libloading::Library, libloading::Error> {
    #[cfg(unix)]
    {
        use libloading::os::unix::{Library, RTLD_LOCAL, RTLD_NOW};
        // SAFETY: the caller's.
        unsafe { Library::open(Some(path), RTLD_NOW | RTLD_LOCAL) }.map(libloading::Library::from)
    }
    // Windows binds every import as it loads.
    #[cfg(windows)]
    // SAFETY: the caller's.
    unsafe {
        libloading::Library::new(path)
    }
}
