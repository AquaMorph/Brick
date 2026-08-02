#include "project.h"

#include <QDateTime>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QRegularExpression>
#include <QSettings>
#include <QUuid>

#include <algorithm>
#include <utility>
#include <vector>

namespace {

constexpr int kProjectFormatVersion = 1;
constexpr int kSceneFormatVersion = 1;
constexpr int kShotFormatVersion = 1;
constexpr int kTakeFormatVersion = 1;
constexpr auto kConfigFileName = "project.conf";
constexpr auto kSceneConfigFileName = "scene.conf";
constexpr auto kShotConfigFileName = "shot.conf";
constexpr auto kTakeConfigFileName = "take.conf";

bool isValidProjectName(const QString& name) {
  static const QRegularExpression validName(R"(^[A-Za-z0-9]+(?: +[A-Za-z0-9]+)*$)");
  return validName.match(name).hasMatch();
}

QString directoryNameForProject(QString name) {
  return name.replace(' ', '_');
}

QString directoryNameForScene(int index, QString name) {
  return QString("%1_%2")
      .arg(index + 1, 4, 10, QLatin1Char('0'))
      .arg(name.replace(' ', '_').toUpper());
}

QString directoryNameForShot(int index, QString name) {
  return QString("%1_%2")
      .arg(index + 1, 4, 10, QLatin1Char('0'))
      .arg(name.replace(' ', '_').toUpper());
}

QString directoryNameForTake(int index) {
  return QString("%1_TAKE").arg(index + 1, 4, 10, QLatin1Char('0'));
}

QString directoryNameForTake(int index, QString) {
  return directoryNameForTake(index);
}

void setError(QString* error, const QString& message) {
  if (error != nullptr) {
    *error = message;
  }
}

bool createTakeDirectory(QDir shot, int index, QString* error) {
  const QString takeDirectory = directoryNameForTake(index);
  if (!shot.mkdir(takeDirectory)) {
    setError(error, "Brick could not create the take folder.");
    return false;
  }

  QDir take(shot.filePath(takeDirectory));
  if (!take.mkpath("frames/preview/high_res") ||
      !take.mkpath("frames/preview/low_res") || !take.mkpath("frames/RAW")) {
    take.removeRecursively();
    setError(error, "Brick could not create the take frame folders.");
    return false;
  }

  const QString configPath = take.filePath(kTakeConfigFileName);
  QSettings config(configPath, QSettings::IniFormat);
  config.beginGroup("Take");
  config.setValue("formatVersion", kTakeFormatVersion);
  config.setValue("createdUtc",
                  QDateTime::currentDateTimeUtc().toString(Qt::ISODate));
  config.endGroup();
  config.sync();
  if (config.status() != QSettings::NoError) {
    take.removeRecursively();
    setError(error, "Brick could not write take.conf.");
    return false;
  }
  return true;
}

struct ContentDirectory {
  QString oldName;
  QString contentName;
};

bool renameContentDirectories(
    const QString& parentDirectory,
    const std::vector<ContentDirectory>& contents, const QString& contentType,
    QString (*directoryName)(int, QString), QString* error) {
  QDir parent(parentDirectory);
  const QString token = QUuid::createUuid().toString(QUuid::Id128);
  std::vector<QString> temporaryNames;
  temporaryNames.reserve(contents.size());

  for (std::size_t index = 0; index < contents.size(); ++index) {
    const QString temporaryName = ".brick-" + contentType + '-' + token + '-' +
                                  QString::number(index);
    temporaryNames.push_back(temporaryName);
    if (!parent.rename(contents[index].oldName, temporaryName)) {
      for (std::size_t rollback = 0; rollback < index; ++rollback) {
        parent.rename(temporaryNames[rollback], contents[rollback].oldName);
      }
      setError(error,
               QString("Brick could not renumber the %1 folders.").arg(contentType));
      return false;
    }
  }

  std::size_t renamed = 0;
  for (; renamed < contents.size(); ++renamed) {
    const QString newName = directoryName(
        static_cast<int>(renamed), contents[renamed].contentName);
    if (!parent.rename(temporaryNames[renamed], newName)) {
      break;
    }
  }
  if (renamed == contents.size()) {
    return true;
  }

  for (std::size_t index = 0; index < renamed; ++index) {
    parent.rename(directoryName(static_cast<int>(index),
                                contents[index].contentName),
                  temporaryNames[index]);
  }
  for (std::size_t index = 0; index < contents.size(); ++index) {
    parent.rename(temporaryNames[index], contents[index].oldName);
  }
  setError(error,
           QString("Brick could not renumber the %1 folders.").arg(contentType));
  return false;
}

}  // namespace


Project::Project(QString name, QString directory, std::vector<QString> scenes,
                  std::vector<std::vector<QString>> shots,
                  std::vector<std::vector<int>> takeCounts)
    : name_(std::move(name)),
      directory_(std::move(directory)),
      scenes_(std::move(scenes)),
      shots_(std::move(shots)),
      takeCounts_(std::move(takeCounts)) {}


std::optional<Project> Project::create(const QString& parentDirectory,
                                       const QString& name, QString* error) {
  if (!isValidProjectName(name)) {
    setError(error,
             "Project names may contain only letters, numbers, and spaces.");
    return std::nullopt;
  }

  QDir parent(parentDirectory);
  if (!parent.exists()) {
    setError(error, "The selected project location does not exist.");
    return std::nullopt;
  }

  const QString directoryName = directoryNameForProject(name);
  const QString projectDirectory = parent.absoluteFilePath(directoryName);
  if (QFileInfo::exists(projectDirectory)) {
    setError(error, "A file or folder with that project name already exists.");
    return std::nullopt;
  }

  if (!parent.mkdir(directoryName)) {
    setError(error, "Brick could not create the project folder.");
    return std::nullopt;
  }

  const QString configPath = QDir(projectDirectory).filePath(kConfigFileName);
  QSettings config(configPath, QSettings::IniFormat);
  config.beginGroup("Project");
  config.setValue("name", name);
  config.setValue("formatVersion", kProjectFormatVersion);
  config.setValue("createdUtc", QDateTime::currentDateTimeUtc().toString(Qt::ISODate));
  config.endGroup();
  config.sync();

  if (config.status() != QSettings::NoError) {
    QFile::remove(configPath);
    parent.rmdir(directoryName);
    setError(error, "Brick could not write project.conf.");
    return std::nullopt;
  }

  return Project(name, QDir(projectDirectory).absolutePath());
}


std::optional<Project> Project::open(const QString& directory, QString* error) {
  const QFileInfo directoryInfo(directory);
  if (!directoryInfo.exists() || !directoryInfo.isDir()) {
    setError(error, "The selected project folder does not exist.");
    return std::nullopt;
  }

  const QString projectDirectory = directoryInfo.absoluteFilePath();
  const QString configPath = QDir(projectDirectory).filePath(kConfigFileName);
  if (!QFileInfo::exists(configPath)) {
    setError(error, "The selected folder does not contain project.conf.");
    return std::nullopt;
  }

  QSettings config(configPath, QSettings::IniFormat);
  config.beginGroup("Project");
  const QString name = config.value("name").toString();
  const int formatVersion = config.value("formatVersion").toInt();
  config.endGroup();

  if (config.status() != QSettings::NoError) {
    setError(error, "Brick could not read project.conf.");
    return std::nullopt;
  }
  if (!isValidProjectName(name)) {
    setError(error, "project.conf does not contain a valid project name.");
    return std::nullopt;
  }
  if (directoryInfo.fileName() != directoryNameForProject(name)) {
    setError(error,
             "The project folder name must contain only letters, numbers, and "
             "underscores and match the project name.");
    return std::nullopt;
  }
  if (formatVersion != kProjectFormatVersion) {
    setError(error, "This project uses an unsupported format version.");
    return std::nullopt;
  }

  QDir project(projectDirectory);
  const QStringList sceneDirectories = project.entryList(
      {"[0-9][0-9][0-9][0-9]_*"}, QDir::Dirs | QDir::NoDotAndDotDot,
      QDir::Name);
  std::vector<QString> scenes;
  std::vector<std::vector<QString>> shots;
  std::vector<std::vector<int>> takeCounts;
  scenes.reserve(sceneDirectories.size());
  shots.reserve(sceneDirectories.size());
  takeCounts.reserve(sceneDirectories.size());
  for (int index = 0; index < sceneDirectories.size(); ++index) {
    const QString& sceneDirectory = sceneDirectories[index];
    if (!sceneDirectory.startsWith(
            QString("%1_").arg(index + 1, 4, 10, QLatin1Char('0')))) {
      setError(error, "The project contains incorrectly numbered scenes.");
      return std::nullopt;
    }

    QSettings sceneConfig(
        project.filePath(sceneDirectory + '/' + kSceneConfigFileName),
        QSettings::IniFormat);
    sceneConfig.beginGroup("Scene");
    const QString sceneName = sceneConfig.value("name").toString();
    const int sceneFormatVersion = sceneConfig.value("formatVersion").toInt();
    sceneConfig.endGroup();
    if (sceneConfig.status() != QSettings::NoError ||
        !isValidProjectName(sceneName) ||
        sceneFormatVersion != kSceneFormatVersion ||
        sceneDirectory != directoryNameForScene(index, sceneName)) {
      setError(error, "The project contains an invalid scene.");
      return std::nullopt;
    }
    scenes.push_back(sceneName);

    QDir scene(project.filePath(sceneDirectory));
    const QStringList shotDirectories = scene.entryList(
        {"[0-9][0-9][0-9][0-9]_*"}, QDir::Dirs | QDir::NoDotAndDotDot,
        QDir::Name);
    std::vector<QString> sceneShots;
    std::vector<int> sceneTakeCounts;
    sceneShots.reserve(shotDirectories.size());
    sceneTakeCounts.reserve(shotDirectories.size());
    for (int shotIndex = 0; shotIndex < shotDirectories.size(); ++shotIndex) {
      const QString& shotDirectory = shotDirectories[shotIndex];
      if (!shotDirectory.startsWith(
              QString("%1_").arg(shotIndex + 1, 4, 10, QLatin1Char('0')))) {
        setError(error, "A scene contains incorrectly numbered shots.");
        return std::nullopt;
      }

      QSettings shotConfig(
          scene.filePath(shotDirectory + '/' + kShotConfigFileName),
          QSettings::IniFormat);
      shotConfig.beginGroup("Shot");
      const QString shotName = shotConfig.value("name").toString();
      const int shotFormatVersion = shotConfig.value("formatVersion").toInt();
      shotConfig.endGroup();
      if (shotConfig.status() != QSettings::NoError ||
          !isValidProjectName(shotName) ||
          shotFormatVersion != kShotFormatVersion ||
          shotDirectory != directoryNameForShot(shotIndex, shotName)) {
        setError(error, "A scene contains an invalid shot.");
        return std::nullopt;
      }
      sceneShots.push_back(shotName);

      QDir shot(scene.filePath(shotDirectory));
      const QStringList takeDirectories = shot.entryList(
          {"[0-9][0-9][0-9][0-9]_*"}, QDir::Dirs | QDir::NoDotAndDotDot,
          QDir::Name);
      for (int takeIndex = 0; takeIndex < takeDirectories.size(); ++takeIndex) {
        const QString& takeDirectory = takeDirectories[takeIndex];
        if (takeDirectory != directoryNameForTake(takeIndex)) {
          setError(error, "A shot contains incorrectly numbered takes.");
          return std::nullopt;
        }

        QSettings takeConfig(shot.filePath(takeDirectory + '/' +
                                           kTakeConfigFileName),
                             QSettings::IniFormat);
        takeConfig.beginGroup("Take");
        const int takeFormatVersion =
            takeConfig.value("formatVersion").toInt();
        takeConfig.endGroup();
        if (takeConfig.status() != QSettings::NoError ||
            takeFormatVersion != kTakeFormatVersion) {
          setError(error, "A shot contains an invalid take.");
          return std::nullopt;
        }
      }
      sceneTakeCounts.push_back(takeDirectories.size());
    }
    shots.push_back(std::move(sceneShots));
    takeCounts.push_back(std::move(sceneTakeCounts));
  }

  return Project(name, projectDirectory, std::move(scenes), std::move(shots),
                 std::move(takeCounts));
}


const QString& Project::name() const { return name_; }


const QString& Project::directory() const { return directory_; }


const std::vector<QString>& Project::scenes() const { return scenes_; }


const std::vector<QString>& Project::shots(int sceneIndex) const {
  return shots_.at(sceneIndex);
}


int Project::takeCount(int sceneIndex, int shotIndex) const {
  return takeCounts_.at(sceneIndex).at(shotIndex);
}


bool Project::createScene(const QString& name, QString* error) {
  if (!isValidProjectName(name)) {
    setError(error, "Scene names may contain only letters, numbers, and spaces.");
    return false;
  }
  if (std::find(scenes_.begin(), scenes_.end(), name) != scenes_.end()) {
    setError(error, "A scene with that name already exists.");
    return false;
  }
  if (scenes_.size() >= 9999) {
    setError(error, "A project cannot contain more than 9999 scenes.");
    return false;
  }

  QDir project(directory_);
  const QString sceneDirectory =
      directoryNameForScene(static_cast<int>(scenes_.size()), name);
  if (!project.mkdir(sceneDirectory)) {
    setError(error, "Brick could not create the scene folder.");
    return false;
  }

  const QString configPath =
      project.filePath(sceneDirectory + '/' + kSceneConfigFileName);
  QSettings config(configPath, QSettings::IniFormat);
  config.beginGroup("Scene");
  config.setValue("name", name);
  config.setValue("formatVersion", kSceneFormatVersion);
  config.setValue("createdUtc",
                  QDateTime::currentDateTimeUtc().toString(Qt::ISODate));
  config.endGroup();
  config.sync();
  if (config.status() != QSettings::NoError) {
    QDir(project.filePath(sceneDirectory)).removeRecursively();
    setError(error, "Brick could not write scene.conf.");
    return false;
  }

  scenes_.push_back(name);
  shots_.emplace_back();
  takeCounts_.emplace_back();
  return true;
}


bool Project::renameScene(int index, const QString& name, QString* error) {
  if (index < 0 || index >= static_cast<int>(scenes_.size())) {
    setError(error, "The selected scene does not exist.");
    return false;
  }
  if (!isValidProjectName(name)) {
    setError(error, "Scene names may contain only letters, numbers, and spaces.");
    return false;
  }
  const auto duplicate = std::find(scenes_.begin(), scenes_.end(), name);
  if (duplicate != scenes_.end() && duplicate != scenes_.begin() + index) {
    setError(error, "A scene with that name already exists.");
    return false;
  }

  const QString oldName = scenes_[index];
  if (name == oldName) {
    return true;
  }

  QDir project(directory_);
  const QString oldDirectory = directoryNameForScene(index, oldName);
  const QString newDirectory = directoryNameForScene(index, name);
  const bool directoryChanged = oldDirectory != newDirectory;
  if (directoryChanged && !project.rename(oldDirectory, newDirectory)) {
    setError(error, "Brick could not rename the scene folder.");
    return false;
  }

  bool configWritten = false;
  {
    QSettings config(
        project.filePath(newDirectory + '/' + kSceneConfigFileName),
        QSettings::IniFormat);
    config.setValue("Scene/name", name);
    config.sync();
    configWritten = config.status() == QSettings::NoError;
    if (!configWritten) {
      config.setValue("Scene/name", oldName);
      config.sync();
    }
  }
  if (!configWritten) {
    const bool directoryRestored =
        !directoryChanged || project.rename(newDirectory, oldDirectory);
    setError(error, directoryRestored
                        ? "Brick could not update scene.conf."
                        : "Brick could not update scene.conf or restore the scene folder.");
    return false;
  }

  scenes_[index] = name;
  return true;
}


bool Project::deleteScene(int index, QString* error) {
  if (index < 0 || index >= static_cast<int>(scenes_.size())) {
    setError(error, "The selected scene does not exist.");
    return false;
  }

  QDir project(directory_);
  const QString deletedDirectory = directoryNameForScene(index, scenes_[index]);
  const QString tombstone = ".brick-scene-delete-" +
                            QUuid::createUuid().toString(QUuid::Id128);
  if (!project.rename(deletedDirectory, tombstone)) {
    setError(error, "Brick could not prepare the scene folder for deletion.");
    return false;
  }

  std::vector<ContentDirectory> remaining;
  remaining.reserve(scenes_.size() - 1);
  for (int oldIndex = 0; oldIndex < static_cast<int>(scenes_.size()); ++oldIndex) {
    if (oldIndex != index) {
      remaining.push_back(
          {directoryNameForScene(oldIndex, scenes_[oldIndex]), scenes_[oldIndex]});
    }
  }
  if (!renameContentDirectories(directory_, remaining, "scene",
                                directoryNameForScene, error)) {
    project.rename(tombstone, deletedDirectory);
    return false;
  }

  scenes_.erase(scenes_.begin() + index);
  shots_.erase(shots_.begin() + index);
  takeCounts_.erase(takeCounts_.begin() + index);
  QDir(project.filePath(tombstone)).removeRecursively();
  return true;
}


bool Project::moveScene(int from, int to, QString* error) {
  const int sceneCount = static_cast<int>(scenes_.size());
  if (from < 0 || from >= sceneCount || to < 0 || to >= sceneCount) {
    setError(error, "The selected scene cannot be moved there.");
    return false;
  }
  if (from == to) {
    return true;
  }

  std::vector<ContentDirectory> reordered;
  reordered.reserve(scenes_.size());
  for (int index = 0; index < sceneCount; ++index) {
    reordered.push_back(
        {directoryNameForScene(index, scenes_[index]), scenes_[index]});
  }
  const ContentDirectory moved = reordered[from];
  reordered.erase(reordered.begin() + from);
  reordered.insert(reordered.begin() + to, moved);
  if (!renameContentDirectories(directory_, reordered, "scene",
                                directoryNameForScene, error)) {
    return false;
  }

  const QString movedName = scenes_[from];
  scenes_.erase(scenes_.begin() + from);
  scenes_.insert(scenes_.begin() + to, movedName);
  auto movedShots = std::move(shots_[from]);
  shots_.erase(shots_.begin() + from);
  shots_.insert(shots_.begin() + to, std::move(movedShots));
  auto movedTakeCounts = std::move(takeCounts_[from]);
  takeCounts_.erase(takeCounts_.begin() + from);
  takeCounts_.insert(takeCounts_.begin() + to, std::move(movedTakeCounts));
  return true;
}


bool Project::createShot(int sceneIndex, const QString& name, QString* error) {
  if (sceneIndex < 0 || sceneIndex >= static_cast<int>(scenes_.size())) {
    setError(error, "The selected scene does not exist.");
    return false;
  }
  if (!isValidProjectName(name)) {
    setError(error, "Shot names may contain only letters, numbers, and spaces.");
    return false;
  }

  auto& sceneShots = shots_[sceneIndex];
  if (std::find(sceneShots.begin(), sceneShots.end(), name) !=
      sceneShots.end()) {
    setError(error, "A shot with that name already exists in this scene.");
    return false;
  }
  if (sceneShots.size() >= 9999) {
    setError(error, "A scene cannot contain more than 9999 shots.");
    return false;
  }

  QDir scene(QDir(directory_).filePath(
      directoryNameForScene(sceneIndex, scenes_[sceneIndex])));
  const QString shotDirectory =
      directoryNameForShot(static_cast<int>(sceneShots.size()), name);
  if (!scene.mkdir(shotDirectory)) {
    setError(error, "Brick could not create the shot folder.");
    return false;
  }

  QDir shot(scene.filePath(shotDirectory));
  if (!shot.mkdir("test_shots")) {
    shot.removeRecursively();
    setError(error, "Brick could not create the test shots folder.");
    return false;
  }

  const QString configPath =
      scene.filePath(shotDirectory + '/' + kShotConfigFileName);
  QSettings config(configPath, QSettings::IniFormat);
  config.beginGroup("Shot");
  config.setValue("name", name);
  config.setValue("formatVersion", kShotFormatVersion);
  config.setValue("createdUtc",
                  QDateTime::currentDateTimeUtc().toString(Qt::ISODate));
  config.endGroup();
  config.sync();
  if (config.status() != QSettings::NoError) {
    QDir(scene.filePath(shotDirectory)).removeRecursively();
    setError(error, "Brick could not write shot.conf.");
    return false;
  }

  if (!createTakeDirectory(shot, 0, error)) {
    shot.removeRecursively();
    return false;
  }

  sceneShots.push_back(name);
  takeCounts_[sceneIndex].push_back(1);
  return true;
}


bool Project::renameShot(int sceneIndex, int shotIndex, const QString& name,
                         QString* error) {
  if (sceneIndex < 0 || sceneIndex >= static_cast<int>(scenes_.size()) ||
      shotIndex < 0 || shotIndex >= static_cast<int>(shots_[sceneIndex].size())) {
    setError(error, "The selected shot does not exist.");
    return false;
  }
  if (!isValidProjectName(name)) {
    setError(error, "Shot names may contain only letters, numbers, and spaces.");
    return false;
  }

  auto& sceneShots = shots_[sceneIndex];
  const auto duplicate = std::find(sceneShots.begin(), sceneShots.end(), name);
  if (duplicate != sceneShots.end() &&
      duplicate != sceneShots.begin() + shotIndex) {
    setError(error, "A shot with that name already exists in this scene.");
    return false;
  }

  const QString oldName = sceneShots[shotIndex];
  if (name == oldName) {
    return true;
  }

  QDir scene(QDir(directory_).filePath(
      directoryNameForScene(sceneIndex, scenes_[sceneIndex])));
  const QString oldDirectory = directoryNameForShot(shotIndex, oldName);
  const QString newDirectory = directoryNameForShot(shotIndex, name);
  const bool directoryChanged = oldDirectory != newDirectory;
  if (directoryChanged && !scene.rename(oldDirectory, newDirectory)) {
    setError(error, "Brick could not rename the shot folder.");
    return false;
  }

  bool configWritten = false;
  {
    QSettings config(scene.filePath(newDirectory + '/' + kShotConfigFileName),
                     QSettings::IniFormat);
    config.setValue("Shot/name", name);
    config.sync();
    configWritten = config.status() == QSettings::NoError;
    if (!configWritten) {
      config.setValue("Shot/name", oldName);
      config.sync();
    }
  }
  if (!configWritten) {
    const bool directoryRestored =
        !directoryChanged || scene.rename(newDirectory, oldDirectory);
    setError(error, directoryRestored
                        ? "Brick could not update shot.conf."
                        : "Brick could not update shot.conf or restore the shot folder.");
    return false;
  }

  sceneShots[shotIndex] = name;
  return true;
}


bool Project::deleteShot(int sceneIndex, int shotIndex, QString* error) {
  if (sceneIndex < 0 || sceneIndex >= static_cast<int>(scenes_.size()) ||
      shotIndex < 0 || shotIndex >= static_cast<int>(shots_[sceneIndex].size())) {
    setError(error, "The selected shot does not exist.");
    return false;
  }

  auto& sceneShots = shots_[sceneIndex];
  QDir scene(QDir(directory_).filePath(
      directoryNameForScene(sceneIndex, scenes_[sceneIndex])));
  const QString deletedDirectory =
      directoryNameForShot(shotIndex, sceneShots[shotIndex]);
  const QString tombstone =
      ".brick-shot-delete-" + QUuid::createUuid().toString(QUuid::Id128);
  if (!scene.rename(deletedDirectory, tombstone)) {
    setError(error, "Brick could not prepare the shot folder for deletion.");
    return false;
  }

  std::vector<ContentDirectory> remaining;
  remaining.reserve(sceneShots.size() - 1);
  for (int oldIndex = 0; oldIndex < static_cast<int>(sceneShots.size());
       ++oldIndex) {
    if (oldIndex != shotIndex) {
      remaining.push_back({directoryNameForShot(oldIndex, sceneShots[oldIndex]),
                           sceneShots[oldIndex]});
    }
  }
  if (!renameContentDirectories(scene.absolutePath(), remaining, "shot",
                                directoryNameForShot, error)) {
    scene.rename(tombstone, deletedDirectory);
    return false;
  }

  sceneShots.erase(sceneShots.begin() + shotIndex);
  takeCounts_[sceneIndex].erase(takeCounts_[sceneIndex].begin() + shotIndex);
  QDir(scene.filePath(tombstone)).removeRecursively();
  return true;
}


bool Project::moveShot(int sceneIndex, int from, int to, QString* error) {
  if (sceneIndex < 0 || sceneIndex >= static_cast<int>(scenes_.size())) {
    setError(error, "The selected scene does not exist.");
    return false;
  }

  auto& sceneShots = shots_[sceneIndex];
  const int shotCount = static_cast<int>(sceneShots.size());
  if (from < 0 || from >= shotCount || to < 0 || to >= shotCount) {
    setError(error, "The selected shot cannot be moved there.");
    return false;
  }
  if (from == to) {
    return true;
  }

  std::vector<ContentDirectory> reordered;
  reordered.reserve(sceneShots.size());
  for (int index = 0; index < shotCount; ++index) {
    reordered.push_back(
        {directoryNameForShot(index, sceneShots[index]), sceneShots[index]});
  }
  const ContentDirectory moved = reordered[from];
  reordered.erase(reordered.begin() + from);
  reordered.insert(reordered.begin() + to, moved);

  const QString scenePath = QDir(directory_).filePath(
      directoryNameForScene(sceneIndex, scenes_[sceneIndex]));
  if (!renameContentDirectories(scenePath, reordered, "shot",
                                directoryNameForShot, error)) {
    return false;
  }

  const QString movedName = sceneShots[from];
  sceneShots.erase(sceneShots.begin() + from);
  sceneShots.insert(sceneShots.begin() + to, movedName);
  const int movedTakeCount = takeCounts_[sceneIndex][from];
  takeCounts_[sceneIndex].erase(takeCounts_[sceneIndex].begin() + from);
  takeCounts_[sceneIndex].insert(takeCounts_[sceneIndex].begin() + to,
                                 movedTakeCount);
  return true;
}


bool Project::createTake(int sceneIndex, int shotIndex, QString* error) {
  if (sceneIndex < 0 || sceneIndex >= static_cast<int>(scenes_.size()) ||
      shotIndex < 0 || shotIndex >= static_cast<int>(shots_[sceneIndex].size())) {
    setError(error, "The selected shot does not exist.");
    return false;
  }
  int& count = takeCounts_[sceneIndex][shotIndex];
  if (count >= 9999) {
    setError(error, "A shot cannot contain more than 9999 takes.");
    return false;
  }

  QDir shot(QDir(directory_).filePath(
      directoryNameForScene(sceneIndex, scenes_[sceneIndex]) + '/' +
      directoryNameForShot(shotIndex, shots_[sceneIndex][shotIndex])));
  if (!createTakeDirectory(shot, count, error)) {
    return false;
  }

  ++count;
  return true;
}


bool Project::deleteTake(int sceneIndex, int shotIndex, int takeIndex,
                         QString* error) {
  if (sceneIndex < 0 || sceneIndex >= static_cast<int>(scenes_.size()) ||
      shotIndex < 0 || shotIndex >= static_cast<int>(shots_[sceneIndex].size()) ||
      takeIndex < 0 || takeIndex >= takeCounts_[sceneIndex][shotIndex]) {
    setError(error, "The selected take does not exist.");
    return false;
  }

  int& count = takeCounts_[sceneIndex][shotIndex];
  QDir shot(QDir(directory_).filePath(
      directoryNameForScene(sceneIndex, scenes_[sceneIndex]) + '/' +
      directoryNameForShot(shotIndex, shots_[sceneIndex][shotIndex])));
  const QString deletedDirectory = directoryNameForTake(takeIndex);
  const QString tombstone =
      ".brick-take-delete-" + QUuid::createUuid().toString(QUuid::Id128);
  if (!shot.rename(deletedDirectory, tombstone)) {
    setError(error, "Brick could not prepare the take folder for deletion.");
    return false;
  }

  std::vector<ContentDirectory> remaining;
  remaining.reserve(count - 1);
  for (int oldIndex = 0; oldIndex < count; ++oldIndex) {
    if (oldIndex != takeIndex) {
      remaining.push_back({directoryNameForTake(oldIndex), {}});
    }
  }
  if (!renameContentDirectories(shot.absolutePath(), remaining, "take",
                                directoryNameForTake, error)) {
    shot.rename(tombstone, deletedDirectory);
    return false;
  }

  --count;
  QDir(shot.filePath(tombstone)).removeRecursively();
  return true;
}
