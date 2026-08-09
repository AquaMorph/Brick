#pragma once

#include <QImage>
#include <QObject>
#include <QString>
#include <QtGlobal>

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

enum class CameraSettingType { Choice, SteppedChoice, Toggle, IntegerRange, DecimalRange };

struct CameraSetting {
  QString id;
  QString label;
  QString value;
  std::vector<CameraSettingChoice> choices;
  CameraSettingType type = CameraSettingType::Choice;
  double minimum = 0.0;
  double maximum = 0.0;
  double step = 1.0;
  int decimals = 0;
  QString suffix;
  QString group;
  bool enabled = true;
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
  virtual void capture(quint64 captureId, const QString& destinationBase) = 0;
  virtual void setSetting(const QString& id, const QString& value) = 0;

 signals:
  void previewFrame(const QImage& image);
  void settingsChanged();
  void captureCompleted(quint64 captureId, const QString& filePath);
  void captureFailed(quint64 captureId, const QString& message);
  void errorOccurred(const QString& message);
};
