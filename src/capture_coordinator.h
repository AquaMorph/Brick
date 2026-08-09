#pragma once

#include "camera.h"

#include <QObject>
#include <QTemporaryDir>

#include <memory>
#include <optional>
#include <vector>

enum class CapturePurpose { TestShot, AnimationFrame };

class CaptureCoordinator : public QObject {
  Q_OBJECT

 public:
  explicit CaptureCoordinator(QObject* parent = nullptr);

  [[nodiscard]] std::vector<CameraDevice> availableDevices() const;
  bool selectCamera(const std::optional<CameraDevice>& device,
                    QString* error = nullptr);
  [[nodiscard]] CameraSession* cameraSession() const;
  [[nodiscard]] bool isReady() const;
  [[nodiscard]] bool isCapturing() const;
  [[nodiscard]] quint64 requestCapture(CapturePurpose purpose,
                                       QString* error = nullptr);

 signals:
  void cameraChanged(CameraSession* camera);
  void previewFrame(const QImage& image);
  void settingsChanged();
  void cameraError(const QString& message);
  void captureStateChanged(bool capturing);
  void captureCompleted(quint64 captureId, CapturePurpose purpose,
                        const QString& filePath);
  void captureFailed(quint64 captureId, CapturePurpose purpose,
                     const QString& message);

 private:
  struct ActiveCapture {
    quint64 id;
    quint64 cameraGeneration;
    CapturePurpose purpose;
    QString destinationBase;
  };

  void completeCapture(quint64 captureId, const QString& filePath);
  void failCapture(quint64 captureId, const QString& message);

  std::unique_ptr<CameraSession> camera_;
  QTemporaryDir captureDirectory_;
  std::optional<ActiveCapture> activeCapture_;
  quint64 nextCaptureId_ = 1;
  quint64 cameraGeneration_ = 0;
};
