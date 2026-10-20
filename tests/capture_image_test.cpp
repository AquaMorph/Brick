#include "core/project.h"
#include "media/capture_image.h"
#include "storage/capture_recovery.h"

#include <QCoreApplication>
#include <QCryptographicHash>
#include <QFile>
#include <QImage>
#include <QTemporaryDir>

#include <iostream>
#include <stdexcept>

namespace {

void require(bool condition, const QString& message) {
  if (!condition) {
    throw std::runtime_error(message.toStdString());
  }
}

QByteArray digest(const QString& path) {
  QFile file(path);
  require(file.open(QIODevice::ReadOnly), "Could not read test asset.");
  QCryptographicHash hash(QCryptographicHash::Sha256);
  require(hash.addData(&file), "Could not hash test asset.");
  return hash.result();
}

}  // namespace


int main(int argc, char* argv[]) {
  QCoreApplication application(argc, argv);
  try {
    QTemporaryDir temporary;
    require(temporary.isValid(), "Could not create test storage.");
    const QString jpeg = temporary.filePath("capture.jpg");
    QImage fixture(128, 96, QImage::Format_RGB32);
    fixture.fill(Qt::red);
    require(fixture.save(jpeg), "Could not save JPEG fixture.");
    QString error;
    require(loadCaptureImage(jpeg, {}, &error).size() == fixture.size(), error);
    require(loadCaptureImage(jpeg, {32, 32}, &error).size() == QSize(32, 24),
            "JPEG preview lost its aspect ratio.");
    const QString brokenRaw = temporary.filePath("broken.cr3");
    QFile broken(brokenRaw);
    require(broken.open(QIODevice::WriteOnly) && broken.write("incomplete capture") > 0,
            "Could not write invalid RAW fixture.");
    broken.close();
    const auto before = digest(brokenRaw);
    require(loadCaptureImage(brokenRaw, {}, &error).isNull() && !error.isEmpty(),
            "Invalid RAW did not report a decoding error.");
    require(digest(brokenRaw) == before, "Failed decoding changed the original RAW.");

    auto project = Project::create(temporary.path(), "RAW Test", &error);
    require(project && project->createScene("Scene", &error) &&
                project->createShot(0, "Shot", &error), error);
    bool retrySafe = false;
    require(!project->importFrame(0, 0, 0, brokenRaw, QDateTime::currentDateTimeUtc(),
                                   &error, &retrySafe) && retrySafe,
            "Failed RAW import was not safe to retry.");
    require(project->frames(0, 0, 0, &error).empty(), "Failed RAW import created a frame.");

    // Supply a real CR3/CR2 to exercise development, durable recovery, previews,
    // and preservation of the exact RAW bytes without touching the user's file.
    if (argc == 2) {
      const QString source = application.arguments()[1];
      const auto original = digest(source);
      require(!loadCaptureImage(source, {128, 96}, &error).isNull(), error);
      CaptureRecovery recovery(temporary.filePath("recovery"));
      const auto destination = project->captureDestination(0, 0, 0, &error);
      require(destination.has_value(), error);
      TestShot metadata;
      metadata.capturedUtc = QDateTime::currentDateTimeUtc();
      const QString base = recovery.begin(CapturePurpose::AnimationFrame, *destination,
                                           metadata, &error);
      require(!base.isEmpty(), error);
      const QString capture = base + '.' + QFileInfo(source).suffix();
      require(QFile::copy(source, capture), "Could not copy RAW fixture.");
      const QString job = QFileInfo(base).absolutePath();
      require(recovery.markReady(job, capture, &error), error);
      recovery.release(job);
      require(recovery.retry(job, &error), error);
      const auto frames = project->frames(0, 0, 0, &error);
      require(frames.size() == 1 && !QImage(frames[0].highResPath).isNull() &&
                  !QImage(frames[0].lowResPath).isNull(), "RAW previews were not imported.");
      require(digest(frames[0].rawPath) == original && digest(source) == original,
              "RAW import changed or lost the original bytes.");
      const auto shot = project->importTestShot(0, 0, source, metadata.capturedUtc,
                                                "EOS R", {"canon", "fixture", {}},
                                                {}, {}, &error);
      require(shot.has_value(), error);
      require(digest(shot->filePath) == original &&
                  !loadCaptureImage(shot->filePath, {128, 96}, &error).isNull(),
              "Cinematography RAW test image could not be displayed.");
    }
  } catch (const std::exception& exception) {
    std::cerr << exception.what() << '\n';
    return 1;
  }
  return 0;
}
