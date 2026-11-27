//! Application settings that are not project data, stored in the operating
//! system's config directory (P11).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::project::{self, disk};

/// How many projects Open Recent and the project picker remember (F5, P2).
const MAX_RECENT: usize = 10;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentProject {
    pub path: PathBuf,
    pub name: String,
}

impl RecentProject {
    /// Whether the project is still on disk.
    pub fn exists(&self) -> bool {
        project::Project::exists(&self.path)
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct SettingsFile {
    /// Most recently opened first.
    #[serde(default)]
    recent: Vec<RecentProject>,
}

pub struct Settings {
    /// Where the settings are saved, or `None` when the platform has no
    /// config directory.
    path: Option<PathBuf>,
    file: SettingsFile,
}

impl Settings {
    /// Loads the settings from the config directory. A missing file gives the
    /// defaults. A file that cannot be read gives the defaults and the error.
    pub fn load() -> (Self, Option<project::Error>) {
        let path = directories::ProjectDirs::from("", "", "Brick")
            .map(|dirs| dirs.config_dir().join("settings.toml"));
        Self::load_from(path)
    }

    fn load_from(path: Option<PathBuf>) -> (Self, Option<project::Error>) {
        let (file, error) = match &path {
            Some(p) if p.exists() => match disk::read_config(p) {
                Ok(file) => (file, None),
                Err(e) => (SettingsFile::default(), Some(e)),
            },
            _ => (SettingsFile::default(), None),
        };
        (Self { path, file }, error)
    }

    /// Recently opened projects, most recent first.
    pub fn recent(&self) -> &[RecentProject] {
        &self.file.recent
    }

    /// Moves `path` to the front of the recent projects.
    pub fn add_recent(&mut self, path: &Path, name: &str) -> project::Result<()> {
        self.file.recent.retain(|r| r.path != path);
        self.file.recent.insert(
            0,
            RecentProject {
                path: path.to_owned(),
                name: name.to_owned(),
            },
        );
        self.file.recent.truncate(MAX_RECENT);
        self.save()
    }

    /// Forgets every recent project. Project data is not touched (F5).
    pub fn clear_recent(&mut self) -> project::Result<()> {
        self.file.recent.clear();
        self.save()
    }

    fn save(&self) -> project::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            disk::create_dir(parent)?;
        }
        disk::write_config(path, &self.file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_projects_are_most_recent_first_and_unique() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("brick/settings.toml");
        let (mut settings, error) = Settings::load_from(Some(path.clone()));
        assert!(error.is_none());

        settings.add_recent(Path::new("/a"), "A").unwrap();
        settings.add_recent(Path::new("/b"), "B").unwrap();
        settings.add_recent(Path::new("/a"), "A2").unwrap();

        let (reloaded, _) = Settings::load_from(Some(path.clone()));
        let names: Vec<_> = reloaded.recent().iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["A2", "B"]);

        settings.clear_recent().unwrap();
        let (reloaded, _) = Settings::load_from(Some(path));
        assert!(reloaded.recent().is_empty());
    }

    #[test]
    fn recent_list_is_capped() {
        let (mut settings, _) = Settings::load_from(None);
        for i in 0..MAX_RECENT + 3 {
            settings
                .add_recent(&PathBuf::from(format!("/p{i}")), "P")
                .unwrap();
        }
        assert_eq!(settings.recent().len(), MAX_RECENT);
        assert_eq!(settings.recent()[0].path, Path::new("/p12"));
    }
}
