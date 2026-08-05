#include "camera.h"

#include <QCamera>
#include <QCameraDevice>
#include <QImageCapture>
#include <QMediaCaptureSession>
#include <QMediaDevices>
#include <QVideoFrame>
#include <QVideoSink>

#include <algorithm>
#include <map>
#include <utility>

#ifdef BRICK_HAS_CANON_EDSDK
#include "canon_camera.h"
#endif

namespace {

QString exposureModeValue(QCamera::ExposureMode mode) {
  return QString::number(static_cast<int>(mode));
}

QString exposureModeLabel(QCamera::ExposureMode mode) {
  switch (mode) {
    case QCamera::ExposureAuto:
      return "Auto";
    case QCamera::ExposureManual:
      return "Manual";
    case QCamera::ExposurePortrait:
      return "Portrait";
    case QCamera::ExposureNight:
      return "Night";
    case QCamera::ExposureSports:
      return "Sports";
    case QCamera::ExposureSnow:
      return "Snow";
    case QCamera::ExposureBeach:
      return "Beach";
    case QCamera::ExposureAction:
      return "Action";
    case QCamera::ExposureLandscape:
      return "Landscape";
    case QCamera::ExposureNightPortrait:
      return "Night portrait";
    case QCamera::ExposureTheatre:
      return "Theatre";
    case QCamera::ExposureSunset:
      return "Sunset";
    case QCamera::ExposureSteadyPhoto:
      return "Steady photo";
    case QCamera::ExposureFireworks:
      return "Fireworks";
    case QCamera::ExposureParty:
      return "Party";
    case QCamera::ExposureCandlelight:
      return "Candlelight";
    case QCamera::ExposureBarcode:
      return "Barcode";
    default:
      return "Mode " + QString::number(static_cast<int>(mode));
  }
}

QString whiteBalanceLabel(QCamera::WhiteBalanceMode mode) {
  switch (mode) {
    case QCamera::WhiteBalanceAuto:
      return "Auto";
    case QCamera::WhiteBalanceManual:
      return "Manual";
    case QCamera::WhiteBalanceSunlight:
      return "Sunlight";
    case QCamera::WhiteBalanceCloudy:
      return "Cloudy";
    case QCamera::WhiteBalanceShade:
      return "Shade";
    case QCamera::WhiteBalanceTungsten:
      return "Tungsten";
    case QCamera::WhiteBalanceFluorescent:
      return "Fluorescent";
    case QCamera::WhiteBalanceFlash:
      return "Flash";
    case QCamera::WhiteBalanceSunset:
      return "Sunset";
    default:
      return "Mode " + QString::number(static_cast<int>(mode));
  }
}

QString focusModeLabel(QCamera::FocusMode mode) {
  switch (mode) {
    case QCamera::FocusModeAuto:
      return "Auto";
    case QCamera::FocusModeAutoNear:
      return "Auto near";
    case QCamera::FocusModeAutoFar:
      return "Auto far";
    case QCamera::FocusModeHyperfocal:
      return "Hyperfocal";
    case QCamera::FocusModeInfinity:
      return "Infinity";
    case QCamera::FocusModeManual:
      return "Manual";
    default:
      return "Mode " + QString::number(static_cast<int>(mode));
  }
}

class WebcamSession final : public CameraSession {
 public:
  WebcamSession(QCameraDevice device, QObject* parent)
      : CameraSession(parent), device_(std::move(device)) {
    camera_ = std::make_unique<QCamera>(device_);
    capture_ = std::make_unique<QImageCapture>();
    sink_ = std::make_unique<QVideoSink>();
    mediaSession_.setCamera(camera_.get());
    mediaSession_.setImageCapture(capture_.get());
    mediaSession_.setVideoSink(sink_.get());

    connect(sink_.get(), &QVideoSink::videoFrameChanged, this,
            [this](const QVideoFrame& frame) {
              const QImage image = frame.toImage();
              if (!image.isNull()) {
                emit previewFrame(image);
              }
            });
    connect(capture_.get(), &QImageCapture::imageSaved, this,
            [this](int, const QString& fileName) {
              emit captureCompleted(fileName);
            });
    connect(capture_.get(), &QImageCapture::errorOccurred, this,
            [this](int, QImageCapture::Error, const QString& message) {
              emit errorOccurred("Webcam capture failed: " + message);
            });
    connect(camera_.get(), &QCamera::errorOccurred, this,
            [this](QCamera::Error, const QString& message) {
              emit errorOccurred("Webcam error: " + message);
            });
  }

  [[nodiscard]] QString backend() const override { return "webcam"; }
  [[nodiscard]] QString deviceId() const override {
    return QString::fromUtf8(device_.id());
  }
  [[nodiscard]] QString displayName() const override {
    return device_.description();
  }
  [[nodiscard]] bool isReady() const override { return camera_->isAvailable(); }

  [[nodiscard]] std::vector<CameraSetting> settings() const override {
    std::vector<CameraSetting> result;

    std::vector<QCamera::ExposureMode> exposureModes;
    for (int value = QCamera::ExposureAuto; value <= QCamera::ExposureBarcode;
         ++value) {
      const auto mode = static_cast<QCamera::ExposureMode>(value);
      if (camera_->isExposureModeSupported(mode)) {
        exposureModes.push_back(mode);
      }
    }
    if (!exposureModes.empty()) {
      CameraSetting setting{"exposureMode", "Exposure mode",
                            exposureModeValue(camera_->exposureMode()), {}};
      for (const auto mode : exposureModes) {
        setting.choices.push_back(
            {exposureModeValue(mode), exposureModeLabel(mode)});
      }
      result.push_back(std::move(setting));
    }

    if (camera_->supportedFeatures().testFlag(
            QCamera::Feature::IsoSensitivity)) {
      CameraSetting setting{"iso", "ISO",
                            QString::number(camera_->manualIsoSensitivity()), {}};
      setting.choices.push_back({"-1", "Auto"});
      const int minimum = std::max(1, camera_->minimumIsoSensitivity());
      const int maximum = camera_->maximumIsoSensitivity();
      for (int value : {100, 200, 400, 800, 1600, 3200, 6400}) {
        if (value >= minimum && value <= maximum) {
          setting.choices.push_back(
              {QString::number(value), QString::number(value)});
        }
      }
      result.push_back(std::move(setting));
    }

    if (camera_->supportedFeatures().testFlag(
            QCamera::Feature::ManualExposureTime)) {
      CameraSetting setting{
          "exposureTime", "Shutter",
          QString::number(camera_->manualExposureTime(), 'g', 6),
          {{"-1", "Auto"}}};
      for (const auto& [seconds, label] :
           std::vector<std::pair<double, QString>>{{1.0 / 1000.0, "1/1000"},
                                                   {1.0 / 500.0, "1/500"},
                                                   {1.0 / 250.0, "1/250"},
                                                   {1.0 / 125.0, "1/125"},
                                                   {1.0 / 60.0, "1/60"},
                                                   {1.0 / 30.0, "1/30"},
                                                   {1.0 / 15.0, "1/15"}}) {
        if (seconds >= camera_->minimumExposureTime() &&
            seconds <= camera_->maximumExposureTime()) {
          setting.choices.push_back({QString::number(seconds, 'g', 8), label});
        }
      }
      result.push_back(std::move(setting));
    }

    std::vector<QCamera::WhiteBalanceMode> whiteBalanceModes;
    for (int value = QCamera::WhiteBalanceAuto;
         value <= QCamera::WhiteBalanceSunset; ++value) {
      const auto mode = static_cast<QCamera::WhiteBalanceMode>(value);
      if (camera_->isWhiteBalanceModeSupported(mode)) {
        whiteBalanceModes.push_back(mode);
      }
    }
    if (!whiteBalanceModes.empty()) {
      CameraSetting setting{
          "whiteBalance", "White balance",
          QString::number(static_cast<int>(camera_->whiteBalanceMode())), {}};
      for (const auto mode : whiteBalanceModes) {
        setting.choices.push_back(
            {QString::number(static_cast<int>(mode)), whiteBalanceLabel(mode)});
      }
      result.push_back(std::move(setting));
    }

    std::vector<QCamera::FocusMode> focusModes;
    for (int value = QCamera::FocusModeAuto;
         value <= QCamera::FocusModeManual; ++value) {
      const auto mode = static_cast<QCamera::FocusMode>(value);
      if (camera_->isFocusModeSupported(mode)) {
        focusModes.push_back(mode);
      }
    }
    if (!focusModes.empty()) {
      CameraSetting setting{
          "focusMode", "Focus",
          QString::number(static_cast<int>(camera_->focusMode())), {}};
      for (const auto mode : focusModes) {
        setting.choices.push_back(
            {QString::number(static_cast<int>(mode)), focusModeLabel(mode)});
      }
      result.push_back(std::move(setting));
    }
    return result;
  }

  void start() override { camera_->start(); }
  void stop() override { camera_->stop(); }

  void capture(const QString& destinationBase) override {
    if (!capture_->isReadyForCapture()) {
      emit errorOccurred("The webcam is not ready to capture an image.");
      return;
    }
    capture_->captureToFile(destinationBase + ".jpg");
  }

  void setSetting(const QString& id, const QString& value) override {
    bool converted = false;
    if (id == "exposureMode") {
      camera_->setExposureMode(
          static_cast<QCamera::ExposureMode>(value.toInt(&converted)));
    } else if (id == "iso") {
      const int iso = value.toInt(&converted);
      if (converted) {
        if (iso < 0) {
          camera_->setAutoIsoSensitivity();
        } else {
          camera_->setManualIsoSensitivity(iso);
        }
      }
    } else if (id == "exposureTime") {
      const float seconds = value.toFloat(&converted);
      if (converted) {
        if (seconds < 0) {
          camera_->setAutoExposureTime();
        } else {
          camera_->setManualExposureTime(seconds);
        }
      }
    } else if (id == "whiteBalance") {
      camera_->setWhiteBalanceMode(
          static_cast<QCamera::WhiteBalanceMode>(value.toInt(&converted)));
    } else if (id == "focusMode") {
      camera_->setFocusMode(
          static_cast<QCamera::FocusMode>(value.toInt(&converted)));
    }
    if (!converted) {
      emit errorOccurred("The webcam rejected an invalid camera setting.");
      return;
    }
    emit settingsChanged();
  }

 private:
  QCameraDevice device_;
  std::unique_ptr<QCamera> camera_;
  std::unique_ptr<QImageCapture> capture_;
  std::unique_ptr<QVideoSink> sink_;
  QMediaCaptureSession mediaSession_;
};

}  // namespace


std::vector<CameraDevice> availableCameras() {
  std::vector<CameraDevice> devices;
#ifdef BRICK_HAS_CANON_EDSDK
  const auto canonDevices = availableCanonCameras();
  devices.insert(devices.end(), canonDevices.begin(), canonDevices.end());
#endif
  for (const QCameraDevice& device : QMediaDevices::videoInputs()) {
    devices.push_back(
        {"webcam", QString::fromUtf8(device.id()), device.description()});
  }
  return devices;
}


std::unique_ptr<CameraSession> openCamera(const CameraDevice& device,
                                          QObject* parent) {
#ifdef BRICK_HAS_CANON_EDSDK
  if (device.backend == "canon") {
    return openCanonCamera(device, parent);
  }
#endif
  if (device.backend != "webcam") {
    return nullptr;
  }
  const auto devices = QMediaDevices::videoInputs();
  const auto match = std::find_if(
      devices.begin(), devices.end(), [&device](const QCameraDevice& candidate) {
        return QString::fromUtf8(candidate.id()) == device.id;
      });
  if (match == devices.end()) {
    return nullptr;
  }
  return std::make_unique<WebcamSession>(*match, parent);
}
