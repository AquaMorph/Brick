#include "edsdkwrapper.h"

EdsError getFirstCamera(EdsCameraRef *camera) {
  EdsError err = EDS_ERR_OK;
  EdsCameraListRef cameraList = NULL;
  EdsUInt32 count = 0;
  // Get camera list
  err = EdsGetCameraList(&cameraList);
  // Get number of cameras
  if(err == EDS_ERR_OK) {
    err = EdsGetChildCount(cameraList, &count);
    if(count == 0) {
      err = EDS_ERR_DEVICE_NOT_FOUND;
    }
  }
  // Get first camera retrieved
  if(err == EDS_ERR_OK) {
    err = EdsGetChildAtIndex(cameraList , 0 , camera);
  }
  // Release camera list
  if(cameraList != NULL) {
    EdsRelease(cameraList);
    cameraList = NULL;
  }
  return err;
}

EdsError takePicture(EdsCameraRef camera) {
  EdsError err;
  err = EdsSendCommand(camera , kEdsCameraCommand_PressShutterButton
    , kEdsCameraCommand_ShutterButton_Completely);
  err = EdsSendCommand(camera, kEdsCameraCommand_PressShutterButton
    , kEdsCameraCommand_ShutterButton_OFF);
  return err;
}

EdsError startLiveview(EdsCameraRef camera) {
  EdsError err = EDS_ERR_OK;
  // Get the output device for the live view image
  EdsUInt32 device;
  err = EdsGetPropertyData(camera, kEdsPropID_Evf_OutputDevice, 0 , sizeof(device), &device );
  // PC live view starts by setting the PC as the output device for the live view image.
  if(err == EDS_ERR_OK) {
    device |= kEdsEvfOutputDevice_PC;
  err = EdsSetPropertyData(camera, kEdsPropID_Evf_OutputDevice, 0 , sizeof(device), &device);
  }
  // A property change event notification is issued from the camera if property settings are made successfully.
  // Start downloading of the live view image once the property change notification arrives.
  return err;
}

EdsError downloadEvfData(EdsCameraRef camera) {
  EdsError err = EDS_ERR_OK;
  EdsStreamRef stream = NULL;
  EdsEvfImageRef evfImage = NULL;
  // Create memory stream.
  err = EdsCreateMemoryStream( 0, &stream);
  // Create EvfImageRef.
  if(err == EDS_ERR_OK) {
    err = EdsCreateEvfImageRef(stream, &evfImage);
  }
  // Download live view image data.
  if(err == EDS_ERR_OK) {
    err = EdsDownloadEvfImage(camera, evfImage);
  }
  // Get the incidental data of the image.
  if(err == EDS_ERR_OK) {
    // Get the zoom ratio
    EdsUInt32 zoom;
    EdsGetPropertyData(evfImage, kEdsPropID_Evf_ZoomPosition, 0 , sizeof(zoom), &zoom);
    // Get the focus and zoom border position
    EdsPoint point;
    EdsGetPropertyData(evfImage, kEdsPropID_Evf_ZoomPosition, 0 , sizeof(point), &point);
  }
  //
  // Display image
  //
  // Release stream
  if(stream != NULL) {
    EdsRelease(stream);
    stream = NULL;
  }
  // Release evfImage
  if(evfImage != NULL) {
    EdsRelease(evfImage);
    evfImage = NULL;
  }
  return err;
}

EdsError endLiveview(EdsCameraRef camera) {
  EdsError err = EDS_ERR_OK;
  // Get the output device for the live view image
  EdsUInt32 device;
  err = EdsGetPropertyData(camera, kEdsPropID_Evf_OutputDevice, 0 , sizeof(device), &device );
  // PC live view ends if the PC is disconnected from the live view image output device.
  if(err == EDS_ERR_OK) {
    device &= ~kEdsEvfOutputDevice_PC;
    err = EdsSetPropertyData(camera, kEdsPropID_Evf_OutputDevice, 0 , sizeof(device), &device);
  }
  return err;
}

char* getCameraName(EdsCameraRef camera) {
  char tmp[EDS_MAX_NAME] = {0};
  EdsGetPropertyData(camera, kEdsPropID_ProductName , 0 , sizeof(tmp), tmp);
  char* cameraName = (char *) malloc(sizeof(tmp));
  strcpy(cameraName, tmp);
  return cameraName;
}
