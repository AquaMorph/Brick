#pragma once

#include <algorithm>
#include <cstdint>

namespace PlaybackTiming {

inline std::int64_t transitionAt(std::int64_t elapsedNanoseconds, int fps,
                                 std::int64_t currentTransition) {
  return std::max(currentTransition + 1,
                  elapsedNanoseconds * fps / 1'000'000'000);
}

inline int delayMilliseconds(std::int64_t elapsedNanoseconds, int fps,
                             std::int64_t currentTransition) {
  const std::int64_t deadlineNanoseconds =
      (currentTransition + 1) * 1'000'000'000 / fps;
  const std::int64_t remainingNanoseconds =
      deadlineNanoseconds - elapsedNanoseconds;
  return static_cast<int>(
      std::max<std::int64_t>(1, (remainingNanoseconds + 999'999) / 1'000'000));
}

}  // namespace PlaybackTiming
