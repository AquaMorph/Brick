#include "project.h"

#include <QDateTime>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QRegularExpression>
#include <QSettings>

#include <utility>

namespace {

constexpr int kProjectFormatVersion = 1;
constexpr auto kConfigFileName = "project.conf";

bool isValidProjectName(const QString& name) {
  static const QRegularExpression validName(R"(^[A-Za-z0-9]+(?: +[A-Za-z0-9]+)*$)");
  return validName.match(name).hasMatch();
}

QString directoryNameForProject(QString name) {
  return name.replace(' ', '_');
}

void setError(QString* error, const QString& message) {
  if (error != nullptr) {
    *error = message;
  }
}

}  // namespace


Project::Project(QString name, QString directory)
    : name_(std::move(name)), directory_(std::move(directory)) {}


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

  return Project(name, projectDirectory);
}


const QString& Project::name() const { return name_; }


const QString& Project::directory() const { return directory_; }
