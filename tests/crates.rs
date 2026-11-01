//! The crates' rules (#165; the `architecture` skill): what a crate may depend on follows from its role,
//! and an implementation shows the app nothing but functions. The app (this package) may depend on all.

use std::path::{Path, PathBuf};

/// What a crate is, read from its name: one of the two shared crates, a feature's contract (`-api`), or a
/// feature's implementation.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Role {
    Core,
    Kit,
    Contract,
    Implementation,
}

fn role(name: &str) -> Role {
    match name {
        "artoo-core" => Role::Core,
        "artoo-ui-kit" => Role::Kit,
        _ if name.ends_with("-api") => Role::Contract,
        _ => Role::Implementation,
    }
}

/// The core depends on nothing of Artoo's, the kit on the core alone, a contract on the core and the kit (its
/// types may be the kit's: a part's selection), an implementation on anything but another implementation.
fn may_depend(from: Role, to: Role) -> bool {
    match from {
        Role::Core => false,
        Role::Kit => to == Role::Core,
        Role::Contract => matches!(to, Role::Core | Role::Kit),
        Role::Implementation => to != Role::Implementation,
    }
}

/// Edges the roles forbid, allowed for now, each until the stage that removes it.
const ALLOWED: &[(&str, &str)] = &[];

/// Implementations whose `lib.rs` still shows more than functions, each until the stage that narrows it.
const OPEN: &[&str] = &[
    // The app's views reach their modules: stage 3 gives hosts its contract, stage 4 moves the views.
    "artoo-hosts",
    "artoo-agents",
    // The review (stage 4, R3): the app's views read its modules, its entity included, until the Changes list and
    // the head line follow and its contract (`artoo-review-api`) holds what pages speak.
    "artoo-review",
    // Merge requests (stage 4, R5): the app's views read the state it keeps until they move into it.
    "artoo-requests",
    // Branches (stage 4, B): the app's views read the history it keeps until they move into it.
    "artoo-branches",
    // The frame (stage 4, K): the tab draws its pieces and reads their state until the chrome is the shell's.
    "artoo-shell",
];

/// The `artoo-*` crates `manifest` depends on, its dev-dependencies left out.
fn depends_on(manifest: &str) -> Vec<String> {
    let mut deps = Vec::new();
    let mut in_deps = false;
    for line in manifest.lines().map(str::trim) {
        if line.starts_with('[') {
            in_deps = line.ends_with("dependencies]") && !line.contains("dev-dependencies");
        } else if in_deps && let Some((key, _)) = line.split_once('=') {
            // `artoo-core = { … }` or `artoo-core.workspace = true`.
            let name = key.trim().split('.').next().unwrap_or_default();
            if name.starts_with("artoo-") {
                deps.push(name.to_owned());
            }
        }
    }
    deps
}

const FUNCTIONS: &[&str] = &["pub fn ", "pub const fn ", "pub async fn "];

/// The top-level items `lib` makes public that are not functions.
fn shown(lib: &str) -> Vec<&str> {
    lib.lines()
        .filter(|l| l.starts_with("pub ") && !FUNCTIONS.iter().any(|f| l.starts_with(f)))
        .collect()
}

struct Crate {
    dir: PathBuf,
    name: String,
    deps: Vec<String>,
}

/// Every crate under `crates/`.
fn crates() -> Vec<Crate> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("crates");
    let mut out = Vec::new();
    for entry in std::fs::read_dir(root).unwrap() {
        let dir = entry.unwrap().path();
        let manifest = std::fs::read_to_string(dir.join("Cargo.toml")).unwrap();
        let name = manifest
            .lines()
            .find_map(|l| l.strip_prefix("name = \"")?.strip_suffix('"'))
            .unwrap()
            .to_owned();
        let deps = depends_on(&manifest);
        out.push(Crate { dir, name, deps });
    }
    out
}

#[test]
fn a_crate_depends_only_on_what_its_role_allows() {
    let mut found = Vec::new();
    for c in crates() {
        for dep in &c.deps {
            let allowed = ALLOWED.contains(&(c.name.as_str(), dep.as_str()));
            if !may_depend(role(&c.name), role(dep)) && !allowed {
                found.push(format!("{} → {dep}", c.name));
            }
        }
    }
    assert!(
        found.is_empty(),
        "dependencies their roles forbid: {found:?}"
    );
}

#[test]
fn an_allowed_edge_goes_once_it_is_gone() {
    let crates = crates();
    for (from, to) in ALLOWED {
        let still = crates
            .iter()
            .any(|c| c.name == *from && c.deps.iter().any(|d| d == to));
        assert!(still, "{from} → {to} is gone: take it out of ALLOWED");
    }
}

#[test]
fn an_implementation_shows_only_functions() {
    let mut found = Vec::new();
    for c in crates() {
        if role(&c.name) == Role::Implementation && !OPEN.contains(&c.name.as_str()) {
            let lib = std::fs::read_to_string(c.dir.join("src/lib.rs")).unwrap();
            found.extend(shown(&lib).iter().map(|item| format!("{}: {item}", c.name)));
        }
    }
    assert!(
        found.is_empty(),
        "implementations showing more than functions (make them pub(crate)): {found:?}"
    );
}

#[test]
fn an_open_surface_goes_once_it_is_narrow() {
    let crates = crates();
    for name in OPEN {
        let c = crates.iter().find(|c| c.name == *name).unwrap();
        assert_eq!(
            role(name),
            Role::Implementation,
            "{name} in OPEN is no implementation"
        );
        let lib = std::fs::read_to_string(c.dir.join("src/lib.rs")).unwrap();
        assert!(
            !shown(&lib).is_empty(),
            "{name} shows only functions now: take it out of OPEN"
        );
    }
}

#[test]
fn a_crates_role_is_read_from_its_name() {
    assert_eq!(role("artoo-core"), Role::Core);
    assert_eq!(role("artoo-ui-kit"), Role::Kit);
    assert_eq!(role("artoo-hosts-api"), Role::Contract);
    assert_eq!(role("artoo-hosts"), Role::Implementation);
    assert!(may_depend(Role::Implementation, Role::Contract));
    assert!(!may_depend(Role::Implementation, Role::Implementation));
    assert!(!may_depend(Role::Contract, Role::Contract));
    assert!(may_depend(Role::Contract, Role::Kit));
    assert!(!may_depend(Role::Kit, Role::Contract));
}

#[test]
fn a_libs_shown_items_are_its_public_non_functions() {
    let lib = "//! A feature.\n\nmod pages;\npub(crate) mod rows;\npub mod mr;\npub use mr::Request;\n\
               pub struct Hidden;\npub const fn zoom() -> f32 {}\npub fn install(cx: &mut App) {\n    \
               pub fn inner() {}\n}\n";
    assert_eq!(
        shown(lib),
        ["pub mod mr;", "pub use mr::Request;", "pub struct Hidden;"]
    );
}

#[test]
fn a_manifests_dependencies_are_read_without_its_dev_dependencies() {
    let manifest = "[package]\nname = \"artoo-x\"\n\n[dependencies]\nartoo-core = { path = \"../core\" }\n\
                    artoo-hosts.workspace = true\nserde.workspace = true\n\n\
                    [target.'cfg(unix)'.dependencies]\nartoo-agents = { path = \"../agents\" }\n\n\
                    [dev-dependencies]\nartoo-reports = { path = \"../reports\" }\n";
    assert_eq!(
        depends_on(manifest),
        ["artoo-core", "artoo-hosts", "artoo-agents"]
    );
}
