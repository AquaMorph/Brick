//! Finds the Canon EDSDK installed by `cargo xtask edsdk` (R10, B2) and links
//! it. Without the SDK, the crate is built without it and only reports that
//! DSLR support is not available (R11).
//!
//! The SDK is looked up in `edsdk/<target_os>/` at the workspace root, or in
//! the directory named by `BRICK_EDSDK_DIR`, laid out as described in
//! `docs/edsdk.md`.

use std::env;
use std::path::{Path, PathBuf};

fn main() {
    println!("cargo::rustc-check-cfg=cfg(brick_edsdk)");
    println!("cargo::rerun-if-env-changed=BRICK_EDSDK_DIR");

    let os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let sdk = match env::var_os("BRICK_EDSDK_DIR") {
        Some(dir) => PathBuf::from(dir),
        None => Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../edsdk")
            .join(&os),
    };

    let Some(libs) = library_dir(&sdk, &os, &arch) else {
        // Cargo reruns a build script every build while a path it watches is
        // missing, so only this file is watched. `cargo xtask edsdk` touches
        // it after installing the SDK so the next build finds it.
        println!("cargo::rerun-if-changed=build.rs");
        return;
    };
    println!("cargo::rerun-if-changed={}", sdk.display());
    println!("cargo::rustc-cfg=brick_edsdk");
    // Read by the program's build script as DEP_EDSDK_DIR.
    println!("cargo::metadata=dir={}", libs.display());

    // The libraries themselves are named by `#[link]` in `src/sys.rs`.
    let kind = if os == "macos" { "framework" } else { "native" };
    println!("cargo::rustc-link-search={kind}={}", libs.display());
    // Lets this crate's tests find the library when they run. The program
    // does the same in its own build script.
    if os != "windows" {
        println!("cargo::rustc-link-arg=-Wl,-rpath,{}", libs.display());
    }
}

/// The directory holding the SDK library for the target, if it is installed.
fn library_dir(sdk: &Path, os: &str, arch: &str) -> Option<PathBuf> {
    let (dir, library) = match os {
        "macos" => (sdk.to_owned(), "EDSDK.framework/EDSDK"),
        "linux" => (sdk.join(arch), "libEDSDK.so"),
        "windows" => (sdk.join(arch), "EDSDK.lib"),
        _ => return None,
    };
    let dir = dir.canonicalize().ok()?;
    dir.join(library).exists().then_some(dir)
}
