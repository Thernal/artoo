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

    rpaths();

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

/// Where the app finds the plugins' SDK and the `libstd` it links (#175: with a shared library in the graph, std is
/// one too): beside itself (a copy of it, a test in `deps/`), in `deps/` (`target/<profile>/artoo`), in the bundle's
/// Frameworks or the AppImage's `lib/`, and last the toolchain's libstd, for a build run where it was made — not in a
/// release's (`ARTOO_PACKAGED`, its libraries travel with it), and bundle.sh takes it out of a local one. Windows looks
/// beside the `.exe` itself.
fn rpaths() {
    println!("cargo:rerun-if-env-changed=ARTOO_PACKAGED");
    let (me, dirs) = match std::env::var("CARGO_CFG_TARGET_OS").as_deref() {
        Ok("macos") => ("@executable_path", ["", "/deps", "/../Frameworks"]),
        Ok("linux") => ("$ORIGIN", ["", "/deps", "/../lib"]),
        _ => return,
    };
    for dir in dirs {
        println!("cargo:rustc-link-arg-bins=-Wl,-rpath,{me}{dir}");
    }
    if std::env::var_os("ARTOO_PACKAGED").is_some() {
        return;
    }
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_owned());
    let target = std::env::var("TARGET").unwrap_or_default();
    let libdir = std::process::Command::new(rustc)
        .args(["--print", "target-libdir", "--target", &target])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned());
    if let Some(libdir) = libdir {
        println!("cargo:rustc-link-arg-bins=-Wl,-rpath,{libdir}");
    }
}
