#include "storage/capture_recovery.h"

#include <QCoreApplication>
#include <QDir>
#include <QFile>
#include <QImage>
#include <QJsonDocument>
#include <QJsonObject>
#include <QProcess>
#include <QSaveFile>
#include <QSettings>
#include <QTemporaryDir>

#include <iostream>
#include <stdexcept>

namespace {

void require(bool condition, const char* message) {
  if (!condition) {
    throw std::runtime_error(message);
  }
}

void write(const QString& path, const QByteArray& bytes) {
  QSaveFile file(path);
  require(file.open(QIODevice::WriteOnly) && file.write(bytes) == bytes.size() &&
              file.commit(), "Could not write fixture.");
}

void state(const QString& job, const QString& value) {
  QFile file(QDir(job).filePath("manifest.json"));
  require(file.open(QIODevice::ReadOnly), "Could not read manifest.");
  auto m = QJsonDocument::fromJson(file.readAll()).object();
  file.close();
  m["state"] = value;
  write(file.fileName(), QJsonDocument(m).toJson());
}

QString capture(CaptureRecovery& recovery, CapturePurpose purpose,
                const CaptureDestination& destination, const TestShot& metadata,
                QString* error) {
  const QString base = recovery.begin(purpose, destination, metadata, error);
  require(!base.isEmpty(), "Could not begin capture.");
  const QString job = QFileInfo(base).absolutePath();
  require(QFileInfo::exists(QDir(job).filePath("manifest.json")),
          "Manifest was not written before capture.");
  QImage image(16, 16, QImage::Format_RGB32);
  image.fill(Qt::red);
  require(image.save(base + ".jpg"), "Could not write image.");
  require(recovery.markReady(job, base + ".jpg", error), "Could not mark ready.");
  recovery.release(job);
  return job;
}

}  // namespace


int main(int argc, char* argv[]) {
  QCoreApplication application(argc, argv);
  QString error;
  try {
    if (application.arguments().size() == 4) {
      CaptureRecovery recovery(application.arguments()[1]);
      auto project = Project::open(application.arguments()[2], &error);
      require(project.has_value(), "Child could not open project.");
      auto destination = project->captureDestination(0, 1, -1, &error);
      require(destination.has_value(), "Child could not resolve destination.");
      const QString base = recovery.begin(CapturePurpose::TestShot, *destination, {}, &error);
      require(!base.isEmpty(), "Child could not begin capture.");
      write(base + ".jpg", "interrupted camera bytes");
      if (application.arguments()[3] == "ready") {
        require(recovery.markReady(QFileInfo(base).absolutePath(), base + ".jpg", &error),
                "Child could not mark ready.");
      }
      std::cout << base.toStdString() << std::endl;
      return application.exec();
    }
    QTemporaryDir temporary;
    require(temporary.isValid(), "Could not create temporary directory.");
    QSettings::setDefaultFormat(QSettings::IniFormat);
    QSettings::setPath(QSettings::IniFormat, QSettings::UserScope, temporary.path());
    auto project = Project::create(temporary.path(), "Film", &error);
    require(project && project->createScene("Scene", &error) &&
                project->createShot(0, "Shot", &error) &&
                project->createShot(0, "Other", &error), "Could not create project.");
    const auto test = project->captureDestination(0, 0, -1, &error);
    const auto frame = project->captureDestination(0, 0, 0, &error);
    require(test && frame, "Missing destination.");
    TestShot metadata;
    metadata.capturedUtc = QDateTime::fromString("2026-09-01T12:34:56.123Z", Qt::ISODateWithMs);
    metadata.cameraDisplayName = "Fake Camera";
    metadata.cameraBackend = "fake";
    metadata.cameraDeviceId = "device";
    metadata.capturedSettings = {{"iso", "100"}};
    metadata.displaySettings = {{"iso", "ISO 100"}};
    const QString root = temporary.filePath("recovery");
    QString job;
    {
      CaptureRecovery recovery(root);
      job = capture(recovery, CapturePurpose::TestShot, *test, metadata, &error);
    }
    CaptureRecovery recovery(root);
    require(recovery.captures().size() == 1 && recovery.captures()[0].ready,
            "Ready capture did not survive restart.");
    const QString movedProject = temporary.filePath("offline");
    require(QDir().rename(project->directory(), movedProject), "Could not move project offline.");
    require(!recovery.retry(job, &error) && QFileInfo::exists(job) &&
                recovery.captures()[0].ready, "Failed resolution lost capture or retryability.");
    require(QDir().rename(movedProject, test->projectDirectory), "Could not restore project.");
    require(project->renameShot(0, 0, "Renamed", &error) &&
                project->moveShot(0, 0, 1, &error), "Could not move shot.");
    require(recovery.retry(job, &error) && !QFileInfo::exists(job), "Retry did not clean up.");
    auto imported = project->testShots(0, 1, &error);
    require(imported.size() == 1 && imported[0].capturedUtc == metadata.capturedUtc &&
                imported[0].cameraDisplayName == metadata.cameraDisplayName &&
                imported[0].cameraBackend == metadata.cameraBackend &&
                imported[0].cameraDeviceId == metadata.cameraDeviceId &&
                imported[0].capturedSettings == metadata.capturedSettings &&
                imported[0].displaySettings == metadata.displaySettings,
            "Original identity or metadata was not preserved.");
    require(!recovery.retry(job, &error), "Removed job was retried.");

    job = capture(recovery, CapturePurpose::AnimationFrame, *frame, metadata, &error);
    require(recovery.retry(job, &error), "Frame import failed.");
    const auto frames = project->frames(0, 1, 0, &error);
    require(frames.size() == 1 && frames[0].capturedUtc == metadata.capturedUtc,
            "Frame timestamp was lost.");

    job = capture(recovery, CapturePurpose::TestShot, *test, metadata, &error);
    state(job, "importing");
    CaptureRecovery restarted(root);
    require(!restarted.retry(job, &error) && error.contains("duplicates") &&
                !restarted.captures()[0].ready, "Ambiguous import was retried.");
    const QString saved = temporary.filePath("saved.jpg");
    require(restarted.saveElsewhere(job, saved, &error) && QFileInfo::exists(job) &&
                QFileInfo::exists(saved), "Save elsewhere lost originals.");
    require(!restarted.saveElsewhere(job, saved, &error), "Save overwrote an existing file.");
    require(restarted.discard(job, &error) && QFileInfo::exists(saved), "Discard removed saved image.");

    // Unknown sidecars must survive successful import, even after restart.
    job = capture(recovery, CapturePurpose::TestShot, *test, metadata, &error);
    write(QDir(job).filePath("capture.raw"), "orphan raw");
    require(recovery.retry(job, &error) && QFileInfo::exists(job), "Orphan asset was auto-deleted.");
    require(!restarted.retry(job, &error), "Committed job with retained assets was retried.");
    const QString exports = temporary.filePath("exports");
    require(QDir().mkdir(exports) && restarted.saveElsewhere(job, exports, &error) &&
                QFileInfo::exists(QDir(exports).filePath("capture.raw")), "Multiple assets not saved.");
    require(restarted.discard(job, &error), "Could not discard committed job.");

    // A cleanup failure must not convert a committed job back to retryable.
    job = capture(recovery, CapturePurpose::TestShot, *test, metadata, &error);
    require(QDir(job).mkdir("unexpected"), "Could not create cleanup blocker.");
    require(recovery.retry(job, &error) && !restarted.retry(job, &error) &&
                !restarted.discard(job, &error), "Unsafe cleanup or duplicate retry.");
    require(QDir(job).rmdir("unexpected") && restarted.discard(job, &error),
            "Could not clean up blocked job.");

    // Active locks protect jobs from another recovery instance as well.
    QString base = recovery.begin(CapturePurpose::TestShot, *test, metadata, &error);
    require(!base.isEmpty(), "Could not begin incomplete capture.");
    job = QFileInfo(base).absolutePath();
    write(base + ".jpg", "partial image");
    require(restarted.captures().empty() && !restarted.retry(job, &error) &&
                !restarted.saveElsewhere(job, exports, &error) &&
                !restarted.discard(job, &error), "Active job was available for recovery.");
    recovery.release(job);
    require(restarted.captures().size() == 1 && !restarted.captures()[0].ready &&
                !restarted.retry(job, &error), "Incomplete job disappeared or became retryable.");
    require(restarted.saveElsewhere(job, temporary.filePath("partial.jpg"), &error) &&
                restarted.discard(job, &error), "Incomplete asset not recoverable.");

    // Kill a writer without destructors to exercise stale locks and on-disk state.
    for (const QString& mode : {QString("capturing"), QString("ready")}) {
      QProcess writer;
      writer.start(QCoreApplication::applicationFilePath(), {root, test->projectDirectory, mode});
      require(writer.waitForStarted() && writer.waitForReadyRead(), "Writer process did not start.");
      const QString childBase = QString::fromUtf8(writer.readAllStandardOutput()).trimmed();
      require(!childBase.isEmpty() && restarted.captures().empty(), "Live writer was exposed.");
      writer.kill();
      require(writer.waitForFinished(), "Could not stop writer process.");
      const auto survivors = restarted.captures();
      require(survivors.size() == 1 && survivors[0].ready == (mode == "ready") &&
                  QFileInfo::exists(childBase + ".jpg"), "Crash lost assets or manifest state.");
      require(restarted.discard(survivors[0].directory, &error), "Stale process lock blocked recovery.");
    }

    const QString orphan = QDir(root).filePath("orphan");
    require(QDir().mkdir(orphan), "Could not make orphan.");
    write(QDir(orphan).filePath("image.jpg"), "orphan image");
    write(QDir(orphan).filePath("manifest.json"), "{invalid");
    require(!restarted.captures()[0].ready && !restarted.retry(orphan, &error) &&
                restarted.saveElsewhere(orphan, temporary.filePath("orphan.jpg"), &error) &&
                restarted.discard(orphan, &error), "Malformed manifest lost assets.");
    require(!restarted.discard(root, &error) && !restarted.discard(temporary.path(), &error),
            "Out-of-root deletion accepted.");
#ifdef Q_OS_UNIX
    const QString link = QDir(root).filePath("linked-job");
    require(QFile::link(exports, link) && !restarted.discard(link, &error) &&
                QFileInfo::exists(exports), "Symlink deletion accepted.");
    require(QFile::remove(link), "Could not remove fixture link.");
#endif

    // Prewrite validation failures remain retryable, including after restart.
    base = recovery.begin(CapturePurpose::AnimationFrame, *frame, metadata, &error);
    require(!base.isEmpty(), "Could not begin invalid image capture.");
    job = QFileInfo(base).absolutePath();
    write(base + ".jpg", "invalid image");
    require(recovery.markReady(job, base + ".jpg", &error), "Could not ready invalid image.");
    recovery.release(job);
    require(!recovery.retry(job, &error) && QFileInfo::exists(base + ".jpg") &&
                restarted.captures()[0].ready && !restarted.retry(job, &error) &&
                restarted.discard(job, &error),
            "Prewrite failure lost assets or retryability.");

    // Fail after the high-resolution write, then restore the missing output directory.
    job = capture(recovery, CapturePurpose::AnimationFrame, *frame, metadata, &error);
    const QString take = project->takeDirectory(0, 1, 0, &error);
    const QString lowDirectory = QDir(take).filePath("frames/preview/low_res");
    const QString heldDirectory = lowDirectory + "-held";
    const QString nextHigh = QDir(take).filePath("frames/preview/high_res/000002.jpg");
    require(QDir().rename(lowDirectory, heldDirectory), "Could not hide output directory.");
    require(!recovery.retry(job, &error) && restarted.captures()[0].ready &&
                !QFileInfo::exists(nextHigh), "Failed frame import did not roll back safely.");
    require(QDir().rename(heldDirectory, lowDirectory) && restarted.retry(job, &error) &&
                project->frames(0, 1, 0, &error).size() == 2,
            "Restored output directory did not allow retry.");

    // A preexisting low-resolution target must neither be overwritten nor cleaned up.
    job = capture(recovery, CapturePurpose::AnimationFrame, *frame, metadata, &error);
    const QString protectedLow = QDir(lowDirectory).filePath("000003.jpg");
    write(protectedLow, "existing image");
    bool retrySafe = false;
    require(!project->importFrame(0, 1, 0, QDir(job).filePath("capture.jpg"),
                                  metadata.capturedUtc, &error, &retrySafe) && retrySafe &&
                !QFileInfo::exists(QDir(take).filePath("frames/preview/high_res/000003.jpg")),
            "Existing target failure did not safely clean new outputs.");
    QFile protectedFile(protectedLow);
    require(protectedFile.open(QIODevice::ReadOnly) && protectedFile.readAll() == "existing image",
            "Preexisting frame target was modified.");
    protectedFile.close();
    require(QFile::remove(protectedLow) && restarted.retry(job, &error),
            "Retry after removing target conflict failed.");

    // Test-shot metadata failure must clean its copied image and permit retry.
    const auto other = project->captureDestination(0, 0, -1, &error);
    require(other.has_value(), "Could not resolve other shot.");
    job = capture(recovery, CapturePurpose::TestShot, *other, metadata, &error);
    const QString tests = project->testShotDirectory(0, 0, &error);
    const QString blockedMetadata = QDir(tests).filePath("000001.conf");
    require(QDir().mkdir(blockedMetadata), "Could not block test metadata.");
    require(!recovery.retry(job, &error) && restarted.captures()[0].ready &&
                QFileInfo(blockedMetadata).isDir() &&
                !QFileInfo::exists(QDir(tests).filePath("000001.jpg")),
            "Test-shot rollback removed preexisting output or retained copied image.");
    require(QDir().rmdir(blockedMetadata) && restarted.retry(job, &error) &&
                project->testShots(0, 0, &error).size() == 1,
            "Test-shot import could not retry after failure.");
    retrySafe = true;
    require(project->importTestShot(0, 0, saved, metadata.capturedUtc, {}, {}, {}, {},
                                    &error, &retrySafe).has_value() && !retrySafe,
            "Successful import incorrectly reported retry-safe failure.");

    job = capture(recovery, CapturePurpose::TestShot, *test, metadata, &error);
    require(project->deleteShot(0, 1, &error) &&
                project->createShot(0, "Renamed", &error), "Could not replace destination.");
    require(!restarted.retry(job, &error) && restarted.captures()[0].ready,
            "Recreated destination accepted old identity.");
    require(restarted.discard(job, &error), "Could not discard unresolved capture.");
  } catch (const std::exception& failure) {
    std::cerr << failure.what() << '\n' << error.toStdString() << '\n';
    return 1;
  }
  return 0;
}
