//! Copying a whole project for Save Project As (F6, F7).

use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

use super::{CACHE_DIR, Error, LOCK_FILE, PROJECT_CONF, ProjectConf, Result, clean_name, disk};

/// Copies the project in `source` to the new directory `dest` and names the
/// copy `name`. `.cache/` is left out because it is rebuilt on demand (P9).
///
/// The copy is built in a temporary directory next to `dest` and renamed into
/// place once every file is flushed, so `dest` never holds a partial project
/// (F7). The source project is only read.
pub fn copy_project(source: &Path, dest: &Path, name: &str) -> Result<()> {
    let name = clean_name(name)?;
    if dest.exists() {
        return Err(Error::Invalid(format!(
            "{} already exists.",
            dest.display()
        )));
    }
    let parent = dest
        .parent()
        .ok_or_else(|| Error::Invalid(format!("{} has no parent folder.", dest.display())))?;
    disk::create_dir(parent)?;
    let temp = temp_dir(dest);

    let result = copy_dir(source, &temp, true).and_then(|()| {
        let conf_path = temp.join(PROJECT_CONF);
        let mut conf: ProjectConf = disk::read_config(&conf_path)?;
        conf.name = name;
        disk::write_config(&conf_path, &conf)?;
        disk::rename(&temp, dest)
    });
    if result.is_err() {
        // Only the unfinished copy is removed. The source is never touched.
        let _ = fs::remove_dir_all(&temp);
    }
    result
}

/// A hidden directory name next to `dest` that is not in use.
fn temp_dir(dest: &Path) -> PathBuf {
    let file_name = dest.file_name().unwrap_or_default().to_string_lossy();
    let mut attempt = 0u32;
    loop {
        let candidate = dest.with_file_name(format!(".{file_name}.partial{attempt}"));
        if !candidate.exists() {
            return candidate;
        }
        attempt += 1;
    }
}

fn copy_dir(from: &Path, to: &Path, is_root: bool) -> Result<()> {
    fs::create_dir(to).map_err(|e| Error::io(to, e))?;
    let entries = fs::read_dir(from).map_err(|e| Error::io(from, e))?;
    for entry in entries {
        let entry = entry.map_err(|e| Error::io(from, e))?;
        let name = entry.file_name();
        if is_root && (name == CACHE_DIR || name == LOCK_FILE) {
            continue;
        }
        let source = entry.path();
        let target = to.join(&name);
        let kind = entry.file_type().map_err(|e| Error::io(&source, e))?;
        if kind.is_dir() {
            copy_dir(&source, &target, false)?;
        } else {
            copy_file(&source, &target).map_err(|e| Error::io(&source, e))?;
        }
    }
    sync_dir(to).map_err(|e| Error::io(to, e))
}

fn copy_file(from: &Path, to: &Path) -> io::Result<()> {
    fs::copy(from, to)?;
    File::options().write(true).open(to)?.sync_all()
}

/// Flushes a directory's entries to disk. Only possible on Unix; Windows
/// flushes metadata on its own.
fn sync_dir(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}
