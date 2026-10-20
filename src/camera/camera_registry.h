#pragma once

#include "camera.h"

#include <memory>
#include <vector>

class CameraDiscovery final : public QObject {
  Q_OBJECT

 public:
  using QObject::QObject;

 signals:
  void camerasChanged();
};

[[nodiscard]] CameraDiscovery* cameraDiscovery();
[[nodiscard]] std::vector<CameraDevice> availableCameras();
[[nodiscard]] std::unique_ptr<CameraSession> openCamera(
    const CameraDevice& device, QObject* parent = nullptr);
