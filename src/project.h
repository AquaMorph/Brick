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

  bool createScene(const QString& name, QString* error);
  bool renameScene(int index, const QString& name, QString* error);
  bool deleteScene(int index, QString* error);
  bool moveScene(int from, int to, QString* error);

 private:
  Project(QString name, QString directory, std::vector<QString> scenes = {});

  QString name_;
  QString directory_;
  std::vector<QString> scenes_;
};
