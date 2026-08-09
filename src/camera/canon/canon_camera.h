#pragma once

#include "../camera.h"

#include <memory>
#include <vector>

[[nodiscard]] std::vector<CameraDevice> availableCanonCameras();
[[nodiscard]] std::unique_ptr<CameraSession> openCanonCamera(
    const CameraDevice& device, QObject* parent);
