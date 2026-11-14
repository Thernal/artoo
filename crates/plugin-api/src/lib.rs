//! The plugins' contract (#175), inside the SDK: what a plugin is to the app — its stamp, what it says of itself
//! (`Meta`) and what it was built against, and its install, the two things `plugin!` writes into it — and what
//! plugins offer each other (`services`), and the tools a plugin gives agents (`tools`, #213). A plugin reaches it through the SDK (`artoo_plugin_sdk::plugin!`).
//!
//! The app reads the stamp from the plugin's file before it loads anything of it: which plugin it is, what it needs,
//! and whether it was built against this SDK. One built against another is refused there, by name, since loading it
//! would run code that reads the app's types with another layout. Only then is it loaded, its every symbol bound at
//! once, and its install called — Rust's ABI, which the same SDK build makes safe.

pub mod services;
pub mod tools;

/// GPUI, for what `plugin!` writes (`$crate::gpui::App`).
#[doc(hidden)]
pub use gpui;

/// The plugins this run installed, as `id version`: what a report of a problem says of them.
struct Installed(Vec<String>);

impl gpui::Global for Installed {}

/// Says which plugins this run installed (the app's, once they have).
pub fn set_loaded(cx: &mut gpui::App, plugins: Vec<String>) {
    cx.set_global(Installed(plugins));
}

/// The plugins this run installed, as `id version`.
pub fn loaded(cx: &gpui::App) -> Vec<String> {
    (cx.try_global::<Installed>())
        .map(|i| i.0.clone())
        .unwrap_or_default()
}

/// This SDK's ABI: its version (the app's) and the compiler that built it. A plugin built for another is refused.
pub const ABI: &str = concat!(
    "artoo-plugin-sdk/",
    env!("CARGO_PKG_VERSION"),
    " ",
    env!("SDK_RUSTC")
);

/// The section of a plugin's file its stamp is in: Mach-O's in `__DATA` (`segment,section` in `link_section`),
/// PE's eight characters at most.
pub const STAMP_SECTION: &str = if cfg!(target_vendor = "apple") {
    "__artoo_plugin"
} else if cfg!(windows) {
    ".artoo"
} else {
    ".artoo_plugin"
};

/// What a plugin says of itself, read from its file before it is loaded. One line each in the stamp: no field holds
/// a newline, and a contract's key no space.
pub struct Meta {
    /// Its stable id: `jira`. Its folder's name under the plugins' folder, and what turns it off.
    pub id: &'static str,
    /// Its name on the plugins' page.
    pub name: &'static str,
    /// Its version: its crate's (`env!("CARGO_PKG_VERSION")`).
    pub version: &'static str,
    /// Where its code is (`https://github.com/<owner>/<repo>`): where a crash report of it is filed.
    pub repository: &'static str,
    /// The contracts it offers other plugins, each `name@major` (`jira.api@1`, `services::offer`).
    pub provides: &'static [&'static str],
    /// The contracts it needs: it is installed after the plugins offering them, and not at all without one.
    pub requires: &'static [&'static str],
}

/// A plugin's stamp as the app reads it from the file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stamp {
    /// The SDK's ABI it was built for (`ABI`).
    pub abi: String,
    /// The SDK build it was built against: that file's identity (`LC_UUID`, the GNU build-id, the PE's link time and
    /// size).
    pub build: String,
    pub id: String,
    pub name: String,
    pub version: String,
    pub repository: String,
    pub provides: Vec<String>,
    pub requires: Vec<String>,
}

/// The stamp's lines: the ABI, the SDK build, then `Meta`'s fields, a list's items apart by spaces.
const fn lines<'a>(meta: &'a Meta, build: &'a str) -> [(&'a str, &'a [&'a str]); 8] {
    [
        (ABI, &[]),
        (build, &[]),
        (meta.id, &[]),
        (meta.name, &[]),
        (meta.version, &[]),
        (meta.repository, &[]),
        ("", meta.provides),
        ("", meta.requires),
    ]
}

/// Whether `text` holds `byte`.
const fn holds(text: &str, byte: u8) -> bool {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == byte {
            return true;
        }
        i += 1;
    }
    false
}

/// The length of the stamp of `meta` built against the SDK build `build` (`stamp`). A field holding a newline, or a
/// contract's key a space, stops the plugin's build here.
pub const fn stamp_len(meta: &Meta, build: &str) -> usize {
    let lines = lines(meta, build);
    let mut len = 0;
    let mut l = 0;
    while l < lines.len() {
        let (text, items) = lines[l];
        assert!(!holds(text, b'\n'), "a plugin's meta holds no newline");
        len += text.len() + 1;
        let mut i = 0;
        while i < items.len() {
            assert!(
                !holds(items[i], b'\n') && !holds(items[i], b' ') && !items[i].is_empty(),
                "a contract's key is one word"
            );
            len += items[i].len() + if i > 0 { 1 } else { 0 };
            i += 1;
        }
        l += 1;
    }
    len
}

/// The stamp of `meta` built against the SDK build `build`, `N` its `stamp_len`.
pub const fn stamp<const N: usize>(meta: &Meta, build: &str) -> [u8; N] {
    const fn put(out: &mut [u8], at: usize, text: &str) -> usize {
        let bytes = text.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            out[at + i] = bytes[i];
            i += 1;
        }
        at + bytes.len()
    }
    let lines = lines(meta, build);
    let mut out = [0u8; N];
    let mut at = 0;
    let mut l = 0;
    while l < lines.len() {
        let (text, items) = lines[l];
        at = put(&mut out, at, text);
        let mut i = 0;
        while i < items.len() {
            if i > 0 {
                at = put(&mut out, at, " ");
            }
            at = put(&mut out, at, items[i]);
            i += 1;
        }
        at = put(&mut out, at, "\n");
        l += 1;
    }
    out
}

/// A plugin's stamp read back from its section's bytes, or `None` when they are not one.
pub fn read_stamp(bytes: &[u8]) -> Option<Stamp> {
    let text = std::str::from_utf8(bytes).ok()?.trim_end_matches('\0');
    let mut lines = text.strip_suffix('\n')?.split('\n');
    let mut next = || lines.next().map(str::to_owned);
    let list = |line: String| line.split_whitespace().map(str::to_owned).collect();
    let stamp = Stamp {
        abi: next()?,
        build: next()?,
        id: next()?,
        name: next()?,
        version: next()?,
        repository: next()?,
        provides: list(next()?),
        requires: list(next()?),
    };
    (next().is_none() && !stamp.id.is_empty()).then_some(stamp)
}

/// A plugin's entry points: its stamp and its install, called once its stamp is this SDK build's and what it
/// requires is installed.
///
/// ```ignore
/// artoo_plugin_sdk::plugin!(
///     Meta { id: "jira", name: "Jira", version: env!("CARGO_PKG_VERSION"),
///            repository: "https://github.com/Thernal/artoo-jira", provides: &[], requires: &[] },
///     install
/// );
/// fn install(cx: &mut gpui::App) { /* offer a page, a section, a tool window */ }
/// ```
///
/// It is built with `ARTOO_SDK_BUILD` in its environment, the SDK build it is built against (the template's build
/// sets it).
#[macro_export]
macro_rules! plugin {
    ($meta:expr, $install:path) => {
        const ARTOO_PLUGIN_META: $crate::Meta = $meta;

        /// What this plugin is and was built against, read from its file before it is loaded. Exported, so no
        /// linker drops it as unused.
        #[unsafe(no_mangle)]
        #[used]
        #[cfg_attr(
            target_vendor = "apple",
            unsafe(link_section = "__DATA,__artoo_plugin")
        )]
        #[cfg_attr(windows, unsafe(link_section = ".artoo"))]
        #[cfg_attr(
            not(any(target_vendor = "apple", windows)),
            unsafe(link_section = ".artoo_plugin")
        )]
        pub static ARTOO_PLUGIN_STAMP: [u8; $crate::stamp_len(
            &ARTOO_PLUGIN_META,
            env!("ARTOO_SDK_BUILD"),
        )] = $crate::stamp(&ARTOO_PLUGIN_META, env!("ARTOO_SDK_BUILD"));

        /// What it adds to the app.
        #[unsafe(no_mangle)]
        pub fn artoo_plugin_install(cx: &mut $crate::gpui::App) {
            $install(cx)
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    const META: Meta = Meta {
        id: "greeter",
        name: "Greeter",
        version: "0.2.0",
        repository: "https://github.com/Thernal/artoo-greeter",
        provides: &[],
        requires: &["hello.api@1", "jira.api@2"],
    };
    const BUILD: &str = "5C0F3B9E-2A41-3D7C-9B1E-6F2A8C4D0E17";

    #[test]
    fn a_stamp_reads_back_as_written() {
        let written: [u8; stamp_len(&META, BUILD)] = stamp(&META, BUILD);
        let read = read_stamp(&written).unwrap();
        assert_eq!((read.abi.as_str(), read.build.as_str()), (ABI, BUILD));
        assert_eq!(
            (read.id.as_str(), read.version.as_str()),
            ("greeter", "0.2.0")
        );
        assert_eq!(read.provides, Vec::<String>::new());
        assert_eq!(read.requires, ["hello.api@1", "jira.api@2"]);
        assert!(ABI.starts_with("artoo-plugin-sdk/") && ABI.contains("rustc "));
        // A section the linker padded reads the same.
        let mut padded = written.to_vec();
        padded.extend([0; 7]);
        assert_eq!(read_stamp(&padded), Some(read));
    }

    #[test]
    fn what_is_not_a_stamp_reads_as_none() {
        assert_eq!(read_stamp(b"not a stamp"), None);
        assert_eq!(read_stamp(b"a\nb\n\nd\ne\nf\n\n\n"), None);
        assert_eq!(read_stamp(&[0xff, 0xfe, b'\n']), None);
    }
}
