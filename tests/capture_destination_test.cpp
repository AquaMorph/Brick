#include "core/project.h"

#include <QCoreApplication>
#include <QDir>
#include <QSettings>
#include <QTemporaryDir>
#include <QUuid>

#include <iostream>
#include <stdexcept>

namespace {

void require(bool condition, const char* message) {
  if (!condition) {
    throw std::runtime_error(message);
  }
}

void setIdentity(const QString& path, const QString& key, const QString& id) {
  QSettings config(path, QSettings::IniFormat);
  if (id.isEmpty()) {
    config.remove(key);
  } else {
    config.setValue(key, id);
  }
  config.sync();
  require(config.status() == QSettings::NoError, "Could not edit identity fixture.");
}

}  // namespace


int main(int argc, char* argv[]) {
  QCoreApplication application(argc, argv);
  QCoreApplication::setOrganizationName("Brick");
  QCoreApplication::setApplicationName("CaptureDestinationTests");
  QString error;
  try {
    QTemporaryDir temporary;
    require(temporary.isValid(), "Could not create temporary directory.");
    QSettings::setDefaultFormat(QSettings::IniFormat);
    QSettings::setPath(QSettings::IniFormat, QSettings::UserScope, temporary.path());
    auto project = Project::create(temporary.path(), "Film", &error);
    require(project.has_value(), "Could not create project.");
    require(project->createScene("First", &error) &&
                project->createScene("Second", &error) &&
                project->createShot(0, "Target", &error) &&
                project->createShot(0, "Other", &error) &&
                project->createTake(0, 0, &error),
            "Could not create hierarchy.");
    const auto frame = project->captureDestination(0, 0, 1, &error);
    const auto test = project->captureDestination(0, 0, -1, &error);
    require(frame && test, "Could not capture destinations.");
    require(!QUuid(frame->projectId).isNull() && !QUuid(frame->shotId).isNull() &&
                !QUuid(frame->takeId).isNull() && test->takeId.isEmpty() &&
                frame->shotId == test->shotId,
            "Destinations do not contain UUIDs or the test sentinel.");
    require(!project->captureDestination(-1, 0, 0, &error) &&
                !project->captureDestination(0, -1, 0, &error) &&
                !project->captureDestination(0, 0, -2, &error) &&
                !project->captureDestination(0, 0, 2, &error),
            "Invalid capture indexes were accepted.");

    const auto stale = *project;
    require(project->selectTake(0, 0, 1, &error) &&
                project->saveTakeFrameRate(0, 0, 1, 12, &error) &&
                project->saveCurrentShotCameraSettings(0, 0, {}, &error) &&
                project->renameScene(0, "Renamed Scene", &error) &&
                project->renameShot(0, 0, "Renamed Shot", &error) &&
                project->moveShot(0, 0, 1, &error) &&
                project->moveScene(0, 1, &error) &&
                project->deleteTake(1, 1, 0, &error),
            "Could not mutate hierarchy.");
    const auto moved = project->captureDestination(1, 1, 0, &error);
    require(moved && moved->projectId == frame->projectId &&
                moved->shotId == frame->shotId && moved->takeId == frame->takeId,
            "Mutations did not preserve the in-memory identities.");
    require(stale.resolveCaptureDestination(*frame, &error) ==
                Project::ActiveTake{1, 1, 0},
            "Resolution used cached hierarchy instead of disk.");
    project = Project::open(frame->projectDirectory, &error);
    require(project && project->resolveCaptureDestination(*frame, &error) ==
                           Project::ActiveTake{1, 1, 0} &&
                project->resolveCaptureDestination(*test, &error) ==
                    Project::ActiveTake{1, 1, -1},
            "Identities did not survive rename, reorder, renumber and reopen.");

    // Reject missing/malformed identities anywhere, not just at the destination.
    const QString projectConfig = QDir(project->directory()).filePath("project.conf");
    const QString shotConfig = QDir(project->shotDirectory(1, 1)).filePath("shot.conf");
    const QString takeConfig = QDir(project->takeDirectory(1, 1, 0)).filePath("take.conf");
    struct IdentityFixture { QString path; QString key; QString id; };
    for (const auto& fixture : {
             IdentityFixture{projectConfig, "Project/id", frame->projectId},
             IdentityFixture{shotConfig, "Shot/id", frame->shotId},
             IdentityFixture{takeConfig, "Take/id", frame->takeId}}) {
      for (const QString& invalid : {QString{}, QString{"invalid"},
                                    QUuid{}.toString(QUuid::WithoutBraces)}) {
        setIdentity(fixture.path, fixture.key, invalid);
        error.clear();
        require(!project->resolveCaptureDestination(*frame, &error) && !error.isEmpty() &&
                    !Project::open(project->directory(), &error),
                "Missing or malformed identity was accepted.");
      }
      setIdentity(fixture.path, fixture.key, fixture.id);
    }
    const auto other = project->captureDestination(1, 0, 0, &error);
    require(other.has_value(), "Could not capture other destination.");
    const QString otherShot = QDir(project->shotDirectory(1, 0)).filePath("shot.conf");
    const QString otherTake = QDir(project->takeDirectory(1, 0, 0)).filePath("take.conf");
    setIdentity(otherShot, "Shot/id", frame->shotId);
    require(!project->resolveCaptureDestination(*test, &error),
            "Duplicate shot identity was accepted.");
    setIdentity(otherShot, "Shot/id", other->shotId);
    setIdentity(otherTake, "Take/id", frame->takeId);
    require(!project->resolveCaptureDestination(*frame, &error),
            "Duplicate take identity across shots was accepted.");
    setIdentity(otherTake, "Take/id", other->takeId);
    auto wrongParent = *frame;
    wrongParent.shotId = other->shotId;
    require(!project->resolveCaptureDestination(wrongParent, &error),
            "Take was resolved under the wrong shot.");

    require(project->deleteTake(1, 1, 0, &error) && project->createTake(1, 1, &error),
            "Could not recreate take at the same path.");
    project = Project::open(frame->projectDirectory, &error);
    require(project && !project->resolveCaptureDestination(*frame, &error) &&
                project->resolveCaptureDestination(*test, &error).has_value(),
            "Recreated take inherited identity or test destination depended on take.");
    require(project->deleteShot(1, 1, &error) &&
                project->createShot(1, "Renamed Shot", &error),
            "Could not recreate shot at the same path.");
    project = Project::open(test->projectDirectory, &error);
    require(project && !project->resolveCaptureDestination(*test, &error) &&
                !stale.captureDestination(0, 0, -1, &error),
            "Recreated shot inherited identity.");
    const auto beforeSceneDeletion = project->captureDestination(1, 1, -1, &error);
    require(beforeSceneDeletion.has_value(), "Could not capture recreated shot.");
    require(project->deleteScene(1, &error) &&
                project->createScene("Renamed Scene", &error) &&
                project->createShot(1, "Other", &error) &&
                project->createShot(1, "Renamed Shot", &error),
            "Could not recreate scene at the same path.");
    project = Project::open(test->projectDirectory, &error);
    require(project && !project->resolveCaptureDestination(*beforeSceneDeletion, &error),
            "Recreated scene inherited shot identity.");

    const auto beforeProjectDeletion = project->captureDestination(1, 1, 0, &error);
    require(beforeProjectDeletion.has_value(), "Could not capture final destination.");
    const auto oldProject = *project;
    require(QDir(project->directory()).removeRecursively(), "Could not delete project.");
    project = Project::create(temporary.path(), "Film", &error);
    require(project && project->createScene("First", &error) &&
                project->createShot(0, "Target", &error), "Could not recreate project.");
    // Even copied child IDs cannot make a replacement project the original.
    setIdentity(QDir(project->shotDirectory(0, 0)).filePath("shot.conf"),
                "Shot/id", beforeProjectDeletion->shotId);
    setIdentity(QDir(project->takeDirectory(0, 0, 0)).filePath("take.conf"),
                "Take/id", beforeProjectDeletion->takeId);
    project = Project::open(beforeProjectDeletion->projectDirectory, &error);
    require(project && !project->resolveCaptureDestination(*beforeProjectDeletion, &error) &&
                !oldProject.resolveCaptureDestination(*beforeProjectDeletion, &error),
            "Recreated project inherited identity.");
  } catch (const std::exception& failure) {
    std::cerr << failure.what() << '\n' << error.toStdString() << '\n';
    return 1;
  }
  return 0;
}
