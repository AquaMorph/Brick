#include "canon_camera.h"

#include <EDSDK.h>

#include <QByteArray>
#include <QFile>
#include <QFileInfo>
#include <QTimer>
#include <QtGlobal>
#include <algorithm>
#include <array>
#include <cmath>
#include <map>
#include <utility>

namespace {

QString errorText(EdsError error) {
  return QString("Canon EDSDK error 0x%1").arg(error, 8, 16, QLatin1Char('0'));
}

QString hexValue(EdsInt32 value) { return QString::number(static_cast<EdsUInt32>(value), 16); }

QString canonLabel(EdsPropertyID property, EdsInt32 value) {
  static const std::map<EdsInt32, QString> iso = {{0x00, "Auto"}, {0x48, "100"},  {0x50, "200"},
                                                  {0x58, "400"},  {0x60, "800"},  {0x68, "1600"},
                                                  {0x70, "3200"}, {0x78, "6400"}, {0x80, "12800"}};
  static const std::map<EdsInt32, QString> aperture = {
      {0x18, "f/1.0"}, {0x20, "f/1.4"}, {0x28, "f/2.0"}, {0x30, "f/2.8"},
      {0x38, "f/4.0"}, {0x40, "f/5.6"}, {0x48, "f/8.0"}, {0x50, "f/11"},
      {0x58, "f/16"},  {0x60, "f/22"},  {0x68, "f/32"}};
  static const std::map<EdsInt32, QString> shutter = {
      {0x0c, "Bulb"},   {0x10, "30 s"},   {0x13, "25 s"},   {0x14, "20 s"},
      {0x15, "20 s"},   {0x18, "15 s"},   {0x1b, "13 s"},   {0x1c, "10 s"},
      {0x1d, "10 s"},   {0x20, "8 s"},    {0x23, "6 s"},    {0x24, "6 s"},
      {0x25, "5 s"},    {0x28, "4 s"},    {0x2b, "3.2 s"},  {0x2c, "3 s"},
      {0x2d, "2.5 s"},  {0x30, "2 s"},    {0x33, "1.6 s"},  {0x34, "1.5 s"},
      {0x35, "1.3 s"},  {0x38, "1 s"},    {0x3b, "0.8 s"},  {0x3c, "0.7 s"},
      {0x3d, "0.6 s"},  {0x40, "1/2"},    {0x43, "0.4 s"},  {0x44, "0.3 s"},
      {0x45, "0.3 s"},  {0x48, "1/4"},    {0x4b, "1/5"},    {0x4c, "1/6"},
      {0x4d, "1/6"},    {0x50, "1/8"},    {0x53, "1/10"},   {0x54, "1/10"},
      {0x55, "1/13"},   {0x58, "1/15"},   {0x5b, "1/20"},   {0x5c, "1/20"},
      {0x5d, "1/25"},   {0x60, "1/30"},   {0x63, "1/40"},   {0x64, "1/45"},
      {0x65, "1/50"},   {0x68, "1/60"},   {0x6b, "1/80"},   {0x6c, "1/90"},
      {0x6d, "1/100"},  {0x70, "1/125"},  {0x73, "1/160"},  {0x74, "1/180"},
      {0x75, "1/200"},  {0x78, "1/250"},  {0x7b, "1/320"},  {0x7c, "1/350"},
      {0x7d, "1/400"},  {0x80, "1/500"},  {0x83, "1/640"},  {0x84, "1/750"},
      {0x85, "1/800"},  {0x88, "1/1000"}, {0x8b, "1/1250"}, {0x8c, "1/1500"},
      {0x8d, "1/1600"}, {0x90, "1/2000"}, {0x93, "1/2500"}, {0x94, "1/3000"},
      {0x95, "1/3200"}, {0x98, "1/4000"}, {0x9b, "1/5000"}, {0x9c, "1/6000"},
      {0x9d, "1/6400"}, {0xa0, "1/8000"}};
  static const std::map<EdsInt32, QString> whiteBalance = {
      {0, "Auto"},        {1, "Daylight"}, {2, "Cloudy"}, {3, "Tungsten"},
      {4, "Fluorescent"}, {5, "Flash"},    {8, "Shade"},  {9, "Color temperature"}};
  static const std::map<EdsInt32, QString> pictureStyle = {
      {kEdsPictureStyle_Standard, "Standard"},   {kEdsPictureStyle_Portrait, "Portrait"},
      {kEdsPictureStyle_Landscape, "Landscape"}, {kEdsPictureStyle_Neutral, "Neutral"},
      {kEdsPictureStyle_Faithful, "Faithful"},   {kEdsPictureStyle_Monochrome, "Monochrome"},
      {kEdsPictureStyle_Auto, "Auto"},           {kEdsPictureStyle_FineDetail, "Fine detail"},
      {kEdsPictureStyle_User1, "User 1"},        {kEdsPictureStyle_User2, "User 2"},
      {kEdsPictureStyle_User3, "User 3"}};
  static const std::map<EdsInt32, QString> aeMode = {
      {kEdsAEMode_Program, "Program"},
      {kEdsAEMode_Tv, "Shutter priority"},
      {kEdsAEMode_Av, "Aperture priority"},
      {kEdsAEMode_Manual, "Manual"},
      {kEdsAEMode_Bulb, "Bulb"},
      {kEdsAEMode_A_DEP, "Automatic depth-of-field"},
      {kEdsAEMode_DEP, "Depth-of-field"},
      {kEdsAEMode_Custom, "Custom 1"},
      {kEdsAEMode_Lock, "Lock"},
      {kEdsAEMode_Green, "Auto"},
      {kEdsAEMode_NightPortrait, "Night portrait"},
      {kEdsAEMode_Sports, "Sports"},
      {kEdsAEMode_Portrait, "Portrait"},
      {kEdsAEMode_Landscape, "Landscape"},
      {kEdsAEMode_Closeup, "Close-up"},
      {kEdsAEMode_FlashOff, "Flash off"},
      {0x10, "Custom 2"},
      {0x11, "Custom 3"},
      {kEdsAEMode_CreativeAuto, "Creative auto"},
      {kEdsAEMode_Movie, "Movie"},
      {kEdsAEMode_PhotoInMovie, "Photo in movie"},
      {kEdsAEMode_SceneIntelligentAuto, "Scene intelligent auto"},
      {kEdsAEMode_NightScenes, "Handheld night scene"},
      {kEdsAEMode_BacklitScenes, "HDR backlight control"},
      {kEdsAEMode_SCN, "Special scene"},
      {kEdsAEMode_Children, "Children"},
      {kEdsAEMode_Food, "Food"},
      {kEdsAEMode_CandlelightPortraits, "Candlelight portrait"},
      {kEdsAEMode_CreativeFilter, "Creative filter"},
      {kEdsAEMode_RoughMonoChrome, "Grainy black and white"},
      {kEdsAEMode_SoftFocus, "Soft focus"},
      {kEdsAEMode_ToyCamera, "Toy camera"},
      {kEdsAEMode_Fisheye, "Fish-eye"},
      {kEdsAEMode_WaterColor, "Water painting"},
      {kEdsAEMode_Miniature, "Miniature"},
      {kEdsAEMode_Hdr_Standard, "HDR standard"},
      {kEdsAEMode_Hdr_Vivid, "HDR vivid"},
      {kEdsAEMode_Hdr_Bold, "HDR bold"},
      {kEdsAEMode_Hdr_Embossed, "HDR embossed"},
      {kEdsAEMode_Movie_Fantasy, "Movie fantasy"},
      {kEdsAEMode_Movie_Old, "Movie old movies"},
      {kEdsAEMode_Movie_Memory, "Movie memory"},
      {kEdsAEMode_Movie_DirectMono, "Movie dramatic black and white"},
      {kEdsAEMode_Movie_Mini, "Movie miniature"},
      {kEdsAEMode_PanningAssist, "Panning"},
      {kEdsAEMode_GroupPhoto, "Group photo"},
      {kEdsAEMode_Myself, "Self portrait"},
      {kEdsAEMode_PlusMovieAuto, "Hybrid auto"},
      {kEdsAEMode_SmoothSkin, "Smooth skin"},
      {kEdsAEMode_Panorama, "Panorama"},
      {kEdsAEMode_Silent, "Silent"},
      {kEdsAEMode_Flexible, "Flexible priority"},
      {kEdsAEMode_OilPainting, "Oil painting"},
      {kEdsAEMode_Fireworks, "Fireworks"},
      {kEdsAEMode_StarPortrait, "Star portrait"},
      {kEdsAEMode_StarNightscape, "Star nightscape"},
      {kEdsAEMode_StarTrails, "Star trails"},
      {kEdsAEMode_StarTimelapseMovie, "Star time-lapse movie"},
      {kEdsAEMode_BackgroundBlur, "Background blur"},
      {kEdsAEMode_VideoBlog, "Video blog"},
      {kEdsAEMode_Unknown, "Unknown"}};
  static const std::map<EdsInt32, QString> imageQuality = {
      {EdsImageQuality_LJ, "JPEG large"},
      {EdsImageQuality_LJF, "JPEG large fine"},
      {EdsImageQuality_LJN, "JPEG large normal"},
      {EdsImageQuality_MJ, "JPEG medium"},
      {EdsImageQuality_SJ, "JPEG small"},
      {EdsImageQuality_LR, "RAW"},
      {EdsImageQuality_LRLJF, "RAW + JPEG large fine"},
      {EdsImageQuality_LRLJN, "RAW + JPEG large normal"}};
  static const std::map<EdsInt32, QString> metering = {
      {1, "Spot"}, {3, "Evaluative"}, {4, "Partial"}, {5, "Center-weighted"}};
  const std::map<EdsInt32, QString>* values = nullptr;
  if (property == kEdsPropID_ISOSpeed) {
    values = &iso;
  } else if (property == kEdsPropID_Av) {
    values = &aperture;
  } else if (property == kEdsPropID_Tv) {
    values = &shutter;
  } else if (property == kEdsPropID_WhiteBalance) {
    values = &whiteBalance;
  } else if (property == kEdsPropID_AEModeSelect) {
    values = &aeMode;
  } else if (property == kEdsPropID_ImageQuality) {
    values = &imageQuality;
  } else if (property == kEdsPropID_PictureStyle) {
    values = &pictureStyle;
  } else if (property == kEdsPropID_MeteringMode) {
    values = &metering;
  }
  if (values != nullptr) {
    const auto match = values->find(value);
    if (match != values->end()) {
      return match->second;
    }
  }
  if (property == kEdsPropID_ExposureCompensation) {
    const int signedValue = value > 0x80 ? value - 0x100 : value;
    return QString::number(signedValue / 8.0, 'g', 2) + " EV";
  }
  return "0x" + hexValue(value).rightJustified(2, '0');
}

struct CanonProperty {
  EdsPropertyID id;
  const char* key;
  const char* label;
  CameraSettingType type = CameraSettingType::Choice;
  const char* group = "Capture";
};

constexpr std::array<CanonProperty, 10> kCanonProperties = {{
    {kEdsPropID_AEModeSelect, "aeMode", "Exposure mode"},
    {kEdsPropID_Tv, "shutter", "Shutter speed", CameraSettingType::SteppedChoice},
    {kEdsPropID_Av, "aperture", "Aperture", CameraSettingType::SteppedChoice},
    {kEdsPropID_ISOSpeed, "iso", "ISO", CameraSettingType::SteppedChoice},
    {kEdsPropID_ExposureCompensation, "exposureCompensation", "Exposure compensation",
     CameraSettingType::SteppedChoice},
    {kEdsPropID_WhiteBalance, "whiteBalance", "White balance"},
    {kEdsPropID_PictureStyle, "pictureStyle", "Picture style"},
    {kEdsPropID_ImageQuality, "imageQuality", "Image quality"},
    {kEdsPropID_MeteringMode, "meteringMode", "Metering"},
    {kEdsPropID_DriveMode, "driveMode", "Drive mode"},
}};

class CanonSession final : public CameraSession {
 public:
  CanonSession(CameraDevice device, QObject* parent)
      : CameraSession(parent), device_(std::move(device)) {
    pollTimer_.setInterval(33);
    connect(&pollTimer_, &QTimer::timeout, this, [this] { poll(); });
    initialize();
  }

  ~CanonSession() override {
    stop();
    if (camera_ != nullptr) {
      if (sessionOpen_) {
        EdsCloseSession(camera_);
      }
      EdsRelease(camera_);
    }
    if (sdkInitialized_) {
      EdsTerminateSDK();
    }
  }

  [[nodiscard]] QString backend() const override { return device_.backend; }

  [[nodiscard]] QString deviceId() const override { return device_.id; }

  [[nodiscard]] QString displayName() const override { return device_.displayName; }

  [[nodiscard]] bool isReady() const override { return sessionOpen_; }

  [[nodiscard]] std::vector<CameraSetting> settings() const override {
    std::vector<CameraSetting> result;
    if (!sessionOpen_) {
      return result;
    }
    for (const auto& property : kCanonProperties) {
      EdsPropertyDesc descriptor{};
      EdsInt32 current = 0;
      if (EdsGetPropertyDesc(camera_, property.id, &descriptor) != EDS_ERR_OK ||
          descriptor.numElements <= 0 ||
          EdsGetPropertyData(camera_, property.id, 0, sizeof(current), &current) != EDS_ERR_OK) {
        continue;
      }
      CameraSetting setting{property.key, property.label, hexValue(current), {}};
      setting.type = property.type;
      setting.group = property.group;
      for (int index = 0; index < descriptor.numElements; ++index) {
        const EdsInt32 value = descriptor.propDesc[index];
        setting.choices.push_back({hexValue(value), canonLabel(property.id, value)});
      }
      if (property.id == kEdsPropID_Tv) {
        std::ranges::sort(setting.choices, [](const CameraSettingChoice& left,
                                              const CameraSettingChoice& right) {
          return left.value.toUInt(nullptr, 16) > right.value.toUInt(nullptr, 16);
        });
      }
      result.push_back(std::move(setting));
    }

    CameraSetting externalFlash{
        "externalFlash", "External flash", externalFlash_ ? "1" : "0", {{"0", "Off"}, {"1", "On"}}};
    externalFlash.type = CameraSettingType::Toggle;
    externalFlash.group = "Capture";
    result.push_back(std::move(externalFlash));

    CameraSetting previewOffset{"exposurePreviewOffset",
                                "Exposure preview offset",
                                QString::number(exposurePreviewOffset_, 'f', 1),
                                {}};
    previewOffset.type = CameraSettingType::DecimalRange;
    previewOffset.minimum = -3.0;
    previewOffset.maximum = 3.0;
    previewOffset.step = 0.1;
    previewOffset.decimals = 1;
    previewOffset.suffix = " EV";
    previewOffset.group = "Live preview";
    result.push_back(std::move(previewOffset));

    EdsUInt32 depthOfField = depthOfFieldPreview_ ? 1 : 0;
    if (EdsGetPropertyData(camera_, kEdsPropID_Evf_DepthOfFieldPreview, 0, sizeof(depthOfField),
                           &depthOfField) == EDS_ERR_OK) {
      depthOfFieldPreview_ = depthOfField != 0;
    }
    CameraSetting depthPreview{"depthOfFieldPreview",
                               "Depth of field preview",
                               depthOfFieldPreview_ ? "1" : "0",
                               {{"0", "Off"}, {"1", "On"}}};
    depthPreview.type = CameraSettingType::Toggle;
    depthPreview.group = "Live preview";
    result.push_back(std::move(depthPreview));

    CameraSetting simulation{
        "lvSimulation", "LV simulation", lvSimulation_ ? "1" : "0", {{"0", "Off"}, {"1", "On"}}};
    simulation.type = CameraSettingType::Toggle;
    simulation.group = "Live preview";
    result.push_back(std::move(simulation));
    return result;
  }

  void start() override {
    if (!sessionOpen_) {
      emit errorOccurred(initializationError_);
      return;
    }
    EdsUInt32 output = 0;
    EdsError error =
        EdsGetPropertyData(camera_, kEdsPropID_Evf_OutputDevice, 0, sizeof(output), &output);
    output |= kEdsEvfOutputDevice_PC;
    if (error == EDS_ERR_OK) {
      error = EdsSetPropertyData(camera_, kEdsPropID_Evf_OutputDevice, 0, sizeof(output), &output);
    }
    if (error != EDS_ERR_OK) {
      emit errorOccurred("Could not start Canon live view: " + errorText(error));
      return;
    }
    liveViewStarted_ = true;
    pollTimer_.start();
    emit settingsChanged();
  }

  void stop() override {
    pollTimer_.stop();
    if (!liveViewStarted_ || camera_ == nullptr) {
      return;
    }
    EdsUInt32 output = 0;
    if (EdsGetPropertyData(camera_, kEdsPropID_Evf_OutputDevice, 0, sizeof(output), &output) ==
        EDS_ERR_OK) {
      output &= ~static_cast<EdsUInt32>(kEdsEvfOutputDevice_PC);
      EdsSetPropertyData(camera_, kEdsPropID_Evf_OutputDevice, 0, sizeof(output), &output);
    }
    liveViewStarted_ = false;
  }

  void capture(const QString& destinationBase) override {
    if (!sessionOpen_) {
      emit errorOccurred(initializationError_);
      return;
    }
    if (depthOfFieldPreview_) {
      const EdsError previewError = setDepthOfFieldPreview(false);
      if (previewError != EDS_ERR_OK) {
        emit errorOccurred("Could not release Canon depth of field preview for capture: " +
                           errorText(previewError));
        return;
      }
      restoreDepthOfFieldPreviewAfterCapture_ = true;
      emit settingsChanged();
    }
    pendingCaptureBase_ = destinationBase;
    pendingCaptureWarning_.clear();
    EdsError error = EDS_ERR_OK;
    if (restoreDepthOfFieldPreviewAfterCapture_) {
      error = EdsSendCommand(camera_, kEdsCameraCommand_PressShutterButton,
                             kEdsCameraCommand_ShutterButton_Completely_NonAF);
      const EdsError releaseError = EdsSendCommand(
          camera_, kEdsCameraCommand_PressShutterButton, kEdsCameraCommand_ShutterButton_OFF);
      if (error == EDS_ERR_OK && releaseError != EDS_ERR_OK) {
        pendingCaptureWarning_ =
            "Canon could not release the shutter button: " + errorText(releaseError);
      }
    } else {
      error = EdsSendCommand(camera_, kEdsCameraCommand_TakePicture, 0);
    }
    if (error != EDS_ERR_OK) {
      pendingCaptureBase_.clear();
      pendingCaptureWarning_.clear();
      const EdsError previewError = restoreDepthOfFieldPreview();
      QString message = "Canon capture failed: " + errorText(error);
      if (previewError != EDS_ERR_OK) {
        message += ". Could not restore depth of field preview: " + errorText(previewError);
      }
      emit errorOccurred(message);
    }
  }

  void setSetting(const QString& id, const QString& value) override {
    if (id == "externalFlash" || id == "lvSimulation") {
      const bool enabled = value == "1";
      if (id == "externalFlash") {
        externalFlash_ = enabled;
      } else {
        lvSimulation_ = enabled;
      }
      emit settingsChanged();
      return;
    }
    if (id == "exposurePreviewOffset") {
      bool converted = false;
      const double offset = value.toDouble(&converted);
      if (!converted || offset < -3.0 || offset > 3.0) {
        emit errorOccurred("The exposure preview offset is invalid.");
        return;
      }
      exposurePreviewOffset_ = offset;
      emit settingsChanged();
      return;
    }
    if (id == "depthOfFieldPreview") {
      const EdsError error = setDepthOfFieldPreview(value == "1");
      if (error != EDS_ERR_OK) {
        emit errorOccurred("Canon rejected depth of field preview: " + errorText(error));
        return;
      }
      emit settingsChanged();
      return;
    }
    const auto property =
        std::find_if(kCanonProperties.begin(), kCanonProperties.end(),
                     [&id](const CanonProperty& candidate) { return id == candidate.key; });
    bool converted = false;
    const EdsUInt32 nativeValue = value.toUInt(&converted, 16);
    if (property == kCanonProperties.end() || !converted) {
      emit errorOccurred("The Canon camera setting is invalid.");
      return;
    }
    const EdsError error =
        EdsSetPropertyData(camera_, property->id, 0, sizeof(nativeValue), &nativeValue);
    if (error != EDS_ERR_OK) {
      emit errorOccurred("Canon rejected the setting: " + errorText(error));
      return;
    }
    emit settingsChanged();
  }

 private:
  EdsError setDepthOfFieldPreview(bool enabled) {
    const EdsUInt32 value = enabled ? kEdsEvfDepthOfFieldPreview_ON
                                    : kEdsEvfDepthOfFieldPreview_OFF;
    const EdsError error = EdsSetPropertyData(
        camera_, kEdsPropID_Evf_DepthOfFieldPreview, 0, sizeof(value), &value);
    if (error == EDS_ERR_OK) {
      depthOfFieldPreview_ = enabled;
    }
    return error;
  }

  EdsError restoreDepthOfFieldPreview() {
    if (!restoreDepthOfFieldPreviewAfterCapture_) {
      return EDS_ERR_OK;
    }
    restoreDepthOfFieldPreviewAfterCapture_ = false;
    const EdsError error = setDepthOfFieldPreview(true);
    emit settingsChanged();
    return error;
  }

  static EdsError EDSCALLBACK objectEvent(EdsObjectEvent event, EdsBaseRef ref, EdsVoid* context) {
    auto* self = static_cast<CanonSession*>(context);
    if (event == kEdsObjectEvent_DirItemRequestTransfer && ref != nullptr) {
      self->download(static_cast<EdsDirectoryItemRef>(ref));
    }
    if (ref != nullptr) {
      EdsRelease(ref);
    }
    return EDS_ERR_OK;
  }

  static EdsError EDSCALLBACK propertyEvent(EdsPropertyEvent, EdsPropertyID, EdsUInt32,
                                            EdsVoid* context) {
    emit static_cast<CanonSession*>(context)->settingsChanged();
    return EDS_ERR_OK;
  }

  static EdsError EDSCALLBACK stateEvent(EdsStateEvent event, EdsUInt32, EdsVoid* context) {
    if (event == kEdsStateEvent_Shutdown) {
      emit static_cast<CanonSession*>(context)->errorOccurred("The Canon camera was disconnected.");
    }
    return EDS_ERR_OK;
  }

  void initialize() {
    EdsError error = EdsInitializeSDK();
    if (error != EDS_ERR_OK) {
      initializationError_ = "Could not initialize Canon EDSDK: " + errorText(error);
      return;
    }
    sdkInitialized_ = true;

    EdsCameraListRef list = nullptr;
    EdsUInt32 count = 0;
    error = EdsGetCameraList(&list);
    if (error == EDS_ERR_OK) {
      error = EdsGetChildCount(list, &count);
    }
    for (EdsUInt32 index = 0; error == EDS_ERR_OK && index < count; ++index) {
      EdsCameraRef camera = nullptr;
      error = EdsGetChildAtIndex(list, static_cast<EdsInt32>(index),
                                 reinterpret_cast<EdsBaseRef*>(&camera));
      EdsDeviceInfo info{};
      if (error == EDS_ERR_OK) {
        error = EdsGetDeviceInfo(camera, &info);
      }
      if (error == EDS_ERR_OK && QString::fromUtf8(info.szPortName) == device_.id) {
        camera_ = camera;
        break;
      }
      if (camera != nullptr) {
        EdsRelease(camera);
      }
    }
    if (list != nullptr) {
      EdsRelease(list);
    }
    if (camera_ == nullptr) {
      initializationError_ = "The selected Canon camera is no longer available.";
      return;
    }

    error = EdsOpenSession(camera_);
    if (error == EDS_ERR_OK) {
      sessionOpen_ = true;
      EdsSetObjectEventHandler(camera_, kEdsObjectEvent_All, objectEvent, this);
      EdsSetPropertyEventHandler(camera_, kEdsPropertyEvent_All, propertyEvent, this);
      EdsSetCameraStateEventHandler(camera_, kEdsStateEvent_All, stateEvent, this);
      EdsUInt32 saveTo = kEdsSaveTo_Host;
      error = EdsSetPropertyData(camera_, kEdsPropID_SaveTo, 0, sizeof(saveTo), &saveTo);
      EdsCapacity capacity{0x7fffffff, 0x1000, 1};
      if (error == EDS_ERR_OK) {
        error = EdsSetCapacity(camera_, capacity);
      }
    }
    if (error != EDS_ERR_OK) {
      initializationError_ = "Could not open the Canon camera: " + errorText(error);
      sessionOpen_ = false;
    }
  }

  void poll() {
    EdsGetEvent();
    if (!liveViewStarted_) {
      return;
    }
    EdsStreamRef stream = nullptr;
    EdsEvfImageRef image = nullptr;
    EdsError error = EdsCreateMemoryStream(0, &stream);
    if (error == EDS_ERR_OK) {
      error = EdsCreateEvfImageRef(stream, &image);
    }
    if (error == EDS_ERR_OK) {
      error = EdsDownloadEvfImage(camera_, image);
    }
    if (error == EDS_ERR_OK) {
      EdsVoid* bytes = nullptr;
      EdsUInt64 length = 0;
      if (EdsGetPointer(stream, &bytes) == EDS_ERR_OK &&
          EdsGetLength(stream, &length) == EDS_ERR_OK) {
        QImage frame =
            QImage::fromData(static_cast<const uchar*>(bytes), static_cast<int>(length), "JPEG");
        if (!frame.isNull()) {
          if (lvSimulation_ && exposurePreviewOffset_ != 0.0) {
            frame.convertTo(QImage::Format_RGB32);
            const double multiplier = std::exp2(exposurePreviewOffset_);
            for (int y = 0; y < frame.height(); ++y) {
              auto* pixels = reinterpret_cast<QRgb*>(frame.scanLine(y));
              for (int x = 0; x < frame.width(); ++x) {
                const QRgb pixel = pixels[x];
                pixels[x] = qRgb(std::clamp(static_cast<int>(qRed(pixel) * multiplier), 0, 255),
                                 std::clamp(static_cast<int>(qGreen(pixel) * multiplier), 0, 255),
                                 std::clamp(static_cast<int>(qBlue(pixel) * multiplier), 0, 255));
              }
            }
          }
          emit previewFrame(frame);
        }
      }
    }
    if (image != nullptr) {
      EdsRelease(image);
    }
    if (stream != nullptr) {
      EdsRelease(stream);
    }
  }

  void download(EdsDirectoryItemRef item) {
    if (pendingCaptureBase_.isEmpty()) {
      EdsDownloadCancel(item);
      return;
    }
    EdsDirectoryItemInfo info{};
    EdsError error = EdsGetDirectoryItemInfo(item, &info);
    const QString suffix = QFileInfo(QString::fromUtf8(info.szFileName)).suffix();
    const QString filePath =
        pendingCaptureBase_ + '.' + (suffix.isEmpty() ? QString("jpg") : suffix.toLower());
    EdsStreamRef stream = nullptr;
    if (error == EDS_ERR_OK) {
      const QByteArray encodedPath = QFile::encodeName(filePath);
      error = EdsCreateFileStream(encodedPath.constData(), kEdsFileCreateDisposition_CreateAlways,
                                  kEdsAccess_ReadWrite, &stream);
    }
    if (error == EDS_ERR_OK) {
      error = EdsDownload(item, info.size, stream);
    }
    if (error == EDS_ERR_OK) {
      error = EdsDownloadComplete(item);
    }
    if (stream != nullptr) {
      EdsRelease(stream);
    }
    pendingCaptureBase_.clear();
    const QString warning = std::move(pendingCaptureWarning_);
    pendingCaptureWarning_.clear();
    const EdsError previewError = restoreDepthOfFieldPreview();
    if (error == EDS_ERR_OK) {
      emit captureCompleted(filePath);
    } else {
      QFile::remove(filePath);
      emit errorOccurred("Could not download the Canon image: " + errorText(error));
    }
    if (previewError != EDS_ERR_OK) {
      emit errorOccurred("Could not restore Canon depth of field preview: " +
                         errorText(previewError));
    }
    if (!warning.isEmpty()) {
      emit errorOccurred(warning);
    }
  }

  CameraDevice device_;
  EdsCameraRef camera_ = nullptr;
  QTimer pollTimer_;
  QString initializationError_;
  QString pendingCaptureBase_;
  QString pendingCaptureWarning_;
  bool sdkInitialized_ = false;
  bool sessionOpen_ = false;
  bool liveViewStarted_ = false;
  mutable bool depthOfFieldPreview_ = false;
  bool restoreDepthOfFieldPreviewAfterCapture_ = false;
  bool externalFlash_ = false;
  bool lvSimulation_ = true;
  double exposurePreviewOffset_ = 0.0;
};

}  // namespace

std::vector<CameraDevice> availableCanonCameras() {
  std::vector<CameraDevice> devices;
  if (EdsInitializeSDK() != EDS_ERR_OK) {
    return devices;
  }
  EdsCameraListRef list = nullptr;
  EdsUInt32 count = 0;
  if (EdsGetCameraList(&list) == EDS_ERR_OK && EdsGetChildCount(list, &count) == EDS_ERR_OK) {
    for (EdsUInt32 index = 0; index < count; ++index) {
      EdsCameraRef camera = nullptr;
      if (EdsGetChildAtIndex(list, static_cast<EdsInt32>(index),
                             reinterpret_cast<EdsBaseRef*>(&camera)) != EDS_ERR_OK) {
        continue;
      }
      EdsDeviceInfo info{};
      if (EdsGetDeviceInfo(camera, &info) == EDS_ERR_OK) {
        devices.push_back({"canon", QString::fromUtf8(info.szPortName),
                           QString::fromUtf8(info.szDeviceDescription)});
      }
      EdsRelease(camera);
    }
  }
  if (list != nullptr) {
    EdsRelease(list);
  }
  EdsTerminateSDK();
  return devices;
}

std::unique_ptr<CameraSession> openCanonCamera(const CameraDevice& device, QObject* parent) {
  return std::make_unique<CanonSession>(device, parent);
}
