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
  auto created = Project::create(temporaryDirectory.path(), "Example Film", &error);
  if (!expect(created.has_value(), "A valid project was not created.")) {
    std::cerr << error.toStdString() << '\n';
    return 1;
  }

  const QString projectPath = QDir(temporaryDirectory.path()).filePath("Example Film");
  bool passed = true;
  passed &= expect(created->name() == "Example Film",
                   "The created project has the wrong name.");
  passed &= expect(QFile::exists(QDir(projectPath).filePath("project.conf")),
                   "Project creation did not write project.conf.");

  auto opened = Project::open(projectPath, &error);
  passed &= expect(opened.has_value(), "The created project could not be opened.");
  if (opened.has_value()) {
    passed &= expect(opened->name() == "Example Film",
                     "The opened project has the wrong name.");
  }

  passed &= expect(!Project::create(temporaryDirectory.path(), "Example Film", &error)
                        .has_value(),
                   "Creation should reject an existing project folder.");
  passed &= expect(!Project::create(temporaryDirectory.path(), "bad/name", &error)
                        .has_value(),
                   "Creation should reject path separators in project names.");
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
