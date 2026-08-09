#pragma once

#include <QString>

#include <memory>
#include <optional>

struct V4l2RangeControl {
  int value;
  int minimum;
  int maximum;
  int step;
};

class V4l2Controls {
 public:
  explicit V4l2Controls(const QString& deviceId);
  ~V4l2Controls();

  V4l2Controls(const V4l2Controls&) = delete;
  V4l2Controls& operator=(const V4l2Controls&) = delete;

  [[nodiscard]] std::optional<V4l2RangeControl> gain() const;
  [[nodiscard]] std::optional<V4l2RangeControl> exposure() const;
  bool setGain(int value);
  bool setExposure(int value);

 private:
  class Impl;
  std::unique_ptr<Impl> impl_;
};
