#include "v4l2_controls.h"

#include <fcntl.h>
#include <linux/videodev2.h>
#include <sys/ioctl.h>
#include <unistd.h>

#include <QByteArray>

#include <algorithm>

class V4l2Controls::Impl {
 public:
  explicit Impl(const QString& deviceId) {
    const int pathStart = deviceId.indexOf("/dev/video");
    if (pathStart >= 0) {
      const QByteArray path = deviceId.sliced(pathStart).toUtf8();
      descriptor_ = open(path.constData(), O_RDWR | O_NONBLOCK);
    }
    if (control(V4L2_CID_EXPOSURE_ABSOLUTE)) {
      setControl(V4L2_CID_EXPOSURE_AUTO, V4L2_EXPOSURE_MANUAL);
    }
  }

  ~Impl() {
    if (descriptor_ >= 0) {
      close(descriptor_);
    }
  }

  [[nodiscard]] std::optional<V4l2RangeControl> control(quint32 id) const {
    if (descriptor_ < 0) {
      return std::nullopt;
    }
    v4l2_queryctrl query{};
    query.id = id;
    if (ioctl(descriptor_, VIDIOC_QUERYCTRL, &query) < 0 ||
        query.flags & (V4L2_CTRL_FLAG_DISABLED | V4L2_CTRL_FLAG_READ_ONLY)) {
      return std::nullopt;
    }
    v4l2_control control{};
    control.id = id;
    if (ioctl(descriptor_, VIDIOC_G_CTRL, &control) < 0) {
      return std::nullopt;
    }
    return V4l2RangeControl{control.value, query.minimum, query.maximum,
                            std::max(1, query.step)};
  }

  bool setControl(quint32 id, int value) {
    if (descriptor_ < 0) {
      return false;
    }
    v4l2_control control{};
    control.id = id;
    control.value = value;
    return ioctl(descriptor_, VIDIOC_S_CTRL, &control) == 0;
  }

 private:
  int descriptor_ = -1;
};

V4l2Controls::V4l2Controls(const QString& deviceId)
    : impl_(std::make_unique<Impl>(deviceId)) {}

V4l2Controls::~V4l2Controls() = default;

std::optional<V4l2RangeControl> V4l2Controls::gain() const {
  return impl_->control(V4L2_CID_GAIN);
}

std::optional<V4l2RangeControl> V4l2Controls::exposure() const {
  return impl_->control(V4L2_CID_EXPOSURE_ABSOLUTE);
}

bool V4l2Controls::setGain(int value) {
  return impl_->setControl(V4L2_CID_GAIN, value);
}

bool V4l2Controls::setExposure(int value) {
  return impl_->setControl(V4L2_CID_EXPOSURE_ABSOLUTE, value);
}
