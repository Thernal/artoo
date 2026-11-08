//! The commit a build is made from, for the version the app says (a report, Settings): `ARTOO_COMMIT`,
//! else `git rev-parse --short HEAD`, else "unknown" (#118). The app's, which hands it to the core as it
//! starts (`artoo_core::set_commit`): a commit rebuilds the app alone, not every crate under it.

fn main() {
    let commit = std::env::var("ARTOO_COMMIT").ok().or_else(|| {
        std::process::Command::new("git")
            .args(["rev-parse", "--short=7", "HEAD"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
    });
    println!(
        "cargo:rustc-env=ARTOO_COMMIT={}",
        commit.unwrap_or_else(|| "unknown".to_owned())
    );
    println!("cargo:rerun-if-env-changed=ARTOO_COMMIT");
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs/heads");

    // Windows: the `.exe` carries the icon as resource 1, the one GPUI gives its windows (Explorer and the taskbar
    // read it too). Made from `Artoo.icns`'s master without macOS's margin.
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/Artoo.ico");
        res.compile().expect("the Windows resource (the icon)");
    }
    println!("cargo:rerun-if-changed=assets/Artoo.ico");
}
