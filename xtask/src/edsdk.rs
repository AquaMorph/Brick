//! EDSDK setup (R10, B2). Unpacks the Canon EDSDK downloads into the
//! git-ignored `edsdk/` directory at the workspace root, laid out as:
//!
//! ```text
//! edsdk/
//! ├── readme.txt            Canon's notice (IJG acknowledgement)
//! ├── linux/
//! │   ├── VERSION
//! │   ├── include/          EDSDK.h, EDSDKErrors.h, EDSDKTypes.h
//! │   ├── x86_64/           libEDSDK.so
//! │   ├── aarch64/          libEDSDK.so
//! │   └── arm/              libEDSDK.so
//! ├── windows/
//! │   ├── VERSION
//! │   ├── include/
//! │   ├── x86_64/           EDSDK.dll, EdsImage.dll, EDSDK.lib
//! │   └── x86/              EDSDK.dll, EdsImage.dll, EDSDK.lib
//! └── macos/
//!     ├── VERSION
//!     ├── include/
//!     └── EDSDK.framework   universal (x86_64 + arm64)
//! ```
//!
//! Directory names match Cargo's `target_os` and `target_arch` values so a
//! build script can find the SDK at `edsdk/<target_os>/<target_arch>/`.

use std::fs::{self, File};
use std::io::{self, Cursor, Read, Seek};
use std::path::{Path, PathBuf};
use std::process::Command;

use zip::ZipArchive;

const HEADERS: [&str; 3] = ["EDSDK.h", "EDSDKErrors.h", "EDSDKTypes.h"];

#[derive(Clone, Copy)]
enum Platform {
    Linux,
    Windows,
    Macos,
}

impl Platform {
    fn dir(self) -> &'static str {
        match self {
            Platform::Linux => "linux",
            Platform::Windows => "windows",
            Platform::Macos => "macos",
        }
    }

    /// Files that must exist under `edsdk/<platform>/` after installing.
    fn required(self) -> Vec<String> {
        let mut files: Vec<String> = HEADERS.iter().map(|h| format!("include/{h}")).collect();
        let libs: &[&str] = match self {
            Platform::Linux => &[
                "x86_64/libEDSDK.so",
                "aarch64/libEDSDK.so",
                "arm/libEDSDK.so",
            ],
            Platform::Windows => &[
                "x86_64/EDSDK.dll",
                "x86_64/EdsImage.dll",
                "x86_64/EDSDK.lib",
                "x86/EDSDK.dll",
                "x86/EdsImage.dll",
                "x86/EDSDK.lib",
            ],
            Platform::Macos => &["EDSDK.framework/Versions/A/EDSDK"],
        };
        files.extend(libs.iter().map(|s| (*s).to_owned()));
        files
    }
}

/// Installs the SDK from each archive. With no archives given, every
/// `EDSDK*.zip` in the workspace root is used.
pub fn run(archives: &[&str]) -> Result<(), String> {
    let root = workspace_root();
    let archives: Vec<PathBuf> = if archives.is_empty() {
        find_archives(&root)?
    } else {
        archives.iter().map(PathBuf::from).collect()
    };
    if archives.is_empty() {
        return Err(format!(
            "no EDSDK*.zip found in {}. Download the EDSDK from Canon (see docs/edsdk.md) \
             or pass the archive paths",
            root.display()
        ));
    }

    let edsdk = root.join("edsdk");
    fs::create_dir_all(&edsdk).map_err(|e| format!("failed to create {}: {e}", edsdk.display()))?;

    for archive in &archives {
        install(archive, &edsdk).map_err(|e| format!("{}: {e}", archive.display()))?;
    }
    Ok(())
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives inside the workspace")
        .to_owned()
}

fn find_archives(root: &Path) -> Result<Vec<PathBuf>, String> {
    let entries =
        fs::read_dir(root).map_err(|e| format!("failed to read {}: {e}", root.display()))?;
    let mut archives: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|name| {
                        name.starts_with("EDSDK") && name.to_ascii_lowercase().ends_with(".zip")
                    })
        })
        .collect();
    archives.sort();
    Ok(archives)
}

fn install(archive: &Path, edsdk: &Path) -> Result<(), String> {
    let file = File::open(archive).map_err(|e| format!("failed to open: {e}"))?;
    let mut zip = ZipArchive::new(file).map_err(|e| format!("not a zip archive: {e}"))?;
    let names = entry_names(&zip)?;

    let platform = detect(&names).ok_or(
        "does not look like an EDSDK download for Linux, Windows, or macOS \
         (EDSDK/Header/EDSDK.h or Macintosh.dmg.zip not found)",
    )?;
    let version = read_version(&mut zip, &names).unwrap_or_else(|| "unknown".into());

    // Build in a staging directory and swap it in once complete so a failed
    // install never leaves a half-populated SDK behind.
    let staging = edsdk.join(format!(".{}.partial", platform.dir()));
    remove_dir(&staging)?;
    fs::create_dir_all(&staging).map_err(io_err(&staging))?;

    let result = match platform {
        Platform::Linux | Platform::Windows => {
            extract_libraries(&mut zip, &names, platform, &staging)
        }
        Platform::Macos => extract_macos(&mut zip, &names, edsdk, &staging),
    }
    .and_then(|()| {
        if let Some(readme) = names.iter().find(|n| base_name(n) == "readme.txt") {
            extract_entry(&mut zip, readme, &edsdk.join("readme.txt"))?;
        }
        fs::write(staging.join("VERSION"), format!("{version}\n")).map_err(io_err(&staging))?;
        verify(platform, &staging)
    });
    if let Err(e) = result {
        let _ = fs::remove_dir_all(&staging);
        return Err(e);
    }

    let target = edsdk.join(platform.dir());
    remove_dir(&target)?;
    fs::rename(&staging, &target).map_err(io_err(&target))?;
    println!(
        "installed EDSDK {version} for {} into {}",
        platform.dir(),
        target.display()
    );
    Ok(())
}

fn detect(names: &[String]) -> Option<Platform> {
    if names.iter().any(|n| base_name(n) == "Macintosh.dmg.zip") {
        return Some(Platform::Macos);
    }
    let prefix = sdk_prefix(names)?;
    if names
        .iter()
        .any(|n| *n == format!("{prefix}EDSDK_64/Dll/EDSDK.dll"))
    {
        Some(Platform::Windows)
    } else if names
        .iter()
        .any(|n| n.starts_with(&prefix) && n.ends_with("/libEDSDK.so"))
    {
        Some(Platform::Linux)
    } else {
        None
    }
}

/// The path inside the archive that holds `EDSDK/`, usually empty but some
/// downloads wrap everything in an extra folder.
fn sdk_prefix(names: &[String]) -> Option<String> {
    names
        .iter()
        .find_map(|n| n.strip_suffix("EDSDK/Header/EDSDK.h"))
        .filter(|p| p.is_empty() || p.ends_with('/'))
        .map(str::to_owned)
}

fn read_version<R: Read + Seek>(zip: &mut ZipArchive<R>, names: &[String]) -> Option<String> {
    let note = names.iter().find(|n| base_name(n) == "ReleaseNote.txt")?;
    let mut text = String::new();
    zip.by_name(note).ok()?.read_to_string(&mut text).ok()?;
    text.lines().find_map(|line| {
        let rest = line.trim().strip_prefix("Ver ")?;
        Some(rest.split_whitespace().next()?.to_owned())
    })
}

fn extract_libraries<R: Read + Seek>(
    zip: &mut ZipArchive<R>,
    names: &[String],
    platform: Platform,
    staging: &Path,
) -> Result<(), String> {
    let prefix = sdk_prefix(names).unwrap_or_default();
    let mapping: &[(&str, &str)] = match platform {
        Platform::Linux => &[
            ("EDSDK/Header", "include"),
            ("EDSDK/Library/x86_64", "x86_64"),
            ("EDSDK/Library/ARM64", "aarch64"),
            ("EDSDK/Library/ARM32", "arm"),
        ],
        Platform::Windows => &[
            ("EDSDK/Header", "include"),
            ("EDSDK_64/Dll", "x86_64"),
            ("EDSDK_64/Library", "x86_64"),
            ("EDSDK/Dll", "x86"),
            ("EDSDK/Library", "x86"),
        ],
        Platform::Macos => unreachable!("macOS ships a disk image"),
    };
    for (source, dest) in mapping {
        let dir = format!("{prefix}{source}/");
        for name in names {
            let Some(file) = name.strip_prefix(&dir) else {
                continue;
            };
            if file.is_empty() || file.contains('/') {
                continue;
            }
            extract_entry(zip, name, &staging.join(dest).join(file))?;
        }
    }
    Ok(())
}

/// The macOS download is a zip holding `Macintosh.dmg.zip`, which holds the
/// disk image with the headers and `EDSDK.framework`.
fn extract_macos<R: Read + Seek>(
    zip: &mut ZipArchive<R>,
    names: &[String],
    edsdk: &Path,
    staging: &Path,
) -> Result<(), String> {
    let inner_name = names
        .iter()
        .find(|n| base_name(n) == "Macintosh.dmg.zip")
        .expect("checked by detect");
    let mut inner_bytes = Vec::new();
    zip.by_name(inner_name)
        .map_err(|e| format!("{inner_name}: {e}"))?
        .read_to_end(&mut inner_bytes)
        .map_err(|e| format!("{inner_name}: {e}"))?;
    let mut inner =
        ZipArchive::new(Cursor::new(inner_bytes)).map_err(|e| format!("{inner_name}: {e}"))?;
    let dmg_name = entry_names(&inner)?
        .into_iter()
        .find(|n| n.ends_with(".dmg") && !n.starts_with("__MACOSX/"))
        .ok_or_else(|| format!("{inner_name} holds no disk image"))?;

    let work = edsdk.join(".macos.work");
    remove_dir(&work)?;
    fs::create_dir_all(&work).map_err(io_err(&work))?;
    let result = (|| {
        let dmg = work.join("Macintosh.dmg");
        extract_entry(&mut inner, &dmg_name, &dmg)?;
        let volume = work.join("volume");
        let mounted = open_disk_image(&dmg, &volume)?;
        let copied = copy_from_volume(&volume, staging);
        if mounted {
            let _ = Command::new("hdiutil")
                .args(["detach", "-quiet"])
                .arg(&volume)
                .status();
        }
        copied
    })();
    let _ = fs::remove_dir_all(&work);
    result
}

/// Makes the disk image's contents available under `volume`. Returns whether
/// it was mounted (and so needs detaching) rather than extracted.
fn open_disk_image(dmg: &Path, volume: &Path) -> Result<bool, String> {
    if cfg!(target_os = "macos") {
        fs::create_dir_all(volume).map_err(io_err(volume))?;
        let status = Command::new("hdiutil")
            .args(["attach", "-quiet", "-nobrowse", "-readonly", "-mountpoint"])
            .arg(volume)
            .arg(dmg)
            .status()
            .map_err(|e| format!("failed to run hdiutil: {e}"))?;
        return if status.success() {
            Ok(true)
        } else {
            Err("hdiutil could not mount the EDSDK disk image".into())
        };
    }
    let mut output = std::ffi::OsString::from("-o");
    output.push(volume);
    let status = Command::new("7z")
        .args(["x", "-y", "-bso0", "-bsp0"])
        .arg(output)
        .arg(dmg)
        .status()
        .map_err(|e| {
            format!("failed to run 7z ({e}). Install 7-Zip to unpack the macOS disk image")
        })?;
    if status.success() {
        Ok(false)
    } else {
        Err("7z could not extract the EDSDK disk image".into())
    }
}

fn copy_from_volume(volume: &Path, staging: &Path) -> Result<(), String> {
    // A mounted image puts `EDSDK/` at the volume root; 7z extracts it under
    // a folder named after the volume.
    let mut candidates = vec![volume.to_owned()];
    if let Ok(entries) = fs::read_dir(volume) {
        candidates.extend(entries.filter_map(Result::ok).map(|e| e.path()));
    }
    let sdk = candidates
        .into_iter()
        .map(|dir| dir.join("EDSDK"))
        .find(|dir| dir.join("Framework/EDSDK.framework").exists())
        .ok_or("EDSDK/Framework/EDSDK.framework not found in the disk image")?;

    for header in HEADERS {
        let source = sdk.join("Header").join(header);
        let dest = staging.join("include").join(header);
        fs::create_dir_all(staging.join("include")).map_err(io_err(staging))?;
        fs::copy(&source, &dest).map_err(io_err(&source))?;
    }
    copy_tree(
        &sdk.join("Framework/EDSDK.framework"),
        &staging.join("EDSDK.framework"),
    )
}

/// Copies a directory, keeping symbolic links (frameworks rely on them) and
/// skipping Finder metadata.
fn copy_tree(source: &Path, dest: &Path) -> Result<(), String> {
    fs::create_dir_all(dest).map_err(io_err(dest))?;
    for entry in fs::read_dir(source).map_err(io_err(source))? {
        let entry = entry.map_err(io_err(source))?;
        let name = entry.file_name();
        if name == ".DS_Store" || name == ".fseventsd" {
            continue;
        }
        let from = entry.path();
        let to = dest.join(&name);
        let kind = entry.file_type().map_err(io_err(&from))?;
        if kind.is_symlink() {
            copy_symlink(&from, &to)?;
        } else if kind.is_dir() {
            copy_tree(&from, &to)?;
        } else {
            fs::copy(&from, &to).map_err(io_err(&from))?;
        }
    }
    Ok(())
}

#[cfg(unix)]
fn copy_symlink(from: &Path, to: &Path) -> Result<(), String> {
    let link = fs::read_link(from).map_err(io_err(from))?;
    std::os::unix::fs::symlink(link, to).map_err(io_err(to))
}

#[cfg(not(unix))]
fn copy_symlink(from: &Path, to: &Path) -> Result<(), String> {
    // Without symlinks, store what the link points at.
    if fs::metadata(from).map_err(io_err(from))?.is_dir() {
        copy_tree(from, to)
    } else {
        fs::copy(from, to).map(drop).map_err(io_err(from))
    }
}

fn extract_entry<R: Read + Seek>(
    zip: &mut ZipArchive<R>,
    name: &str,
    dest: &Path,
) -> Result<(), String> {
    let mut entry = zip.by_name(name).map_err(|e| format!("{name}: {e}"))?;
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(io_err(parent))?;
    }
    let mut out = File::create(dest).map_err(io_err(dest))?;
    io::copy(&mut entry, &mut out).map_err(|e| format!("{name}: {e}"))?;
    #[cfg(unix)]
    if let Some(mode) = entry.unix_mode() {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dest, fs::Permissions::from_mode((mode & 0o755) | 0o644))
            .map_err(io_err(dest))?;
    }
    Ok(())
}

fn verify(platform: Platform, staging: &Path) -> Result<(), String> {
    let missing: Vec<String> = platform
        .required()
        .into_iter()
        .filter(|file| !staging.join(file).exists())
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!("archive is missing {}", missing.join(", ")))
    }
}

fn remove_dir(path: &Path) -> Result<(), String> {
    match fs::remove_dir_all(path) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(format!("{}: {e}", path.display())),
        _ => Ok(()),
    }
}

fn entry_names<R: Read + Seek>(zip: &ZipArchive<R>) -> Result<Vec<String>, String> {
    zip.file_names()
        .map(|name| {
            name.map(|n| n.into_owned())
                .map_err(|e| format!("bad entry name: {e}"))
        })
        .collect()
}

fn base_name(name: &str) -> &str {
    name.rsplit('/').next().unwrap_or(name)
}

fn io_err(path: &Path) -> impl Fn(io::Error) -> String + '_ {
    move |e| format!("{}: {e}", path.display())
}

#[cfg(test)]
mod tests {
    use super::{Platform, detect, sdk_prefix};

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn detects_each_download() {
        let linux = names(&["EDSDK/Header/EDSDK.h", "EDSDK/Library/x86_64/libEDSDK.so"]);
        let windows = names(&["EDSDK/Header/EDSDK.h", "EDSDK_64/Dll/EDSDK.dll"]);
        let macos = names(&["readme.txt", "Macintosh.dmg.zip"]);
        assert!(matches!(detect(&linux), Some(Platform::Linux)));
        assert!(matches!(detect(&windows), Some(Platform::Windows)));
        assert!(matches!(detect(&macos), Some(Platform::Macos)));
        assert!(detect(&names(&["readme.txt"])).is_none());
    }

    #[test]
    fn finds_wrapped_sdk() {
        let wrapped = names(&["EDSDK_v13/EDSDK/Header/EDSDK.h"]);
        assert_eq!(sdk_prefix(&wrapped).as_deref(), Some("EDSDK_v13/"));
        let bogus = names(&["NOTEDSDK/Header/EDSDK.h"]);
        assert_eq!(sdk_prefix(&bogus), None);
    }
}
