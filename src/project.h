#pragma once

#include <QString>

#include <optional>

class Project {
 public:
  static std::optional<Project> create(const QString& parentDirectory,
                                       const QString& name, QString* error);
  static std::optional<Project> open(const QString& directory, QString* error);

  [[nodiscard]] const QString& name() const;
  [[nodiscard]] const QString& directory() const;

 private:
  Project(QString name, QString directory);

  QString name_;
  QString directory_;
};
