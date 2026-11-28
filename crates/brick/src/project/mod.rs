//! A project on disk: Project > Scene > Shot > Take > Frames (P1, P4).
//!
//! Every change made through [`Project`] is written to disk before the call
//! returns (P6), so the in-memory model always matches the project directory.

mod copy;
pub(crate) mod disk;

use std::fmt;
use std::fs::{File, TryLockError};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use crate::camera::{CameraSettings, Capture, Setting};

const PROJECT_CONF: &str = "project.conf";
const SCENE_CONF: &str = "scene.conf";
const SHOT_CONF: &str = "shot.conf";
const TAKE_CONF: &str = "take.conf";
/// Extension of the config file beside each test shot recording the camera
/// settings it was taken with (P15).
const TEST_CONF_EXTENSION: &str = "conf";
/// Test shots of a shot (P15).
const TESTS_DIR: &str = "tests";
/// File locked while a project is open, so only one instance can open it (P10).
const LOCK_FILE: &str = ".lock";
/// Generated data that can be rebuilt at any time (P9).
const CACHE_DIR: &str = ".cache";

pub use copy::copy_project;

/// Longest scene or shot name accepted, keeping directory paths well within
/// platform path limits.
pub const MAX_NAME_LEN: usize = 64;

/// A failure reading or changing the project.
#[derive(Debug)]
pub enum Error {
    Io { path: PathBuf, source: io::Error },
    Config { path: PathBuf, message: String },
    Invalid(String),
}

impl Error {
    fn io(path: &Path, source: io::Error) -> Self {
        Error::Io {
            path: path.to_owned(),
            source,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io { path, source } => write!(f, "{}: {source}", path.display()),
            Error::Config { path, message } => {
                write!(
                    f,
                    "{} is not a valid config file: {message}",
                    path.display()
                )
            }
            Error::Invalid(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

/// One of the supported animation frame rates (W1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u32", into = "u32")]
pub struct FrameRate(u32);

impl FrameRate {
    pub const SUPPORTED: [u32; 5] = [8, 12, 15, 24, 30];

    pub fn fps(self) -> u32 {
        self.0
    }
}

impl Default for FrameRate {
    /// 12 fps (P3).
    fn default() -> Self {
        FrameRate(12)
    }
}

impl TryFrom<u32> for FrameRate {
    type Error = String;

    fn try_from(fps: u32) -> std::result::Result<Self, String> {
        if Self::SUPPORTED.contains(&fps) {
            Ok(FrameRate(fps))
        } else {
            Err(format!(
                "{fps} fps is not supported, use one of {:?}",
                Self::SUPPORTED
            ))
        }
    }
}

impl From<FrameRate> for u32 {
    fn from(rate: FrameRate) -> u32 {
        rate.0
    }
}

/// Identifies a shot by the positions of its scene and itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShotId {
    pub scene: usize,
    pub shot: usize,
}

/// Identifies a take by its shot and its take number (DT2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TakeId {
    pub scene: usize,
    pub shot: usize,
    pub take: u32,
}

impl TakeId {
    pub fn shot_id(self) -> ShotId {
        ShotId {
            scene: self.scene,
            shot: self.shot,
        }
    }
}

/// Identifies a shot for as long as the project is open, even when shots are
/// reordered or renamed. Used for work that finishes later, such as a capture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ShotKey(u64);

impl ShotKey {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        ShotKey(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

impl ShotId {
    pub fn take(self, take: u32) -> TakeId {
        TakeId {
            scene: self.scene,
            shot: self.shot,
            take,
        }
    }
}

// Config file formats (P12).

#[derive(Serialize, Deserialize)]
struct ProjectConf {
    name: String,
    frame_rate: FrameRate,
    /// The active take (W7), by one based scene and shot position.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    active: Option<ActiveConf>,
}

#[derive(Serialize, Deserialize)]
struct ActiveConf {
    scene: usize,
    shot: usize,
    take: u32,
}

#[derive(Serialize, Deserialize)]
struct SceneConf {
    name: String,
    #[serde(default)]
    notes: String,
}

#[derive(Serialize, Deserialize)]
struct ShotConf {
    name: String,
    #[serde(default)]
    notes: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    chosen_take: Option<u32>,
    /// The number the next new take gets. Take numbers are never reused.
    #[serde(default = "first_number")]
    next_take: u32,
    /// The number the next test shot gets. Image numbers are never reused
    /// (P5).
    #[serde(default = "first_number")]
    next_test: u32,
    /// Camera settings applied whenever a take of this shot is active (CT7).
    #[serde(default, skip_serializing_if = "CameraSettings::is_empty")]
    camera: CameraSettings,
}

fn first_number() -> u32 {
    1
}

/// The config file beside a test shot (P15).
#[derive(Serialize, Deserialize)]
struct TestConf {
    /// The camera settings the test shot was taken with.
    #[serde(default)]
    camera: CameraSettings,
}

#[derive(Serialize, Deserialize)]
struct TakeConf {
    #[serde(default)]
    notes: String,
    /// Image numbers in playback order. A held frame repeats an image number
    /// (AT10).
    #[serde(default)]
    frames: Vec<u32>,
}

pub struct Scene {
    dir: String,
    name: String,
    notes: String,
    shots: Vec<Shot>,
}

impl Scene {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn notes(&self) -> &str {
        &self.notes
    }

    pub fn shots(&self) -> &[Shot] {
        &self.shots
    }

    fn conf(&self) -> SceneConf {
        SceneConf {
            name: self.name.clone(),
            notes: self.notes.clone(),
        }
    }
}

pub struct Shot {
    key: ShotKey,
    dir: String,
    name: String,
    notes: String,
    chosen_take: Option<u32>,
    next_take: u32,
    next_test: u32,
    camera: CameraSettings,
    /// Test shots in capture order (CT5).
    tests: Vec<TestShot>,
    takes: Vec<Take>,
}

impl Shot {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn notes(&self) -> &str {
        &self.notes
    }

    pub fn takes(&self) -> &[Take] {
        &self.takes
    }

    pub fn take(&self, number: u32) -> Option<&Take> {
        self.takes.iter().find(|t| t.number == number)
    }

    /// The take export uses by default (DT6).
    pub fn chosen_take(&self) -> Option<u32> {
        self.chosen_take
    }

    pub fn key(&self) -> ShotKey {
        self.key
    }

    /// The camera settings saved for this shot (CT7).
    pub fn camera(&self) -> &CameraSettings {
        &self.camera
    }

    /// Test shots, oldest first (CT5).
    pub fn tests(&self) -> &[TestShot] {
        &self.tests
    }

    pub fn test(&self, number: u32) -> Option<&TestShot> {
        self.tests.iter().find(|t| t.number == number)
    }

    fn conf(&self) -> ShotConf {
        ShotConf {
            name: self.name.clone(),
            notes: self.notes.clone(),
            chosen_take: self.chosen_take,
            next_take: self.next_take,
            next_test: self.next_test,
            camera: self.camera.clone(),
        }
    }
}

/// A test shot of a shot (CT5, P15).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestShot {
    number: u32,
    camera: CameraSettings,
}

impl TestShot {
    pub fn number(&self) -> u32 {
        self.number
    }

    /// The camera settings the test shot was taken with.
    pub fn camera(&self) -> &CameraSettings {
        &self.camera
    }
}

pub struct Take {
    number: u32,
    notes: String,
    frames: Vec<u32>,
}

impl Take {
    pub fn number(&self) -> u32 {
        self.number
    }

    pub fn notes(&self) -> &str {
        &self.notes
    }

    /// Image numbers in playback order, holds included.
    pub fn frames(&self) -> &[u32] {
        &self.frames
    }

    fn conf(&self) -> TakeConf {
        TakeConf {
            notes: self.notes.clone(),
            frames: self.frames.clone(),
        }
    }
}

pub struct Project {
    /// Held for as long as the project is open (P10). The operating system
    /// releases the lock when the file is dropped or the process exits.
    _lock: File,
    root: PathBuf,
    name: String,
    frame_rate: FrameRate,
    scenes: Vec<Scene>,
    active: Option<TakeId>,
}

impl Project {
    /// Creates an empty project in `root`, which may already exist but must
    /// not already hold a project.
    pub fn create(root: &Path, name: &str, frame_rate: FrameRate) -> Result<Self> {
        let name = clean_name(name)?;
        let conf_path = root.join(PROJECT_CONF);
        if conf_path.exists() {
            return Err(Error::Invalid(format!(
                "{} already contains a project",
                root.display()
            )));
        }
        disk::create_dir(root)?;
        let project = Project {
            _lock: lock(root)?,
            root: root.to_owned(),
            name,
            frame_rate,
            scenes: Vec::new(),
            active: None,
        };
        project.save_project_conf()?;
        Ok(project)
    }

    /// Whether `root` holds a project.
    pub fn exists(root: &Path) -> bool {
        root.join(PROJECT_CONF).is_file()
    }

    /// Opens the project in `root`. Directory names that do not match their
    /// position and name (P13), for example after a crash part way through a
    /// reorder, are corrected.
    pub fn open(root: &Path) -> Result<Self> {
        let lock = lock(root)?;
        let conf: ProjectConf = disk::read_config(&root.join(PROJECT_CONF))?;
        let mut scenes = Vec::new();
        for dir in disk::level_dirs(root, SCENE_CONF)? {
            scenes.push(load_scene(&root.join(&dir), dir)?);
        }
        let mut project = Project {
            _lock: lock,
            root: root.to_owned(),
            name: conf.name,
            frame_rate: conf.frame_rate,
            scenes,
            active: None,
        };
        project.active = conf.active.and_then(|a| {
            let id = TakeId {
                scene: a.scene.checked_sub(1)?,
                shot: a.shot.checked_sub(1)?,
                take: a.take,
            };
            project.take(id).map(|_| id)
        });
        project.sync_scene_dirs()?;
        for scene in 0..project.scenes.len() {
            project.sync_shot_dirs(scene)?;
        }
        Ok(project)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn frame_rate(&self) -> FrameRate {
        self.frame_rate
    }

    pub fn scenes(&self) -> &[Scene] {
        &self.scenes
    }

    pub fn scene(&self, scene: usize) -> Option<&Scene> {
        self.scenes.get(scene)
    }

    pub fn shot(&self, id: ShotId) -> Option<&Shot> {
        self.scenes.get(id.scene)?.shots.get(id.shot)
    }

    pub fn take(&self, id: TakeId) -> Option<&Take> {
        self.shot(id.shot_id())?.take(id.take)
    }

    /// The take that capture and playback work on (W7).
    pub fn active(&self) -> Option<TakeId> {
        self.active
    }

    /// The low resolution preview of the first frame of a take, used as its
    /// thumbnail (DT5).
    pub fn take_thumbnail(&self, id: TakeId) -> Option<PathBuf> {
        let image = *self.take(id)?.frames.first()?;
        Some(
            self.take_path(id)
                .join("frames/preview/low_res")
                .join(image_name(image, "jpg")),
        )
    }

    // Scenes

    /// Appends a scene and returns its position.
    pub fn add_scene(&mut self, name: &str) -> Result<usize> {
        let name = clean_name(name)?;
        let index = self.scenes.len();
        let scene = Scene {
            dir: disk::level_dir_name(index + 1, &name),
            name,
            notes: String::new(),
            shots: Vec::new(),
        };
        let path = self.root.join(&scene.dir);
        disk::create_dir(&path)?;
        disk::write_config(&path.join(SCENE_CONF), &scene.conf())?;
        self.scenes.push(scene);
        Ok(index)
    }

    pub fn rename_scene(&mut self, scene: usize, name: &str) -> Result<()> {
        let name = clean_name(name)?;
        let path = self.scene_path(scene);
        let entry = self.scene_mut(scene)?;
        let mut conf = entry.conf();
        conf.name = name.clone();
        disk::write_config(&path.join(SCENE_CONF), &conf)?;
        entry.name = name;
        self.sync_scene_dirs()
    }

    pub fn set_scene_notes(&mut self, scene: usize, notes: &str) -> Result<()> {
        let path = self.scene_path(scene);
        let entry = self.scene_mut(scene)?;
        let mut conf = entry.conf();
        conf.notes = notes.to_owned();
        disk::write_config(&path.join(SCENE_CONF), &conf)?;
        entry.notes = conf.notes;
        Ok(())
    }

    /// Moves the scene at `from` so it ends up at position `to`.
    pub fn move_scene(&mut self, from: usize, to: usize) -> Result<()> {
        if from >= self.scenes.len() || to >= self.scenes.len() {
            return Err(Error::Invalid("No such scene.".into()));
        }
        let scene = self.scenes.remove(from);
        self.scenes.insert(to, scene);
        if let Some(active) = &mut self.active {
            active.scene = moved_index(active.scene, from, to);
        }
        self.sync_scene_dirs()?;
        self.save_project_conf()
    }

    /// Moves a scene with all its shots and takes to the trash (P8).
    pub fn remove_scene(&mut self, scene: usize) -> Result<()> {
        let dir = PathBuf::from(&self.scene_mut(scene)?.dir);
        disk::move_to_trash(&self.root, &dir)?;
        self.scenes.remove(scene);
        self.active = self.active.and_then(|mut active| {
            active.scene = removed_index(active.scene, scene)?;
            Some(active)
        });
        self.sync_scene_dirs()?;
        self.save_project_conf()
    }

    // Shots

    /// Appends a shot to a scene and returns its id.
    pub fn add_shot(&mut self, scene: usize, name: &str) -> Result<ShotId> {
        let name = clean_name(name)?;
        let scene_path = self.scene_path(scene);
        let entry = self.scene_mut(scene)?;
        let index = entry.shots.len();
        let shot = Shot {
            key: ShotKey::new(),
            dir: disk::level_dir_name(index + 1, &name),
            name,
            notes: String::new(),
            chosen_take: None,
            next_take: 1,
            next_test: 1,
            camera: CameraSettings::default(),
            tests: Vec::new(),
            takes: Vec::new(),
        };
        let path = scene_path.join(&shot.dir);
        disk::create_dir(&path)?;
        disk::write_config(&path.join(SHOT_CONF), &shot.conf())?;
        entry.shots.push(shot);
        Ok(ShotId { scene, shot: index })
    }

    pub fn rename_shot(&mut self, id: ShotId, name: &str) -> Result<()> {
        let name = clean_name(name)?;
        self.update_shot(id, |shot| shot.name = name)?;
        self.sync_shot_dirs(id.scene)
    }

    pub fn set_shot_notes(&mut self, id: ShotId, notes: &str) -> Result<()> {
        self.update_shot(id, |shot| shot.notes = notes.to_owned())
    }

    /// Marks a take as the shot's chosen take, or clears it with `None` (DT6).
    pub fn set_chosen_take(&mut self, id: ShotId, take: Option<u32>) -> Result<()> {
        if let Some(number) = take
            && self.take(id.take(number)).is_none()
        {
            return Err(Error::Invalid("No such take.".into()));
        }
        self.update_shot(id, |shot| shot.chosen_take = take)
    }

    /// Finds the shot with `key`, wherever it has moved to.
    pub fn find_shot(&self, key: ShotKey) -> Option<ShotId> {
        self.scenes.iter().enumerate().find_map(|(scene, s)| {
            let shot = s.shots.iter().position(|shot| shot.key == key)?;
            Some(ShotId { scene, shot })
        })
    }

    /// Saves the camera settings for a shot (CT7).
    pub fn set_shot_camera(&mut self, id: ShotId, settings: &CameraSettings) -> Result<()> {
        if self.shot(id).is_some_and(|shot| shot.camera == *settings) {
            return Ok(());
        }
        self.update_shot(id, |shot| shot.camera = settings.clone())
    }

    /// Where a test shot's JPEG is stored (P15).
    pub fn test_shot_path(&self, id: ShotId, number: u32) -> PathBuf {
        self.root
            .join(self.shot_relative(id))
            .join(TESTS_DIR)
            .join(image_name(number, "jpg"))
    }

    /// Writes a captured test shot and the camera settings it was taken with
    /// to the shot's `tests/` folder and returns its number (CT5, P15). The
    /// files are flushed to disk before the test shot is listed (P7).
    pub fn add_test_shot(
        &mut self,
        id: ShotId,
        capture: &Capture,
        camera: &CameraSettings,
    ) -> Result<u32> {
        let shot = self
            .shot(id)
            .ok_or_else(|| Error::Invalid("No such shot.".into()))?;
        let number = shot.next_test;
        // Reserve the number before any file exists so a crash can never hand
        // it out twice (P5).
        self.update_shot(id, |shot| shot.next_test = number + 1)?;
        let dir = self.root.join(self.shot_relative(id)).join(TESTS_DIR);
        disk::create_dir(&dir)?;
        if let Some((extension, raw)) = &capture.raw {
            disk::write_image(&dir.join(image_name(number, extension)), raw)?;
        }
        let conf = TestConf {
            camera: camera.clone(),
        };
        disk::write_config(&dir.join(image_name(number, TEST_CONF_EXTENSION)), &conf)?;
        // The JPEG goes last: a test shot is listed once its JPEG exists.
        disk::write_image(&dir.join(image_name(number, "jpg")), &capture.jpeg)?;
        self.scenes[id.scene].shots[id.shot].tests.push(TestShot {
            number,
            camera: conf.camera,
        });
        Ok(number)
    }

    /// Makes the camera settings a test shot was taken with the shot's
    /// settings (CT11). Settings the test shot did not record are kept.
    pub fn use_test_settings(&mut self, id: ShotId, number: u32) -> Result<()> {
        let shot = self
            .shot(id)
            .ok_or_else(|| Error::Invalid("No such shot.".into()))?;
        let test = shot
            .test(number)
            .ok_or_else(|| Error::Invalid("No such test shot.".into()))?;
        let mut camera = shot.camera.clone();
        for setting in Setting::ALL {
            if let Some(value) = test.camera.get(setting) {
                camera.set(setting, Some(value.to_owned()));
            }
        }
        self.set_shot_camera(id, &camera)
    }

    /// Moves a shot within its scene so it ends up at position `to`.
    pub fn move_shot(&mut self, id: ShotId, to: usize) -> Result<()> {
        let scene = self.scene_mut(id.scene)?;
        if id.shot >= scene.shots.len() || to >= scene.shots.len() {
            return Err(Error::Invalid("No such shot.".into()));
        }
        let shot = scene.shots.remove(id.shot);
        scene.shots.insert(to, shot);
        if let Some(active) = &mut self.active
            && active.scene == id.scene
        {
            active.shot = moved_index(active.shot, id.shot, to);
        }
        self.sync_shot_dirs(id.scene)?;
        self.save_project_conf()
    }

    /// Moves a shot with all its takes to the trash (P8).
    pub fn remove_shot(&mut self, id: ShotId) -> Result<()> {
        self.shot(id)
            .ok_or_else(|| Error::Invalid("No such shot.".into()))?;
        disk::move_to_trash(&self.root, &self.shot_relative(id))?;
        self.scenes[id.scene].shots.remove(id.shot);
        self.active = self.active.and_then(|mut active| {
            if active.scene == id.scene {
                active.shot = removed_index(active.shot, id.shot)?;
            }
            Some(active)
        });
        self.sync_shot_dirs(id.scene)?;
        self.save_project_conf()
    }

    // Takes

    /// Starts a new take in a shot, numbered after the last take ever made in
    /// that shot (DT2).
    pub fn add_take(&mut self, id: ShotId) -> Result<TakeId> {
        let shot = self
            .shot(id)
            .ok_or_else(|| Error::Invalid("No such shot.".into()))?;
        let number = shot.next_take;
        // Reserve the number before the directory exists so a crash can never
        // hand it out twice.
        self.update_shot(id, |shot| shot.next_take = number + 1)?;
        let take = Take {
            number,
            notes: String::new(),
            frames: Vec::new(),
        };
        let take_id = id.take(number);
        let path = self.take_path(take_id);
        disk::create_dir(&path)?;
        disk::write_config(&path.join(TAKE_CONF), &take.conf())?;
        self.scenes[id.scene].shots[id.shot].takes.push(take);
        Ok(take_id)
    }

    pub fn set_take_notes(&mut self, id: TakeId, notes: &str) -> Result<()> {
        let path = self.take_path(id).join(TAKE_CONF);
        let take = self
            .scenes
            .get_mut(id.scene)
            .and_then(|s| s.shots.get_mut(id.shot))
            .and_then(|s| s.takes.iter_mut().find(|t| t.number == id.take))
            .ok_or_else(|| Error::Invalid("No such take.".into()))?;
        let mut conf = take.conf();
        conf.notes = notes.to_owned();
        disk::write_config(&path, &conf)?;
        take.notes = conf.notes;
        Ok(())
    }

    /// Makes a take the one capture and playback work on (W7, DT4).
    pub fn set_active(&mut self, id: Option<TakeId>) -> Result<()> {
        if let Some(id) = id
            && self.take(id).is_none()
        {
            return Err(Error::Invalid("No such take.".into()));
        }
        let previous = self.active;
        self.active = id;
        if let Err(e) = self.save_project_conf() {
            self.active = previous;
            return Err(e);
        }
        Ok(())
    }

    /// Moves a take and its frames to the trash (P8).
    pub fn remove_take(&mut self, id: TakeId) -> Result<()> {
        self.take(id)
            .ok_or_else(|| Error::Invalid("No such take.".into()))?;
        if self.shot(id.shot_id()).and_then(Shot::chosen_take) == Some(id.take) {
            self.update_shot(id.shot_id(), |shot| shot.chosen_take = None)?;
        }
        let relative = self
            .shot_relative(id.shot_id())
            .join(disk::take_dir_name(id.take));
        disk::move_to_trash(&self.root, &relative)?;
        self.scenes[id.scene].shots[id.shot]
            .takes
            .retain(|t| t.number != id.take);
        if self.active == Some(id) {
            self.active = None;
            self.save_project_conf()?;
        }
        Ok(())
    }

    // Internals

    fn scene_mut(&mut self, scene: usize) -> Result<&mut Scene> {
        self.scenes
            .get_mut(scene)
            .ok_or_else(|| Error::Invalid("No such scene.".into()))
    }

    fn scene_path(&self, scene: usize) -> PathBuf {
        match self.scenes.get(scene) {
            Some(s) => self.root.join(&s.dir),
            None => self.root.clone(),
        }
    }

    fn shot_relative(&self, id: ShotId) -> PathBuf {
        let scene = &self.scenes[id.scene];
        Path::new(&scene.dir).join(&scene.shots[id.shot].dir)
    }

    fn take_path(&self, id: TakeId) -> PathBuf {
        self.root
            .join(self.shot_relative(id.shot_id()))
            .join(disk::take_dir_name(id.take))
    }

    /// Applies `change` to a shot after writing it to `shot.conf`.
    fn update_shot(&mut self, id: ShotId, change: impl FnOnce(&mut Shot)) -> Result<()> {
        if self.shot(id).is_none() {
            return Err(Error::Invalid("No such shot.".into()));
        }
        let path = self.root.join(self.shot_relative(id)).join(SHOT_CONF);
        let shot = &mut self.scenes[id.scene].shots[id.shot];
        let old = shot.conf();
        change(shot);
        if let Err(e) = disk::write_config(&path, &shot.conf()) {
            shot.name = old.name;
            shot.notes = old.notes;
            shot.chosen_take = old.chosen_take;
            shot.next_take = old.next_take;
            shot.next_test = old.next_test;
            shot.camera = old.camera;
            return Err(e);
        }
        Ok(())
    }

    fn save_project_conf(&self) -> Result<()> {
        let conf = ProjectConf {
            name: self.name.clone(),
            frame_rate: self.frame_rate,
            active: self.active.map(|a| ActiveConf {
                scene: a.scene + 1,
                shot: a.shot + 1,
                take: a.take,
            }),
        };
        disk::write_config(&self.root.join(PROJECT_CONF), &conf)
    }

    fn sync_scene_dirs(&mut self) -> Result<()> {
        let wanted: Vec<String> = self
            .scenes
            .iter()
            .enumerate()
            .map(|(i, s)| disk::level_dir_name(i + 1, &s.name))
            .collect();
        let mut current: Vec<&mut String> = self.scenes.iter_mut().map(|s| &mut s.dir).collect();
        disk::sync_dir_names(&self.root, &mut current, &wanted)
    }

    fn sync_shot_dirs(&mut self, scene: usize) -> Result<()> {
        let path = self.scene_path(scene);
        let scene = self.scene_mut(scene)?;
        let wanted: Vec<String> = scene
            .shots
            .iter()
            .enumerate()
            .map(|(i, s)| disk::level_dir_name(i + 1, &s.name))
            .collect();
        let mut current: Vec<&mut String> = scene.shots.iter_mut().map(|s| &mut s.dir).collect();
        disk::sync_dir_names(&path, &mut current, &wanted)
    }
}

fn load_scene(path: &Path, dir: String) -> Result<Scene> {
    let conf: SceneConf = disk::read_config(&path.join(SCENE_CONF))?;
    let mut shots = Vec::new();
    for shot_dir in disk::level_dirs(path, SHOT_CONF)? {
        shots.push(load_shot(&path.join(&shot_dir), shot_dir)?);
    }
    Ok(Scene {
        dir,
        name: conf.name,
        notes: conf.notes,
        shots,
    })
}

fn load_shot(path: &Path, dir: String) -> Result<Shot> {
    let conf: ShotConf = disk::read_config(&path.join(SHOT_CONF))?;
    let mut takes = Vec::new();
    for take_dir in disk::level_dirs(path, TAKE_CONF)? {
        let Some(number) = disk::dir_number(&take_dir) else {
            continue;
        };
        if take_dir != disk::take_dir_name(number) {
            continue;
        }
        let take: TakeConf = disk::read_config(&path.join(&take_dir).join(TAKE_CONF))?;
        takes.push(Take {
            number,
            notes: take.notes,
            frames: take.frames,
        });
    }
    let highest = takes.iter().map(|t| t.number).max().unwrap_or(0);
    let chosen_take = conf
        .chosen_take
        .filter(|n| takes.iter().any(|t| t.number == *n));
    let tests_dir = path.join(TESTS_DIR);
    let mut tests = Vec::new();
    for number in disk::image_numbers(&tests_dir, "jpg")? {
        let conf = tests_dir.join(image_name(number, TEST_CONF_EXTENSION));
        // A test shot without its config file still shows; it just has no
        // settings to offer.
        let camera = if conf.exists() {
            disk::read_config::<TestConf>(&conf)?.camera
        } else {
            CameraSettings::default()
        };
        tests.push(TestShot { number, camera });
    }
    let highest_test = tests.last().map_or(0, |t| t.number);
    Ok(Shot {
        key: ShotKey::new(),
        dir,
        name: conf.name,
        notes: conf.notes,
        chosen_take,
        next_take: conf.next_take.max(highest + 1),
        next_test: conf.next_test.max(highest_test + 1),
        camera: conf.camera,
        tests,
        takes,
    })
}

/// The file name of a captured image: a six digit number (P5).
fn image_name(number: u32, extension: &str) -> String {
    format!("{number:06}.{extension}")
}

/// Takes the lock that keeps a second instance from opening the project
/// (P10).
fn lock(root: &Path) -> Result<File> {
    let path = root.join(LOCK_FILE);
    let file = File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .map_err(|e| Error::io(&path, e))?;
    match file.try_lock() {
        Ok(()) => Ok(file),
        Err(TryLockError::WouldBlock) => Err(Error::Invalid(format!(
            "{} is already open in Brick.",
            root.display()
        ))),
        Err(TryLockError::Error(e)) => Err(Error::io(&path, e)),
    }
}

/// The directory name for a new project called `name`: the name with
/// characters that are not letters, digits, `-`, or `_` replaced by
/// underscores (P4).
pub fn project_dir_name(name: &str) -> String {
    name.trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Trims a project, scene, or shot name and checks it can be used.
fn clean_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::Invalid("A name cannot be empty.".into()));
    }
    if name.chars().count() > MAX_NAME_LEN {
        return Err(Error::Invalid(format!(
            "A name can be at most {MAX_NAME_LEN} characters."
        )));
    }
    Ok(name.to_owned())
}

/// Where the item at `index` ends up after the item at `from` moves to `to`.
fn moved_index(index: usize, from: usize, to: usize) -> usize {
    if index == from {
        to
    } else if from < index && index <= to {
        index - 1
    } else if to <= index && index < from {
        index + 1
    } else {
        index
    }
}

/// Where the item at `index` ends up after the item at `removed` is removed,
/// or `None` if it was the removed item.
fn removed_index(index: usize, removed: usize) -> Option<usize> {
    match index.cmp(&removed) {
        std::cmp::Ordering::Less => Some(index),
        std::cmp::Ordering::Equal => None,
        std::cmp::Ordering::Greater => Some(index - 1),
    }
}

#[cfg(test)]
mod tests;
