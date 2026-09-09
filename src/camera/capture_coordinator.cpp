#include "capture_coordinator.h"

#include "camera_registry.h"

#include <QFileInfo>
#include <QMetaObject>

#include <utility>

CaptureCoordinator::CaptureCoordinator(QObject* parent) : QObject(parent) {}


CaptureCoordinator::~CaptureCoordinator() {
  if (camera_) {
    camera_->disconnect(this);
    camera_->stop();
    camera_.reset();
  }
}


CaptureRecovery& CaptureCoordinator::recovery() { return recovery_; }


std::vector<CameraDevice> CaptureCoordinator::availableDevices() const {
  return availableCameras();
}


bool CaptureCoordinator::selectCamera(
    const std::optional<CameraDevice>& device, QString* error) {
  if (activeCapture_.has_value()) {
    if (error != nullptr) {
      *error = "The camera cannot be changed while a capture is active.";
    }
    return false;
  }

  if (camera_) {
    emit cameraChanged(nullptr);
    camera_->disconnect(this);
    camera_->stop();
    camera_.reset();
  }
  ++cameraGeneration_;

  if (!device.has_value()) {
    return true;
  }
  camera_ = openCamera(*device);
  if (!camera_) {
    if (error != nullptr) {
      *error = "Brick could not open the selected camera.";
    }
    return false;
  }

  connect(camera_.get(), &CameraSession::previewFrame, this,
          &CaptureCoordinator::previewFrame);
  connect(camera_.get(), &CameraSession::settingsChanged, this,
          &CaptureCoordinator::settingsChanged);
  connect(camera_.get(), &CameraSession::errorOccurred, this,
          [this, generation = cameraGeneration_](const QString& message) {
            if (generation != cameraGeneration_ || !camera_) {
              return;
            }
            if (activeCapture_.has_value()) {
              failCapture(activeCapture_->id, message);
            } else {
              emit cameraError(message);
            }
          }, Qt::QueuedConnection);
  connect(camera_.get(), &CameraSession::captureCompleted, this,
          [this](quint64 captureId, const QString& filePath) {
            completeCapture(captureId, filePath);
          }, Qt::QueuedConnection);
  connect(camera_.get(), &CameraSession::captureFailed, this,
          [this](quint64 captureId, const QString& message) {
            failCapture(captureId, message);
          }, Qt::QueuedConnection);
  camera_->start();
  if (!camera_->isReady()) {
    camera_->disconnect(this);
    camera_->stop();
    camera_.reset();
    if (error != nullptr) {
      *error = "Brick could not initialize the selected camera.";
    }
    return false;
  }
  emit cameraChanged(camera_.get());
  return true;
}


CameraSession* CaptureCoordinator::cameraSession() const {
  return camera_.get();
}


bool CaptureCoordinator::isReady() const {
  return camera_ != nullptr && camera_->isReady();
}


bool CaptureCoordinator::isCapturing() const {
  return activeCapture_.has_value();
}


quint64 CaptureCoordinator::requestCapture(CapturePurpose purpose,
                                            const CaptureDestination& destination,
                                            const TestShot& metadata,
                                            QString* error) {
  if (activeCapture_.has_value()) {
    if (error != nullptr) {
      *error = "Another capture is already active.";
    }
    return 0;
  }
  if (!isReady()) {
    if (error != nullptr) {
      *error = "The camera is not ready to capture an image.";
    }
    return 0;
  }
  const QString base = recovery_.begin(purpose, destination, metadata, error);
  if (base.isEmpty()) {
    return 0;
  }

  const quint64 captureId = nextCaptureId_++;
  activeCapture_ = ActiveCapture{captureId, cameraGeneration_, purpose,
                                 base};
  emit captureStateChanged(true);

  const quint64 generation = cameraGeneration_;
  QMetaObject::invokeMethod(
      this,
      [this, captureId, generation] {
        if (!activeCapture_.has_value() || activeCapture_->id != captureId ||
            activeCapture_->cameraGeneration != generation ||
            cameraGeneration_ != generation || camera_ == nullptr) {
          return;
        }
        camera_->capture(captureId, activeCapture_->destinationBase);
      },
      Qt::QueuedConnection);
  return captureId;
}


void CaptureCoordinator::completeCapture(quint64 captureId,
                                         const QString& filePath) {
  if (!activeCapture_.has_value() || activeCapture_->id != captureId) {
    return;
  }
  const CapturePurpose purpose = activeCapture_->purpose;
  const QString job = QFileInfo(activeCapture_->destinationBase).absolutePath();
  QString error;
  if (!recovery_.markReady(job, filePath, &error)) {
    failCapture(captureId, error);
    return;
  }
  recovery_.release(job);
  activeCapture_.reset();
  emit captureStateChanged(false);
  emit captureCompleted(captureId, purpose, filePath);
}


void CaptureCoordinator::failCapture(quint64 captureId,
                                     const QString& message) {
  if (!activeCapture_.has_value() || activeCapture_->id != captureId) {
    return;
  }
  const CapturePurpose purpose = activeCapture_->purpose;
  const QString job = QFileInfo(activeCapture_->destinationBase).absolutePath();
  // Generic camera errors may arrive while the backend is still writing.
  if (camera_) {
    camera_->disconnect(this);
    camera_->stop();
    emit cameraChanged(nullptr);
    camera_.reset();
    ++cameraGeneration_;
  }
  recovery_.release(job);
  activeCapture_.reset();
  emit captureStateChanged(false);
  emit captureFailed(captureId, purpose,
                     message + " Any downloaded assets are retained at " + job +
                         ". Use File > Recover Captures to save or discard them.");
}
