#pragma once

#include <QImage>
#include <QObject>
#include <QString>

#include <memory>
#include <vector>

struct CameraDevice {
  QString backend;
  QString id;
  QString displayName;
};

struct CameraSettingChoice {
  QString value;
  QString label;
};

struct CameraSetting {
  QString id;
  QString label;
  QString value;
  std::vector<CameraSettingChoice> choices;
};

class CameraSession : public QObject {
  Q_OBJECT

 public:
  using QObject::QObject;
  ~CameraSession() override = default;

  [[nodiscard]] virtual QString backend() const = 0;
  [[nodiscard]] virtual QString deviceId() const = 0;
  [[nodiscard]] virtual QString displayName() const = 0;
  [[nodiscard]] virtual bool isReady() const = 0;
  [[nodiscard]] virtual std::vector<CameraSetting> settings() const = 0;

  virtual void start() = 0;
  virtual void stop() = 0;
  virtual void capture(const QString& destinationBase) = 0;
  virtual void setSetting(const QString& id, const QString& value) = 0;

 signals:
  void previewFrame(const QImage& image);
  void settingsChanged();
  void captureCompleted(const QString& filePath);
  void errorOccurred(const QString& message);
};

[[nodiscard]] std::vector<CameraDevice> availableCameras();
[[nodiscard]] std::unique_ptr<CameraSession> openCamera(
    const CameraDevice& device, QObject* parent = nullptr);
