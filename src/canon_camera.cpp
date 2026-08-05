#include "canon_camera.h"

#include <EDSDK.h>

#include <QByteArray>
#include <QFile>
#include <QFileInfo>
#include <QTimer>
#include <QtGlobal>
#include <algorithm>
#include <array>
#include <cmath>
#include <map>
#include <utility>

namespace {

QString errorText(EdsError error) {
  return QString("Canon EDSDK error 0x%1").arg(error, 8, 16, QLatin1Char('0'));
}

QString hexValue(EdsInt32 value) { return QString::number(static_cast<EdsUInt32>(value), 16); }

bool isAutomaticIso(EdsInt32 value) { return value == 0x00 || value == 0x01; }

QString canonLabel(EdsPropertyID property, EdsInt32 value) {
  static const std::map<EdsInt32, QString> iso = {
      {0x00, "Auto"},    {0x01, "Auto ISO"}, {0x28, "6"},       {0x30, "12"},
      {0x38, "25"},      {0x40, "50"},       {0x43, "64"},      {0x45, "80"},
      {0x48, "100"},     {0x4b, "125"},      {0x4d, "160"},     {0x50, "200"},
      {0x53, "250"},     {0x55, "320"},      {0x58, "400"},     {0x5b, "500"},
      {0x5d, "640"},     {0x60, "800"},      {0x63, "1000"},    {0x65, "1250"},
      {0x68, "1600"},    {0x6b, "2000"},     {0x6d, "2500"},    {0x70, "3200"},
      {0x73, "4000"},    {0x75, "5000"},     {0x78, "6400"},    {0x7b, "8000"},
      {0x7d, "10000"},   {0x80, "12800"},    {0x83, "16000"},   {0x85, "20000"},
      {0x88, "25600"},   {0x8b, "32000"},    {0x8d, "40000"},   {0x90, "51200"},
      {0x93, "64000"},   {0x95, "80000"},    {0x98, "102400"},  {0xa0, "204800"},
      {0xa8, "409600"},  {0xb0, "819200"}};
  static const std::map<EdsInt32, QString> aperture = {
      {0x08, "f/1.0"}, {0x0b, "f/1.1"}, {0x0c, "f/1.2"}, {0x0d, "f/1.2"},
      {0x10, "f/1.4"}, {0x13, "f/1.6"}, {0x14, "f/1.8"}, {0x15, "f/1.8"},
      {0x18, "f/2.0"}, {0x1b, "f/2.2"}, {0x1c, "f/2.5"}, {0x1d, "f/2.5"},
      {0x20, "f/2.8"}, {0x23, "f/3.2"}, {0x85, "f/3.4"}, {0x24, "f/3.5"},
      {0x25, "f/3.5"},
      {0x28, "f/4.0"}, {0x2b, "f/4.5"}, {0x2c, "f/4.5"}, {0x2d, "f/5.0"},
      {0x30, "f/5.6"}, {0x33, "f/6.3"}, {0x34, "f/6.7"}, {0x35, "f/7.1"},
      {0x38, "f/8.0"}, {0x3b, "f/9.0"}, {0x3c, "f/9.5"}, {0x3d, "f/10"},
      {0x40, "f/11"},  {0x43, "f/13"},  {0x44, "f/13"},  {0x45, "f/14"},
      {0x48, "f/16"},  {0x4b, "f/18"},  {0x4c, "f/19"},  {0x4d, "f/20"},
      {0x50, "f/22"},  {0x53, "f/25"},  {0x54, "f/27"},  {0x55, "f/29"},
      {0x58, "f/32"},  {0x5b, "f/36"},  {0x5c, "f/38"},  {0x5d, "f/40"},
      {0x60, "f/45"},  {0x63, "f/51"},  {0x64, "f/54"},  {0x65, "f/57"},
      {0x68, "f/64"},  {0x6b, "f/72"},  {0x6c, "f/76"},  {0x6d, "f/80"},
      {0x70, "f/91"}};
  static const std::map<EdsInt32, QString> shutter = {
      {0x0c, "Bulb"},   {0x10, "30 s"},   {0x13, "25 s"},   {0x14, "20 s"},
      {0x15, "20 s"},   {0x18, "15 s"},   {0x1b, "13 s"},   {0x1c, "10 s"},
      {0x1d, "10 s"},   {0x20, "8 s"},    {0x23, "6 s"},    {0x24, "6 s"},
      {0x25, "5 s"},    {0x28, "4 s"},    {0x2b, "3.2 s"},  {0x2c, "3 s"},
      {0x2d, "2.5 s"},  {0x30, "2 s"},    {0x33, "1.6 s"},  {0x34, "1.5 s"},
      {0x35, "1.3 s"},  {0x38, "1 s"},    {0x3b, "0.8 s"},  {0x3c, "0.7 s"},
      {0x3d, "0.6 s"},  {0x40, "1/2"},    {0x43, "0.4 s"},  {0x44, "0.3 s"},
      {0x45, "0.3 s"},  {0x48, "1/4"},    {0x4b, "1/5"},    {0x4c, "1/6"},
      {0x4d, "1/6"},    {0x50, "1/8"},    {0x53, "1/10"},   {0x54, "1/10"},
      {0x55, "1/13"},   {0x58, "1/15"},   {0x5b, "1/20"},   {0x5c, "1/20"},
      {0x5d, "1/25"},   {0x60, "1/30"},   {0x63, "1/40"},   {0x64, "1/45"},
      {0x65, "1/50"},   {0x68, "1/60"},   {0x6b, "1/80"},   {0x6c, "1/90"},
      {0x6d, "1/100"},  {0x70, "1/125"},  {0x73, "1/160"},  {0x74, "1/180"},
      {0x75, "1/200"},  {0x78, "1/250"},  {0x7b, "1/320"},  {0x7c, "1/350"},
      {0x7d, "1/400"},  {0x80, "1/500"},  {0x83, "1/640"},  {0x84, "1/750"},
      {0x85, "1/800"},  {0x88, "1/1000"}, {0x8b, "1/1250"}, {0x8c, "1/1500"},
      {0x8d, "1/1600"}, {0x90, "1/2000"}, {0x93, "1/2500"}, {0x94, "1/3000"},
      {0x95, "1/3200"}, {0x98, "1/4000"}, {0x9b, "1/5000"}, {0x9c, "1/6000"},
      {0x9d, "1/6400"}, {0xa0, "1/8000"}};
  static const std::map<EdsInt32, QString> whiteBalance = {
      {kEdsWhiteBalance_Auto, "Auto"},
      {kEdsWhiteBalance_Daylight, "Daylight"},
      {kEdsWhiteBalance_Cloudy, "Cloudy"},
      {kEdsWhiteBalance_Tungsten, "Tungsten"},
      {kEdsWhiteBalance_Fluorescent, "Fluorescent"},
      {kEdsWhiteBalance_Strobe, "Flash"},
      {kEdsWhiteBalance_WhitePaper, "Custom 1"},
      {kEdsWhiteBalance_Shade, "Shade"},
      {kEdsWhiteBalance_ColorTemp, "Manual"},
      {kEdsWhiteBalance_PCSet1, "PC set 1"},
      {kEdsWhiteBalance_PCSet2, "PC set 2"},
      {kEdsWhiteBalance_PCSet3, "PC set 3"},
      {kEdsWhiteBalance_WhitePaper2, "Custom 2"},
      {kEdsWhiteBalance_WhitePaper3, "Custom 3"},
      {kEdsWhiteBalance_WhitePaper4, "Custom 4"},
      {kEdsWhiteBalance_WhitePaper5, "Custom 5"},
      {kEdsWhiteBalance_PCSet4, "PC set 4"},
      {kEdsWhiteBalance_PCSet5, "PC set 5"},
      {kEdsWhiteBalance_AwbWhite, "Auto (white priority)"},
      {kEdsWhiteBalance_Click, "Click white balance"},
      {kEdsWhiteBalance_Pasted, "Pasted white balance"}};
  static const std::map<EdsInt32, QString> pictureStyle = {
      {kEdsPictureStyle_Standard, "Standard"},   {kEdsPictureStyle_Portrait, "Portrait"},
      {kEdsPictureStyle_Landscape, "Landscape"}, {kEdsPictureStyle_Neutral, "Neutral"},
      {kEdsPictureStyle_Faithful, "Faithful"},   {kEdsPictureStyle_Monochrome, "Monochrome"},
      {kEdsPictureStyle_Auto, "Auto"},           {kEdsPictureStyle_FineDetail, "Fine detail"},
      {kEdsPictureStyle_User1, "User 1"},        {kEdsPictureStyle_User2, "User 2"},
      {kEdsPictureStyle_User3, "User 3"}};
  static const std::map<EdsInt32, QString> aeMode = {
      {kEdsAEMode_Program, "Program"},
      {kEdsAEMode_Tv, "Shutter priority"},
      {kEdsAEMode_Av, "Aperture priority"},
      {kEdsAEMode_Manual, "Manual"},
      {kEdsAEMode_Bulb, "Bulb"},
      {kEdsAEMode_A_DEP, "Automatic depth-of-field"},
      {kEdsAEMode_DEP, "Depth-of-field"},
      {kEdsAEMode_Custom, "Custom 1"},
      {kEdsAEMode_Lock, "Lock"},
      {kEdsAEMode_Green, "Auto"},
      {kEdsAEMode_NightPortrait, "Night portrait"},
      {kEdsAEMode_Sports, "Sports"},
      {kEdsAEMode_Portrait, "Portrait"},
      {kEdsAEMode_Landscape, "Landscape"},
      {kEdsAEMode_Closeup, "Close-up"},
      {kEdsAEMode_FlashOff, "Flash off"},
      {0x10, "Custom 2"},
      {0x11, "Custom 3"},
      {kEdsAEMode_CreativeAuto, "Creative auto"},
      {kEdsAEMode_Movie, "Movie"},
      {kEdsAEMode_PhotoInMovie, "Photo in movie"},
      {kEdsAEMode_SceneIntelligentAuto, "Scene intelligent auto"},
      {kEdsAEMode_NightScenes, "Handheld night scene"},
      {kEdsAEMode_BacklitScenes, "HDR backlight control"},
      {kEdsAEMode_SCN, "Special scene"},
      {kEdsAEMode_Children, "Children"},
      {kEdsAEMode_Food, "Food"},
      {kEdsAEMode_CandlelightPortraits, "Candlelight portrait"},
      {kEdsAEMode_CreativeFilter, "Creative filter"},
      {kEdsAEMode_RoughMonoChrome, "Grainy black and white"},
      {kEdsAEMode_SoftFocus, "Soft focus"},
      {kEdsAEMode_ToyCamera, "Toy camera"},
      {kEdsAEMode_Fisheye, "Fish-eye"},
      {kEdsAEMode_WaterColor, "Water painting"},
      {kEdsAEMode_Miniature, "Miniature"},
      {kEdsAEMode_Hdr_Standard, "HDR standard"},
      {kEdsAEMode_Hdr_Vivid, "HDR vivid"},
      {kEdsAEMode_Hdr_Bold, "HDR bold"},
      {kEdsAEMode_Hdr_Embossed, "HDR embossed"},
      {kEdsAEMode_Movie_Fantasy, "Movie fantasy"},
      {kEdsAEMode_Movie_Old, "Movie old movies"},
      {kEdsAEMode_Movie_Memory, "Movie memory"},
      {kEdsAEMode_Movie_DirectMono, "Movie dramatic black and white"},
      {kEdsAEMode_Movie_Mini, "Movie miniature"},
      {kEdsAEMode_PanningAssist, "Panning"},
      {kEdsAEMode_GroupPhoto, "Group photo"},
      {kEdsAEMode_Myself, "Self portrait"},
      {kEdsAEMode_PlusMovieAuto, "Hybrid auto"},
      {kEdsAEMode_SmoothSkin, "Smooth skin"},
      {kEdsAEMode_Panorama, "Panorama"},
      {kEdsAEMode_Silent, "Silent"},
      {kEdsAEMode_Flexible, "Flexible priority"},
      {kEdsAEMode_OilPainting, "Oil painting"},
      {kEdsAEMode_Fireworks, "Fireworks"},
      {kEdsAEMode_StarPortrait, "Star portrait"},
      {kEdsAEMode_StarNightscape, "Star nightscape"},
      {kEdsAEMode_StarTrails, "Star trails"},
      {kEdsAEMode_StarTimelapseMovie, "Star time-lapse movie"},
      {kEdsAEMode_BackgroundBlur, "Background blur"},
      {kEdsAEMode_VideoBlog, "Video blog"},
      {kEdsAEMode_Unknown, "Unknown"}};
  static const std::map<EdsInt32, QString> imageQuality = {
      {EdsImageQuality_LJ, "JPEG large"},
      {EdsImageQuality_MJ, "JPEG medium"},
      {EdsImageQuality_M1J, "JPEG medium 1"},
      {EdsImageQuality_M1F, "JPEG medium 1 fine"},
      {EdsImageQuality_M1N, "JPEG medium 1 normal"},
      {EdsImageQuality_M2J, "JPEG medium 2"},
      {EdsImageQuality_M2F, "JPEG medium 2 fine"},
      {EdsImageQuality_M2N, "JPEG medium 2 normal"},
      {EdsImageQuality_SJ, "JPEG small"},
      {EdsImageQuality_S1J, "JPEG small 1"},
      {EdsImageQuality_S2J, "JPEG small 2"},
      {EdsImageQuality_LJF, "JPEG large fine"},
      {EdsImageQuality_LJN, "JPEG large normal"},
      {EdsImageQuality_MJF, "JPEG medium fine"},
      {EdsImageQuality_MJN, "JPEG medium normal"},
      {EdsImageQuality_SJF, "JPEG small fine"},
      {EdsImageQuality_SJN, "JPEG small normal"},
      {EdsImageQuality_S1JF, "JPEG small 1 fine"},
      {EdsImageQuality_S1JN, "JPEG small 1 normal"},
      {EdsImageQuality_S2JF, "JPEG small 2 fine"},
      {EdsImageQuality_S3JF, "JPEG small 3 fine"},
      {EdsImageQuality_LR, "RAW"},
      {EdsImageQuality_LRLJF, "RAW + JPEG large fine"},
      {EdsImageQuality_LRLJN, "RAW + JPEG large normal"},
      {EdsImageQuality_LRMJF, "RAW + JPEG medium fine"},
      {EdsImageQuality_LRMJN, "RAW + JPEG medium normal"},
      {EdsImageQuality_LRSJF, "RAW + JPEG small fine"},
      {EdsImageQuality_LRSJN, "RAW + JPEG small normal"},
      {EdsImageQuality_LRS1JF, "RAW + JPEG small 1 fine"},
      {EdsImageQuality_LRS1JN, "RAW + JPEG small 1 normal"},
      {EdsImageQuality_LRS2JF, "RAW + JPEG small 2 fine"},
      {EdsImageQuality_LRS3JF, "RAW + JPEG small 3 fine"},
      {EdsImageQuality_LRLJ, "RAW + JPEG large"},
      {EdsImageQuality_LRMJ, "RAW + JPEG medium"},
      {EdsImageQuality_LRM1J, "RAW + JPEG medium 1"},
      {EdsImageQuality_LRM1F, "RAW + JPEG medium 1 fine"},
      {EdsImageQuality_LRM1N, "RAW + JPEG medium 1 normal"},
      {EdsImageQuality_LRM2J, "RAW + JPEG medium 2"},
      {EdsImageQuality_LRM2F, "RAW + JPEG medium 2 fine"},
      {EdsImageQuality_LRM2N, "RAW + JPEG medium 2 normal"},
      {EdsImageQuality_LRSJ, "RAW + JPEG small"},
      {EdsImageQuality_LRS1J, "RAW + JPEG small 1"},
      {EdsImageQuality_LRS2J, "RAW + JPEG small 2"},
      {EdsImageQuality_MR, "MRAW"},
      {EdsImageQuality_MRLJF, "MRAW + JPEG large fine"},
      {EdsImageQuality_MRLJN, "MRAW + JPEG large normal"},
      {EdsImageQuality_MRMJF, "MRAW + JPEG medium fine"},
      {EdsImageQuality_MRMJN, "MRAW + JPEG medium normal"},
      {EdsImageQuality_MRSJF, "MRAW + JPEG small fine"},
      {EdsImageQuality_MRSJN, "MRAW + JPEG small normal"},
      {EdsImageQuality_MRS1JF, "MRAW + JPEG small 1 fine"},
      {EdsImageQuality_MRS1JN, "MRAW + JPEG small 1 normal"},
      {EdsImageQuality_MRS2JF, "MRAW + JPEG small 2 fine"},
      {EdsImageQuality_MRS3JF, "MRAW + JPEG small 3 fine"},
      {EdsImageQuality_MRLJ, "MRAW + JPEG large"},
      {EdsImageQuality_MRM1J, "MRAW + JPEG medium 1"},
      {EdsImageQuality_MRM1F, "MRAW + JPEG medium 1 fine"},
      {EdsImageQuality_MRM1N, "MRAW + JPEG medium 1 normal"},
      {EdsImageQuality_MRM2J, "MRAW + JPEG medium 2"},
      {EdsImageQuality_MRM2F, "MRAW + JPEG medium 2 fine"},
      {EdsImageQuality_MRM2N, "MRAW + JPEG medium 2 normal"},
      {EdsImageQuality_MRSJ, "MRAW + JPEG small"},
      {EdsImageQuality_SR, "SRAW"},
      {EdsImageQuality_SRLJF, "SRAW + JPEG large fine"},
      {EdsImageQuality_SRLJN, "SRAW + JPEG large normal"},
      {EdsImageQuality_SRMJF, "SRAW + JPEG medium fine"},
      {EdsImageQuality_SRMJN, "SRAW + JPEG medium normal"},
      {EdsImageQuality_SRSJF, "SRAW + JPEG small fine"},
      {EdsImageQuality_SRSJN, "SRAW + JPEG small normal"},
      {EdsImageQuality_SRS1JF, "SRAW + JPEG small 1 fine"},
      {EdsImageQuality_SRS1JN, "SRAW + JPEG small 1 normal"},
      {EdsImageQuality_SRS2JF, "SRAW + JPEG small 2 fine"},
      {EdsImageQuality_SRS3JF, "SRAW + JPEG small 3 fine"},
      {EdsImageQuality_SRLJ, "SRAW + JPEG large"},
      {EdsImageQuality_SRM1J, "SRAW + JPEG medium 1"},
      {EdsImageQuality_SRM1F, "SRAW + JPEG medium 1 fine"},
      {EdsImageQuality_SRM1N, "SRAW + JPEG medium 1 normal"},
      {EdsImageQuality_SRM2J, "SRAW + JPEG medium 2"},
      {EdsImageQuality_SRM2F, "SRAW + JPEG medium 2 fine"},
      {EdsImageQuality_SRM2N, "SRAW + JPEG medium 2 normal"},
      {EdsImageQuality_SRSJ, "SRAW + JPEG small"},
      {EdsImageQuality_CR, "C-RAW"},
      {EdsImageQuality_CRLJF, "C-RAW + JPEG large fine"},
      {EdsImageQuality_CRMJF, "C-RAW + JPEG medium fine"},
      {EdsImageQuality_CRM1JF, "C-RAW + JPEG medium 1 fine"},
      {EdsImageQuality_CRM2JF, "C-RAW + JPEG medium 2 fine"},
      {EdsImageQuality_CRSJF, "C-RAW + JPEG small fine"},
      {EdsImageQuality_CRS1JF, "C-RAW + JPEG small 1 fine"},
      {EdsImageQuality_CRS2JF, "C-RAW + JPEG small 2 fine"},
      {EdsImageQuality_CRS3JF, "C-RAW + JPEG small 3 fine"},
      {EdsImageQuality_CRLJN, "C-RAW + JPEG large normal"},
      {EdsImageQuality_CRMJN, "C-RAW + JPEG medium normal"},
      {EdsImageQuality_CRM1JN, "C-RAW + JPEG medium 1 normal"},
      {EdsImageQuality_CRM2JN, "C-RAW + JPEG medium 2 normal"},
      {EdsImageQuality_CRSJN, "C-RAW + JPEG small normal"},
      {EdsImageQuality_CRS1JN, "C-RAW + JPEG small 1 normal"},
      {EdsImageQuality_CRLJ, "C-RAW + JPEG large"},
      {EdsImageQuality_CRMJ, "C-RAW + JPEG medium"},
      {EdsImageQuality_CRM1J, "C-RAW + JPEG medium 1"},
      {EdsImageQuality_CRM2J, "C-RAW + JPEG medium 2"},
      {EdsImageQuality_CRSJ, "C-RAW + JPEG small"},
      {EdsImageQuality_CRS1J, "C-RAW + JPEG small 1"},
      {EdsImageQuality_CRS2J, "C-RAW + JPEG small 2"},
      {EdsImageQuality_HEIFL, "HEIF large"},
      {EdsImageQuality_HEIFM, "HEIF medium"},
      {EdsImageQuality_HEIFM1, "HEIF medium 1"},
      {EdsImageQuality_HEIFM2, "HEIF medium 2"},
      {EdsImageQuality_HEIFLF, "HEIF large fine"},
      {EdsImageQuality_HEIFLN, "HEIF large normal"},
      {EdsImageQuality_HEIFMF, "HEIF medium fine"},
      {EdsImageQuality_HEIFMN, "HEIF medium normal"},
      {EdsImageQuality_HEIFS1, "HEIF small 1"},
      {EdsImageQuality_HEIFS1F, "HEIF small 1 fine"},
      {EdsImageQuality_HEIFS1N, "HEIF small 1 normal"},
      {EdsImageQuality_HEIFS2, "HEIF small 2"},
      {EdsImageQuality_HEIFS2F, "HEIF small 2 fine"},
      {EdsImageQuality_RHEIFL, "RAW + HEIF large"},
      {EdsImageQuality_RHEIFLF, "RAW + HEIF large fine"},
      {EdsImageQuality_RHEIFLN, "RAW + HEIF large normal"},
      {EdsImageQuality_RHEIFM, "RAW + HEIF medium"},
      {EdsImageQuality_RHEIFM1, "RAW + HEIF medium 1"},
      {EdsImageQuality_RHEIFM2, "RAW + HEIF medium 2"},
      {EdsImageQuality_RHEIFMF, "RAW + HEIF medium fine"},
      {EdsImageQuality_RHEIFMN, "RAW + HEIF medium normal"},
      {EdsImageQuality_RHEIFS1, "RAW + HEIF small 1"},
      {EdsImageQuality_RHEIFS1F, "RAW + HEIF small 1 fine"},
      {EdsImageQuality_RHEIFS1N, "RAW + HEIF small 1 normal"},
      {EdsImageQuality_RHEIFS2, "RAW + HEIF small 2"},
      {EdsImageQuality_RHEIFS2F, "RAW + HEIF small 2 fine"},
      {EdsImageQuality_CRHEIFL, "C-RAW + HEIF large"},
      {EdsImageQuality_CRHEIFLF, "C-RAW + HEIF large fine"},
      {EdsImageQuality_CRHEIFLN, "C-RAW + HEIF large normal"},
      {EdsImageQuality_CRHEIFM, "C-RAW + HEIF medium"},
      {EdsImageQuality_CRHEIFMF, "C-RAW + HEIF medium fine"},
      {EdsImageQuality_CRHEIFMN, "C-RAW + HEIF medium normal"},
      {EdsImageQuality_CRHEIFM1, "C-RAW + HEIF medium 1"},
      {EdsImageQuality_CRHEIFM2, "C-RAW + HEIF medium 2"},
      {EdsImageQuality_CRHEIFS1, "C-RAW + HEIF small 1"},
      {EdsImageQuality_CRHEIFS1F, "C-RAW + HEIF small 1 fine"},
      {EdsImageQuality_CRHEIFS1N, "C-RAW + HEIF small 1 normal"},
      {EdsImageQuality_CRHEIFS2, "C-RAW + HEIF small 2"},
      {EdsImageQuality_CRHEIFS2F, "C-RAW + HEIF small 2 fine"},
      {EdsImageQuality_Unknown, "Unknown"}};
  static const std::map<EdsInt32, QString> metering = {
      {1, "Spot"}, {3, "Evaluative"}, {4, "Partial"}, {5, "Center-weighted"}};
  static const std::map<EdsInt32, QString> driveMode = {
      {0x00, "Single-frame shooting"},
      {0x01, "Continuous shooting"},
      {0x02, "Video"},
      {0x03, "Not used"},
      {0x04, "High-speed continuous shooting"},
      {0x05, "Low-speed continuous shooting"},
      {0x06, "Single silent shooting"},
      {0x07, "10-second self-timer + continuous shooting"},
      {0x10, "10-second self-timer"},
      {0x11, "2-second self-timer"},
      {0x12, "Super high-speed continuous shooting"},
      {0x13, "Silent single shooting"},
      {0x14, "Silent continuous shooting"},
      {0x15, "Silent high-speed continuous shooting"},
      {0x16, "Silent low-speed continuous shooting"}};
  const std::map<EdsInt32, QString>* values = nullptr;
  if (property == kEdsPropID_ISOSpeed) {
    values = &iso;
  } else if (property == kEdsPropID_Av) {
    values = &aperture;
  } else if (property == kEdsPropID_Tv) {
    values = &shutter;
  } else if (property == kEdsPropID_WhiteBalance) {
    values = &whiteBalance;
  } else if (property == kEdsPropID_AEModeSelect) {
    values = &aeMode;
  } else if (property == kEdsPropID_ImageQuality) {
    values = &imageQuality;
  } else if (property == kEdsPropID_PictureStyle) {
    values = &pictureStyle;
  } else if (property == kEdsPropID_MeteringMode) {
    values = &metering;
  } else if (property == kEdsPropID_DriveMode) {
    values = &driveMode;
  }
  if (values != nullptr) {
    const auto match = values->find(value);
    if (match != values->end()) {
      return match->second;
    }
  }
  if (property == kEdsPropID_ExposureCompensation) {
    const int signedValue = value > 0x80 ? value - 0x100 : value;
    return QString::number(signedValue / 8.0, 'g', 2) + " EV";
  }
  if (property == kEdsPropID_ColorTemperature) {
    return QString::number(value) + " K";
  }
  return "0x" + hexValue(value).rightJustified(2, '0');
}

struct CanonProperty {
  EdsPropertyID id;
  const char* key;
  const char* label;
  CameraSettingType type = CameraSettingType::Choice;
  const char* group = "Capture";
};

constexpr std::array<CanonProperty, 11> kCanonProperties = {{
    {kEdsPropID_AEModeSelect, "aeMode", "Exposure mode"},
    {kEdsPropID_Tv, "shutter", "Shutter speed", CameraSettingType::SteppedChoice},
    {kEdsPropID_Av, "aperture", "Aperture", CameraSettingType::SteppedChoice},
    {kEdsPropID_ISOSpeed, "iso", "ISO", CameraSettingType::SteppedChoice},
    {kEdsPropID_ExposureCompensation, "exposureCompensation", "Exposure compensation",
     CameraSettingType::SteppedChoice},
    {kEdsPropID_WhiteBalance, "whiteBalance", "White balance"},
    {kEdsPropID_ColorTemperature, "colorTemperature", "Color temperature",
     CameraSettingType::SteppedChoice},
    {kEdsPropID_PictureStyle, "pictureStyle", "Picture style"},
    {kEdsPropID_ImageQuality, "imageQuality", "Image quality"},
    {kEdsPropID_MeteringMode, "meteringMode", "Metering"},
    {kEdsPropID_DriveMode, "driveMode", "Drive mode"},
}};

class CanonSession final : public CameraSession {
 public:
  CanonSession(CameraDevice device, QObject* parent)
      : CameraSession(parent), device_(std::move(device)) {
    pollTimer_.setInterval(33);
    connect(&pollTimer_, &QTimer::timeout, this, [this] { poll(); });
    initialize();
  }

  ~CanonSession() override {
    stop();
    if (camera_ != nullptr) {
      if (sessionOpen_) {
        EdsCloseSession(camera_);
      }
      EdsRelease(camera_);
    }
    if (sdkInitialized_) {
      EdsTerminateSDK();
    }
  }

  [[nodiscard]] QString backend() const override { return device_.backend; }

  [[nodiscard]] QString deviceId() const override { return device_.id; }

  [[nodiscard]] QString displayName() const override { return device_.displayName; }

  [[nodiscard]] bool isReady() const override { return sessionOpen_; }

  [[nodiscard]] std::vector<CameraSetting> settings() const override {
    std::vector<CameraSetting> result;
    if (!sessionOpen_) {
      return result;
    }
    for (const auto& property : kCanonProperties) {
      EdsPropertyDesc descriptor{};
      EdsInt32 current = 0;
      if (EdsGetPropertyDesc(camera_, property.id, &descriptor) != EDS_ERR_OK ||
          descriptor.numElements <= 0 ||
          EdsGetPropertyData(camera_, property.id, 0, sizeof(current), &current) != EDS_ERR_OK) {
        continue;
      }
      CameraSetting setting{property.key, property.label, hexValue(current), {}};
      setting.type = property.type;
      setting.group = property.group;
      for (int index = 0; index < descriptor.numElements; ++index) {
        const EdsInt32 value = descriptor.propDesc[index];
        if (property.id == kEdsPropID_ISOSpeed && isAutomaticIso(value)) {
          continue;
        }
        setting.choices.push_back({hexValue(value), canonLabel(property.id, value)});
      }
      if (property.id == kEdsPropID_ISOSpeed) {
        const bool automatic = isAutomaticIso(current);
        if (!automatic) {
          manualIsoValue_ = static_cast<EdsUInt32>(current);
        } else if (std::ranges::none_of(setting.choices, [this](const CameraSettingChoice& choice) {
                     return choice.value.toUInt(nullptr, 16) == manualIsoValue_;
                   }) &&
                   !setting.choices.empty()) {
          manualIsoValue_ = setting.choices.front().value.toUInt(nullptr, 16);
        }
        std::vector<CameraSettingChoice> modes;
        const bool supportsAutomatic = std::any_of(
            descriptor.propDesc, descriptor.propDesc + descriptor.numElements, isAutomaticIso);
        if (supportsAutomatic) {
          modes.push_back({"auto", "Auto"});
        }
        if (!setting.choices.empty()) {
          modes.push_back({"manual", "Manual"});
        }
        CameraSetting mode{"isoMode", "ISO mode", automatic ? "auto" : "manual",
                           std::move(modes)};
        mode.group = property.group;
        result.push_back(std::move(mode));
        if (setting.choices.empty()) {
          continue;
        }
        setting.value = hexValue(static_cast<EdsInt32>(manualIsoValue_));
        setting.enabled = !automatic;
      }
      if (property.id == kEdsPropID_ColorTemperature) {
        EdsInt32 whiteBalance = kEdsWhiteBalance_Auto;
        setting.enabled =
            EdsGetPropertyData(camera_, kEdsPropID_WhiteBalance, 0, sizeof(whiteBalance),
                               &whiteBalance) == EDS_ERR_OK &&
            whiteBalance == kEdsWhiteBalance_ColorTemp;
      }
      if (property.id == kEdsPropID_Tv) {
        std::ranges::sort(setting.choices, [](const CameraSettingChoice& left,
                                              const CameraSettingChoice& right) {
          return left.value.toUInt(nullptr, 16) > right.value.toUInt(nullptr, 16);
        });
      }
      result.push_back(std::move(setting));
    }

    CameraSetting externalFlash{
        "externalFlash", "External flash", externalFlash_ ? "1" : "0", {{"0", "Off"}, {"1", "On"}}};
    externalFlash.type = CameraSettingType::Toggle;
    externalFlash.group = "Capture";
    result.push_back(std::move(externalFlash));

    CameraSetting previewOffset{"exposurePreviewOffset",
                                "Exposure preview offset",
                                QString::number(exposurePreviewOffset_, 'f', 1),
                                {}};
    previewOffset.type = CameraSettingType::DecimalRange;
    previewOffset.minimum = -3.0;
    previewOffset.maximum = 3.0;
    previewOffset.step = 0.1;
    previewOffset.decimals = 1;
    previewOffset.suffix = " EV";
    previewOffset.group = "Live preview";
    result.push_back(std::move(previewOffset));

    EdsUInt32 depthOfField = depthOfFieldPreview_ ? 1 : 0;
    if (EdsGetPropertyData(camera_, kEdsPropID_Evf_DepthOfFieldPreview, 0, sizeof(depthOfField),
                           &depthOfField) == EDS_ERR_OK) {
      depthOfFieldPreview_ = depthOfField != 0;
    }
    CameraSetting depthPreview{"depthOfFieldPreview",
                               "Depth of field preview",
                               depthOfFieldPreview_ ? "1" : "0",
                               {{"0", "Off"}, {"1", "On"}}};
    depthPreview.type = CameraSettingType::Toggle;
    depthPreview.group = "Live preview";
    result.push_back(std::move(depthPreview));

    CameraSetting simulation{
        "lvSimulation", "LV simulation", lvSimulation_ ? "1" : "0", {{"0", "Off"}, {"1", "On"}}};
    simulation.type = CameraSettingType::Toggle;
    simulation.group = "Live preview";
    result.push_back(std::move(simulation));
    return result;
  }

  void start() override {
    if (!sessionOpen_) {
      emit errorOccurred(initializationError_);
      return;
    }
    EdsUInt32 output = 0;
    EdsError error =
        EdsGetPropertyData(camera_, kEdsPropID_Evf_OutputDevice, 0, sizeof(output), &output);
    output |= kEdsEvfOutputDevice_PC;
    if (error == EDS_ERR_OK) {
      error = EdsSetPropertyData(camera_, kEdsPropID_Evf_OutputDevice, 0, sizeof(output), &output);
    }
    if (error != EDS_ERR_OK) {
      emit errorOccurred("Could not start Canon live view: " + errorText(error));
      return;
    }
    liveViewStarted_ = true;
    pollTimer_.start();
    emit settingsChanged();
  }

  void stop() override {
    pollTimer_.stop();
    if (!liveViewStarted_ || camera_ == nullptr) {
      return;
    }
    EdsUInt32 output = 0;
    if (EdsGetPropertyData(camera_, kEdsPropID_Evf_OutputDevice, 0, sizeof(output), &output) ==
        EDS_ERR_OK) {
      output &= ~static_cast<EdsUInt32>(kEdsEvfOutputDevice_PC);
      EdsSetPropertyData(camera_, kEdsPropID_Evf_OutputDevice, 0, sizeof(output), &output);
    }
    liveViewStarted_ = false;
  }

  void capture(const QString& destinationBase) override {
    if (!sessionOpen_) {
      emit errorOccurred(initializationError_);
      return;
    }
    if (depthOfFieldPreview_) {
      const EdsError previewError = setDepthOfFieldPreview(false);
      if (previewError != EDS_ERR_OK) {
        emit errorOccurred("Could not release Canon depth of field preview for capture: " +
                           errorText(previewError));
        return;
      }
      restoreDepthOfFieldPreviewAfterCapture_ = true;
      emit settingsChanged();
    }
    pendingCaptureBase_ = destinationBase;
    pendingCaptureWarning_.clear();
    EdsError error = EDS_ERR_OK;
    if (restoreDepthOfFieldPreviewAfterCapture_) {
      error = EdsSendCommand(camera_, kEdsCameraCommand_PressShutterButton,
                             kEdsCameraCommand_ShutterButton_Completely_NonAF);
      const EdsError releaseError = EdsSendCommand(
          camera_, kEdsCameraCommand_PressShutterButton, kEdsCameraCommand_ShutterButton_OFF);
      if (error == EDS_ERR_OK && releaseError != EDS_ERR_OK) {
        pendingCaptureWarning_ =
            "Canon could not release the shutter button: " + errorText(releaseError);
      }
    } else {
      error = EdsSendCommand(camera_, kEdsCameraCommand_TakePicture, 0);
    }
    if (error != EDS_ERR_OK) {
      pendingCaptureBase_.clear();
      pendingCaptureWarning_.clear();
      const EdsError previewError = restoreDepthOfFieldPreview();
      QString message = "Canon capture failed: " + errorText(error);
      if (previewError != EDS_ERR_OK) {
        message += ". Could not restore depth of field preview: " + errorText(previewError);
      }
      emit errorOccurred(message);
    }
  }

  void setSetting(const QString& id, const QString& value) override {
    if (id == "externalFlash" || id == "lvSimulation") {
      const bool enabled = value == "1";
      if (id == "externalFlash") {
        externalFlash_ = enabled;
      } else {
        lvSimulation_ = enabled;
      }
      emit settingsChanged();
      return;
    }
    if (id == "exposurePreviewOffset") {
      bool converted = false;
      const double offset = value.toDouble(&converted);
      if (!converted || offset < -3.0 || offset > 3.0) {
        emit errorOccurred("The exposure preview offset is invalid.");
        return;
      }
      exposurePreviewOffset_ = offset;
      emit settingsChanged();
      return;
    }
    if (id == "depthOfFieldPreview") {
      const EdsError error = setDepthOfFieldPreview(value == "1");
      if (error != EDS_ERR_OK) {
        emit errorOccurred("Canon rejected depth of field preview: " + errorText(error));
        return;
      }
      emit settingsChanged();
      return;
    }
    if (id == "isoMode") {
      const bool automatic = value == "auto";
      if (!automatic && value != "manual") {
        emit errorOccurred("The ISO mode is invalid.");
        return;
      }
      EdsPropertyDesc descriptor{};
      if (EdsGetPropertyDesc(camera_, kEdsPropID_ISOSpeed, &descriptor) != EDS_ERR_OK ||
          descriptor.numElements <= 0) {
        emit errorOccurred("Could not read the available Canon ISO settings.");
        return;
      }
      EdsUInt32 nativeValue = 0;
      bool found = false;
      const auto match = std::find_if(
          descriptor.propDesc, descriptor.propDesc + descriptor.numElements,
          [this, automatic](EdsInt32 candidate) {
            return automatic ? isAutomaticIso(candidate)
                             : static_cast<EdsUInt32>(candidate) == manualIsoValue_;
          });
      if (match != descriptor.propDesc + descriptor.numElements) {
        nativeValue = static_cast<EdsUInt32>(*match);
        found = true;
      } else if (!automatic) {
        const auto firstManual = std::find_if(
            descriptor.propDesc, descriptor.propDesc + descriptor.numElements,
            [](EdsInt32 candidate) { return !isAutomaticIso(candidate); });
        if (firstManual != descriptor.propDesc + descriptor.numElements) {
          nativeValue = static_cast<EdsUInt32>(*firstManual);
          found = true;
        }
      }
      if (!found ||
          (automatic && !isAutomaticIso(static_cast<EdsInt32>(nativeValue))) ||
          (!automatic && isAutomaticIso(static_cast<EdsInt32>(nativeValue)))) {
        emit errorOccurred("The selected ISO mode is not available on this camera.");
        return;
      }
      const EdsError error = EdsSetPropertyData(camera_, kEdsPropID_ISOSpeed, 0,
                                                sizeof(nativeValue), &nativeValue);
      if (error != EDS_ERR_OK) {
        emit errorOccurred("Canon rejected the ISO mode: " + errorText(error));
        return;
      }
      if (!automatic) {
        manualIsoValue_ = nativeValue;
      }
      emit settingsChanged();
      return;
    }
    const auto property =
        std::find_if(kCanonProperties.begin(), kCanonProperties.end(),
                     [&id](const CanonProperty& candidate) { return id == candidate.key; });
    bool converted = false;
    const EdsUInt32 nativeValue = value.toUInt(&converted, 16);
    if (property == kCanonProperties.end() || !converted) {
      emit errorOccurred("The Canon camera setting is invalid.");
      return;
    }
    const EdsError error =
        EdsSetPropertyData(camera_, property->id, 0, sizeof(nativeValue), &nativeValue);
    if (error != EDS_ERR_OK) {
      emit errorOccurred("Canon rejected the setting: " + errorText(error));
      return;
    }
    if (id == "iso") {
      manualIsoValue_ = nativeValue;
    }
    emit settingsChanged();
  }

 private:
  EdsError setDepthOfFieldPreview(bool enabled) {
    const EdsUInt32 value = enabled ? kEdsEvfDepthOfFieldPreview_ON
                                    : kEdsEvfDepthOfFieldPreview_OFF;
    const EdsError error = EdsSetPropertyData(
        camera_, kEdsPropID_Evf_DepthOfFieldPreview, 0, sizeof(value), &value);
    if (error == EDS_ERR_OK) {
      depthOfFieldPreview_ = enabled;
    }
    return error;
  }

  EdsError restoreDepthOfFieldPreview() {
    if (!restoreDepthOfFieldPreviewAfterCapture_) {
      return EDS_ERR_OK;
    }
    restoreDepthOfFieldPreviewAfterCapture_ = false;
    const EdsError error = setDepthOfFieldPreview(true);
    emit settingsChanged();
    return error;
  }

  static EdsError EDSCALLBACK objectEvent(EdsObjectEvent event, EdsBaseRef ref, EdsVoid* context) {
    auto* self = static_cast<CanonSession*>(context);
    if (event == kEdsObjectEvent_DirItemRequestTransfer && ref != nullptr) {
      self->download(static_cast<EdsDirectoryItemRef>(ref));
    }
    if (ref != nullptr) {
      EdsRelease(ref);
    }
    return EDS_ERR_OK;
  }

  static EdsError EDSCALLBACK propertyEvent(EdsPropertyEvent, EdsPropertyID, EdsUInt32,
                                            EdsVoid* context) {
    emit static_cast<CanonSession*>(context)->settingsChanged();
    return EDS_ERR_OK;
  }

  static EdsError EDSCALLBACK stateEvent(EdsStateEvent event, EdsUInt32, EdsVoid* context) {
    if (event == kEdsStateEvent_Shutdown) {
      emit static_cast<CanonSession*>(context)->errorOccurred("The Canon camera was disconnected.");
    }
    return EDS_ERR_OK;
  }

  void initialize() {
    EdsError error = EdsInitializeSDK();
    if (error != EDS_ERR_OK) {
      initializationError_ = "Could not initialize Canon EDSDK: " + errorText(error);
      return;
    }
    sdkInitialized_ = true;

    EdsCameraListRef list = nullptr;
    EdsUInt32 count = 0;
    error = EdsGetCameraList(&list);
    if (error == EDS_ERR_OK) {
      error = EdsGetChildCount(list, &count);
    }
    for (EdsUInt32 index = 0; error == EDS_ERR_OK && index < count; ++index) {
      EdsCameraRef camera = nullptr;
      error = EdsGetChildAtIndex(list, static_cast<EdsInt32>(index),
                                 reinterpret_cast<EdsBaseRef*>(&camera));
      EdsDeviceInfo info{};
      if (error == EDS_ERR_OK) {
        error = EdsGetDeviceInfo(camera, &info);
      }
      if (error == EDS_ERR_OK && QString::fromUtf8(info.szPortName) == device_.id) {
        camera_ = camera;
        break;
      }
      if (camera != nullptr) {
        EdsRelease(camera);
      }
    }
    if (list != nullptr) {
      EdsRelease(list);
    }
    if (camera_ == nullptr) {
      initializationError_ = "The selected Canon camera is no longer available.";
      return;
    }

    error = EdsOpenSession(camera_);
    if (error == EDS_ERR_OK) {
      sessionOpen_ = true;
      EdsSetObjectEventHandler(camera_, kEdsObjectEvent_All, objectEvent, this);
      EdsSetPropertyEventHandler(camera_, kEdsPropertyEvent_All, propertyEvent, this);
      EdsSetCameraStateEventHandler(camera_, kEdsStateEvent_All, stateEvent, this);
      EdsUInt32 saveTo = kEdsSaveTo_Host;
      error = EdsSetPropertyData(camera_, kEdsPropID_SaveTo, 0, sizeof(saveTo), &saveTo);
      EdsCapacity capacity{0x7fffffff, 0x1000, 1};
      if (error == EDS_ERR_OK) {
        error = EdsSetCapacity(camera_, capacity);
      }
    }
    if (error != EDS_ERR_OK) {
      initializationError_ = "Could not open the Canon camera: " + errorText(error);
      sessionOpen_ = false;
    }
  }

  void poll() {
    EdsGetEvent();
    if (!liveViewStarted_) {
      return;
    }
    EdsStreamRef stream = nullptr;
    EdsEvfImageRef image = nullptr;
    EdsError error = EdsCreateMemoryStream(0, &stream);
    if (error == EDS_ERR_OK) {
      error = EdsCreateEvfImageRef(stream, &image);
    }
    if (error == EDS_ERR_OK) {
      error = EdsDownloadEvfImage(camera_, image);
    }
    if (error == EDS_ERR_OK) {
      EdsVoid* bytes = nullptr;
      EdsUInt64 length = 0;
      if (EdsGetPointer(stream, &bytes) == EDS_ERR_OK &&
          EdsGetLength(stream, &length) == EDS_ERR_OK) {
        QImage frame =
            QImage::fromData(static_cast<const uchar*>(bytes), static_cast<int>(length), "JPEG");
        if (!frame.isNull()) {
          if (lvSimulation_ && exposurePreviewOffset_ != 0.0) {
            frame.convertTo(QImage::Format_RGB32);
            const double multiplier = std::exp2(exposurePreviewOffset_);
            for (int y = 0; y < frame.height(); ++y) {
              auto* pixels = reinterpret_cast<QRgb*>(frame.scanLine(y));
              for (int x = 0; x < frame.width(); ++x) {
                const QRgb pixel = pixels[x];
                pixels[x] = qRgb(std::clamp(static_cast<int>(qRed(pixel) * multiplier), 0, 255),
                                 std::clamp(static_cast<int>(qGreen(pixel) * multiplier), 0, 255),
                                 std::clamp(static_cast<int>(qBlue(pixel) * multiplier), 0, 255));
              }
            }
          }
          emit previewFrame(frame);
        }
      }
    }
    if (image != nullptr) {
      EdsRelease(image);
    }
    if (stream != nullptr) {
      EdsRelease(stream);
    }
  }

  void download(EdsDirectoryItemRef item) {
    if (pendingCaptureBase_.isEmpty()) {
      EdsDownloadCancel(item);
      return;
    }
    EdsDirectoryItemInfo info{};
    EdsError error = EdsGetDirectoryItemInfo(item, &info);
    const QString suffix = QFileInfo(QString::fromUtf8(info.szFileName)).suffix();
    const QString filePath =
        pendingCaptureBase_ + '.' + (suffix.isEmpty() ? QString("jpg") : suffix.toLower());
    EdsStreamRef stream = nullptr;
    if (error == EDS_ERR_OK) {
      const QByteArray encodedPath = QFile::encodeName(filePath);
      error = EdsCreateFileStream(encodedPath.constData(), kEdsFileCreateDisposition_CreateAlways,
                                  kEdsAccess_ReadWrite, &stream);
    }
    if (error == EDS_ERR_OK) {
      error = EdsDownload(item, info.size, stream);
    }
    if (error == EDS_ERR_OK) {
      error = EdsDownloadComplete(item);
    }
    if (stream != nullptr) {
      EdsRelease(stream);
    }
    pendingCaptureBase_.clear();
    const QString warning = std::move(pendingCaptureWarning_);
    pendingCaptureWarning_.clear();
    const EdsError previewError = restoreDepthOfFieldPreview();
    if (error == EDS_ERR_OK) {
      emit captureCompleted(filePath);
    } else {
      QFile::remove(filePath);
      emit errorOccurred("Could not download the Canon image: " + errorText(error));
    }
    if (previewError != EDS_ERR_OK) {
      emit errorOccurred("Could not restore Canon depth of field preview: " +
                         errorText(previewError));
    }
    if (!warning.isEmpty()) {
      emit errorOccurred(warning);
    }
  }

  CameraDevice device_;
  EdsCameraRef camera_ = nullptr;
  QTimer pollTimer_;
  QString initializationError_;
  QString pendingCaptureBase_;
  QString pendingCaptureWarning_;
  bool sdkInitialized_ = false;
  bool sessionOpen_ = false;
  bool liveViewStarted_ = false;
  mutable bool depthOfFieldPreview_ = false;
  bool restoreDepthOfFieldPreviewAfterCapture_ = false;
  bool externalFlash_ = false;
  bool lvSimulation_ = true;
  double exposurePreviewOffset_ = 0.0;
  mutable EdsUInt32 manualIsoValue_ = 0x48;
};

}  // namespace

std::vector<CameraDevice> availableCanonCameras() {
  std::vector<CameraDevice> devices;
  if (EdsInitializeSDK() != EDS_ERR_OK) {
    return devices;
  }
  EdsCameraListRef list = nullptr;
  EdsUInt32 count = 0;
  if (EdsGetCameraList(&list) == EDS_ERR_OK && EdsGetChildCount(list, &count) == EDS_ERR_OK) {
    for (EdsUInt32 index = 0; index < count; ++index) {
      EdsCameraRef camera = nullptr;
      if (EdsGetChildAtIndex(list, static_cast<EdsInt32>(index),
                             reinterpret_cast<EdsBaseRef*>(&camera)) != EDS_ERR_OK) {
        continue;
      }
      EdsDeviceInfo info{};
      if (EdsGetDeviceInfo(camera, &info) == EDS_ERR_OK) {
        devices.push_back({"canon", QString::fromUtf8(info.szPortName),
                           QString::fromUtf8(info.szDeviceDescription)});
      }
      EdsRelease(camera);
    }
  }
  if (list != nullptr) {
    EdsRelease(list);
  }
  EdsTerminateSDK();
  return devices;
}

std::unique_ptr<CameraSession> openCanonCamera(const CameraDevice& device, QObject* parent) {
  return std::make_unique<CanonSession>(device, parent);
}
