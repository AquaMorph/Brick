#pragma once

#include "camera.h"

#include <memory>
#include <vector>

[[nodiscard]] std::vector<CameraDevice> availableCameras();
[[nodiscard]] std::unique_ptr<CameraSession> openCamera(
    const CameraDevice& device, QObject* parent = nullptr);
