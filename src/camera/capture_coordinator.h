#pragma once

#include "camera.h"
#include "storage/capture_recovery.h"

#include <QObject>

#include <memory>
#include <optional>
#include <vector>

class CaptureCoordinator : public QObject {
  Q_OBJECT

 public:
  explicit CaptureCoordinator(QObject* parent = nullptr);
  ~CaptureCoordinator() override;
  CaptureRecovery& recovery();

  [[nodiscard]] std::vector<CameraDevice> availableDevices() const;
  bool selectCamera(const std::optional<CameraDevice>& device,
                    QString* error = nullptr);
  [[nodiscard]] CameraSession* cameraSession() const;
  [[nodiscard]] bool isReady() const;
  [[nodiscard]] bool isCapturing() const;
  [[nodiscard]] quint64 requestCapture(CapturePurpose purpose,
                                       const CaptureDestination& destination,
                                       const TestShot& metadata = {},
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
  CaptureRecovery recovery_;
  std::optional<ActiveCapture> activeCapture_;
  quint64 nextCaptureId_ = 1;
  quint64 cameraGeneration_ = 0;
};
