//! What a plugin builds against (#175): GPUI, its platform, the core, the kit, the shell's contract and every
//! feature's, as one shared library (`crate-type = ["dylib"]`) the app links and every plugin loads into. Each of them
//! is then once in the process: a plugin draws with the app's GPUI and theme, and what it offers lands in the app's
//! registries.
//!
//! A plugin depends on this crate alone and reaches the rest through it (`artoo_plugin_sdk::gpui`, `::shell`, …).
//! It is built against the SDK of the release it runs in: any change here, even a byte, needs every plugin built
//! again (the cost the plugins' spike named, #174).
//!
//! The C libraries under the core (libgit2, Oniguruma, tree-sitter) are inside the SDK, and their functions are not
//! exported: a crate that links the SDK — the app, a plugin — calls them only through Rust code compiled in the SDK. A
//! git2 call generic over its arguments (`Repository::init(path)`, `TreeBuilder::insert(name, …)`) is compiled where it
//! is named, and there it does not link: git goes through the core (`artoo_core::git`; a test's repositories
//! `git::fixture`).
//!
//! What makes a crate a plugin is the plugins' contract, here too: `plugin!` (its meta, its install, the stamp of the
//! SDK it was built against) and what plugins offer each other (`services`).
//!
//! The SDK holds no code of its own, and no test: Cargo gives a `dylib` `-C prefer-dynamic` only when it is not one
//! of the packages asked for, and without it rustc links std into the SDK, so the app, linking std too, does not
//! link. A test target here would make `cargo test --workspace` ask for it; `cargo build --workspace` and
//! `cargo build -p artoo-plugin-sdk` do too — build the app.

pub use artoo_plugin_api::{
    ABI, Meta, STAMP_SECTION, Stamp, plugin, read_stamp, services, stamp, stamp_len,
};

pub use artoo_agents_api as agents;
pub use artoo_branches_api as branches;
pub use artoo_code_api as code;
pub use artoo_core as core;
pub use artoo_hosts_api as hosts;
pub use artoo_images_api as images;
pub use artoo_requests_api as requests;
pub use artoo_review_api as review;
pub use artoo_shell_api as shell;
pub use artoo_ui_kit as kit;
pub use gpui;
/// The platform, for the app alone: it is here because its load-time constructors must share the SDK's image.
pub use gpui_platform;
