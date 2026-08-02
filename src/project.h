#pragma once

#include <QString>

#include <optional>
#include <vector>

class Project {
 public:
  static std::optional<Project> create(const QString& parentDirectory,
                                       const QString& name, QString* error);
  static std::optional<Project> open(const QString& directory, QString* error);

  [[nodiscard]] const QString& name() const;
  [[nodiscard]] const QString& directory() const;
  [[nodiscard]] const std::vector<QString>& scenes() const;
  [[nodiscard]] const std::vector<QString>& shots(int sceneIndex) const;
  [[nodiscard]] int takeCount(int sceneIndex, int shotIndex) const;

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

 private:
  Project(QString name, QString directory, std::vector<QString> scenes = {},
          std::vector<std::vector<QString>> shots = {},
          std::vector<std::vector<int>> takeCounts = {});

  QString name_;
  QString directory_;
  std::vector<QString> scenes_;
  std::vector<std::vector<QString>> shots_;
  std::vector<std::vector<int>> takeCounts_;
};
