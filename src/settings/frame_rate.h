#pragma once

#include <QSettings>

#include <algorithm>
#include <array>

namespace FrameRate {

inline constexpr std::array kSupported = {8, 12, 15, 24, 30};
inline constexpr int kDefault = 24;
inline constexpr auto kDefaultSettingKey = "Animation/defaultFramesPerSecond";

inline bool isSupported(int framesPerSecond) {
  return std::ranges::find(kSupported, framesPerSecond) != kSupported.end();
}

inline int applicationDefault() {
  const int framesPerSecond =
      QSettings().value(kDefaultSettingKey, kDefault).toInt();
  return isSupported(framesPerSecond) ? framesPerSecond : kDefault;
}

inline bool saveApplicationDefault(int framesPerSecond) {
  if (!isSupported(framesPerSecond)) {
    return false;
  }

  QSettings settings;
  settings.setValue(kDefaultSettingKey, framesPerSecond);
  settings.sync();
  return settings.status() == QSettings::NoError;
}

}  // namespace FrameRate
