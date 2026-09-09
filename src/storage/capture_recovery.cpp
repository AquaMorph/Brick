#include "capture_recovery.h"

#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonDocument>
#include <QJsonObject>
#include <QSaveFile>
#include <QStandardPaths>
#include <QUuid>

namespace {

bool fail(QString* error, const QString& message) {
  if (error) {
    *error = message;
  }
  return false;
}

QJsonObject readManifest(const QString& path) {
  QFile file(QDir(path).filePath("manifest.json"));
  if (QFileInfo(file).isSymLink() || !file.open(QIODevice::ReadOnly)) {
    return {};
  }
  return QJsonDocument::fromJson(file.readAll()).object();
}

bool writeManifest(const QString& path, const QJsonObject& manifest, QString* error) {
  QSaveFile file(QDir(path).filePath("manifest.json"));
  const auto bytes = QJsonDocument(manifest).toJson();
  if (!file.open(QIODevice::WriteOnly) || file.write(bytes) != bytes.size() ||
      !file.commit()) {
    return fail(error, "Could not persist the capture recovery manifest.");
  }
  return true;
}

QJsonObject stringMap(const std::map<QString, QString>& values) {
  QJsonObject result;
  for (const auto& [key, value] : values) {
    result.insert(key, value);
  }
  return result;
}

std::map<QString, QString> stringMap(const QJsonObject& values) {
  std::map<QString, QString> result;
  for (auto it = values.begin(); it != values.end(); ++it) {
    result.emplace(it.key(), it.value().toString());
  }
  return result;
}

QFileInfoList assets(const QString& path) {
  auto files = QDir(path).entryInfoList(QDir::Files | QDir::Hidden | QDir::NoSymLinks,
                                       QDir::Name);
  files.removeIf([](const QFileInfo& file) {
    return file.fileName() == "manifest.json" || file.fileName() == ".lock";
  });
  return files;
}

bool validManifest(const QJsonObject& m) {
  const auto purpose = m["purpose"].toString();
  return m["version"].toInt() == 1 &&
         (purpose == "test" || purpose == "frame") &&
         QDir::isAbsolutePath(m["projectDirectory"].toString()) &&
         !QUuid(m["projectId"].toString()).isNull() &&
         !QUuid(m["shotId"].toString()).isNull() &&
         (purpose == "test" ? m["takeId"].toString().isEmpty()
                            : !QUuid(m["takeId"].toString()).isNull()) &&
         QDateTime::fromString(m["timestamp"].toString(), Qt::ISODateWithMs).isValid();
}

QString readyFile(const QString& path, const QJsonObject& m) {
  const QString name = m["file"].toString();
  const QFileInfo file(QDir(path).filePath(name));
  if (name.isEmpty() || name != QFileInfo(name).fileName() ||
      name == "manifest.json" || name == ".lock" ||
      file.isSymLink() || !file.isFile()) {
    return {};
  }
  return file.absoluteFilePath();
}

// Only flat, private job directories are removed. Never follow manifest paths.
bool cleanJob(const QString& path, QString* error) {
  const auto entries = QDir(path).entryInfoList(
      QDir::AllEntries | QDir::Hidden | QDir::System | QDir::NoDotAndDotDot);
  for (const auto& entry : entries) {
    if (entry.isSymLink() || !entry.isFile()) {
      return fail(error, "Unsafe recovery directory contents; cleanup refused.");
    }
  }
  for (const auto& entry : entries) {
    if (entry.fileName() != "manifest.json" && entry.fileName() != ".lock" &&
        !QFile::remove(entry.absoluteFilePath())) {
      return fail(error, "Could not remove recovery assets; the job was retained.");
    }
  }
  const QString manifest = QDir(path).filePath("manifest.json");
  if (QFileInfo::exists(manifest) && !QFile::remove(manifest)) {
    return fail(error, "Could not remove the recovery manifest.");
  }
  return true;
}

}  // namespace


CaptureRecovery::CaptureRecovery(QString rootDirectory)
    : root_(QDir(rootDirectory.isEmpty()
                     ? QStandardPaths::writableLocation(QStandardPaths::AppLocalDataLocation) +
                           "/capture-recovery"
                     : rootDirectory).absolutePath()) {}


QString CaptureRecovery::directory() const { return root_; }


bool CaptureRecovery::validJob(const QString& path, QString* error) const {
  const QFileInfo root(root_);
  const QFileInfo job(path);
  if (root.isSymLink() || !root.isDir() || !job.isDir() || job.isSymLink() ||
      job.absolutePath() != root_ || job.canonicalPath() != root.canonicalFilePath()) {
    return fail(error, "Invalid recovery job path.");
  }
  return true;
}


std::unique_ptr<QLockFile> CaptureRecovery::lock(const QString& path, QString* error) const {
  if (!validJob(path, error)) {
    return {};
  }
  if (QFileInfo(QDir(path).filePath(".lock")).isSymLink()) {
    fail(error, "Unsafe recovery lock path.");
    return {};
  }
  auto result = std::make_unique<QLockFile>(QDir(path).filePath(".lock"));
  result->setStaleLockTime(0);
  if (!result->tryLock()) {
    fail(error, "This capture job is still active or in use.");
    return {};
  }
  return result;
}


QString CaptureRecovery::begin(CapturePurpose purpose, const CaptureDestination& destination,
                               const TestShot& metadata, QString* error) {
  QJsonObject m{{"version", 1}, {"state", "capturing"},
                {"purpose", purpose == CapturePurpose::TestShot ? "test" : "frame"},
                {"projectDirectory", destination.projectDirectory},
                {"projectId", destination.projectId}, {"shotId", destination.shotId},
                {"takeId", destination.takeId},
                {"timestamp", (metadata.capturedUtc.isValid() ? metadata.capturedUtc.toUTC()
                                  : QDateTime::currentDateTimeUtc()).toString(Qt::ISODateWithMs)},
                {"cameraDisplayName", metadata.cameraDisplayName},
                {"cameraBackend", metadata.cameraBackend},
                {"cameraDeviceId", metadata.cameraDeviceId},
                {"capturedSettings", stringMap(metadata.capturedSettings)},
                {"displaySettings", stringMap(metadata.displaySettings)}};
  if (!validManifest(m)) {
    fail(error, "Invalid capture destination or purpose.");
    return {};
  }
  auto project = Project::open(destination.projectDirectory, error);
  if (!project || !project->resolveCaptureDestination(destination, error)) {
    return {};
  }
  if (QFileInfo(root_).isSymLink() || !QDir().mkpath(root_)) {
    fail(error, "Could not create durable capture storage.");
    return {};
  }
  const QString path = QDir(root_).filePath(QUuid::createUuid().toString(QUuid::WithoutBraces));
  if (!QDir().mkdir(path)) {
    fail(error, "Could not create capture job directory.");
    return {};
  }
  auto guard = lock(path, error);
  if (!guard || !writeManifest(path, m, error)) {
    return {};
  }
  active_.emplace(path, std::move(guard));
  return QDir(path).filePath("capture");
}


bool CaptureRecovery::markReady(const QString& path, const QString& filePath, QString* error) {
  if (!active_.contains(path) || !validJob(path, error)) {
    return fail(error, "Capture job is not active.");
  }
  auto m = readManifest(path);
  m["file"] = QFileInfo(filePath).fileName();
  if (!validManifest(m) || QFileInfo(filePath).absolutePath() != path ||
      readyFile(path, m).isEmpty()) {
    return fail(error, "The camera did not produce a valid recovery asset.");
  }
  m["state"] = "ready";
  return writeManifest(path, m, error);
}


void CaptureRecovery::release(const QString& path) { active_.erase(path); }


std::vector<RecoveryCapture> CaptureRecovery::captures() const {
  std::vector<RecoveryCapture> result;
  for (const auto& job : QDir(root_).entryInfoList(QDir::Dirs | QDir::NoDotAndDotDot)) {
    const QString path = job.absoluteFilePath();
    auto guard = lock(path, nullptr);
    if (!guard) {
      continue;
    }
    const auto m = readManifest(path);
    QString file = readyFile(path, m);
    if (file.isEmpty()) {
      const auto files = assets(path);
      if (!files.isEmpty()) {
        file = files.front().absoluteFilePath();
      }
    }
    const QString state = m["state"].toString();
    const bool ready = validManifest(m) && state == "ready" && !readyFile(path, m).isEmpty();
    QString description = "Incomplete or orphan capture; save or discard available.";
    if (state == "importing") {
      description = "Import may already have committed. Check the project; retry is disabled.";
    } else if (state == "committed") {
      description = "Already imported; remaining recovery files may be discarded.";
    } else if (ready) {
      description = m["purpose"].toString() + " capture at " + m["timestamp"].toString() +
                    " for " + m["projectDirectory"].toString();
    }
    result.push_back({path, file, description, ready});
  }
  return result;
}


bool CaptureRecovery::retry(const QString& path, QString* error) {
  auto guard = lock(path, error);
  if (!guard) {
    return false;
  }
  auto m = readManifest(path);
  const QString state = m["state"].toString();
  if (state == "importing" || state == "committed") {
    return fail(error, "This capture may already be imported. Check the original project; "
                       "retry is disabled to prevent duplicates. Save or discard it instead.");
  }
  const QString file = readyFile(path, m);
  if (!validManifest(m) || state != "ready" || file.isEmpty()) {
    return fail(error, "Incomplete or invalid capture job; save or discard it instead.");
  }
  CaptureDestination destination{m["projectDirectory"].toString(), m["projectId"].toString(),
                                 m["shotId"].toString(), m["takeId"].toString()};
  auto project = Project::open(destination.projectDirectory, error);
  if (!project) {
    return false;
  }
  auto target = project->resolveCaptureDestination(destination, error);
  if (!target) {
    return false;
  }
  m["state"] = "importing";
  if (!writeManifest(path, m, error)) {
    return false;
  }
  const auto timestamp = QDateTime::fromString(m["timestamp"].toString(), Qt::ISODateWithMs);
  bool imported = false;
  bool retrySafe = false;
  if (m["purpose"].toString() == "test") {
    imported = project->importTestShot(
        target->sceneIndex, target->shotIndex, file, timestamp,
        m["cameraDisplayName"].toString(),
        {m["cameraBackend"].toString(), m["cameraDeviceId"].toString(), {}},
        stringMap(m["capturedSettings"].toObject()),
        stringMap(m["displaySettings"].toObject()), error, &retrySafe).has_value();
  } else {
    imported = project->importFrame(target->sceneIndex, target->shotIndex, target->takeIndex,
                                    file, timestamp, error, &retrySafe).has_value();
  }
  if (!imported) {
    if (retrySafe) {
      m["state"] = "ready";
      if (writeManifest(path, m, nullptr)) {
        return fail(error, (error ? *error + " " : QString{}) +
                           "Import failed; assets retained. Retry is available.");
      }
    }
    // Failed rollback or manifest persistence leaves the ambiguity marker intact.
    return fail(error, (error ? *error + " " : QString{}) +
                       "Import failed; assets retained. Check the project before saving or discarding.");
  }
  m["state"] = "committed";
  // The durable importing marker already prevents retries if this write or cleanup fails.
  if (writeManifest(path, m, nullptr) && assets(path).size() == 1 &&
      cleanJob(path, nullptr)) {
    guard.reset();
    QDir().rmdir(path);
  }
  return true;
}


bool CaptureRecovery::saveElsewhere(const QString& path, const QString& destination,
                                   QString* error) {
  auto guard = lock(path, error);
  if (!guard) {
    return false;
  }
  const auto files = assets(path);
  const bool directory = QFileInfo(destination).isDir();
  if (files.isEmpty() || (!directory && files.size() != 1)) {
    return fail(error, "Choose an existing directory to save multiple assets; no assets may be missing.");
  }
  for (const auto& file : files) {
    const QString target = directory ? QDir(destination).filePath(file.fileName()) : destination;
    if (!QFile::copy(file.absoluteFilePath(), target)) {
      return fail(error, "Could not copy recovery assets (existing files are never overwritten). "
                         "Originals are retained; some copies may have succeeded.");
    }
  }
  return true;
}


bool CaptureRecovery::discard(const QString& path, QString* error) {
  auto guard = lock(path, error);
  if (!guard || !cleanJob(path, error)) {
    return false;
  }
  guard.reset();
  return QDir().rmdir(path) || fail(error, "Could not remove the empty recovery directory.");
}
