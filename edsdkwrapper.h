#ifndef EDSDKWRAPPER_H
#define EDSDKWRAPPER_H

#include "EDSDK.h"
#include "EDSDKErrors.h"
#include "EDSDKTypes.h"

EdsError getFirstCamera(EdsCameraRef *camera);
EdsError takePicture(EdsCameraRef camera);
EdsError startLiveview(EdsCameraRef camera);
EdsError downloadEvfData(EdsCameraRef camera);
EdsError endLiveview(EdsCameraRef camera);
char* getCameraName(EdsCameraRef camera);

#endif // EDSDKWRAPPER_H
