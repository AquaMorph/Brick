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
                   "Could not create scene content for the reorder test.");
  marker.close();
  passed &= expect(created->moveScene(0, 1, &error),
                   "Scenes could not be reordered.");
  passed &= expect(created->scenes()[0] == "Closing Scene" &&
                       created->scenes()[1] == "Opening Scene",
                   "Scene reorder did not update the project order.");
  passed &= expect(
      QFile::exists(QDir(projectPath).filePath(
          "0002_OPENING_SCENE/marker.txt")),
      "Scene reorder did not preserve scene contents.");

  passed &= expect(created->deleteScene(0, &error),
                   "A scene could not be deleted.");
  passed &= expect(created->scenes().size() == 1 &&
                       created->scenes()[0] == "Opening Scene",
                   "Scene deletion did not update the project.");
  passed &= expect(
      QFile::exists(QDir(projectPath).filePath(
          "0001_OPENING_SCENE/marker.txt")),
      "Scene deletion did not renumber the remaining scene.");
  passed &= expect(
      !QFile::exists(QDir(projectPath).filePath("0001_CLOSING_SCENE")),
      "Scene deletion left the deleted scene folder behind.");

  opened = Project::open(projectPath, &error);
  passed &= expect(opened.has_value() && opened->scenes().size() == 1 &&
                       opened->scenes()[0] == "Opening Scene",
                   "The scene order was not restored when reopening the project.");

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
