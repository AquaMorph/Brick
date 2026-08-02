#pragma once

#include "project.h"

#include <QMainWindow>

#include <optional>

class QListWidget;
class QPushButton;

class MainWindow : public QMainWindow {
 public:
  explicit MainWindow(QWidget* parent = nullptr);

 private:
  void createProject();
  void openProject();
  void createScene();
  void deleteScene();
  void moveScene(int offset);
  void refreshScenes(int selectedRow = -1);
  void updateSceneActions();
  void setProject(Project project);

  std::optional<Project> project_;
  QListWidget* sceneList_ = nullptr;
  QPushButton* newSceneButton_ = nullptr;
  QPushButton* deleteSceneButton_ = nullptr;
  QPushButton* moveSceneUpButton_ = nullptr;
  QPushButton* moveSceneDownButton_ = nullptr;
};
