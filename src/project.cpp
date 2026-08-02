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
constexpr auto kConfigFileName = "project.conf";
constexpr auto kSceneConfigFileName = "scene.conf";

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

void setError(QString* error, const QString& message) {
  if (error != nullptr) {
    *error = message;
  }
}

struct SceneDirectory {
  QString oldName;
  QString sceneName;
};

bool renameSceneDirectories(const QString& projectDirectory,
                            const std::vector<SceneDirectory>& scenes,
                            QString* error) {
  QDir project(projectDirectory);
  const QString token = QUuid::createUuid().toString(QUuid::Id128);
  std::vector<QString> temporaryNames;
  temporaryNames.reserve(scenes.size());

  for (std::size_t index = 0; index < scenes.size(); ++index) {
    const QString temporaryName = ".brick-scene-" + token + '-' +
                                  QString::number(index);
    temporaryNames.push_back(temporaryName);
    if (!project.rename(scenes[index].oldName, temporaryName)) {
      for (std::size_t rollback = 0; rollback < index; ++rollback) {
        project.rename(temporaryNames[rollback], scenes[rollback].oldName);
      }
      setError(error, "Brick could not renumber the scene folders.");
      return false;
    }
  }

  std::size_t renamed = 0;
  for (; renamed < scenes.size(); ++renamed) {
    const QString newName =
        directoryNameForScene(static_cast<int>(renamed), scenes[renamed].sceneName);
    if (!project.rename(temporaryNames[renamed], newName)) {
      break;
    }
  }
  if (renamed == scenes.size()) {
    return true;
  }

  for (std::size_t index = 0; index < renamed; ++index) {
    project.rename(directoryNameForScene(static_cast<int>(index),
                                         scenes[index].sceneName),
                   temporaryNames[index]);
  }
  for (std::size_t index = 0; index < scenes.size(); ++index) {
    project.rename(temporaryNames[index], scenes[index].oldName);
  }
  setError(error, "Brick could not renumber the scene folders.");
  return false;
}

}  // namespace


Project::Project(QString name, QString directory, std::vector<QString> scenes)
    : name_(std::move(name)),
      directory_(std::move(directory)),
      scenes_(std::move(scenes)) {}


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
  scenes.reserve(sceneDirectories.size());
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
  }

  return Project(name, projectDirectory, std::move(scenes));
}


const QString& Project::name() const { return name_; }


const QString& Project::directory() const { return directory_; }


const std::vector<QString>& Project::scenes() const { return scenes_; }


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

  std::vector<SceneDirectory> remaining;
  remaining.reserve(scenes_.size() - 1);
  for (int oldIndex = 0; oldIndex < static_cast<int>(scenes_.size()); ++oldIndex) {
    if (oldIndex != index) {
      remaining.push_back(
          {directoryNameForScene(oldIndex, scenes_[oldIndex]), scenes_[oldIndex]});
    }
  }
  if (!renameSceneDirectories(directory_, remaining, error)) {
    project.rename(tombstone, deletedDirectory);
    return false;
  }

  scenes_.erase(scenes_.begin() + index);
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

  std::vector<SceneDirectory> reordered;
  reordered.reserve(scenes_.size());
  for (int index = 0; index < sceneCount; ++index) {
    reordered.push_back(
        {directoryNameForScene(index, scenes_[index]), scenes_[index]});
  }
  const SceneDirectory moved = reordered[from];
  reordered.erase(reordered.begin() + from);
  reordered.insert(reordered.begin() + to, moved);
  if (!renameSceneDirectories(directory_, reordered, error)) {
    return false;
  }

  const QString movedName = scenes_[from];
  scenes_.erase(scenes_.begin() + from);
  scenes_.insert(scenes_.begin() + to, movedName);
  return true;
}
