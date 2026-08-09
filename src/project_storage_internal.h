#pragma once

#include <QString>


namespace project_storage {

inline constexpr auto kShotConfigFileName = "shot.conf";
inline constexpr auto kTakeConfigFileName = "take.conf";

inline void setError(QString* error, const QString& message) {
  if (error != nullptr) {
    *error = message;
  }
}

}  // namespace project_storage
