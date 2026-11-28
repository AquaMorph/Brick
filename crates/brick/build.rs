//! Lets the program find the Canon EDSDK libraries when it runs, when Brick
//! is built with them (R10, R11). `brick-edsdk`'s build script reports where
//! it found them as `DEP_EDSDK_DIR`.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-env-changed=DEP_EDSDK_DIR");
    let Some(sdk) = env::var_os("DEP_EDSDK_DIR").map(PathBuf::from) else {
        return;
    };
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        copy_dlls(&sdk);
    } else {
        // Development builds load the SDK from where it was installed.
        println!("cargo::rustc-link-arg=-Wl,-rpath,{}", sdk.display());
    }
}

/// Windows looks for DLLs beside the program, so the SDK's DLLs are copied
/// into the build's output directory and beside the test programs.
fn copy_dlls(sdk: &Path) {
    let Some(out_dir) = env::var_os("OUT_DIR").map(PathBuf::from) else {
        return;
    };
    // OUT_DIR is <target>/<profile>/build/<package>/out.
    let Some(profile) = out_dir.ancestors().nth(3) else {
        return;
    };
    let Ok(entries) = fs::read_dir(sdk) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("dll"))
        {
            continue;
        }
        for dir in [profile.to_owned(), profile.join("deps")] {
            if let Some(name) = path.file_name() {
                let _ = fs::create_dir_all(&dir);
                if let Err(e) = fs::copy(&path, dir.join(name)) {
                    println!("cargo::warning=could not copy {}: {e}", path.display());
                }
            }
        }
    }
}
