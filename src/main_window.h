#pragma once

#include "project.h"

#include <QMainWindow>

#include <optional>

class MainWindow : public QMainWindow {
 public:
  explicit MainWindow(QWidget* parent = nullptr);

 private:
  void createProject();
  void openProject();
  void setProject(Project project);

  std::optional<Project> project_;
};
