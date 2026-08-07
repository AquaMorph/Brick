#pragma once

#include "camera.h"
#include "project.h"

#include <QElapsedTimer>
#include <QImage>
#include <QTemporaryDir>
#include <QWidget>

#include <optional>
#include <vector>

class CinematographyWidget;
class QCheckBox;
class QComboBox;
class QLabel;
class QListWidget;
class QPushButton;
class QSlider;
class QTimer;

class AnimationWidget : public QWidget {
 public:
  explicit AnimationWidget(CinematographyWidget* cinematography,
                           QWidget* parent = nullptr);

  void setActiveTake(Project* project,
                     const std::optional<Project::ActiveTake>& activeTake);

 private:
  struct PendingCapture {
    Project* project;
    QString projectDirectory;
    QString takeDirectory;
    Project::ActiveTake take;
  };

  void attachCamera(CameraSession* camera);
  void capture();
  void importCapture(const QString& filePath);
  void refreshFrames(int selectedNumber = -1);
  void selectFrame(int row);
  void showLiveView();
  void updateOnionFrame();
  void deleteSelectedFrame();
  void togglePlayback();
  void stopPlayback(bool returnToLive = true);
  void advancePlayback();
  void schedulePlaybackFrame();
  void navigate(int offset);
  void showFrame(int row);
  void showPlaybackFrame(int row);
  void updateControls();
  void showError(const QString& message);

  Project* project_ = nullptr;
  std::optional<Project::ActiveTake> activeTake_;
  CinematographyWidget* cinematography_ = nullptr;
  CameraSession* camera_ = nullptr;
  std::vector<AnimationFrame> frames_;
  std::optional<PendingCapture> pendingCapture_;
  QTemporaryDir captureDirectory_;
  QImage liveImage_;
  std::vector<QImage> playbackImages_;
  QElapsedTimer playbackClock_;
  int playbackRow_ = -1;
  qint64 playbackTransition_ = 0;
  int playbackStartRow_ = 0;
  bool showingLive_ = true;
  bool cinematographyCaptureActive_ = false;

  QWidget* canvas_ = nullptr;
  QLabel* takeLabel_ = nullptr;
  QLabel* cameraLabel_ = nullptr;
  QLabel* statusLabel_ = nullptr;
  QLabel* errorLabel_ = nullptr;
  QLabel* frameCountLabel_ = nullptr;
  QListWidget* frameStrip_ = nullptr;
  QPushButton* captureButton_ = nullptr;
  QPushButton* liveButton_ = nullptr;
  QPushButton* playButton_ = nullptr;
  QPushButton* deleteButton_ = nullptr;
  QCheckBox* onionCheck_ = nullptr;
  QCheckBox* loopCheck_ = nullptr;
  QComboBox* playbackQuality_ = nullptr;
  QSlider* onionOpacity_ = nullptr;
  QComboBox* fpsCombo_ = nullptr;
  QTimer* playbackTimer_ = nullptr;
};
