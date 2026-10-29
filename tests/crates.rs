//! The one rule of the crates (#165): a feature crate never depends on another feature crate. The shared
//! crates are everyone's, and the app (this package) may depend on all of them.

use std::path::Path;

/// The crates every feature may depend on, each on the ones before it only: the core, then the design
/// system drawn over it.
const SHARED: &[&str] = &["artoo-core", "artoo-ui-kit"];

/// Edges between feature crates allowed for now, each until the stage that removes it.
const ALLOWED: &[(&str, &str)] = &[
    // `mcp` reads requests through `mr` until stage 3's `Host` trait.
    ("artoo-agents", "artoo-hosts"),
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

/// Every crate under `crates/` with what it depends on.
fn crates() -> Vec<(String, Vec<String>)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("crates");
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let manifest = std::fs::read_to_string(entry.unwrap().path().join("Cargo.toml")).unwrap();
        let name = manifest
            .lines()
            .find_map(|l| l.strip_prefix("name = \"")?.strip_suffix('"'))
            .unwrap()
            .to_owned();
        out.push((name, depends_on(&manifest)));
    }
    out
}

#[test]
fn a_feature_crate_never_depends_on_another() {
    let mut found = Vec::new();
    for (name, deps) in crates() {
        for dep in deps {
            let rank = |c: &str| SHARED.iter().position(|s| *s == c);
            let shared = match (rank(&name), rank(&dep)) {
                (_, None) => false,
                (None, Some(_)) => true,
                (Some(from), Some(to)) => to < from,
            };
            if !shared && !ALLOWED.contains(&(name.as_str(), dep.as_str())) {
                found.push(format!("{name} → {dep}"));
            }
        }
    }
    assert!(
        found.is_empty(),
        "feature crates depending on one another: {found:?}"
    );
}

#[test]
fn an_allowed_edge_goes_once_it_is_gone() {
    let crates = crates();
    for (from, to) in ALLOWED {
        let still = crates
            .iter()
            .any(|(name, deps)| name == from && deps.iter().any(|d| d == to));
        assert!(still, "{from} → {to} is gone: take it out of ALLOWED");
    }
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
