#pragma once

#include "capture_coordinator.h"
#include "project.h"

#include <QElapsedTimer>
#include <QImage>
#include <QWidget>

#include <optional>
#include <vector>

class QCheckBox;
class QComboBox;
class QLabel;
class QListWidget;
class QPushButton;
class QSlider;
class QTimer;

class AnimationWidget : public QWidget {
 public:
  explicit AnimationWidget(CaptureCoordinator* captureCoordinator,
                            QWidget* parent = nullptr);

  void setActiveTake(Project* project,
                     const std::optional<Project::ActiveTake>& activeTake);

 private:
  struct PendingCapture {
    quint64 requestId;
    quint64 contextRevision;
    Project* project;
    QString projectDirectory;
    QString takeDirectory;
    Project::ActiveTake take;
  };

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
  CaptureCoordinator* captureCoordinator_ = nullptr;
  std::vector<AnimationFrame> frames_;
  std::optional<PendingCapture> pendingCapture_;
  quint64 contextRevision_ = 0;
  QImage liveImage_;
  std::vector<QImage> playbackImages_;
  QElapsedTimer playbackClock_;
  int playbackRow_ = -1;
  qint64 playbackTransition_ = 0;
  int playbackStartRow_ = 0;
  bool showingLive_ = true;

  QWidget* canvas_ = nullptr;
  QLabel* takeLabel_ = nullptr;
  QLabel* cameraLabel_ = nullptr;
  QLabel* statusLabel_ = nullptr;
  QLabel* errorLabel_ = nullptr;
  QLabel* frameCountLabel_ = nullptr;
  QListWidget* frameStrip_ = nullptr;
  QPushButton* captureButton_ = nullptr;
  QPushButton* playButton_ = nullptr;
  QPushButton* deleteButton_ = nullptr;
  QCheckBox* onionCheck_ = nullptr;
  QCheckBox* loopCheck_ = nullptr;
  QComboBox* playbackQuality_ = nullptr;
  QSlider* onionOpacity_ = nullptr;
  QComboBox* fpsCombo_ = nullptr;
  QTimer* playbackTimer_ = nullptr;
};
