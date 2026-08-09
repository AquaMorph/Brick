#pragma once

#include "project.h"

#include <QMainWindow>

#include <optional>

class QListWidget;
class QLabel;
class QPushButton;
class CinematographyWidget;
class AnimationWidget;
class CaptureCoordinator;

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
  void createTake();
  void deleteTake();
  void selectTake();
  void refreshTakes(int selectedRow = -1);
  void updateTakeActions();
  void updateActiveTakeViews();
  void updateCinematographyShot();
  void setProject(Project project);

  std::optional<Project> project_;
  QLabel* filmTitleLabel_ = nullptr;
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
  QListWidget* takeList_ = nullptr;
  QPushButton* newTakeButton_ = nullptr;
  QPushButton* deleteTakeButton_ = nullptr;
  QPushButton* selectTakeButton_ = nullptr;
  CaptureCoordinator* captureCoordinator_ = nullptr;
  CinematographyWidget* cinematographyWidget_ = nullptr;
  AnimationWidget* animationWidget_ = nullptr;
};
