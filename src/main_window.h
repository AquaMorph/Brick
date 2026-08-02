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
  void renameScene();
  void deleteScene();
  void moveScene(int offset);
  void refreshScenes(int selectedRow = -1);
  void updateSceneActions();
  void createShot();
  void renameShot();
  void deleteShot();
  void moveShot(int offset);
  void refreshShots(int selectedRow = -1);
  void updateShotActions();
  void setProject(Project project);

  std::optional<Project> project_;
  QListWidget* sceneList_ = nullptr;
  QPushButton* newSceneButton_ = nullptr;
  QPushButton* renameSceneButton_ = nullptr;
  QPushButton* deleteSceneButton_ = nullptr;
  QPushButton* moveSceneUpButton_ = nullptr;
  QPushButton* moveSceneDownButton_ = nullptr;
  QListWidget* shotList_ = nullptr;
  QPushButton* newShotButton_ = nullptr;
  QPushButton* renameShotButton_ = nullptr;
  QPushButton* deleteShotButton_ = nullptr;
  QPushButton* moveShotUpButton_ = nullptr;
  QPushButton* moveShotDownButton_ = nullptr;
};
