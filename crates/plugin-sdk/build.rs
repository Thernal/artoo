//! How the SDK is linked (#175).

fn main() {
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    // Named by `@rpath`, not by where it was built: the app finds it beside itself (a copy, a test), in `deps/`
    // (`target/<profile>`) or in the bundle's Frameworks (the app's build.rs gives those). Linux names it by its
    // soname, rustc's already.
    if os == "macos" {
        println!("cargo:rustc-link-arg=-Wl,-install_name,@rpath/libartoo_plugin_sdk.dylib");
        // A shared library resolves every symbol when it is linked: GPUI's `io_surface` uses the framework without
        // linking it (an executable gets it through the platform crate).
        println!("cargo:rustc-link-lib=framework=IOSurface");
    }
}
