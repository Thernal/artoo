//! What a library's file says without being loaded (#175): a plugin's stamp (`stamp_of`), and a library's identity
//! (`identity`) — the SDK build a plugin names, and the SDK this app loaded (`sdk_build`). Only what is asked is
//! read of a file: the SDK is tens of megabytes.

use artoo_core::platform::libraries::image_at;
use artoo_plugin_api::{STAMP_SECTION, Stamp, read_stamp};
use object::read::macho::{FatArch, MachOFatFile32, MachOFatFile64};
use object::read::pe::{ImageNtHeaders, ImageOptionalHeader, PeFile32, PeFile64};
use object::{
    Architecture, BinaryFormat, FileKind, LittleEndian, Object, ObjectSection, ObjectSegment,
};
use object::{ReadCache, ReadCacheRange};
use std::fs::File;
use std::path::{Path, PathBuf};

/// This machine's architecture: the slice of a universal Mach-O that is read.
const HERE: Architecture = if cfg!(target_arch = "aarch64") {
    Architecture::Aarch64
} else if cfg!(target_arch = "x86_64") {
    Architecture::X86_64
} else {
    Architecture::Unknown
};

/// A file opened to be read in parts.
struct Opened {
    cache: ReadCache<File>,
    len: u64,
}

fn open(path: &Path) -> Result<Opened, String> {
    let file = File::open(path).map_err(|e| format!("could not be read: {e}"))?;
    let len = (file
        .metadata()
        .map_err(|e| format!("could not be read: {e}"))?)
    .len();
    Ok(Opened {
        cache: ReadCache::new(file),
        len,
    })
}

/// Where a universal Mach-O keeps this machine's slice.
fn slice<A: FatArch>(arches: &[A]) -> Result<(u64, u64), String> {
    (arches.iter())
        .find(|a| a.architecture() == HERE)
        .map(|a| a.file_range())
        .ok_or_else(|| "has no build for this machine".to_owned())
}

impl Opened {
    /// Where the object for this machine is: the whole file, or a universal Mach-O's slice.
    fn range(&self) -> Result<ReadCacheRange<'_, File>, String> {
        let whole = self.cache.range(0, self.len);
        let fat = |e: object::Error| format!("is not a library: {e}");
        let (offset, size) = match FileKind::parse(whole).map_err(fat)? {
            FileKind::MachOFat32 => slice(MachOFatFile32::parse(whole).map_err(fat)?.arches())?,
            FileKind::MachOFat64 => slice(MachOFatFile64::parse(whole).map_err(fat)?.arches())?,
            _ => (0, self.len),
        };
        Ok(self.cache.range(offset, size))
    }
}

/// A plugin's stamp, read from its file (`plugin!` wrote it), or why there is none.
pub(crate) fn stamp_of(path: &Path) -> Result<Stamp, String> {
    let opened = open(path)?;
    let range = opened.range()?;
    let file = object::File::parse(range).map_err(|e| format!("is not a library: {e}"))?;
    let section =
        (file.section_by_name(STAMP_SECTION)).ok_or("is not an Artoo plugin: it has no stamp")?;
    let data = (section.data()).map_err(|e| format!("has a stamp that can't be read: {e}"))?;
    read_stamp(data).ok_or_else(|| "has a stamp that does not read as one".to_owned())
}

/// A library's identity: the `LC_UUID` ld64 writes (as `dwarfdump --uuid` says it), the GNU build-id, or a PE's
/// link time and size (what a symbol server knows an image by). Each changes with any byte of what was linked.
pub(crate) fn identity(path: &Path) -> Option<String> {
    let opened = open(path).ok()?;
    let range = opened.range().ok()?;
    let file = object::File::parse(range).ok()?;
    match file.format() {
        BinaryFormat::MachO => file.mach_uuid().ok()?.map(|uuid| uuid_text(&uuid)),
        BinaryFormat::Elf => (file.build_id().ok()?).map(hex),
        BinaryFormat::Pe => pe_identity(range),
        _ => None,
    }
}

/// `5C0F3B9E-2A41-3D7C-9B1E-6F2A8C4D0E17`.
fn uuid_text(uuid: &[u8; 16]) -> String {
    let hex = hex(uuid).to_uppercase();
    let parts = [
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32],
    ];
    parts.join("-")
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// A PE image's link time and size, as its symbol server's key: `5F3B2A1C2d4000`.
fn pe_identity(range: ReadCacheRange<'_, File>) -> Option<String> {
    fn key<H: ImageNtHeaders>(headers: &H) -> String {
        let at = headers.file_header().time_date_stamp.get(LittleEndian);
        let size = headers.optional_header().size_of_image();
        format!("{at:08X}{size:x}")
    }
    if let Ok(pe) = PeFile64::parse(range) {
        return Some(key(pe.nt_headers()));
    }
    PeFile32::parse(range).ok().map(|pe| key(pe.nt_headers()))
}

/// The file of the SDK this app loaded: the image an address of it is in (a function of the plugins' contract,
/// which the SDK holds).
pub(crate) fn sdk_path() -> Option<PathBuf> {
    image_of(artoo_plugin_api::read_stamp as *const ())
}

/// The identity of the SDK this app loaded: what a plugin's stamp must name.
pub(crate) fn sdk_build() -> Option<String> {
    identity(&sdk_path()?)
}

/// Where the library at `path` is in memory, given an address in it: what a stack's frames are compared with.
pub(crate) fn loaded_range(path: &Path, at: *const ()) -> Option<(usize, usize)> {
    let (_, start) = image_at(at)?;
    let opened = open(path).ok()?;
    let file = object::File::parse(opened.range().ok()?).ok()?;
    let (mut low, mut high) = (u64::MAX, 0);
    for segment in file.segments() {
        low = low.min(segment.address());
        high = high.max(segment.address() + segment.size());
    }
    // A PE's sections start past its headers; its image starts at its base.
    if file.format() == BinaryFormat::Pe {
        low = file.relative_address_base();
    }
    (low < high).then(|| (start, start + (high - low) as usize))
}

fn image_of(at: *const ()) -> Option<PathBuf> {
    image_at(at).map(|(path, _)| path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_identity_is_said_as_its_tools_say_it() {
        let uuid = [
            0x5c, 0x0f, 0x3b, 0x9e, 0x2a, 0x41, 0x3d, 0x7c, 0x9b, 0x1e, 0x6f, 0x2a, 0x8c, 0x4d,
            0x0e, 0x17,
        ];
        assert_eq!(uuid_text(&uuid), "5C0F3B9E-2A41-3D7C-9B1E-6F2A8C4D0E17");
        assert_eq!(hex(&[0xab, 0x01]), "ab01");
    }

    #[test]
    fn this_test_is_a_library_with_an_identity_and_no_stamp() {
        // The test's own executable: an object of this machine, with an identity (`LC_UUID`, a build-id, a PE
        // header), and no stamp.
        let me = std::env::current_exe().unwrap();
        assert_eq!(
            stamp_of(&me),
            Err("is not an Artoo plugin: it has no stamp".to_owned())
        );
        if cfg!(any(target_vendor = "apple", windows)) {
            assert!(identity(&me).is_some_and(|id| !id.is_empty()));
        }
        let image = image_of(sdk_path as *const ()).and_then(|p| p.canonicalize().ok());
        assert_eq!(image, me.canonicalize().ok());
    }

    #[test]
    fn what_is_no_library_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let text = dir.path().join("notes.dylib");
        std::fs::write(&text, "not a library").unwrap();
        assert!(stamp_of(&text).unwrap_err().starts_with("is not a library"));
        assert!(identity(&text).is_none());
        let gone = dir.path().join("gone.dylib");
        assert!(
            stamp_of(&gone)
                .unwrap_err()
                .starts_with("could not be read")
        );
    }
}
