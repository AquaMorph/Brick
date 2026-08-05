#pragma once

#include <QDateTime>
#include <QString>

#include <map>
#include <optional>
#include <vector>

struct ShotCameraSettings {
  QString backend;
  QString deviceId;
  std::map<QString, QString> settings;

  bool operator==(const ShotCameraSettings&) const = default;
};

struct TestShot {
  QString filePath;
  QString fileName;
  QDateTime capturedUtc;
  QString cameraDisplayName;
  QString cameraBackend;
  QString cameraDeviceId;
  std::map<QString, QString> capturedSettings;
  std::map<QString, QString> displaySettings;

  bool operator==(const TestShot&) const = default;
};

class Project {
 public:
  struct ActiveTake {
    int sceneIndex;
    int shotIndex;
    int takeIndex;

    bool operator==(const ActiveTake&) const = default;
  };

  static std::optional<Project> create(const QString& parentDirectory,
                                       const QString& name, QString* error);
  static std::optional<Project> open(const QString& directory, QString* error);

  [[nodiscard]] const QString& name() const;
  [[nodiscard]] const QString& directory() const;
  [[nodiscard]] const std::vector<QString>& scenes() const;
  [[nodiscard]] const std::vector<QString>& shots(int sceneIndex) const;
  [[nodiscard]] int takeCount(int sceneIndex, int shotIndex) const;
  [[nodiscard]] const std::optional<ActiveTake>& activeTake() const;
  [[nodiscard]] QString shotDirectory(int sceneIndex, int shotIndex,
                                      QString* error = nullptr) const;
  [[nodiscard]] QString testShotDirectory(int sceneIndex, int shotIndex,
                                          QString* error = nullptr) const;
  [[nodiscard]] std::optional<ShotCameraSettings> currentShotCameraSettings(
      int sceneIndex, int shotIndex, QString* error = nullptr) const;
  [[nodiscard]] std::vector<TestShot> testShots(
      int sceneIndex, int shotIndex, QString* error = nullptr) const;

  bool createScene(const QString& name, QString* error);
  bool renameScene(int index, const QString& name, QString* error);
  bool deleteScene(int index, QString* error);
  bool moveScene(int from, int to, QString* error);
  bool createShot(int sceneIndex, const QString& name, QString* error);
  bool renameShot(int sceneIndex, int shotIndex, const QString& name,
                  QString* error);
  bool deleteShot(int sceneIndex, int shotIndex, QString* error);
  bool moveShot(int sceneIndex, int from, int to, QString* error);
  bool createTake(int sceneIndex, int shotIndex, QString* error);
  bool deleteTake(int sceneIndex, int shotIndex, int takeIndex,
                   QString* error);
  bool selectTake(int sceneIndex, int shotIndex, int takeIndex, QString* error);
  bool saveCurrentShotCameraSettings(int sceneIndex, int shotIndex,
                                     const ShotCameraSettings& settings,
                                     QString* error);
  std::optional<TestShot> importTestShot(
      int sceneIndex, int shotIndex, const QString& capturedImagePath,
      const QDateTime& capturedUtc, const QString& cameraDisplayName,
      const ShotCameraSettings& camera,
      const std::map<QString, QString>& capturedSettings,
      const std::map<QString, QString>& displaySettings, QString* error);
  bool deleteTestShot(int sceneIndex, int shotIndex, const QString& fileName,
                      QString* error);

 private:
  Project(QString name, QString directory, std::vector<QString> scenes = {},
           std::vector<std::vector<QString>> shots = {},
           std::vector<std::vector<int>> takeCounts = {},
           std::optional<ActiveTake> activeTake = std::nullopt);
  bool saveActiveTake(QString* error);

  QString name_;
  QString directory_;
  std::vector<QString> scenes_;
  std::vector<std::vector<QString>> shots_;
  std::vector<std::vector<int>> takeCounts_;
  std::optional<ActiveTake> activeTake_;
};
