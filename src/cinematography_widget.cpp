#include "cinematography_widget.h"

#include "layout_constants.h"

#include <QCheckBox>
#include <QComboBox>
#include <QDateTime>
#include <QDoubleSpinBox>
#include <QFile>
#include <QFileInfo>
#include <QFormLayout>
#include <QFrame>
#include <QFutureWatcher>
#include <QHBoxLayout>
#include <QImageReader>
#include <QLabel>
#include <QListWidget>
#include <QMessageBox>
#include <QMouseEvent>
#include <QPixmap>
#include <QPushButton>
#include <QScrollArea>
#include <QSignalBlocker>
#include <QSizePolicy>
#include <QSlider>
#include <QStyle>
#include <QTimer>
#include <QVBoxLayout>
#include <QtConcurrentRun>
#include <algorithm>
#include <cmath>

namespace {

QImage loadScaledImage(const QString& filePath, const QSize& targetSize) {
  QImageReader reader(filePath);
  const QSize sourceSize = reader.size();
  if (sourceSize.isValid() && targetSize.isValid() &&
      (sourceSize.width() > targetSize.width() || sourceSize.height() > targetSize.height())) {
    reader.setScaledSize(sourceSize.scaled(targetSize, Qt::KeepAspectRatio));
  }
  return reader.read();
}

class DoubleClickResetSlider final : public QSlider {
 public:
  DoubleClickResetSlider(Qt::Orientation orientation, int resetValue, QWidget* parent)
      : QSlider(orientation, parent), resetValue_(resetValue) {}

 protected:
  void mouseDoubleClickEvent(QMouseEvent* event) override {
    setValue(resetValue_);
    event->accept();
  }

 private:
  int resetValue_;
};

QString choiceLabel(const CameraSetting& setting, const QString& value) {
  if (setting.type != CameraSettingType::Choice &&
      setting.type != CameraSettingType::SteppedChoice &&
      setting.type != CameraSettingType::Toggle) {
    return value + setting.suffix;
  }
  const auto choice = std::find_if(
      setting.choices.begin(), setting.choices.end(),
      [&value](const CameraSettingChoice& candidate) { return candidate.value == value; });
  return choice == setting.choices.end() ? value : choice->label;
}

bool sameSettingControls(const std::vector<CameraSetting>& left,
                         const std::vector<CameraSetting>& right) {
  if (left.size() != right.size()) {
    return false;
  }
  for (std::size_t index = 0; index < left.size(); ++index) {
    const CameraSetting& a = left[index];
    const CameraSetting& b = right[index];
    if (a.id != b.id || a.label != b.label || a.type != b.type ||
        a.minimum != b.minimum || a.maximum != b.maximum || a.step != b.step ||
        a.decimals != b.decimals || a.suffix != b.suffix || a.group != b.group ||
        a.enabled != b.enabled || a.choices.size() != b.choices.size()) {
      return false;
    }
    for (std::size_t choiceIndex = 0; choiceIndex < a.choices.size(); ++choiceIndex) {
      if (a.choices[choiceIndex].value != b.choices[choiceIndex].value ||
          a.choices[choiceIndex].label != b.choices[choiceIndex].label) {
        return false;
      }
    }
  }
  return true;
}

bool sameSettings(const std::vector<CameraSetting>& left,
                  const std::vector<CameraSetting>& right) {
  if (!sameSettingControls(left, right)) {
    return false;
  }
  for (std::size_t index = 0; index < left.size(); ++index) {
    if (left[index].value != right[index].value) {
      return false;
    }
  }
  return true;
}

}  // namespace

CinematographyWidget::CinematographyWidget(QWidget* parent) : QWidget(parent) {
  auto* root = new QVBoxLayout(this);
  root->setContentsMargins(20, 18, 20, 18);
  root->setSpacing(14);

  auto* header = new QHBoxLayout;
  shotLabel_ = new QLabel("Select a shot in Direct", this);
  shotLabel_->setObjectName("cinematographyShotLabel");
  header->addWidget(shotLabel_);
  header->addStretch();
  connectionStatusLabel_ = new QLabel("Disconnected  ·  No camera selected", this);
  connectionStatusLabel_->setObjectName("cameraConnectionStatus");
  connectionStatusLabel_->setProperty("connected", false);
  header->addWidget(connectionStatusLabel_);
  header->addSpacing(10);
  liveButton_ = new QPushButton("Return to live", this);
  liveButton_->setEnabled(false);
  header->addWidget(liveButton_);
  root->addLayout(header);

  auto* workspace = new QHBoxLayout;
  workspace->setSpacing(16);

  auto* imageColumn = new QVBoxLayout;
  previewLabel_ = new QLabel("Connect a camera to begin", this);
  previewLabel_->setObjectName("cameraPreview");
  previewLabel_->setAlignment(Qt::AlignCenter);
  previewLabel_->setMinimumSize(480, 270);
  previewLabel_->setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Expanding);
  imageColumn->addWidget(previewLabel_, 4);

  operationStatusLabel_ = new QLabel(this);
  operationStatusLabel_->setObjectName("cameraOperationStatus");
  operationStatusLabel_->setVisible(false);
  imageColumn->addWidget(operationStatusLabel_);

  errorLabel_ = new QLabel(this);
  errorLabel_->setObjectName("cameraErrorStatus");
  errorLabel_->setWordWrap(true);
  errorLabel_->setVisible(false);
  imageColumn->addWidget(errorLabel_);

  auto* galleryHeader = new QHBoxLayout;
  auto* galleryTitle = new QLabel("Test images", this);
  QFont sectionFont = galleryTitle->font();
  sectionFont.setBold(true);
  galleryTitle->setFont(sectionFont);
  galleryStatusLabel_ = new QLabel("No shot selected", this);
  galleryStatusLabel_->setObjectName("galleryStatus");
  galleryHeader->addWidget(galleryTitle);
  galleryHeader->addStretch();
  galleryHeader->addWidget(galleryStatusLabel_);
  imageColumn->addLayout(galleryHeader);

  gallery_ = new QListWidget(this);
  gallery_->setObjectName("testShotGallery");
  gallery_->setViewMode(QListView::IconMode);
  gallery_->setFlow(QListView::LeftToRight);
  gallery_->setWrapping(false);
  gallery_->setResizeMode(QListView::Adjust);
  gallery_->setIconSize(QSize(128, 82));
  gallery_->setSpacing(8);
  gallery_->setMaximumHeight(132);
  imageColumn->addWidget(gallery_);
  workspace->addLayout(imageColumn, 4);

  auto* sidebar = new QFrame(this);
  sidebar->setObjectName("cameraSidebar");
  sidebar->setFixedWidth(UiConstants::kSidebarWidth);
  auto* sidebarLayout = new QVBoxLayout(sidebar);
  sidebarLayout->setContentsMargins(16, 16, 16, 16);
  sidebarLayout->setSpacing(12);

  auto* cameraTitle = new QLabel("Camera", sidebar);
  cameraTitle->setFont(sectionFont);
  sidebarLayout->addWidget(cameraTitle);
  cameraCombo_ = new QComboBox(sidebar);
  cameraCombo_->setObjectName("cameraSelector");
  auto* cameraSelection = new QHBoxLayout;
  cameraSelection->addWidget(cameraCombo_, 1);
  cameraRefreshButton_ = new QPushButton("Refresh", sidebar);
  cameraSelection->addWidget(cameraRefreshButton_);
  sidebarLayout->addLayout(cameraSelection);

  auto* settingsTitle = new QLabel("Camera settings", sidebar);
  settingsTitle->setFont(sectionFont);
  sidebarLayout->addWidget(settingsTitle);
  auto* settingsWidget = new QWidget(sidebar);
  settingsLayout_ = new QFormLayout(settingsWidget);
  settingsLayout_->setContentsMargins(0, 0, 0, 0);
  settingsLayout_->setFieldGrowthPolicy(QFormLayout::AllNonFixedFieldsGrow);
  settingsLayout_->setRowWrapPolicy(QFormLayout::WrapLongRows);
  auto* settingsScroll = new QScrollArea(sidebar);
  settingsScroll->setWidgetResizable(true);
  settingsScroll->setFrameShape(QFrame::NoFrame);
  settingsScroll->setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
  settingsScroll->setWidget(settingsWidget);
  sidebarLayout->addWidget(settingsScroll, 1);

  captureButton_ = new QPushButton("Capture test image", sidebar);
  captureButton_->setObjectName("captureTestShotButton");
  captureButton_->setEnabled(false);
  sidebarLayout->addWidget(captureButton_);

  auto* metadataDivider = new QFrame(sidebar);
  metadataDivider->setFrameShape(QFrame::HLine);
  sidebarLayout->addWidget(metadataDivider);
  auto* metadataTitle = new QLabel("Selected image", sidebar);
  metadataTitle->setFont(sectionFont);
  sidebarLayout->addWidget(metadataTitle);
  metadataLabel_ = new QLabel("Select a test image to view its metadata.", sidebar);
  metadataLabel_->setWordWrap(true);
  metadataLabel_->setTextInteractionFlags(Qt::TextSelectableByMouse);
  sidebarLayout->addWidget(metadataLabel_);
  auto* imageActions = new QHBoxLayout;
  restoreButton_ = new QPushButton("Restore settings", sidebar);
  deleteButton_ = new QPushButton("Delete", sidebar);
  restoreButton_->setEnabled(false);
  deleteButton_->setEnabled(false);
  imageActions->addWidget(restoreButton_);
  imageActions->addWidget(deleteButton_);
  sidebarLayout->addLayout(imageActions);
  workspace->addWidget(sidebar);
  root->addLayout(workspace, 1);

  connect(cameraCombo_, &QComboBox::currentIndexChanged, this,
          [this](int index) { selectCamera(index); });
  connect(cameraRefreshButton_, &QPushButton::clicked, this,
          [this] { refreshCameras(); });
  connect(captureButton_, &QPushButton::clicked, this, [this] { capture(); });
  connect(liveButton_, &QPushButton::clicked, this, [this] { showLiveView(); });
  connect(gallery_, &QListWidget::currentRowChanged, this, [this](int row) { showTestShot(row); });
  connect(restoreButton_, &QPushButton::clicked, this,
          [this] { restoreSelectedTestShotSettings(); });
  connect(deleteButton_, &QPushButton::clicked, this, [this] { deleteSelectedTestShot(); });

  refreshCameras();
}

void CinematographyWidget::setShot(Project* project, int sceneIndex, int shotIndex) {
  project_ = project;
  sceneIndex_ = sceneIndex;
  shotIndex_ = shotIndex;
  setOperationStatus({});
  clearError();
  if (project_ == nullptr || sceneIndex_ < 0 || shotIndex_ < 0) {
    shotLabel_->setText("Select a shot in Direct");
    captureButton_->setEnabled(false);
    refreshGallery();
    return;
  }
  shotLabel_->setText(QString("Scene %1 / Shot %2: %3")
                          .arg(sceneIndex_ + 1, 4, 10, QLatin1Char('0'))
                          .arg(shotIndex_ + 1, 4, 10, QLatin1Char('0'))
                          .arg(project_->shots(sceneIndex_)[shotIndex_]));
  captureButton_->setEnabled(camera_ != nullptr && !externalCaptureActive_ &&
                             !pendingCapture_.has_value());
  refreshGallery();
  restoreShotSettings();
}

CameraSession* CinematographyWidget::cameraSession() const {
  return camera_.get();
}

void CinematographyWidget::setExternalCaptureActive(bool active) {
  externalCaptureActive_ = active;
  cameraCombo_->setEnabled(!active);
  cameraRefreshButton_->setEnabled(!active);
  captureButton_->setEnabled(!active && project_ != nullptr && camera_ != nullptr &&
                             shotIndex_ >= 0 && !pendingCapture_.has_value());
}

void CinematographyWidget::refreshCameras() {
  setOperationStatus("Refreshing cameras...");
  clearError();
  const QString previousBackend = camera_ ? camera_->backend() : QString{};
  const QString previousId = camera_ ? camera_->deviceId() : QString{};
  if (pendingCapture_.has_value()) {
    pendingCapture_.reset();
    emit captureStateChanged(false);
  }
  if (camera_) {
    emit cameraChanged(nullptr);
    camera_->stop();
    camera_.reset();
  }
  devices_ = availableCameras();
  QSignalBlocker blocker(cameraCombo_);
  cameraCombo_->clear();
  cameraCombo_->addItem("No camera");
  int selected = 0;
  for (int index = 0; index < static_cast<int>(devices_.size()); ++index) {
    const CameraDevice& device = devices_[index];
    const QString type = device.backend == "canon" ? "Canon DSLR" : "Webcam";
    cameraCombo_->addItem(device.displayName + "  ·  " + type);
    if (device.backend == previousBackend && device.id == previousId) {
      selected = index + 1;
    }
  }
  cameraCombo_->setCurrentIndex(selected);
  selectCamera(selected);
  setOperationStatus({});
}

void CinematographyWidget::selectCamera(int index) {
  setOperationStatus({});
  clearError();
  if (camera_) {
    emit cameraChanged(nullptr);
    camera_->stop();
    camera_.reset();
  }
  settings_.clear();
  if (index <= 0 || index > static_cast<int>(devices_.size())) {
    rebuildSettings();
    captureButton_->setEnabled(false);
    liveButton_->setEnabled(false);
    setLivePreview(false);
    setConnectionStatus(devices_.empty() ? "No cameras found" : "No camera selected", false);
    updatePreview({}, "Connect or select a camera");
    return;
  }

  camera_ = openCamera(devices_[index - 1], this);
  if (!camera_) {
    setConnectionStatus(devices_[index - 1].displayName + "  ·  Connection failed", false);
    showError("Brick could not open the selected camera.");
    return;
  }
  connect(camera_.get(), &CameraSession::previewFrame, this, [this](const QImage& image) {
    if (gallery_->currentRow() < 0) {
      updatePreview(image);
    }
  });
  connect(camera_.get(), &CameraSession::settingsChanged, this,
          [this] { QTimer::singleShot(0, this, [this] { refreshSettings(); }); });
  connect(camera_.get(), &CameraSession::captureCompleted, this,
          [this](const QString& path) { importCapture(path); });
  connect(camera_.get(), &CameraSession::errorOccurred, this,
           [this](const QString& message) {
             if (pendingCapture_.has_value()) {
               pendingCapture_.reset();
               emit captureStateChanged(false);
             }
             showError(message);
           });
  camera_->start();
  if (!camera_->isReady()) {
    camera_.reset();
    rebuildSettings();
    captureButton_->setEnabled(false);
    liveButton_->setEnabled(false);
    setConnectionStatus(devices_[index - 1].displayName + "  ·  Connection failed", false);
    return;
  }
  emit cameraChanged(camera_.get());
  setConnectionStatus(camera_->displayName(), true);
  captureButton_->setEnabled(project_ != nullptr && shotIndex_ >= 0 &&
                             !externalCaptureActive_);
  rebuildSettings();
  saveCameraSettings();
  showLiveView();
}

void CinematographyWidget::refreshSettings() {
  const auto updated = camera_ ? camera_->settings() : std::vector<CameraSetting>{};
  if (sameSettings(settings_, updated)) {
    return;
  }
  rebuildSettings();
}

void CinematographyWidget::rebuildSettings() {
  while (settingsLayout_->rowCount() > 0) {
    settingsLayout_->removeRow(0);
  }
  settings_ = camera_ ? camera_->settings() : std::vector<CameraSetting>{};
  if (settings_.empty()) {
    settingsLayout_->addRow(
        new QLabel(camera_ ? "No adjustable settings" : "Select a camera", this));
    return;
  }
  QString currentGroup;
  for (const CameraSetting& setting : settings_) {
    if (!setting.group.isEmpty() && setting.group != currentGroup) {
      currentGroup = setting.group;
      auto* heading = new QLabel(currentGroup, this);
      QFont font = heading->font();
      font.setBold(true);
      heading->setFont(font);
      settingsLayout_->addRow(heading);
    }

    if (setting.type == CameraSettingType::Toggle) {
      auto* toggle = new QCheckBox(setting.value == "1" ? "On" : "Off", this);
      toggle->setChecked(setting.value == "1");
      connect(toggle, &QCheckBox::toggled, this, [this, id = setting.id, toggle](bool checked) {
        toggle->setText(checked ? "On" : "Off");
        const QString value = checked ? "1" : "0";
        const auto current =
            std::find_if(settings_.begin(), settings_.end(),
                         [&id](const CameraSetting& candidate) { return candidate.id == id; });
        if (current != settings_.end()) {
          current->value = value;
        }
        if (camera_) {
          applyCameraSetting(id, value);
        }
      });
      settingsLayout_->addRow(setting.label + ':', toggle);
      continue;
    }

    if (setting.type == CameraSettingType::SteppedChoice) {
      auto* control = new QWidget(this);
      auto* layout = new QHBoxLayout(control);
      layout->setContentsMargins(0, 0, 0, 0);
      layout->setSpacing(8);
      auto* slider = new QSlider(Qt::Horizontal, control);
      auto* valueLabel = new QLabel(control);
      slider->setRange(0, static_cast<int>(setting.choices.size()) - 1);
      slider->setTracking(true);
      slider->setTickInterval(1);
      slider->setTickPosition(QSlider::TicksBelow);
      valueLabel->setMinimumWidth(62);
      valueLabel->setAlignment(Qt::AlignRight | Qt::AlignVCenter);
      const auto selected = std::find_if(
          setting.choices.begin(), setting.choices.end(),
          [&setting](const CameraSettingChoice& choice) { return choice.value == setting.value; });
      const int selectedIndex =
          selected == setting.choices.end()
              ? 0
              : static_cast<int>(std::distance(setting.choices.begin(), selected));
      slider->setValue(selectedIndex);
      valueLabel->setText(setting.choices[selectedIndex].label);
      layout->addWidget(slider, 1);
      layout->addWidget(valueLabel);
      control->setEnabled(setting.enabled);
      connect(slider, &QSlider::valueChanged, this,
              [this, id = setting.id, choices = setting.choices, valueLabel](int index) {
                const auto& choice = choices[index];
                valueLabel->setText(choice.label);
                const auto current = std::find_if(
                    settings_.begin(), settings_.end(),
                    [&id](const CameraSetting& candidate) { return candidate.id == id; });
                 if (current != settings_.end()) {
                   current->value = choice.value;
                }
                if (camera_) {
                  applyCameraSetting(id, choice.value);
                }
              });
      settingsLayout_->addRow(setting.label + ':', control);
      continue;
    }

    if (setting.type != CameraSettingType::Choice) {
      auto* control = new QWidget(this);
      auto* layout = new QHBoxLayout(control);
      layout->setContentsMargins(0, 0, 0, 0);
      layout->setSpacing(8);
      const double range = setting.maximum - setting.minimum;
      const int sliderSteps = std::max(1, static_cast<int>(std::round(range / setting.step)));
      const int zeroPosition = range > 0
                                   ? static_cast<int>(std::round(-setting.minimum / range * sliderSteps))
                                   : 0;
      QSlider* slider = setting.id == "exposurePreviewOffset"
                            ? new DoubleClickResetSlider(Qt::Horizontal, zeroPosition, control)
                            : new QSlider(Qt::Horizontal, control);
      auto* spin = new QDoubleSpinBox(control);
      slider->setRange(0, sliderSteps);
      slider->setTracking(true);
      spin->setRange(setting.minimum, setting.maximum);
      spin->setSingleStep(setting.step);
      spin->setDecimals(setting.decimals);
      spin->setSuffix(setting.suffix);
      spin->setKeyboardTracking(false);
      const double value = std::clamp(setting.value.toDouble(), setting.minimum, setting.maximum);
      slider->setValue(range > 0 ? static_cast<int>((value - setting.minimum) / range * sliderSteps)
                                 : 0);
      spin->setValue(value);
      layout->addWidget(slider, 1);
      layout->addWidget(spin);

      const auto applyValue = [this, id = setting.id, spin] {
        if (!camera_) {
          return;
        }
        const auto current =
            std::find_if(settings_.begin(), settings_.end(),
                         [&id](const CameraSetting& item) { return item.id == id; });
        if (current == settings_.end()) {
          return;
        }
        current->value = QString::number(spin->value(), 'f', current->decimals);
        applyCameraSetting(id, current->value);
      };
      connect(slider, &QSlider::valueChanged, this,
              [spin, minimum = setting.minimum, range, sliderSteps, applyValue](int position) {
                QSignalBlocker blocker(spin);
                spin->setValue(minimum + range * position / sliderSteps);
                applyValue();
              });
      connect(spin, &QDoubleSpinBox::valueChanged, this,
              [slider, minimum = setting.minimum, range, sliderSteps](double newValue) {
                QSignalBlocker blocker(slider);
                slider->setValue(
                    range > 0 ? static_cast<int>((newValue - minimum) / range * sliderSteps) : 0);
              });
      connect(spin, &QDoubleSpinBox::editingFinished, this, applyValue);
      settingsLayout_->addRow(setting.label + ':', control);
      continue;
    }
    auto* combo = new QComboBox(this);
    for (const auto& choice : setting.choices) {
      combo->addItem(choice.label, choice.value);
    }
    const int current = combo->findData(setting.value);
    if (current >= 0) {
      combo->setCurrentIndex(current);
    }
    connect(combo, &QComboBox::currentIndexChanged, this, [this, id = setting.id, combo](int) {
      if (camera_) {
        const QString value = combo->currentData().toString();
        const auto current =
            std::find_if(settings_.begin(), settings_.end(),
                         [&id](const CameraSetting& candidate) { return candidate.id == id; });
        if (current != settings_.end()) {
          current->value = value;
        }
        applyCameraSetting(id, value);
      }
    });
    settingsLayout_->addRow(setting.label + ':', combo);
  }
}

void CinematographyWidget::applyCameraSetting(const QString& id, const QString& value) {
  if (camera_ == nullptr) {
    return;
  }
  const auto previous = settings_;
  camera_->setSetting(id, value);
  auto updated = camera_->settings();
  const bool controlsChanged = !sameSettingControls(previous, updated);
  settings_ = std::move(updated);
  if (controlsChanged) {
    QTimer::singleShot(0, this, [this] { rebuildSettings(); });
  }
  saveCameraSettings();
}

void CinematographyWidget::saveCameraSettings() {
  if (project_ == nullptr || camera_ == nullptr || sceneIndex_ < 0 || shotIndex_ < 0) {
    return;
  }
  QString error;
  if (!project_->saveCurrentShotCameraSettings(sceneIndex_, shotIndex_, currentCameraSettings(),
                                               &error)) {
    showError(error);
  }
}

void CinematographyWidget::restoreShotSettings() {
  if (project_ == nullptr || sceneIndex_ < 0 || shotIndex_ < 0) {
    return;
  }
  QString error;
  const auto saved = project_->currentShotCameraSettings(sceneIndex_, shotIndex_, &error);
  if (!saved.has_value() || saved->backend.isEmpty()) {
    return;
  }
  const auto device =
      std::find_if(devices_.begin(), devices_.end(), [&saved](const CameraDevice& candidate) {
        return candidate.backend == saved->backend && candidate.id == saved->deviceId;
      });
  if (device != devices_.end()) {
    const int comboIndex = static_cast<int>(std::distance(devices_.begin(), device)) + 1;
    if (cameraCombo_->currentIndex() != comboIndex) {
      cameraCombo_->setCurrentIndex(comboIndex);
    }
  }
  if (camera_ && camera_->backend() == saved->backend && camera_->deviceId() == saved->deviceId) {
    for (const auto& [id, value] : saved->settings) {
      camera_->setSetting(id, value);
    }
    rebuildSettings();
    saveCameraSettings();
  }
}

void CinematographyWidget::capture() {
  if (!captureDirectory_.isValid()) {
    showError("Brick could not create temporary capture storage.");
    return;
  }
  if (project_ == nullptr || camera_ == nullptr || shotIndex_ < 0 ||
      pendingCapture_.has_value() || externalCaptureActive_) {
    return;
  }
  clearError();
  showLiveView();
  captureButton_->setEnabled(false);
  cameraCombo_->setEnabled(false);
  cameraRefreshButton_->setEnabled(false);
  setOperationStatus("Capturing...");
  QString error;
  const QString shotDirectory = project_->shotDirectory(sceneIndex_, shotIndex_, &error);
  if (shotDirectory.isEmpty()) {
    showError(error);
    return;
  }
  pendingCapture_ = PendingCapture{
      project_,   project_->directory(),  shotDirectory,           sceneIndex_,
      shotIndex_, camera_->displayName(), currentCameraSettings(), currentDisplaySettings()};
  emit captureStateChanged(true);
  const QString base =
      captureDirectory_.filePath("capture-" + QString::number(QDateTime::currentMSecsSinceEpoch()));
  camera_->capture(base);
}

void CinematographyWidget::importCapture(const QString& filePath) {
  if (!pendingCapture_.has_value()) {
    return;
  }
  const PendingCapture capture = std::move(*pendingCapture_);
  pendingCapture_.reset();
  emit captureStateChanged(false);
  captureButton_->setEnabled(true);
  cameraCombo_->setEnabled(!externalCaptureActive_);
  cameraRefreshButton_->setEnabled(!externalCaptureActive_);
  QString error;
  if (project_ != capture.project || project_->directory() != capture.projectDirectory ||
      project_->shotDirectory(capture.sceneIndex, capture.shotIndex, &error) !=
          capture.shotDirectory) {
    showError(
        "The originating shot changed before capture completed. The "
        "image remains at " +
        filePath);
    return;
  }
  const auto imported = capture.project->importTestShot(
      capture.sceneIndex, capture.shotIndex, filePath, QDateTime::currentDateTimeUtc(),
      capture.cameraDisplayName, capture.cameraSettings, capture.cameraSettings.settings,
      capture.displaySettings, &error);
  if (!imported.has_value()) {
    showError(error + " The image remains at " + filePath);
    return;
  }
  QFile::remove(filePath);
  clearError();
  setOperationStatus("Captured " + imported->fileName);
  refreshGallery(imported->fileName);
}

void CinematographyWidget::refreshGallery(const QString& selectedFileName) {
  gallery_->clear();
  testShots_.clear();
  metadataLabel_->setText("Select a test image to view its metadata.");
  restoreButton_->setEnabled(false);
  deleteButton_->setEnabled(false);
  if (project_ == nullptr || sceneIndex_ < 0 || shotIndex_ < 0) {
    galleryStatusLabel_->setText("No shot selected");
    return;
  }
  QString error;
  testShots_ = project_->testShots(sceneIndex_, shotIndex_, &error);
  for (int index = 0; index < static_cast<int>(testShots_.size()); ++index) {
    const TestShot& shot = testShots_[index];
    const QImage thumbnail = loadScaledImage(shot.filePath, {128, 82});
    auto* item =
        new QListWidgetItem(QIcon(QPixmap::fromImage(thumbnail)), shot.fileName, gallery_);
    if (shot.fileName == selectedFileName) {
      gallery_->setCurrentItem(item);
    }
  }
  if (!error.isEmpty()) {
    showError(error);
  } else if (testShots_.empty()) {
    galleryStatusLabel_->setText("No test images for this shot");
  } else {
    galleryStatusLabel_->setText(
        QString("%1 test image%2").arg(testShots_.size()).arg(testShots_.size() == 1 ? "" : "s"));
  }
}

void CinematographyWidget::showTestShot(int row) {
  ++previewLoadId_;
  if (row < 0 || row >= static_cast<int>(testShots_.size())) {
    restoreButton_->setEnabled(false);
    deleteButton_->setEnabled(false);
    return;
  }
  setLivePreview(false);
  const TestShot& shot = testShots_[row];
  updatePreview({}, "Loading image...");
  const quint64 loadId = previewLoadId_;
  auto* watcher = new QFutureWatcher<QImage>(this);
  connect(watcher, &QFutureWatcher<QImage>::finished, this, [this, watcher, loadId] {
    const QImage image = watcher->result();
    watcher->deleteLater();
    if (loadId == previewLoadId_) {
      updatePreview(image, image.isNull() ? "Could not load image" : QString{});
    }
  });
  const QString filePath = shot.filePath;
  const QSize previewSize = previewLabel_->size();
  watcher->setFuture(QtConcurrent::run(
      [filePath, previewSize] { return loadScaledImage(filePath, previewSize); }));

  QStringList metadata = {"<b>" + shot.fileName.toHtmlEscaped() + "</b>",
                          shot.capturedUtc.toLocalTime().toString("yyyy-MM-dd  hh:mm:ss"),
                          shot.cameraDisplayName.toHtmlEscaped()};
  for (const auto& [key, value] : shot.displaySettings) {
    metadata.push_back(key.toHtmlEscaped() + ": " + value.toHtmlEscaped());
  }
  metadataLabel_->setText(metadata.join("<br>"));
  restoreButton_->setEnabled(camera_ != nullptr);
  deleteButton_->setEnabled(true);
}

void CinematographyWidget::showLiveView() {
  ++previewLoadId_;
  setLivePreview(true);
  gallery_->clearSelection();
  gallery_->setCurrentRow(-1);
  metadataLabel_->setText("Live camera settings are shown above.");
  restoreButton_->setEnabled(false);
  deleteButton_->setEnabled(false);
}

void CinematographyWidget::setLivePreview(bool live) {
  liveButton_->setEnabled(!live && camera_ != nullptr);
  previewLabel_->setProperty("live", live);
  previewLabel_->style()->unpolish(previewLabel_);
  previewLabel_->style()->polish(previewLabel_);
}

void CinematographyWidget::deleteSelectedTestShot() {
  const int row = gallery_->currentRow();
  if (project_ == nullptr || row < 0 || row >= static_cast<int>(testShots_.size())) {
    return;
  }
  QMessageBox confirmation(QMessageBox::Warning, "Delete Test Image",
                           "Delete this test image? This cannot be undone.", QMessageBox::Cancel,
                           this);
  auto* deleteAction = confirmation.addButton("Delete", QMessageBox::DestructiveRole);
  confirmation.exec();
  if (confirmation.clickedButton() != deleteAction) {
    return;
  }
  QString error;
  if (!project_->deleteTestShot(sceneIndex_, shotIndex_, testShots_[row].fileName, &error)) {
    showError(error);
    return;
  }
  showLiveView();
  refreshGallery();
}

void CinematographyWidget::restoreSelectedTestShotSettings() {
  const int row = gallery_->currentRow();
  if (camera_ == nullptr || row < 0 || row >= static_cast<int>(testShots_.size())) {
    return;
  }
  const TestShot& shot = testShots_[row];
  if (camera_->backend() != shot.cameraBackend || camera_->deviceId() != shot.cameraDeviceId) {
    const auto device =
        std::find_if(devices_.begin(), devices_.end(), [&shot](const CameraDevice& candidate) {
          return candidate.backend == shot.cameraBackend && candidate.id == shot.cameraDeviceId;
        });
    if (device == devices_.end()) {
      showError("The camera used for this test image is not connected.");
      return;
    }
    cameraCombo_->setCurrentIndex(static_cast<int>(std::distance(devices_.begin(), device)) + 1);
  }
  for (const auto& [id, value] : shot.capturedSettings) {
    camera_->setSetting(id, value);
  }
  rebuildSettings();
  saveCameraSettings();
  clearError();
  setOperationStatus("Restored settings from " + shot.fileName);
}

ShotCameraSettings CinematographyWidget::currentCameraSettings() const {
  ShotCameraSettings result;
  if (camera_ == nullptr) {
    return result;
  }
  result.backend = camera_->backend();
  result.deviceId = camera_->deviceId();
  for (const CameraSetting& setting : settings_) {
    result.settings.insert_or_assign(setting.id, setting.value);
  }
  return result;
}

std::map<QString, QString> CinematographyWidget::currentDisplaySettings() const {
  std::map<QString, QString> result;
  if (camera_ == nullptr) {
    return result;
  }
  for (const CameraSetting& setting : settings_) {
    result.insert_or_assign(setting.label, choiceLabel(setting, setting.value));
  }
  return result;
}

void CinematographyWidget::setConnectionStatus(const QString& text, bool connected) {
  connectionStatusLabel_->setText((connected ? "Connected  ·  " : "Disconnected  ·  ") + text);
  connectionStatusLabel_->setProperty("connected", connected);
  connectionStatusLabel_->style()->unpolish(connectionStatusLabel_);
  connectionStatusLabel_->style()->polish(connectionStatusLabel_);
}

void CinematographyWidget::setOperationStatus(const QString& text) {
  operationStatusLabel_->setText(text);
  operationStatusLabel_->setVisible(!text.isEmpty());
}

void CinematographyWidget::clearError() {
  errorLabel_->clear();
  errorLabel_->setVisible(false);
}

void CinematographyWidget::showError(const QString& message) {
  captureButton_->setEnabled(project_ != nullptr && camera_ != nullptr &&
                             !externalCaptureActive_ &&
                             !pendingCapture_.has_value());
  cameraCombo_->setEnabled(!externalCaptureActive_ &&
                           !pendingCapture_.has_value());
  cameraRefreshButton_->setEnabled(!externalCaptureActive_ &&
                                   !pendingCapture_.has_value());
  setOperationStatus({});
  errorLabel_->setText(message);
  errorLabel_->setVisible(true);
}

void CinematographyWidget::updatePreview(const QImage& image, const QString& placeholder) {
  previewImage_ = image;
  if (image.isNull()) {
    previewLabel_->setPixmap({});
    previewLabel_->setText(placeholder);
    return;
  }
  previewLabel_->setText({});
  previewLabel_->setPixmap(QPixmap::fromImage(image).scaled(
      previewLabel_->size(), Qt::KeepAspectRatio, Qt::SmoothTransformation));
}
