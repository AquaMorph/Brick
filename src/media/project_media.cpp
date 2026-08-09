#include "core/project.h"

#include "settings/frame_rate.h"
#include "storage/project_storage_internal.h"

#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QImage>
#include <QRegularExpression>
#include <QSettings>
#include <QUuid>

#include <algorithm>
#include <utility>
#include <vector>

namespace {

constexpr int kTestShotFormatVersion = 1;
constexpr int kAnimationFrameFormatVersion = 1;

using project_storage::kShotConfigFileName;
using project_storage::kTakeConfigFileName;
using project_storage::setError;

void writeStringMap(QSettings& config, const QString& group,
                    const std::map<QString, QString>& values) {
  config.remove(group);
  config.beginWriteArray(group, static_cast<int>(values.size()));
  int index = 0;
  for (const auto& [key, value] : values) {
    config.setArrayIndex(index++);
    config.setValue("key", key);
    config.setValue("value", value);
  }
  config.endArray();
}

std::map<QString, QString> readStringMap(QSettings& config,
                                         const QString& group) {
  std::map<QString, QString> values;
  const int size = config.beginReadArray(group);
  for (int index = 0; index < size; ++index) {
    config.setArrayIndex(index);
    values.insert_or_assign(config.value("key").toString(),
                            config.value("value").toString());
  }
  config.endArray();
  return values;
}

int testShotNumber(const QString& fileName) {
  static const QRegularExpression numberedImage(R"(^(\d{6})\.[^./\\]+$)");
  const auto match = numberedImage.match(fileName);
  return match.hasMatch() ? match.captured(1).toInt() : 0;
}

QString testShotMetadataFileName(int number) {
  return QString("%1.conf").arg(number, 6, 10, QLatin1Char('0'));
}

QString frameBaseName(int number) {
  return QString("%1").arg(number, 6, 10, QLatin1Char('0'));
}

bool isRawImageSuffix(const QString& suffix) {
  static const QStringList rawSuffixes = {
      "3fr", "arw", "cr2", "cr3", "dng", "erf", "iiq", "kdc",
      "mef", "mos", "mrw", "nef", "nrw", "orf", "pef", "raf",
      "raw", "rw2", "sr2", "srf", "x3f"};
  return rawSuffixes.contains(suffix.toLower());
}

}  // namespace


std::optional<ShotCameraSettings> Project::currentShotCameraSettings(
    int sceneIndex, int shotIndex, QString* error) const {
  const QString path = shotDirectory(sceneIndex, shotIndex, error);
  if (path.isEmpty()) {
    return std::nullopt;
  }
  QSettings config(QDir(path).filePath(kShotConfigFileName),
                   QSettings::IniFormat);
  ShotCameraSettings settings;
  settings.backend = config.value("Camera/backend").toString();
  settings.deviceId = config.value("Camera/deviceId").toString();
  settings.settings = readStringMap(config, "CameraSettings");
  if (config.status() != QSettings::NoError) {
    setError(error, "Brick could not read camera settings from shot.conf.");
    return std::nullopt;
  }
  return settings;
}


std::vector<TestShot> Project::testShots(int sceneIndex, int shotIndex,
                                         QString* error) const {
  const QString directory = testShotDirectory(sceneIndex, shotIndex, error);
  if (directory.isEmpty()) {
    return {};
  }
  const QStringList metadataFiles = QDir(directory).entryList(
      {"??????.conf"}, QDir::Files | QDir::NoDotAndDotDot, QDir::Name);
  std::vector<TestShot> shots;
  shots.reserve(metadataFiles.size());
  for (const QString& metadataFile : metadataFiles) {
    QSettings config(QDir(directory).filePath(metadataFile),
                     QSettings::IniFormat);
    TestShot shot;
    shot.fileName = config.value("TestShot/fileName").toString();
    shot.capturedUtc = QDateTime::fromString(
        config.value("TestShot/capturedUtc").toString(), Qt::ISODate);
    shot.cameraDisplayName = config.value("TestShot/cameraDisplayName").toString();
    shot.cameraBackend = config.value("TestShot/cameraBackend").toString();
    shot.cameraDeviceId = config.value("TestShot/cameraDeviceId").toString();
    shot.capturedSettings = readStringMap(config, "Settings");
    shot.displaySettings = readStringMap(config, "DisplaySettings");
    if (config.status() != QSettings::NoError) {
      setError(error, "Brick could not read test shot metadata.");
      return {};
    }
    if (config.value("TestShot/formatVersion").toInt() !=
            kTestShotFormatVersion ||
        testShotNumber(shot.fileName) != testShotNumber(metadataFile) ||
        !QFileInfo::exists(QDir(directory).filePath(shot.fileName))) {
      continue;
    }
    shot.filePath = QDir(directory).filePath(shot.fileName);
    shots.push_back(std::move(shot));
  }
  return shots;
}


std::vector<AnimationFrame> Project::frames(int sceneIndex, int shotIndex,
                                            int takeIndex,
                                            QString* error) const {
  const QString takePath = takeDirectory(sceneIndex, shotIndex, takeIndex, error);
  if (takePath.isEmpty()) {
    return {};
  }
  const QDir highRes(QDir(takePath).filePath("frames/preview/high_res"));
  const QStringList files = highRes.entryList(
      {"??????.jpg"}, QDir::Files | QDir::NoDotAndDotDot, QDir::Name);
  std::vector<AnimationFrame> result;
  result.reserve(files.size());
  for (const QString& fileName : files) {
    bool validNumber = false;
    const int number = QFileInfo(fileName).completeBaseName().toInt(&validNumber);
    if (!validNumber || number <= 0) {
      continue;
    }
    AnimationFrame frame;
    frame.number = number;
    frame.highResPath = highRes.filePath(fileName);
    const QString lowResPath =
        QDir(takePath).filePath("frames/preview/low_res/" + fileName);
    frame.lowResPath = QFileInfo::exists(lowResPath) ? lowResPath : frame.highResPath;
    const QDir rawDirectory(QDir(takePath).filePath("frames/RAW"));
    const QStringList rawFiles = rawDirectory.entryList(
        {frameBaseName(number) + ".*"}, QDir::Files | QDir::NoDotAndDotDot,
        QDir::Name);
    if (!rawFiles.isEmpty()) {
      frame.rawPath = rawDirectory.filePath(rawFiles.front());
    }
    QSettings metadata(
        QDir(takePath).filePath("frames/" + frameBaseName(number) + ".conf"),
        QSettings::IniFormat);
    if (metadata.value("Frame/formatVersion").toInt() ==
        kAnimationFrameFormatVersion) {
      frame.capturedUtc = QDateTime::fromString(
          metadata.value("Frame/capturedUtc").toString(), Qt::ISODate);
    }
    result.push_back(std::move(frame));
  }
  return result;
}


int Project::takeFrameRate(int sceneIndex, int shotIndex, int takeIndex,
                           QString* error) const {
  const QString path = takeDirectory(sceneIndex, shotIndex, takeIndex, error);
  if (path.isEmpty()) {
    return FrameRate::kDefault;
  }
  QSettings config(QDir(path).filePath(kTakeConfigFileName),
                   QSettings::IniFormat);
  const int frameRate =
      config.value("Take/framesPerSecond", FrameRate::kDefault).toInt();
  if (config.status() != QSettings::NoError) {
    setError(error, "Brick could not read the take frame rate.");
    return FrameRate::kDefault;
  }
  return FrameRate::isSupported(frameRate) ? frameRate : FrameRate::kDefault;
}


bool Project::saveCurrentShotCameraSettings(
    int sceneIndex, int shotIndex, const ShotCameraSettings& settings,
    QString* error) {
  const QString path = shotDirectory(sceneIndex, shotIndex, error);
  if (path.isEmpty()) {
    return false;
  }
  QSettings config(QDir(path).filePath(kShotConfigFileName),
                   QSettings::IniFormat);
  config.setValue("Camera/backend", settings.backend);
  config.setValue("Camera/deviceId", settings.deviceId);
  writeStringMap(config, "CameraSettings", settings.settings);
  config.sync();
  if (config.status() != QSettings::NoError) {
    setError(error, "Brick could not save camera settings in shot.conf.");
    return false;
  }
  return true;
}


std::optional<TestShot> Project::importTestShot(
    int sceneIndex, int shotIndex, const QString& capturedImagePath,
    const QDateTime& capturedUtc, const QString& cameraDisplayName,
    const ShotCameraSettings& camera,
    const std::map<QString, QString>& capturedSettings,
    const std::map<QString, QString>& displaySettings, QString* error) {
  const QFileInfo source(capturedImagePath);
  if (!source.exists() || !source.isFile()) {
    setError(error, "The captured test shot image does not exist.");
    return std::nullopt;
  }
  if (!capturedUtc.isValid()) {
    setError(error, "The test shot capture time is invalid.");
    return std::nullopt;
  }
  const QString suffix = source.suffix();
  if (suffix.isEmpty()) {
    setError(error, "The captured test shot image has no file extension.");
    return std::nullopt;
  }
  const QString directory = testShotDirectory(sceneIndex, shotIndex, error);
  if (directory.isEmpty()) {
    return std::nullopt;
  }
  int largestNumber = 0;
  const QStringList files = QDir(directory).entryList(
      QDir::Files | QDir::NoDotAndDotDot, QDir::Name);
  for (const QString& file : files) {
    largestNumber = std::max(largestNumber, testShotNumber(file));
  }
  if (largestNumber >= 999999) {
    setError(error, "A shot cannot contain more test shot numbers.");
    return std::nullopt;
  }
  const int number = largestNumber + 1;
  const QString group = QString("%1").arg(number, 6, 10, QLatin1Char('0'));
  const QString fileName = group + '.' + suffix.toLower();
  const QString destination = QDir(directory).filePath(fileName);
  if (!QFile::copy(capturedImagePath, destination)) {
    setError(error, "Brick could not import the test shot image.");
    return std::nullopt;
  }
  const QString metadataPath =
      QDir(directory).filePath(testShotMetadataFileName(number));
  QSettings metadata(metadataPath, QSettings::IniFormat);
  metadata.setValue("TestShot/formatVersion", kTestShotFormatVersion);
  metadata.setValue("TestShot/fileName", fileName);
  metadata.setValue("TestShot/capturedUtc",
                    capturedUtc.toUTC().toString(Qt::ISODateWithMs));
  metadata.setValue("TestShot/cameraDisplayName", cameraDisplayName);
  metadata.setValue("TestShot/cameraBackend", camera.backend);
  metadata.setValue("TestShot/cameraDeviceId", camera.deviceId);
  writeStringMap(metadata, "Settings", capturedSettings);
  writeStringMap(metadata, "DisplaySettings", displaySettings);
  metadata.sync();
  if (metadata.status() != QSettings::NoError) {
    QFile::remove(destination);
    QFile::remove(metadataPath);
    setError(error, "Brick could not save test shot metadata.");
    return std::nullopt;
  }
  return TestShot{destination, fileName, capturedUtc.toUTC(), cameraDisplayName,
                  camera.backend, camera.deviceId, capturedSettings,
                  displaySettings};
}


bool Project::deleteTestShot(int sceneIndex, int shotIndex,
                             const QString& fileName, QString* error) {
  const int number = testShotNumber(fileName);
  const QString directory = testShotDirectory(sceneIndex, shotIndex, error);
  if (number == 0 || directory.isEmpty()) {
    if (number == 0) {
      setError(error, "The selected test shot does not exist.");
    }
    return false;
  }
  const QString metadataFile = testShotMetadataFileName(number);
  QSettings metadata(QDir(directory).filePath(metadataFile),
                     QSettings::IniFormat);
  if (metadata.value("TestShot/formatVersion").toInt() !=
          kTestShotFormatVersion ||
      metadata.value("TestShot/fileName").toString() != fileName ||
      !QFileInfo::exists(QDir(directory).filePath(fileName))) {
    setError(error, "The selected test shot does not exist.");
    return false;
  }
  QDir tests(directory);
  const QString imageTombstone =
      ".brick-test-delete-" + QUuid::createUuid().toString(QUuid::Id128);
  const QString metadataTombstone = imageTombstone + ".conf";
  if (!tests.rename(fileName, imageTombstone)) {
    setError(error, "Brick could not prepare the test shot for deletion.");
    return false;
  }
  if (!tests.rename(metadataFile, metadataTombstone)) {
    tests.rename(imageTombstone, fileName);
    setError(error, "Brick could not prepare test shot metadata for deletion.");
    return false;
  }
  // Renaming both files is the commit point. Hidden tombstones can be cleaned
  // up later without making a successfully deleted test shot reappear.
  tests.remove(imageTombstone);
  tests.remove(metadataTombstone);
  return true;
}


std::optional<AnimationFrame> Project::importFrame(
    int sceneIndex, int shotIndex, int takeIndex,
    const QString& capturedImagePath, const QDateTime& capturedUtc,
    QString* error) {
  const QFileInfo source(capturedImagePath);
  if (!source.exists() || !source.isFile()) {
    setError(error, "The captured frame image does not exist.");
    return std::nullopt;
  }
  if (!capturedUtc.isValid()) {
    setError(error, "The frame capture time is invalid.");
    return std::nullopt;
  }
  const QImage image(capturedImagePath);
  if (image.isNull()) {
    setError(error, "Brick could not decode the captured frame image.");
    return std::nullopt;
  }
  const QString takePath = takeDirectory(sceneIndex, shotIndex, takeIndex, error);
  if (takePath.isEmpty()) {
    return std::nullopt;
  }
  const auto existingFrames = frames(sceneIndex, shotIndex, takeIndex, error);
  int largestNumber = 0;
  for (const AnimationFrame& frame : existingFrames) {
    largestNumber = std::max(largestNumber, frame.number);
  }
  QSettings takeConfig(QDir(takePath).filePath(kTakeConfigFileName),
                       QSettings::IniFormat);
  const int number = std::max(
      largestNumber + 1, takeConfig.value("Take/nextFrameNumber", 1).toInt());
  if (number > 999999) {
    setError(error, "A take cannot contain more frame numbers.");
    return std::nullopt;
  }
  const QString baseName = frameBaseName(number);
  const QString highResPath = QDir(takePath).filePath(
      "frames/preview/high_res/" + baseName + ".jpg");
  const QString lowResPath = QDir(takePath).filePath(
      "frames/preview/low_res/" + baseName + ".jpg");
  if (!image.save(highResPath, "JPG", 95)) {
    setError(error, "Brick could not save the high-resolution frame preview.");
    return std::nullopt;
  }
  const QImage lowRes = image.scaled(640, 360, Qt::KeepAspectRatio,
                                     Qt::SmoothTransformation);
  if (!lowRes.save(lowResPath, "JPG", 82)) {
    QFile::remove(highResPath);
    setError(error, "Brick could not save the low-resolution frame preview.");
    return std::nullopt;
  }
  QString rawPath;
  if (isRawImageSuffix(source.suffix())) {
    rawPath = QDir(takePath).filePath("frames/RAW/" + baseName + '.' +
                                      source.suffix().toLower());
    if (!QFile::copy(capturedImagePath, rawPath)) {
      QFile::remove(highResPath);
      QFile::remove(lowResPath);
      setError(error, "Brick could not save the RAW frame.");
      return std::nullopt;
    }
  }
  const QString metadataPath =
      QDir(takePath).filePath("frames/" + baseName + ".conf");
  QSettings metadata(metadataPath, QSettings::IniFormat);
  metadata.setValue("Frame/formatVersion", kAnimationFrameFormatVersion);
  metadata.setValue("Frame/capturedUtc",
                    capturedUtc.toUTC().toString(Qt::ISODateWithMs));
  metadata.sync();
  if (metadata.status() != QSettings::NoError) {
    QFile::remove(highResPath);
    QFile::remove(lowResPath);
    QFile::remove(rawPath);
    QFile::remove(metadataPath);
    setError(error, "Brick could not save frame metadata.");
    return std::nullopt;
  }
  takeConfig.setValue("Take/nextFrameNumber", number + 1);
  takeConfig.sync();
  if (takeConfig.status() != QSettings::NoError) {
    QFile::remove(highResPath);
    QFile::remove(lowResPath);
    QFile::remove(rawPath);
    QFile::remove(metadataPath);
    setError(error, "Brick could not update the next frame number.");
    return std::nullopt;
  }
  return AnimationFrame{number, highResPath, lowResPath, rawPath,
                        capturedUtc.toUTC()};
}


bool Project::deleteFrame(int sceneIndex, int shotIndex, int takeIndex,
                          int number, QString* error) {
  const QString takePath = takeDirectory(sceneIndex, shotIndex, takeIndex, error);
  if (takePath.isEmpty() || number <= 0 || number > 999999) {
    if (number <= 0 || number > 999999) {
      setError(error, "The selected frame does not exist.");
    }
    return false;
  }
  const auto takeFrames = frames(sceneIndex, shotIndex, takeIndex, error);
  const auto frame = std::find_if(
      takeFrames.begin(), takeFrames.end(), [number](const AnimationFrame& candidate) {
        return candidate.number == number;
      });
  if (frame == takeFrames.end()) {
    setError(error, "The selected frame does not exist.");
    return false;
  }
  const QString token =
      ".brick-frame-delete-" + QUuid::createUuid().toString(QUuid::Id128);
  const QStringList paths = {
      frame->highResPath, frame->lowResPath, frame->rawPath,
      QDir(takePath).filePath("frames/" + frameBaseName(number) + ".conf")};
  std::vector<std::pair<QString, QString>> renamed;
  for (const QString& path : paths) {
    if (path.isEmpty() || !QFileInfo::exists(path)) {
      continue;
    }
    const QString tombstone = QFileInfo(path).dir().filePath(
        token + '-' + QString::number(renamed.size()));
    if (!QFile::rename(path, tombstone)) {
      for (auto entry = renamed.rbegin(); entry != renamed.rend(); ++entry) {
        QFile::rename(entry->second, entry->first);
      }
      setError(error, "Brick could not prepare the frame for deletion.");
      return false;
    }
    renamed.emplace_back(path, tombstone);
  }
  for (const auto& [original, tombstone] : renamed) {
    Q_UNUSED(original);
    QFile::remove(tombstone);
  }
  return true;
}


bool Project::saveTakeFrameRate(int sceneIndex, int shotIndex, int takeIndex,
                                int framesPerSecond, QString* error) {
  if (!FrameRate::isSupported(framesPerSecond)) {
    setError(error, "The take frame rate must be 8, 12, 15, 24, or 30 FPS.");
    return false;
  }
  const QString path = takeDirectory(sceneIndex, shotIndex, takeIndex, error);
  if (path.isEmpty()) {
    return false;
  }
  QSettings config(QDir(path).filePath(kTakeConfigFileName),
                   QSettings::IniFormat);
  config.setValue("Take/framesPerSecond", framesPerSecond);
  config.sync();
  if (config.status() != QSettings::NoError) {
    setError(error, "Brick could not save the take frame rate.");
    return false;
  }
  return true;
}
