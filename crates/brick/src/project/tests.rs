use std::fs;

use super::*;

fn new_project() -> (tempfile::TempDir, Project) {
    let dir = tempfile::tempdir().unwrap();
    let project = Project::create(&dir.path().join("Film"), "Film", FrameRate::default()).unwrap();
    (dir, project)
}

fn dirs(path: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(path)
        .unwrap()
        .map(|e| e.unwrap())
        .filter(|e| e.file_type().unwrap().is_dir())
        .map(|e| e.file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}

#[test]
fn hierarchy_round_trips_through_disk() {
    let (_dir, mut project) = new_project();
    let scene = project.add_scene("Scene One").unwrap();
    project.set_scene_notes(scene, "Opening").unwrap();
    let shot = project.add_shot(scene, "Wide").unwrap();
    project.set_shot_notes(shot, "Dolly in").unwrap();
    let take = project.add_take(shot).unwrap();
    project.set_take_notes(take, "Good").unwrap();
    project.set_chosen_take(shot, Some(take.take)).unwrap();
    project.set_active(Some(take)).unwrap();

    let root = project.root().to_owned();
    assert!(
        root.join("0001_SCENE_ONE/0001_WIDE/0001_TAKE/take.conf")
            .is_file()
    );

    drop(project);
    let reopened = Project::open(&root).unwrap();
    assert_eq!(reopened.name(), "Film");
    assert_eq!(reopened.frame_rate().fps(), 12);
    assert_eq!(reopened.scenes()[0].name(), "Scene One");
    assert_eq!(reopened.scenes()[0].notes(), "Opening");
    let shot = reopened.shot(shot).unwrap();
    assert_eq!(shot.name(), "Wide");
    assert_eq!(shot.notes(), "Dolly in");
    assert_eq!(shot.chosen_take(), Some(1));
    assert_eq!(reopened.take(take).unwrap().notes(), "Good");
    assert_eq!(reopened.active(), Some(take));
}

#[test]
fn create_refuses_existing_project() {
    let (_dir, project) = new_project();
    assert!(Project::create(project.root(), "Again", FrameRate::default()).is_err());
}

#[test]
fn unsupported_frame_rate_is_rejected() {
    assert!(FrameRate::try_from(25).is_err());
    assert_eq!(FrameRate::try_from(24).unwrap().fps(), 24);
}

#[test]
fn rename_and_reorder_rename_directories() {
    let (_dir, mut project) = new_project();
    project.add_scene("A").unwrap();
    let b = project.add_scene("B").unwrap();
    let shot = project.add_shot(b, "Close").unwrap();
    let take = project.add_take(shot).unwrap();
    project.set_active(Some(take)).unwrap();

    project.move_scene(b, 0).unwrap();
    assert_eq!(dirs(project.root()), ["0001_B", "0002_A"]);
    assert_eq!(project.active().unwrap().scene, 0);

    project.rename_scene(0, "Big b").unwrap();
    assert_eq!(dirs(project.root()), ["0001_BIG_B", "0002_A"]);
    assert!(
        project
            .root()
            .join("0001_BIG_B/0001_CLOSE/0001_TAKE")
            .is_dir()
    );

    let root = project.root().to_owned();
    drop(project);
    let reopened = Project::open(&root).unwrap();
    assert_eq!(reopened.scenes()[0].name(), "Big b");
    assert_eq!(reopened.active().unwrap().scene, 0);
}

#[test]
fn shots_reorder_within_their_scene() {
    let (_dir, mut project) = new_project();
    let scene = project.add_scene("S").unwrap();
    project.add_shot(scene, "One").unwrap();
    let two = project.add_shot(scene, "Two").unwrap();
    project.add_shot(scene, "Three").unwrap();

    project.move_shot(two, 2).unwrap();
    let names: Vec<_> = project.scenes()[0].shots().iter().map(Shot::name).collect();
    assert_eq!(names, ["One", "Three", "Two"]);
    assert_eq!(
        dirs(&project.root().join("0001_S")),
        ["0001_ONE", "0002_THREE", "0003_TWO"]
    );
}

#[test]
fn removing_moves_to_trash_and_renumbers() {
    let (_dir, mut project) = new_project();
    let a = project.add_scene("A").unwrap();
    project.add_scene("B").unwrap();
    let shot = project.add_shot(a, "Wide").unwrap();
    let take = project.add_take(shot).unwrap();
    project.set_active(Some(take)).unwrap();

    project.remove_scene(a).unwrap();
    assert_eq!(project.active(), None);
    assert_eq!(dirs(project.root()), [".trash", "0001_B"]);

    let trash = project.root().join(disk::TRASH_DIR);
    let batch = fs::read_dir(&trash)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    assert!(batch.join("0001_A/0001_WIDE/0001_TAKE/take.conf").is_file());
}

#[test]
fn take_numbers_are_never_reused() {
    let (_dir, mut project) = new_project();
    let scene = project.add_scene("S").unwrap();
    let shot = project.add_shot(scene, "Wide").unwrap();
    project.add_take(shot).unwrap();
    let second = project.add_take(shot).unwrap();
    project.set_chosen_take(shot, Some(second.take)).unwrap();

    project.remove_take(second).unwrap();
    assert_eq!(project.shot(shot).unwrap().chosen_take(), None);

    let third = project.add_take(shot).unwrap();
    assert_eq!(third.take, 3);

    let root = project.root().to_owned();
    drop(project);
    let reopened = Project::open(&root).unwrap();
    let numbers: Vec<_> = reopened
        .shot(shot)
        .unwrap()
        .takes()
        .iter()
        .map(Take::number)
        .collect();
    assert_eq!(numbers, [1, 3]);
}

#[test]
fn open_repairs_interrupted_rename() {
    let (_dir, mut project) = new_project();
    project.add_scene("A").unwrap();
    project.add_scene("B").unwrap();
    let root = project.root().to_owned();
    // A crash after the first rename of moving B in front of A.
    fs::rename(root.join("0002_B"), root.join("0001_B.moving")).unwrap();
    drop(project);

    let reopened = Project::open(&root).unwrap();
    let names: Vec<_> = reopened.scenes().iter().map(Scene::name).collect();
    assert_eq!(names, ["A", "B"]);
    assert_eq!(dirs(&root), ["0001_A", "0002_B"]);
}

#[test]
fn names_are_validated() {
    let (_dir, mut project) = new_project();
    assert!(project.add_scene("   ").is_err());
    assert!(project.add_scene(&"x".repeat(MAX_NAME_LEN + 1)).is_err());
    assert_eq!(project.scenes().len(), 0);
}

#[test]
fn index_helpers() {
    assert_eq!(moved_index(0, 0, 2), 2);
    assert_eq!(moved_index(1, 0, 2), 0);
    assert_eq!(moved_index(2, 0, 2), 1);
    assert_eq!(moved_index(3, 0, 2), 3);
    assert_eq!(moved_index(0, 2, 0), 1);
    assert_eq!(removed_index(1, 1), None);
    assert_eq!(removed_index(2, 1), Some(1));
}

#[test]
fn an_open_project_cannot_be_opened_again() {
    let (_dir, project) = new_project();
    let root = project.root().to_owned();
    assert!(Project::open(&root).is_err());
    drop(project);
    assert!(Project::open(&root).is_ok());
}

#[test]
fn project_dir_names_replace_unsafe_characters() {
    assert_eq!(project_dir_name(" My Film: Part 2 "), "My_Film__Part_2");
    assert_eq!(project_dir_name("clay-test_1"), "clay-test_1");
}

#[test]
fn copy_includes_trash_but_not_cache() {
    let (dir, mut project) = new_project();
    let a = project.add_scene("A").unwrap();
    project.add_scene("B").unwrap();
    project.remove_scene(a).unwrap();
    let root = project.root().to_owned();
    fs::create_dir(root.join(CACHE_DIR)).unwrap();
    fs::write(root.join(CACHE_DIR).join("thumb.jpg"), "x").unwrap();

    let dest = dir.path().join("Copy");
    copy_project(&root, &dest, "Copy").unwrap();

    assert_eq!(dirs(&dest), [".trash", "0001_B"]);
    assert!(!dest.join(LOCK_FILE).exists());
    // The source is unchanged and still open.
    assert_eq!(dirs(&root), [".cache", ".trash", "0001_B"]);
    assert_eq!(project.name(), "Film");

    let copy = Project::open(&dest).unwrap();
    assert_eq!(copy.name(), "Copy");
    assert_eq!(copy.scenes()[0].name(), "B");
    // No temporary directory is left behind.
    assert_eq!(dirs(dir.path()), ["Copy", "Film"]);
}

#[test]
fn copy_refuses_existing_destination() {
    let (dir, project) = new_project();
    let dest = dir.path().join("Taken");
    fs::create_dir(&dest).unwrap();
    fs::write(dest.join("keep"), "x").unwrap();

    assert!(copy_project(project.root(), &dest, "Taken").is_err());
    assert!(dest.join("keep").is_file());
    assert_eq!(dirs(dir.path()), ["Film", "Taken"]);
}
