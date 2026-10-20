#include "capture_image.h"

#include <QCoreApplication>
#include <QDir>
#include <QFileInfo>
#include <QImageReader>
#include <QProcess>
#include <QTemporaryDir>
#include <QTransform>

namespace {

QImage readImage(const QString& path, const QSize& targetSize, QString* error) {
  QImageReader reader(path);
  reader.setAutoTransform(true);
  const QSize size = reader.size();
  if (size.isValid() && targetSize.isValid() &&
      (size.width() > targetSize.width() || size.height() > targetSize.height())) {
    reader.setScaledSize(size.scaled(targetSize, Qt::KeepAspectRatio));
  }
  QImage image = reader.read();
  if (image.isNull() && error) {
    *error = reader.errorString();
  }
  return image;
}

bool isCanonRaw(const QString& path) {
  const QString suffix = QFileInfo(path).suffix().toLower();
  return suffix == "cr3" || suffix == "cr2" || suffix == "crw";
}

}  // namespace


QImage loadCaptureImage(const QString& path, const QSize& targetSize, QString* error) {
  if (error) {
    error->clear();
  }
  if (!isCanonRaw(path)) {
    return readImage(path, targetSize, error);
  }
#ifdef BRICK_CANON_RAW_HELPER
  const QDir appDirectory(QCoreApplication::applicationDirPath());
#ifdef Q_OS_MACOS
  QString helper = appDirectory.filePath("../Helpers/CanonRaw/brick_canon_raw");
#else
  QString helper = appDirectory.filePath("canon-raw/brick_canon_raw.exe");
#endif
  if (!QFileInfo::exists(helper)) {
    helper = QString::fromUtf8(BRICK_CANON_RAW_HELPER);
  }
  QTemporaryDir temporary;
  if (!temporary.isValid()) {
    if (error) {
      *error = "Could not create temporary storage for the Canon RAW preview.";
    }
    return {};
  }
  const QString preview = temporary.filePath("preview.ppm");
  QProcess decoder;
  decoder.setProgram(helper);
  decoder.setArguments({QFileInfo(path).absoluteFilePath(), preview});
  decoder.start();
  if (!decoder.waitForStarted()) {
    if (error) {
      *error = "Could not start the Canon RAW decoder: " + decoder.errorString() +
               ". On Apple Silicon, Canon's RAW module requires Rosetta.";
    }
    return {};
  }
  if (!decoder.waitForFinished(120000)) {
    decoder.kill();
    decoder.waitForFinished();
    if (error) {
      *error = "The Canon RAW decoder timed out. The original capture is retained.";
    }
    return {};
  }
  if (decoder.exitStatus() != QProcess::NormalExit || decoder.exitCode() != 0) {
    if (error) {
      *error = "Canon RAW decoding failed: " +
               QString::fromUtf8(decoder.readAllStandardError()).trimmed() +
               ". The original capture is retained.";
    }
    return {};
  }
  const int orientation = QString::fromUtf8(decoder.readAllStandardOutput()).trimmed().toInt();
  const bool rotated = orientation == 6 || orientation == 8;
  const QSize decodeSize = rotated ? targetSize.transposed() : targetSize;
  QImage image = readImage(preview, decodeSize, error);
  if (!image.isNull() && (orientation == 3 || rotated)) {
    const int angle = orientation == 3 ? 180 : (orientation == 6 ? 90 : 270);
    image = image.transformed(QTransform().rotate(angle));
  }
  return image;
#else
  if (error) {
    *error = "Canon RAW images require a build with Canon's RAW-processing SDK enabled.";
  }
  return {};
#endif
}
