//! Which build of a plugin this app wants: its release assets are named by the target (#176).

fn main() {
    println!(
        "cargo:rustc-env=ARTOO_TARGET={}",
        std::env::var("TARGET").unwrap_or_default()
    );
}
