#include "camera.h"

#include <QCamera>
#include <QCameraDevice>
#include <QImageCapture>
#include <QMediaCaptureSession>
#include <QMediaDevices>
#include <QSize>
#include <QVideoFrame>
#include <QVideoSink>

#include <algorithm>
#include <array>
#include <cmath>
#include <map>
#include <utility>

#ifdef Q_OS_LINUX
#include <fcntl.h>
#include <linux/videodev2.h>
#include <sys/ioctl.h>
#include <unistd.h>

#include <optional>
#endif

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

QString resolutionValue(const QSize& resolution) {
  return QString("%1x%2").arg(resolution.width()).arg(resolution.height());
}

QString formatValue(const QCameraFormat& format) {
  return QString("%1x%2@%3-%4")
      .arg(format.resolution().width())
      .arg(format.resolution().height())
      .arg(format.minFrameRate(), 0, 'g', 4)
      .arg(format.maxFrameRate(), 0, 'g', 4);
}

QString formatLabel(const QCameraFormat& format) {
  const bool fixedRate = std::abs(format.minFrameRate() -
                                  format.maxFrameRate()) < 0.01;
  const QString rate = fixedRate
                           ? QString::number(format.maxFrameRate(), 'g', 3)
                           : QString("%1-%2")
                                 .arg(format.minFrameRate(), 0, 'g', 3)
                                 .arg(format.maxFrameRate(), 0, 'g', 3);
  return resolutionValue(format.resolution()) + " @ " + rate + " fps";
}

#ifdef Q_OS_LINUX
struct V4l2Control {
  quint32 id;
  int value;
  int minimum;
  int maximum;
  int step;
};

std::optional<V4l2Control> v4l2Control(int descriptor, quint32 id) {
  if (descriptor < 0) {
    return std::nullopt;
  }
  v4l2_queryctrl query{};
  query.id = id;
  if (ioctl(descriptor, VIDIOC_QUERYCTRL, &query) < 0 ||
      query.flags & (V4L2_CTRL_FLAG_DISABLED | V4L2_CTRL_FLAG_READ_ONLY)) {
    return std::nullopt;
  }
  v4l2_control control{};
  control.id = id;
  if (ioctl(descriptor, VIDIOC_G_CTRL, &control) < 0) {
    return std::nullopt;
  }
  return V4l2Control{id, control.value, query.minimum, query.maximum,
                     std::max(1, query.step)};
}
#endif

class WebcamSession final : public CameraSession {
 public:
  WebcamSession(QCameraDevice device, QObject* parent)
      : CameraSession(parent), device_(std::move(device)) {
    camera_ = std::make_unique<QCamera>(device_);
    capture_ = std::make_unique<QImageCapture>();
    sink_ = std::make_unique<QVideoSink>();
#ifdef Q_OS_LINUX
    const QString id = QString::fromUtf8(device_.id());
    const int pathStart = id.indexOf("/dev/video");
    if (pathStart >= 0) {
      const QByteArray path = id.sliced(pathStart).toUtf8();
      v4l2Descriptor_ = open(path.constData(), O_RDWR | O_NONBLOCK);
    }
    if (v4l2Control(v4l2Descriptor_, V4L2_CID_EXPOSURE_ABSOLUTE)) {
      v4l2_control automaticExposure{};
      automaticExposure.id = V4L2_CID_EXPOSURE_AUTO;
      automaticExposure.value = V4L2_EXPOSURE_MANUAL;
      ioctl(v4l2Descriptor_, VIDIOC_S_CTRL, &automaticExposure);
    }
#endif
    mediaSession_.setCamera(camera_.get());
    mediaSession_.setImageCapture(capture_.get());
    mediaSession_.setVideoSink(sink_.get());

    const auto features = camera_->supportedFeatures();
    if (camera_->isExposureModeSupported(QCamera::ExposureManual)) {
      float exposureTime = camera_->exposureTime();
      camera_->setExposureMode(QCamera::ExposureManual);
      if (features.testFlag(QCamera::Feature::ManualExposureTime) &&
          camera_->minimumExposureTime() > 0 &&
          camera_->maximumExposureTime() >= camera_->minimumExposureTime()) {
        exposureTime = std::clamp(exposureTime > 0 ? exposureTime : 1.0F / 60.0F,
                                  camera_->minimumExposureTime(),
                                  camera_->maximumExposureTime());
        camera_->setManualExposureTime(exposureTime);
      }
    }
    if (camera_->isWhiteBalanceModeSupported(QCamera::WhiteBalanceManual)) {
      camera_->setWhiteBalanceMode(QCamera::WhiteBalanceManual);
      if (features.testFlag(QCamera::Feature::ColorTemperature) &&
          camera_->colorTemperature() <= 0) {
        camera_->setColorTemperature(5600);
      }
    }
    if (camera_->isFocusModeSupported(QCamera::FocusModeManual)) {
      camera_->setFocusMode(QCamera::FocusModeManual);
    }

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
    connect(camera_.get(), &QCamera::supportedFeaturesChanged, this,
            &CameraSession::settingsChanged);
    connect(camera_.get(), &QCamera::cameraFormatChanged, this,
            &CameraSession::settingsChanged);
    connect(camera_.get(), &QCamera::manualExposureTimeChanged, this,
            [this] { emit settingsChanged(); });
    connect(camera_.get(), &QCamera::exposureCompensationChanged, this,
            [this] { emit settingsChanged(); });
    connect(camera_.get(), &QCamera::exposureModeChanged, this,
            [this] { emit settingsChanged(); });
    connect(camera_.get(), &QCamera::whiteBalanceModeChanged, this,
            [this] { emit settingsChanged(); });
    connect(camera_.get(), &QCamera::colorTemperatureChanged, this,
            [this] { emit settingsChanged(); });
    connect(camera_.get(), &QCamera::focusModeChanged, this,
            [this] { emit settingsChanged(); });
    connect(camera_.get(), &QCamera::focusDistanceChanged, this,
            [this] { emit settingsChanged(); });
    connect(camera_.get(), &QCamera::zoomFactorChanged, this,
            [this] { emit settingsChanged(); });
  }

  ~WebcamSession() override {
#ifdef Q_OS_LINUX
    if (v4l2Descriptor_ >= 0) {
      close(v4l2Descriptor_);
    }
#endif
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

#ifdef Q_OS_LINUX
    if (const auto gain = v4l2Control(v4l2Descriptor_, V4L2_CID_GAIN)) {
      CameraSetting setting{"gain", "Gain", QString::number(gain->value), {}};
      setting.type = CameraSettingType::IntegerRange;
      setting.minimum = gain->minimum;
      setting.maximum = gain->maximum;
      setting.step = gain->step;
      result.push_back(std::move(setting));
    }

    const auto nativeExposure =
        v4l2Control(v4l2Descriptor_, V4L2_CID_EXPOSURE_ABSOLUTE);
    if (nativeExposure) {
      CameraSetting setting{"exposure", "Exposure",
                            QString::number(nativeExposure->value), {}};
      setting.type = CameraSettingType::IntegerRange;
      setting.minimum = nativeExposure->minimum;
      setting.maximum = nativeExposure->maximum;
      setting.step = nativeExposure->step;
      result.push_back(std::move(setting));
    }
#endif

    if (
#ifdef Q_OS_LINUX
        !nativeExposure &&
#endif
        camera_->supportedFeatures().testFlag(
            QCamera::Feature::ManualExposureTime)) {
      float exposureTime = camera_->manualExposureTime();
      if (exposureTime <= 0 && camera_->minimumExposureTime() > 0 &&
          camera_->maximumExposureTime() >= camera_->minimumExposureTime()) {
        exposureTime = std::clamp(camera_->exposureTime() > 0
                                      ? camera_->exposureTime()
                                      : 1.0F / 60.0F,
                                  camera_->minimumExposureTime(),
                                  camera_->maximumExposureTime());
      }
      CameraSetting setting{
          "exposureTime", "Exposure",
          QString::number(exposureTime, 'g', 6),
          {{"-1", "Auto"}}};
      for (const auto& [seconds, label] : std::vector<std::pair<double, QString>>{
               {1.0 / 8000.0, "1/8000"}, {1.0 / 4000.0, "1/4000"},
               {1.0 / 2000.0, "1/2000"}, {1.0 / 1000.0, "1/1000"},
               {1.0 / 500.0, "1/500"},   {1.0 / 250.0, "1/250"},
               {1.0 / 125.0, "1/125"},   {1.0 / 60.0, "1/60"},
               {1.0 / 30.0, "1/30"},     {1.0 / 15.0, "1/15"},
               {1.0 / 8.0, "1/8"},       {1.0 / 4.0, "1/4"},
               {1.0 / 2.0, "1/2"},       {1.0, "1 s"},
               {2.0, "2 s"},             {4.0, "4 s"},
               {8.0, "8 s"},             {15.0, "15 s"},
               {30.0, "30 s"}}) {
        if (seconds >= camera_->minimumExposureTime() &&
            seconds <= camera_->maximumExposureTime()) {
          setting.choices.push_back({QString::number(seconds, 'g', 8), label});
        }
      }
      if (exposureTime > 0 &&
          std::none_of(setting.choices.begin(), setting.choices.end(),
                       [&setting](const CameraSettingChoice& choice) {
                         return std::abs(choice.value.toDouble() -
                                         setting.value.toDouble()) < 0.000001;
                       })) {
        setting.choices.push_back(
            {setting.value, QString::number(exposureTime, 'g', 4) + " s"});
      }
      result.push_back(std::move(setting));
    }

    if (camera_->supportedFeatures().testFlag(
            QCamera::Feature::ExposureCompensation)) {
      CameraSetting setting{"exposureCompensation", "Exposure compensation",
                            QString::number(camera_->exposureCompensation()), {}};
      setting.type = CameraSettingType::DecimalRange;
      setting.minimum = -4;
      setting.maximum = 4;
      setting.step = 0.1;
      setting.decimals = 1;
      setting.suffix = " EV";
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


    if (camera_->supportedFeatures().testFlag(QCamera::Feature::ColorTemperature)) {
      const int colorTemperature =
          camera_->colorTemperature() > 0 ? camera_->colorTemperature() : 5600;
      CameraSetting setting{"colorTemperature", "Color temperature",
                            QString::number(colorTemperature), {}};
      setting.type = CameraSettingType::IntegerRange;
      setting.minimum = 2000;
      setting.maximum = 10000;
      setting.step = 100;
      setting.suffix = " K";
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


    if (camera_->supportedFeatures().testFlag(QCamera::Feature::FocusDistance)) {
      CameraSetting setting{"focusDistance", "Focus distance",
                            QString::number(camera_->focusDistance(), 'g', 4), {}};
      setting.type = CameraSettingType::DecimalRange;
      setting.minimum = 0;
      setting.maximum = 1;
      setting.step = 0.01;
      setting.decimals = 2;
      result.push_back(std::move(setting));
    }

    if (camera_->maximumZoomFactor() > camera_->minimumZoomFactor()) {
      CameraSetting setting{"zoom", "Zoom",
                            QString::number(camera_->zoomFactor(), 'g', 4), {}};
      setting.type = CameraSettingType::DecimalRange;
      setting.minimum = camera_->minimumZoomFactor();
      setting.maximum = camera_->maximumZoomFactor();
      setting.step = 0.1;
      setting.decimals = 1;
      setting.suffix = "x";
      result.push_back(std::move(setting));
    }

    const auto formats = device_.videoFormats();
    if (!formats.empty()) {
      CameraSetting setting{"videoFormat", "Video format",
                            formatValue(camera_->cameraFormat()), {}};
      for (const QCameraFormat& format : formats) {
        const QString value = formatValue(format);
        if (std::none_of(setting.choices.begin(), setting.choices.end(),
                         [&value](const CameraSettingChoice& choice) {
                           return choice.value == value;
                         })) {
          setting.choices.push_back({value, formatLabel(format)});
        }
      }
      result.push_back(std::move(setting));
    }

    const auto resolutions = device_.photoResolutions();
    if (!resolutions.empty()) {
      CameraSetting setting{"photoResolution", "Photo resolution",
                            resolutionValue(capture_->resolution()), {}};
      for (const QSize& resolution : resolutions) {
        setting.choices.push_back(
            {resolutionValue(resolution), resolutionValue(resolution)});
      }
      result.push_back(std::move(setting));
    }

    CameraSetting quality{"photoQuality", "Photo quality",
                          QString::number(static_cast<int>(capture_->quality())),
                          {}};
    for (const auto& [value, label] :
         std::array<std::pair<QImageCapture::Quality, QString>, 5>{
             {{QImageCapture::VeryLowQuality, "Very low"},
              {QImageCapture::LowQuality, "Low"},
              {QImageCapture::NormalQuality, "Normal"},
              {QImageCapture::HighQuality, "High"},
              {QImageCapture::VeryHighQuality, "Very high"}}}) {
      quality.choices.push_back(
          {QString::number(static_cast<int>(value)), label});
    }
    result.push_back(std::move(quality));

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
    } else if (id == "exposureCompensation") {
      camera_->setExposureCompensation(value.toFloat(&converted));
    } else if (id == "colorTemperature") {
      camera_->setColorTemperature(value.toInt(&converted));
    } else if (id == "focusDistance") {
      camera_->setFocusDistance(value.toFloat(&converted));
    } else if (id == "zoom") {
      camera_->setZoomFactor(value.toFloat(&converted));
#ifdef Q_OS_LINUX
    } else if (id == "gain" || id == "exposure") {
      v4l2_control control{};
      control.id = id == "gain" ? V4L2_CID_GAIN : V4L2_CID_EXPOSURE_ABSOLUTE;
      control.value = value.toInt(&converted);
      converted = converted && v4l2Descriptor_ >= 0 &&
                  ioctl(v4l2Descriptor_, VIDIOC_S_CTRL, &control) == 0;
#endif
    } else if (id == "videoFormat") {
      const auto formats = device_.videoFormats();
      const auto format = std::find_if(
          formats.begin(), formats.end(), [&value](const QCameraFormat& candidate) {
            return formatValue(candidate) == value;
          });
      converted = format != formats.end();
      if (converted) {
        camera_->setCameraFormat(*format);
      }
    } else if (id == "photoResolution") {
      const auto resolutions = device_.photoResolutions();
      const auto resolution = std::find_if(
          resolutions.begin(), resolutions.end(), [&value](const QSize& candidate) {
            return resolutionValue(candidate) == value;
          });
      converted = resolution != resolutions.end();
      if (converted) {
        capture_->setResolution(*resolution);
      }
    } else if (id == "photoQuality") {
      capture_->setQuality(
          static_cast<QImageCapture::Quality>(value.toInt(&converted)));
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
#ifdef Q_OS_LINUX
  int v4l2Descriptor_ = -1;
#endif
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
