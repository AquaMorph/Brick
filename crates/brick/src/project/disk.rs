//! Low level file system helpers for the project directory: atomic config
//! writes (P6), directory naming (P13), and the project trash (P8).

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde::de::DeserializeOwned;

use super::Error;

/// Name of the project trash directory (P8).
pub const TRASH_DIR: &str = ".trash";

/// Suffix of a directory caught halfway through a rename by [`sync_dir_names`].
const MOVING_SUFFIX: &str = ".moving";

/// Reads and parses a TOML config file.
pub fn read_config<T: DeserializeOwned>(path: &Path) -> Result<T, Error> {
    let text = fs::read_to_string(path).map_err(|e| Error::io(path, e))?;
    toml::from_str(&text).map_err(|e| Error::Config {
        path: path.to_owned(),
        message: e.to_string(),
    })
}

/// Writes a TOML config file atomically: the data goes to a temporary file
/// that is flushed and then renamed over the target, so the target is always
/// either the old or the new contents (P6).
pub fn write_config<T: Serialize>(path: &Path, value: &T) -> Result<(), Error> {
    let text = toml::to_string(value).map_err(|e| Error::Config {
        path: path.to_owned(),
        message: e.to_string(),
    })?;
    write_atomic(path, text.as_bytes()).map_err(|e| Error::io(path, e))
}

fn write_atomic(path: &Path, contents: &[u8]) -> io::Result<()> {
    let mut tmp_name = path.file_name().unwrap_or_default().to_owned();
    tmp_name.push(".tmp");
    let tmp = path.with_file_name(tmp_name);
    {
        let mut file = File::create(&tmp)?;
        file.write_all(contents)?;
        file.sync_all()?;
    }
    fs::rename(&tmp, path)?;
    sync_parent(path)
}

/// Writes a captured image and flushes it to disk before returning (P7). The
/// image is written under a temporary name first, so a crash never leaves a
/// partial image under its real name. An existing image is never replaced
/// (P5).
pub fn write_image(path: &Path, contents: &[u8]) -> Result<(), Error> {
    if path.exists() {
        return Err(Error::io(
            path,
            io::Error::new(io::ErrorKind::AlreadyExists, "already exists"),
        ));
    }
    write_atomic(path, contents).map_err(|e| Error::io(path, e))
}

/// The numbers of the captured images in `dir` with `extension`, in order
/// (P5). A missing directory has no images.
pub fn image_numbers(dir: &Path, extension: &str) -> Result<Vec<u32>, Error> {
    Ok(image_files(dir)?
        .into_iter()
        .filter(|(_, ext)| ext == extension)
        .map(|(number, _)| number)
        .collect())
}

/// The captured images in `dir` as their number and extension, ordered by
/// number (P5). Files not named like a captured image are skipped. A missing
/// directory has no images.
pub fn image_files(dir: &Path) -> Result<Vec<(u32, String)>, Error> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(Error::io(dir, e)),
    };
    let mut images = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| Error::io(dir, e))?;
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        let Some((stem, ext)) = name.split_once('.') else {
            continue;
        };
        if stem.len() == 6
            && stem.bytes().all(|b| b.is_ascii_digit())
            && !ext.is_empty()
            && !ext.contains('.')
            && let Ok(number) = stem.parse::<u32>()
        {
            images.push((number, ext.to_owned()));
        }
    }
    images.sort_unstable();
    Ok(images)
}

/// Creates a directory and flushes the new entry to disk.
pub fn create_dir(path: &Path) -> Result<(), Error> {
    fs::create_dir_all(path).map_err(|e| Error::io(path, e))?;
    sync_parent(path).map_err(|e| Error::io(path, e))
}

/// Renames a directory and flushes the change to disk.
pub fn rename(from: &Path, to: &Path) -> Result<(), Error> {
    if to.exists() {
        return Err(Error::io(
            to,
            io::Error::new(io::ErrorKind::AlreadyExists, "already exists"),
        ));
    }
    fs::rename(from, to).map_err(|e| Error::io(from, e))?;
    sync_parent(to).map_err(|e| Error::io(to, e))
}

/// Flushes the directory entry of `path` so a rename or create survives power
/// loss (R13). Only possible on Unix; Windows flushes metadata on its own.
pub fn sync_parent(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    if let Some(parent) = path.parent() {
        File::open(parent)?.sync_all()?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

/// The directory name of a scene or shot: `NNNN_NAME` (P13).
pub fn level_dir_name(position: usize, name: &str) -> String {
    let name: String = name
        .trim()
        .chars()
        .flat_map(char::to_uppercase)
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    format!("{position:04}_{name}")
}

/// The directory name of a take: `NNNN_TAKE` (P13).
pub fn take_dir_name(number: u32) -> String {
    format!("{number:04}_TAKE")
}

/// The position or take number a level directory name starts with.
pub fn dir_number(name: &str) -> Option<u32> {
    let digits = name.split('_').next()?;
    if digits.len() == 4 && digits.bytes().all(|b| b.is_ascii_digit()) {
        digits.parse().ok()
    } else {
        None
    }
}

/// Subdirectories of `parent` that contain `config`, ordered by the number
/// their name starts with. Hidden directories (`.trash`, `.cache`) are skipped.
pub fn level_dirs(parent: &Path, config: &str) -> Result<Vec<String>, Error> {
    let entries = fs::read_dir(parent).map_err(|e| Error::io(parent, e))?;
    let mut dirs = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| Error::io(parent, e))?;
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if name.starts_with('.') || !entry.path().join(config).is_file() {
            continue;
        }
        dirs.push(name);
    }
    dirs.sort_by(|a, b| {
        let key = |n: &str| dir_number(n).unwrap_or(u32::MAX);
        key(a).cmp(&key(b)).then_with(|| a.cmp(b))
    });
    Ok(dirs)
}

/// Renames the directories in `parent` so each `current` name becomes its
/// `wanted` name. Renames go through a temporary name first so two
/// directories can swap names. After a crash part way through, every
/// directory still holds its config file and keeps a number prefix, so the
/// project opens and the names are fixed on the next sync (R13).
///
/// `current` is updated as each rename lands, so it matches the disk even when
/// an error stops the sync early.
pub fn sync_dir_names(
    parent: &Path,
    current: &mut [&mut String],
    wanted: &[String],
) -> Result<(), Error> {
    debug_assert_eq!(current.len(), wanted.len());
    let mut moving = Vec::new();
    for (index, (name, want)) in current.iter_mut().zip(wanted).enumerate() {
        if **name != *want {
            let temp = format!("{want}{MOVING_SUFFIX}");
            rename(&parent.join(&**name), &parent.join(&temp))?;
            **name = temp;
            moving.push(index);
        }
    }
    for index in moving {
        rename(&parent.join(&*current[index]), &parent.join(&wanted[index]))?;
        *current[index] = wanted[index].clone();
    }
    Ok(())
}

/// Moves a scene, shot, or take directory into the project trash (P8). The
/// directory keeps its path relative to the project root under a time stamped
/// folder, so it can be identified later.
pub fn move_to_trash(root: &Path, relative: &Path) -> Result<PathBuf, Error> {
    let mut moved = move_all_to_trash(root, &[relative.to_owned()])?;
    Ok(moved.remove(0))
}

/// Moves several files or directories into one time stamped folder in the
/// project trash (P8) and returns where each one went. If one cannot be
/// moved, the ones already moved are put back.
pub fn move_all_to_trash(root: &Path, relative: &[PathBuf]) -> Result<Vec<PathBuf>, Error> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let trash = root.join(TRASH_DIR);
    let mut stamp = millis;
    let mut batch = trash.join(stamp.to_string());
    while batch.exists() {
        stamp += 1;
        batch = trash.join(stamp.to_string());
    }
    let mut moved: Vec<PathBuf> = Vec::new();
    for path in relative {
        let target = batch.join(path);
        let result = match target.parent() {
            Some(parent) => create_dir(parent),
            None => Ok(()),
        }
        .and_then(|()| rename(&root.join(path), &target));
        if let Err(e) = result {
            for (path, target) in relative.iter().zip(&moved) {
                let _ = rename(target, &root.join(path));
            }
            return Err(e);
        }
        moved.push(target);
    }
    Ok(moved)
}

/// Moves a file out of the trash back to `to`, creating its directory if it
/// is gone. An existing file is never replaced.
pub fn restore_from_trash(from: &Path, to: &Path) -> Result<(), Error> {
    if let Some(parent) = to.parent() {
        create_dir(parent)?;
    }
    rename(from, to)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_names_are_upper_case_with_underscores() {
        assert_eq!(level_dir_name(1, "Scene One"), "0001_SCENE_ONE");
        assert_eq!(level_dir_name(12, " car/chase! 2 "), "0012_CAR_CHASE__2");
        assert_eq!(level_dir_name(3, "café"), "0003_CAFÉ");
        assert_eq!(take_dir_name(7), "0007_TAKE");
    }

    #[test]
    fn dir_numbers_need_four_digits() {
        assert_eq!(dir_number("0012_CAR"), Some(12));
        assert_eq!(dir_number("0003_TAKE.moving"), Some(3));
        assert_eq!(dir_number("12_CAR"), None);
        assert_eq!(dir_number("CAR"), None);
    }

    #[test]
    fn sync_swaps_names() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("0001_A")).unwrap();
        fs::create_dir(dir.path().join("0002_B")).unwrap();
        fs::write(dir.path().join("0001_A/marker"), "a").unwrap();

        let mut a = "0001_A".to_owned();
        let mut b = "0002_B".to_owned();
        let wanted = ["0002_A".to_owned(), "0001_B".to_owned()];
        sync_dir_names(dir.path(), &mut [&mut a, &mut b], &wanted).unwrap();

        assert_eq!([a, b], wanted);
        assert!(dir.path().join("0002_A/marker").is_file());
        assert!(dir.path().join("0001_B").is_dir());
    }

    #[test]
    fn image_files_keep_their_extension() {
        let dir = tempfile::tempdir().unwrap();
        for name in [
            "000002.cr3",
            "000001.jpg",
            "000001.conf",
            "12.jpg",
            "notes.txt",
        ] {
            fs::write(dir.path().join(name), "x").unwrap();
        }
        assert_eq!(
            image_files(dir.path()).unwrap(),
            [
                (1, "conf".to_owned()),
                (1, "jpg".to_owned()),
                (2, "cr3".to_owned())
            ]
        );
        assert_eq!(image_numbers(dir.path(), "jpg").unwrap(), [1]);
        assert!(image_files(&dir.path().join("missing")).unwrap().is_empty());
    }

    #[test]
    fn trash_batches_can_be_restored() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("a/b")).unwrap();
        fs::write(root.join("a/b/1.jpg"), "1").unwrap();
        fs::write(root.join("a/2.jpg"), "2").unwrap();
        let files = [PathBuf::from("a/b/1.jpg"), PathBuf::from("a/2.jpg")];
        let moved = move_all_to_trash(root, &files).unwrap();
        assert!(!root.join("a/b/1.jpg").exists());
        assert_eq!(moved[0].parent().unwrap().parent(), moved[1].parent());
        assert_eq!(fs::read_to_string(&moved[1]).unwrap(), "2");

        // A file missing part way through puts the others back.
        fs::write(root.join("a/3.jpg"), "3").unwrap();
        let missing = [PathBuf::from("a/3.jpg"), PathBuf::from("a/4.jpg")];
        assert!(move_all_to_trash(root, &missing).is_err());
        assert!(root.join("a/3.jpg").is_file());

        fs::remove_dir(root.join("a/b")).unwrap();
        restore_from_trash(&moved[0], &root.join("a/b/1.jpg")).unwrap();
        assert_eq!(fs::read_to_string(root.join("a/b/1.jpg")).unwrap(), "1");
        assert!(restore_from_trash(&moved[1], &root.join("a/3.jpg")).is_err());
    }
}
