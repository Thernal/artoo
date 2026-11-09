//! The compiler that builds the SDK is part of its ABI (`ABI`): the one building this crate, inside it.

fn main() {
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_owned());
    let version = std::process::Command::new(rustc)
        .arg("--version")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned());
    println!(
        "cargo:rustc-env=SDK_RUSTC={}",
        version.unwrap_or_else(|| "rustc unknown".to_owned())
    );
    println!("cargo:rerun-if-env-changed=RUSTC");
}
