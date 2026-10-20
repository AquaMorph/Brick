#include <EDSDK.h>

#include <iomanip>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <limits>

namespace {

class SdkLifetime {
 public:
  SdkLifetime() : error(EdsInitializeSDK()) {}
  ~SdkLifetime() {
    if (error == EDS_ERR_OK) {
      EdsTerminateSDK();
    }
  }

  EdsError error;
};

class SdkRef {
 public:
  SdkRef() = default;
  SdkRef(const SdkRef&) = delete;
  SdkRef& operator=(const SdkRef&) = delete;
  ~SdkRef() {
    if (value) {
      EdsRelease(value);
    }
  }

  EdsBaseRef value = nullptr;
};

int reportError(const char* operation, EdsError error) {
  std::cerr << operation << " (Canon EDSDK error 0x" << std::hex << std::setfill('0')
            << std::setw(8) << error << ')';
  return 1;
}

}  // namespace


#ifdef _WIN32
int wmain(int argc, wchar_t* argv[]) {
#else
int main(int argc, char* argv[]) {
#endif
  if (argc != 3) {
    std::cerr << "Usage: brick_canon_raw INPUT_RAW OUTPUT_PPM";
    return 1;
  }
  SdkLifetime sdk;
  if (sdk.error != EDS_ERR_OK) {
    return reportError("Could not initialize the RAW SDK", sdk.error);
  }
  SdkRef input;
#ifdef _WIN32
  EdsError error = EdsCreateFileStreamEx(
      argv[1], kEdsFileCreateDisposition_OpenExisting, kEdsAccess_Read, &input.value);
#else
  EdsError error = EdsCreateFileStream(
      argv[1], kEdsFileCreateDisposition_OpenExisting, kEdsAccess_Read, &input.value);
#endif
  if (error != EDS_ERR_OK) {
    return reportError("Could not open the RAW capture", error);
  }
  SdkRef image;
  error = EdsCreateImageRef(input.value, &image.value);
  if (error != EDS_ERR_OK) {
    return reportError("Could not read the RAW capture", error);
  }
  // Develop full-resolution RAW using the camera's recorded settings. The
  // input stream is read-only, so developing a preview cannot change the RAW.
  EdsImageInfo info{};
  error = EdsGetImageInfo(image.value, kEdsImageSrc_RAWFullView, &info);
  if (error != EDS_ERR_OK) {
    return reportError("Could not read full-resolution RAW information", error);
  }
  const EdsUInt32 colorSpace = kEdsColorSpace_sRGB;
  error = EdsSetPropertyData(image.value, kEdsPropID_ColorSpace, 0,
                             sizeof(colorSpace), &colorSpace);
  if (error != EDS_ERR_OK) {
    return reportError("Could not set the RAW preview color space", error);
  }
  const auto width = info.effectiveRect.size.width;
  const auto height = info.effectiveRect.size.height;
  if (width <= 0 || height <= 0) {
    return reportError("Invalid RAW image dimensions", EDS_ERR_INVALID_PARAMETER);
  }
  const auto size = static_cast<EdsUInt64>(width) * height * 3;
  if (size > static_cast<EdsUInt64>(std::numeric_limits<std::streamsize>::max())) {
    return reportError("RAW image is too large", EDS_ERR_INVALID_LENGTH);
  }
  SdkRef output;
  error = EdsCreateMemoryStream(0, &output.value);
  if (error != EDS_ERR_OK) {
    return reportError("Could not create the RAW preview", error);
  }
  error = EdsGetImage(image.value, kEdsImageSrc_RAWFullView, kEdsTargetImageType_RGB,
                      info.effectiveRect, info.effectiveRect.size, output.value);
  if (error != EDS_ERR_OK) {
    return reportError("Could not develop the RAW capture", error);
  }
  EdsVoid* pixels = nullptr;
  EdsUInt64 length = 0;
  error = EdsGetPointer(output.value, &pixels);
  if (error == EDS_ERR_OK) {
    error = EdsGetLength(output.value, &length);
  }
  if (error != EDS_ERR_OK || pixels == nullptr || length < size) {
    return reportError("Incomplete RAW preview", error == EDS_ERR_OK ? EDS_ERR_INVALID_LENGTH : error);
  }
  // PPM transports the SDK's RGB pixels losslessly; Qt writes the project JPEGs.
  std::ofstream file(std::filesystem::path(argv[2]), std::ios::binary | std::ios::trunc);
  file << "P6\n" << width << ' ' << height << "\n255\n";
  file.write(static_cast<const char*>(pixels), static_cast<std::streamsize>(size));
  file.close();
  if (!file) {
    return reportError("Could not write the RAW preview", EDS_ERR_FILE_WRITE_ERROR);
  }
  EdsUInt32 orientation = 1;
  EdsGetPropertyData(image.value, kEdsPropID_Orientation, 0, sizeof(orientation), &orientation);
  std::cout << orientation;
  return 0;
}
