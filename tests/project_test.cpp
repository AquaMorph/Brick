#include "project.h"

#include <QCoreApplication>
#include <QDir>
#include <QFile>
#include <QSettings>
#include <QTemporaryDir>

#include <iostream>

namespace {

bool expect(bool condition, const char* message) {
  if (!condition) {
    std::cerr << message << '\n';
  }
  return condition;
}

}  // namespace


int main(int argc, char* argv[]) {
  QCoreApplication application(argc, argv);
  QTemporaryDir temporaryDirectory;
  if (!expect(temporaryDirectory.isValid(),
              "Could not create the temporary test directory.")) {
    return 1;
  }

  QString error;
  auto created =
      Project::create(temporaryDirectory.path(), "Example Film", &error);
  if (!expect(created.has_value(), "A valid project was not created.")) {
    std::cerr << error.toStdString() << '\n';
    return 1;
  }

  const QString projectPath =
      QDir(temporaryDirectory.path()).filePath("Example_Film");
  bool passed = true;
  passed &= expect(created->name() == "Example Film",
                   "The created project has the wrong name.");
  passed &= expect(QFile::exists(QDir(projectPath).filePath("project.conf")),
                   "Project creation did not write project.conf.");
  passed &= expect(!projectPath.contains(' '),
                   "The generated project folder contains a space.");

  auto opened = Project::open(projectPath, &error);
  passed &= expect(opened.has_value(), "The created project could not be opened.");
  if (opened.has_value()) {
    passed &= expect(opened->name() == "Example Film",
                      "The opened project has the wrong name.");
  }

  passed &= expect(created->createScene("Opening Scene", &error),
                   "A valid scene was not created.");
  passed &= expect(created->createScene("Closing Scene", &error),
                   "A second valid scene was not created.");
  passed &= expect(created->scenes().size() == 2,
                   "The project has the wrong scene count.");
  passed &= expect(
      QFile::exists(QDir(projectPath).filePath(
          "0001_OPENING_SCENE/scene.conf")),
      "Scene creation did not write the first scene.conf.");
  passed &= expect(
      !created->createScene("Opening Scene", &error),
      "Scene creation should reject duplicate names.");
  passed &= expect(!created->createScene("Bad/Scene", &error),
                   "Scene creation should reject path separators.");

  QFile marker(QDir(projectPath).filePath("0001_OPENING_SCENE/marker.txt"));
  passed &= expect(marker.open(QIODevice::WriteOnly),
                    "Could not create scene content for the rename test.");
  marker.close();
  passed &= expect(created->renameScene(0, "First Scene", &error),
                    "A scene could not be renamed.");
  passed &= expect(created->scenes()[0] == "First Scene",
                    "Scene rename did not update the project.");
  passed &= expect(
      QFile::exists(QDir(projectPath).filePath("0001_FIRST_SCENE/marker.txt")),
      "Scene rename did not preserve scene contents.");
  passed &= expect(
      !QFile::exists(QDir(projectPath).filePath("0001_OPENING_SCENE")),
      "Scene rename left the old scene folder behind.");
  {
    QSettings renamedConfig(
        QDir(projectPath).filePath("0001_FIRST_SCENE/scene.conf"),
        QSettings::IniFormat);
    passed &= expect(
        renamedConfig.value("Scene/name").toString() == "First Scene",
        "Scene rename did not update scene.conf.");
    passed &= expect(
        !renamedConfig.value("Scene/createdUtc").toString().isEmpty(),
        "Scene rename removed the creation timestamp.");
  }
  passed &= expect(!created->renameScene(1, "First Scene", &error),
                    "Scene rename should reject duplicate names.");
  passed &= expect(!created->renameScene(0, "Bad/Scene", &error),
                    "Scene rename should reject path separators.");
  passed &= expect(!created->renameScene(-1, "Missing Scene", &error),
                    "Scene rename should reject invalid indexes.");
  passed &= expect(created->renameScene(0, "first scene", &error),
                    "A case-only scene rename failed.");
  passed &= expect(
      QFile::exists(QDir(projectPath).filePath("0001_FIRST_SCENE/marker.txt")),
      "A case-only scene rename moved or lost scene contents.");

  passed &= expect(created->createShot(0, "Wide Shot", &error),
                   "A valid shot was not created.");
  passed &= expect(created->createShot(0, "Establishing Shot", &error),
                   "A second valid shot was not created.");
  passed &= expect(created->shots(0).size() == 2,
                   "The scene has the wrong shot count.");
  passed &= expect(
      QFile::exists(QDir(projectPath).filePath(
          "0001_FIRST_SCENE/0001_WIDE_SHOT/shot.conf")),
      "Shot creation did not write shot.conf.");
  passed &= expect(
      QDir(QDir(projectPath).filePath(
               "0001_FIRST_SCENE/0001_WIDE_SHOT"))
          .exists("test_shots"),
      "Shot creation did not create the test shots folder.");
  passed &= expect(created->takeCount(0, 0) == 1,
                   "Shot creation did not add its first take.");
  passed &= expect(
      QFile::exists(QDir(projectPath).filePath(
          "0001_FIRST_SCENE/0001_WIDE_SHOT/0001_TAKE/take.conf")),
      "Shot creation did not write the first take.conf.");
  passed &= expect(
      QDir(QDir(projectPath).filePath(
               "0001_FIRST_SCENE/0001_WIDE_SHOT/0001_TAKE"))
          .exists("frames/preview/high_res") &&
          QDir(QDir(projectPath).filePath(
               "0001_FIRST_SCENE/0001_WIDE_SHOT/0001_TAKE"))
              .exists("frames/preview/low_res") &&
          QDir(QDir(projectPath).filePath(
               "0001_FIRST_SCENE/0001_WIDE_SHOT/0001_TAKE"))
              .exists("frames/RAW"),
      "Shot creation did not create the first take's frame folders.");
  passed &= expect(created->createTake(0, 0, &error),
                   "A second take was not created.");
  passed &= expect(created->takeCount(0, 0) == 2,
                   "Take creation did not update the shot.");
  passed &= expect(
      QFile::exists(QDir(projectPath).filePath(
          "0001_FIRST_SCENE/0001_WIDE_SHOT/0002_TAKE/take.conf")),
      "Take creation did not write take.conf.");
  passed &= expect(created->createTake(0, 0, &error),
                    "A third take was not created.");
  passed &= expect(created->selectTake(0, 0, 2, &error),
                    "A take could not be selected.");
  passed &= expect(created->activeTake() == Project::ActiveTake{0, 0, 2},
                    "Selecting a take did not update the active take.");
  {
    QSettings projectConfig(QDir(projectPath).filePath("project.conf"),
                            QSettings::IniFormat);
    passed &= expect(projectConfig.value("Project/activeSceneIndex").toInt() == 0 &&
                         projectConfig.value("Project/activeShotIndex").toInt() == 0 &&
                         projectConfig.value("Project/activeTakeIndex").toInt() == 2,
                     "Selecting a take was not saved in project.conf.");
  }
  opened = Project::open(projectPath, &error);
  passed &= expect(opened.has_value() &&
                       opened->activeTake() == Project::ActiveTake{0, 0, 2},
                    "The active take was not restored when reopening.");
  passed &= expect(!created->selectTake(0, 0, 3, &error),
                    "Selecting an invalid take should fail.");
  QFile takeMarker(QDir(projectPath).filePath(
      "0001_FIRST_SCENE/0001_WIDE_SHOT/0003_TAKE/marker.txt"));
  passed &= expect(takeMarker.open(QIODevice::WriteOnly),
                   "Could not create take content for the deletion test.");
  takeMarker.close();
  passed &= expect(created->deleteTake(0, 0, 1, &error),
                   "A take could not be deleted.");
  passed &= expect(created->takeCount(0, 0) == 2,
                    "Take deletion did not update the shot.");
  passed &= expect(created->activeTake() == Project::ActiveTake{0, 0, 1},
                    "Take deletion did not renumber the active take.");
  passed &= expect(
      QFile::exists(QDir(projectPath).filePath(
          "0001_FIRST_SCENE/0001_WIDE_SHOT/0002_TAKE/marker.txt")),
      "Take deletion did not renumber or preserve the remaining take.");
  passed &= expect(
      !QFile::exists(QDir(projectPath).filePath(
          "0001_FIRST_SCENE/0001_WIDE_SHOT/0003_TAKE")),
      "Take deletion left the old take folder behind.");
  passed &= expect(!created->deleteTake(0, 0, 2, &error),
                   "Take deletion should reject invalid indexes.");
  passed &= expect(!created->createShot(0, "Wide Shot", &error),
                   "Shot creation should reject duplicate names.");
  passed &= expect(!created->createShot(0, "Bad/Shot", &error),
                   "Shot creation should reject path separators.");

  QFile shotMarker(QDir(projectPath).filePath(
      "0001_FIRST_SCENE/0001_WIDE_SHOT/marker.txt"));
  passed &= expect(shotMarker.open(QIODevice::WriteOnly),
                   "Could not create shot content for the rename test.");
  shotMarker.close();
  passed &= expect(created->renameShot(0, 0, "Close Up", &error),
                   "A shot could not be renamed.");
  passed &= expect(created->shots(0)[0] == "Close Up",
                   "Shot rename did not update the scene.");
  passed &= expect(
      QFile::exists(QDir(projectPath).filePath(
          "0001_FIRST_SCENE/0001_CLOSE_UP/marker.txt")),
      "Shot rename did not preserve shot contents.");
  passed &= expect(!created->renameShot(0, 1, "Close Up", &error),
                   "Shot rename should reject duplicate names.");
  passed &= expect(!created->renameShot(0, 0, "Bad/Shot", &error),
                   "Shot rename should reject path separators.");
  passed &= expect(!created->renameShot(-1, 0, "Missing Shot", &error),
                   "Shot rename should reject invalid scenes.");

  passed &= expect(created->moveShot(0, 0, 1, &error),
                   "Shots could not be reordered.");
  passed &= expect(created->shots(0)[0] == "Establishing Shot" &&
                       created->shots(0)[1] == "Close Up",
                   "Shot reorder did not update the shot order.");
  passed &= expect(
      QFile::exists(QDir(projectPath).filePath(
          "0001_FIRST_SCENE/0002_CLOSE_UP/marker.txt")),
      "Shot reorder did not preserve shot contents.");
  passed &= expect(created->takeCount(0, 1) == 2 &&
                       QFile::exists(QDir(projectPath).filePath(
                           "0001_FIRST_SCENE/0002_CLOSE_UP/"
                           "0002_TAKE/take.conf")),
                    "Shot reorder did not preserve its takes.");
  passed &= expect(created->activeTake() == Project::ActiveTake{0, 1, 1},
                    "Shot reorder did not move the active take.");

  passed &= expect(created->deleteShot(0, 0, &error),
                   "A shot could not be deleted.");
  passed &= expect(created->shots(0).size() == 1 &&
                       created->shots(0)[0] == "Close Up",
                    "Shot deletion did not update the shot order.");
  passed &= expect(created->activeTake() == Project::ActiveTake{0, 0, 1},
                    "Shot deletion did not renumber the active take.");
  passed &= expect(
      QFile::exists(QDir(projectPath).filePath(
          "0001_FIRST_SCENE/0001_CLOSE_UP/marker.txt")),
      "Shot deletion did not renumber the remaining shot.");

  opened = Project::open(projectPath, &error);
  passed &= expect(opened.has_value() && opened->scenes()[0] == "first scene" &&
                        opened->shots(0).size() == 1 &&
                        opened->shots(0)[0] == "Close Up" &&
                        opened->takeCount(0, 0) == 2,
                    "The scene, shots, and takes were not restored when reopening.");

  passed &= expect(created->moveScene(0, 1, &error),
                    "Scenes could not be reordered.");
  passed &= expect(created->scenes()[0] == "Closing Scene" &&
                        created->scenes()[1] == "first scene",
                    "Scene reorder did not update the project order.");
  passed &= expect(created->activeTake() == Project::ActiveTake{1, 0, 1},
                    "Scene reorder did not move the active take.");
  passed &= expect(
      QFile::exists(QDir(projectPath).filePath(
          "0002_FIRST_SCENE/marker.txt")),
      "Scene reorder did not preserve scene contents.");
  passed &= expect(
      QFile::exists(QDir(projectPath).filePath(
          "0002_FIRST_SCENE/0001_CLOSE_UP/marker.txt")),
      "Scene reorder did not preserve its shots.");
  passed &= expect(
      QFile::exists(QDir(projectPath).filePath(
          "0002_FIRST_SCENE/0001_CLOSE_UP/0002_TAKE/take.conf")),
      "Scene reorder did not preserve its takes.");

  passed &= expect(created->deleteScene(0, &error),
                   "A scene could not be deleted.");
  passed &= expect(created->scenes().size() == 1 &&
                        created->scenes()[0] == "first scene",
                    "Scene deletion did not update the project.");
  passed &= expect(created->activeTake() == Project::ActiveTake{0, 0, 1},
                    "Scene deletion did not renumber the active take.");
  passed &= expect(
      QFile::exists(QDir(projectPath).filePath(
          "0001_FIRST_SCENE/marker.txt")),
      "Scene deletion did not renumber the remaining scene.");
  passed &= expect(
      QFile::exists(QDir(projectPath).filePath(
          "0001_FIRST_SCENE/0001_CLOSE_UP/marker.txt")),
      "Scene deletion did not preserve the remaining scene's shots.");
  passed &= expect(
      QFile::exists(QDir(projectPath).filePath(
          "0001_FIRST_SCENE/0001_CLOSE_UP/0002_TAKE/take.conf")),
      "Scene deletion did not preserve the remaining shot's takes.");
  passed &= expect(
      !QFile::exists(QDir(projectPath).filePath("0001_CLOSING_SCENE")),
      "Scene deletion left the deleted scene folder behind.");

  opened = Project::open(projectPath, &error);
  passed &= expect(opened.has_value() && opened->scenes().size() == 1 &&
                        opened->scenes()[0] == "first scene" &&
                        opened->shots(0).size() == 1 &&
                        opened->shots(0)[0] == "Close Up" &&
                        opened->takeCount(0, 0) == 2,
                    "The scene, shot, and take order was not restored when reopening.");

  passed &= expect(!Project::create(temporaryDirectory.path(), "Example Film",
                                    &error)
                         .has_value(),
                   "Creation should reject an existing project folder.");
  passed &= expect(!Project::create(temporaryDirectory.path(), "bad/name", &error)
                        .has_value(),
                   "Creation should reject path separators in project names.");
  passed &= expect(!Project::create(temporaryDirectory.path(), "Film-2", &error)
                        .has_value(),
                   "Creation should reject non-alphanumeric punctuation.");
  passed &= expect(!Project::open(temporaryDirectory.path(), &error).has_value(),
                   "Opening should reject a folder without project.conf.");

  QSettings config(QDir(projectPath).filePath("project.conf"),
                   QSettings::IniFormat);
  config.setValue("Project/formatVersion", 999);
  config.sync();
  passed &= expect(!Project::open(projectPath, &error).has_value(),
                   "Opening should reject unsupported project formats.");

  return passed ? 0 : 1;
}
