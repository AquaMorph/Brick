#pragma once

#include "camera/capture_coordinator.h"
#include "core/project.h"

#include <QImage>
#include <QWidget>

#include <optional>
#include <vector>

class QComboBox;
class QFormLayout;
class QLabel;
class QListWidget;
class QPushButton;
class QScrollArea;
class QVBoxLayout;

class CinematographyWidget : public QWidget {
  Q_OBJECT

 public:
  explicit CinematographyWidget(CaptureCoordinator* captureCoordinator,
                                QWidget* parent = nullptr);

  void setShot(Project* project, int sceneIndex, int shotIndex);

 private:
  struct PendingCapture {
    quint64 requestId;
    quint64 contextRevision;
    Project* project;
    QString projectDirectory;
    QString shotDirectory;
    int sceneIndex;
    int shotIndex;
    QString cameraDisplayName;
    ShotCameraSettings cameraSettings;
    std::map<QString, QString> displaySettings;
  };

  void refreshCameras();
  void selectCamera(int index);
  void refreshSettings();
  void rebuildSettings();
  void applyCameraSetting(const QString& id, const QString& value);
  void saveCameraSettings();
  void restoreShotSettings();
  void capture();
  void importCapture(const QString& filePath);
  void refreshGallery(const QString& selectedFileName = {});
  void showTestShot(int row);
  void showLiveView();
  void setLivePreview(bool live);
  void deleteSelectedTestShot();
  void restoreSelectedTestShotSettings();
  [[nodiscard]] ShotCameraSettings currentCameraSettings() const;
  [[nodiscard]] std::map<QString, QString> currentDisplaySettings() const;
  void setConnectionStatus(const QString& text, bool connected);
  void setOperationStatus(const QString& text);
  void clearError();
  void showError(const QString& message);
  void updatePreview(const QImage& image, const QString& placeholder = {});

  Project* project_ = nullptr;
  int sceneIndex_ = -1;
  int shotIndex_ = -1;
  std::vector<CameraDevice> devices_;
  CaptureCoordinator* captureCoordinator_ = nullptr;
  CameraSession* camera_ = nullptr;
  std::vector<CameraSetting> settings_;
  std::vector<TestShot> testShots_;
  std::optional<PendingCapture> pendingCapture_;
  quint64 contextRevision_ = 0;
  quint64 previewLoadId_ = 0;
  QImage previewImage_;

  QLabel* shotLabel_ = nullptr;
  QLabel* previewLabel_ = nullptr;
  QLabel* connectionStatusLabel_ = nullptr;
  QLabel* operationStatusLabel_ = nullptr;
  QLabel* errorLabel_ = nullptr;
  QLabel* galleryStatusLabel_ = nullptr;
  QComboBox* cameraCombo_ = nullptr;
  QPushButton* cameraRefreshButton_ = nullptr;
  QPushButton* captureButton_ = nullptr;
  QPushButton* liveButton_ = nullptr;
  QFormLayout* settingsLayout_ = nullptr;
  QWidget* settingsWidget_ = nullptr;
  QListWidget* gallery_ = nullptr;
  QLabel* metadataLabel_ = nullptr;
  QPushButton* restoreButton_ = nullptr;
  QPushButton* deleteButton_ = nullptr;
};
