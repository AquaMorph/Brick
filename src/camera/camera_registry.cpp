#include "camera_registry.h"

#include "webcam/webcam_camera.h"

#ifdef BRICK_HAS_CANON_EDSDK
#include "canon/canon_camera.h"
#endif

std::vector<CameraDevice> availableCameras() {
  std::vector<CameraDevice> devices;
#ifdef BRICK_HAS_CANON_EDSDK
  const auto canonDevices = availableCanonCameras();
  devices.insert(devices.end(), canonDevices.begin(), canonDevices.end());
#endif
  const auto webcams = availableWebcams();
  devices.insert(devices.end(), webcams.begin(), webcams.end());
  return devices;
}


std::unique_ptr<CameraSession> openCamera(const CameraDevice& device,
                                          QObject* parent) {
#ifdef BRICK_HAS_CANON_EDSDK
  if (device.backend == "canon") {
    return openCanonCamera(device, parent);
  }
#endif
  if (device.backend == "webcam") {
    return openWebcam(device, parent);
  }
  return nullptr;
}
