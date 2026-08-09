#pragma once

#include "../camera.h"

#include <memory>
#include <vector>

[[nodiscard]] std::vector<CameraDevice> availableWebcams();
[[nodiscard]] std::unique_ptr<CameraSession> openWebcam(
    const CameraDevice& device, QObject* parent);
