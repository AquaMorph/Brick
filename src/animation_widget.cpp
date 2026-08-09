#include "animation_widget.h"

#include "frame_rate.h"
#include "playback_timing.h"
#include "layout_constants.h"

#include <QCheckBox>
#include <QComboBox>
#include <QDateTime>
#include <QFile>
#include <QFrame>
#include <QHBoxLayout>
#include <QKeySequence>
#include <QLabel>
#include <QListWidget>
#include <QMenu>
#include <QMessageBox>
#include <QImageReader>
#include <QPainter>
#include <QPushButton>
#include <QShortcut>
#include <QSignalBlocker>
#include <QSizePolicy>
#include <QSlider>
#include <QTimer>
#include <QVBoxLayout>

#include <algorithm>

namespace {

class AnimationCanvas final : public QWidget {
 public:
  explicit AnimationCanvas(QWidget* parent = nullptr) : QWidget(parent) {
    setMinimumSize(480, 270);
    setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Expanding);
  }

  void setBaseImage(const QImage& image, const QString& placeholder = {}) {
    baseImage_ = image;
    placeholder_ = placeholder;
    update();
  }

  void setOnionImage(const QImage& image, qreal opacity) {
    onionImage_ = image;
    onionOpacity_ = opacity;
    update();
  }

 protected:
  void paintEvent(QPaintEvent*) override {
    QPainter painter(this);
    painter.fillRect(rect(), QColor("#090909"));
    if (baseImage_.isNull()) {
      painter.setPen(QColor("#999999"));
      painter.drawText(rect(), Qt::AlignCenter | Qt::TextWordWrap, placeholder_);
      return;
    }
    const QSize scaled = baseImage_.size().scaled(size(), Qt::KeepAspectRatio);
    const QRect target((width() - scaled.width()) / 2,
                       (height() - scaled.height()) / 2, scaled.width(),
                       scaled.height());
    painter.setRenderHint(QPainter::SmoothPixmapTransform);
    painter.drawImage(target, baseImage_);
    if (!onionImage_.isNull() && onionOpacity_ > 0.0) {
      painter.setOpacity(onionOpacity_);
      painter.drawImage(target, onionImage_);
    }
  }

 private:
  QImage baseImage_;
  QImage onionImage_;
  QString placeholder_;
  qreal onionOpacity_ = 0.0;
};

QString frameLabel(int number) {
  return QString("%1").arg(number, 6, 10, QLatin1Char('0'));
}

QIcon liveViewIcon() {
  QPixmap thumbnail(112, 63);
  thumbnail.fill(QColor("#171717"));

  QPainter painter(&thumbnail);
  painter.setRenderHint(QPainter::Antialiasing);
  painter.setPen(QPen(QColor("#8f2d24"), 2));
  painter.setBrush(QColor("#d6d6d6"));
  painter.drawRoundedRect(QRectF(36, 21, 40, 25), 3, 3);
  painter.drawRoundedRect(QRectF(43, 17, 13, 7), 2, 2);
  painter.setBrush(QColor("#171717"));
  painter.drawEllipse(QPointF(56, 33.5), 8, 8);
  painter.setPen(Qt::NoPen);
  painter.setBrush(QColor("#8f2d24"));
  painter.drawEllipse(QPointF(56, 33.5), 4, 4);
  return QIcon(thumbnail);
}

}  // namespace


AnimationWidget::AnimationWidget(CaptureCoordinator* captureCoordinator,
                                 QWidget* parent)
    : QWidget(parent), captureCoordinator_(captureCoordinator) {
  auto* root = new QVBoxLayout(this);
  root->setContentsMargins(20, 18, 20, 18);
  root->setSpacing(14);

  auto* header = new QHBoxLayout;
  takeLabel_ = new QLabel("No active take selected", this);
  cameraLabel_ = new QLabel("No camera connected", this);
  header->addWidget(takeLabel_);
  header->addStretch();
  header->addWidget(cameraLabel_);
  root->addLayout(header);

  auto* workspace = new QHBoxLayout;
  workspace->setSpacing(16);
  auto* imageColumn = new QVBoxLayout;
  canvas_ = new AnimationCanvas(this);
  canvas_->setObjectName("animationCanvas");
  imageColumn->addWidget(canvas_, 1);
  statusLabel_ = new QLabel(this);
  statusLabel_->setVisible(false);
  imageColumn->addWidget(statusLabel_);
  errorLabel_ = new QLabel(this);
  errorLabel_->setObjectName("cameraErrorStatus");
  errorLabel_->setWordWrap(true);
  errorLabel_->setVisible(false);
  imageColumn->addWidget(errorLabel_);

  auto* stripHeader = new QHBoxLayout;
  auto* stripTitle = new QLabel("Frames", this);
  QFont sectionFont = stripTitle->font();
  sectionFont.setBold(true);
  stripTitle->setFont(sectionFont);
  frameCountLabel_ = new QLabel("No active take", this);
  stripHeader->addWidget(stripTitle);
  stripHeader->addStretch();
  stripHeader->addWidget(frameCountLabel_);
  imageColumn->addLayout(stripHeader);
  frameStrip_ = new QListWidget(this);
  frameStrip_->setObjectName("animationFrameStrip");
  frameStrip_->setViewMode(QListView::IconMode);
  frameStrip_->setFlow(QListView::LeftToRight);
  frameStrip_->setWrapping(false);
  frameStrip_->setResizeMode(QListView::Adjust);
  frameStrip_->setIconSize(QSize(112, 63));
  frameStrip_->setSpacing(8);
  frameStrip_->setMaximumHeight(118);
  frameStrip_->setContextMenuPolicy(Qt::CustomContextMenu);
  imageColumn->addWidget(frameStrip_);
  workspace->addLayout(imageColumn, 1);

  auto* sidebar = new QFrame(this);
  sidebar->setObjectName("cameraSidebar");
  sidebar->setFixedWidth(UiConstants::kSidebarWidth);
  auto* side = new QVBoxLayout(sidebar);
  side->setContentsMargins(16, 16, 16, 16);
  side->setSpacing(12);
  auto* captureTitle = new QLabel("Capture", sidebar);
  captureTitle->setFont(sectionFont);
  side->addWidget(captureTitle);
  captureButton_ = new QPushButton("Capture frame  (Enter)", sidebar);
  captureButton_->setObjectName("captureAnimationFrameButton");
  side->addWidget(captureButton_);
  auto* hint = new QLabel(
      "Select and configure the camera in Cinematography. The same live "
      "camera is used here.", sidebar);
  hint->setWordWrap(true);
  side->addWidget(hint);

  auto* onionTitle = new QLabel("Onion skin", sidebar);
  onionTitle->setFont(sectionFont);
  side->addWidget(onionTitle);
  onionCheck_ = new QCheckBox("Overlay previous frame", sidebar);
  onionCheck_->setChecked(true);
  side->addWidget(onionCheck_);
  auto* opacityRow = new QHBoxLayout;
  opacityRow->addWidget(new QLabel("Opacity", sidebar));
  onionOpacity_ = new QSlider(Qt::Horizontal, sidebar);
  onionOpacity_->setRange(0, 100);
  onionOpacity_->setValue(45);
  opacityRow->addWidget(onionOpacity_, 1);
  side->addLayout(opacityRow);

  auto* playbackTitle = new QLabel("Playback", sidebar);
  playbackTitle->setFont(sectionFont);
  side->addWidget(playbackTitle);
  auto* transport = new QHBoxLayout;
  auto* firstButton = new QPushButton("|<", sidebar);
  auto* previousButton = new QPushButton("<", sidebar);
  playButton_ = new QPushButton("Play", sidebar);
  auto* nextButton = new QPushButton(">", sidebar);
  auto* lastButton = new QPushButton(">|", sidebar);
  transport->addWidget(firstButton);
  transport->addWidget(previousButton);
  transport->addWidget(playButton_, 1);
  transport->addWidget(nextButton);
  transport->addWidget(lastButton);
  side->addLayout(transport);
  auto* playbackOptions = new QHBoxLayout;
  fpsCombo_ = new QComboBox(sidebar);
  for (const int framesPerSecond : FrameRate::kSupported) {
    fpsCombo_->addItem(QString("%1 FPS").arg(framesPerSecond),
                       framesPerSecond);
  }
  fpsCombo_->setCurrentIndex(
      fpsCombo_->findData(FrameRate::applicationDefault()));
  loopCheck_ = new QCheckBox("Loop", sidebar);
  playbackOptions->addWidget(fpsCombo_);
  playbackOptions->addStretch();
  playbackOptions->addWidget(loopCheck_);
  side->addLayout(playbackOptions);
  playbackQuality_ = new QComboBox(sidebar);
  playbackQuality_->addItem("Live-view preview");
  playbackQuality_->addItem("High-resolution preview");
  playbackQuality_->setToolTip(
      "Live-view preview uses the low-resolution frame directory for reliable "
      "playback. High-resolution preview uses captured preview images, never "
      "RAW files.");
  side->addWidget(playbackQuality_);
  deleteButton_ = new QPushButton("Delete selected frame", sidebar);
  side->addWidget(deleteButton_);
  side->addStretch();
  workspace->addWidget(sidebar);
  root->addLayout(workspace, 1);

  playbackTimer_ = new QTimer(this);
  playbackTimer_->setTimerType(Qt::PreciseTimer);
  playbackTimer_->setSingleShot(true);
  connect(playbackTimer_, &QTimer::timeout, this,
          [this] { advancePlayback(); });
  connect(captureCoordinator_, &CaptureCoordinator::cameraChanged, this,
          [this](CameraSession* camera) {
            if (camera == nullptr) {
              cameraLabel_->setText("No camera connected");
              liveImage_ = {};
              static_cast<AnimationCanvas*>(canvas_)->setBaseImage(
                  {}, "Connect a camera in Cinematography");
            } else {
              cameraLabel_->setText("Connected  ·  " + camera->displayName());
            }
            updateControls();
          });
  connect(captureCoordinator_, &CaptureCoordinator::previewFrame, this,
          [this](const QImage& image) {
            liveImage_ = image;
            if (showingLive_ && !playbackTimer_->isActive()) {
              static_cast<AnimationCanvas*>(canvas_)->setBaseImage(image);
            }
          });
  connect(captureCoordinator_, &CaptureCoordinator::cameraError, this,
          [this](const QString& error) { showError(error); });
  connect(captureCoordinator_, &CaptureCoordinator::captureStateChanged, this,
          [this](bool) { updateControls(); });
  connect(captureCoordinator_, &CaptureCoordinator::captureCompleted, this,
          [this](quint64 captureId, CapturePurpose purpose, const QString& path) {
            if (purpose == CapturePurpose::AnimationFrame &&
                pendingCapture_.has_value() &&
                pendingCapture_->requestId == captureId) {
              importCapture(path);
            }
          });
  connect(captureCoordinator_, &CaptureCoordinator::captureFailed, this,
          [this](quint64 captureId, CapturePurpose purpose,
                 const QString& message) {
            if (purpose != CapturePurpose::AnimationFrame ||
                !pendingCapture_.has_value() ||
                pendingCapture_->requestId != captureId) {
              return;
            }
            pendingCapture_.reset();
            showError(message);
            updateControls();
          });
  connect(captureButton_, &QPushButton::clicked, this,
          [this] { capture(); });
  connect(frameStrip_, &QListWidget::currentRowChanged, this,
          [this](int row) { selectFrame(row); });
  connect(frameStrip_, &QListWidget::customContextMenuRequested, this,
          [this](const QPoint& position) {
            QListWidgetItem* item = frameStrip_->itemAt(position);
            if (item == nullptr || frameStrip_->row(item) >=
                                       static_cast<int>(frames_.size()) ||
                pendingCapture_.has_value() ||
                playbackTimer_->isActive()) {
              return;
            }
            frameStrip_->setCurrentItem(item);
            QMenu menu(frameStrip_);
            QAction* deleteAction = menu.addAction("Delete");
            if (menu.exec(frameStrip_->viewport()->mapToGlobal(position)) ==
                deleteAction) {
              deleteSelectedFrame();
            }
          });
  connect(deleteButton_, &QPushButton::clicked, this,
          [this] { deleteSelectedFrame(); });
  connect(onionCheck_, &QCheckBox::toggled, this,
          [this] { updateOnionFrame(); });
  connect(onionOpacity_, &QSlider::valueChanged, this,
          [this] { updateOnionFrame(); });
  connect(playButton_, &QPushButton::clicked, this,
          [this] { togglePlayback(); });
  connect(firstButton, &QPushButton::clicked, this,
          [this] { navigate(-1000000); });
  connect(previousButton, &QPushButton::clicked, this,
          [this] { navigate(-1); });
  connect(nextButton, &QPushButton::clicked, this,
          [this] { navigate(1); });
  connect(lastButton, &QPushButton::clicked, this,
          [this] { navigate(1000000); });
  connect(fpsCombo_, &QComboBox::currentIndexChanged, this, [this](int index) {
    const int value = fpsCombo_->itemData(index).toInt();
    if (activeTake_.has_value()) {
      QString error;
      if (!project_->saveTakeFrameRate(activeTake_->sceneIndex,
                                       activeTake_->shotIndex,
                                       activeTake_->takeIndex, value, &error)) {
        showError(error);
      }
    }
    if (!FrameRate::saveApplicationDefault(value)) {
      showError("Brick could not save the default frame rate.");
    }
    if (playbackTimer_->isActive()) {
      playbackClock_.restart();
      playbackTransition_ = 0;
      playbackStartRow_ = playbackRow_;
      schedulePlaybackFrame();
    }
  });

  auto* captureShortcut = new QShortcut(QKeySequence(Qt::Key_Return), this);
  captureShortcut->setContext(Qt::WidgetWithChildrenShortcut);
  connect(captureShortcut, &QShortcut::activated, this, [this] { capture(); });
  auto* enterShortcut = new QShortcut(QKeySequence(Qt::Key_Enter), this);
  enterShortcut->setContext(Qt::WidgetWithChildrenShortcut);
  connect(enterShortcut, &QShortcut::activated, this, [this] { capture(); });
  auto* playShortcut = new QShortcut(QKeySequence(Qt::Key_Space), this);
  playShortcut->setContext(Qt::WidgetWithChildrenShortcut);
  connect(playShortcut, &QShortcut::activated, this,
          [this] { togglePlayback(); });
  auto* previousShortcut = new QShortcut(QKeySequence(Qt::Key_Left), this);
  previousShortcut->setContext(Qt::WidgetWithChildrenShortcut);
  connect(previousShortcut, &QShortcut::activated, this,
          [this] { navigate(-1); });
  auto* nextShortcut = new QShortcut(QKeySequence(Qt::Key_Right), this);
  nextShortcut->setContext(Qt::WidgetWithChildrenShortcut);
  connect(nextShortcut, &QShortcut::activated, this,
          [this] { navigate(1); });

  if (CameraSession* camera = captureCoordinator_->cameraSession()) {
    cameraLabel_->setText("Connected  ·  " + camera->displayName());
  }
  updateControls();
}


void AnimationWidget::setActiveTake(
    Project* project, const std::optional<Project::ActiveTake>& activeTake) {
  ++contextRevision_;
  stopPlayback(false);
  project_ = project;
  activeTake_ = activeTake;
  errorLabel_->setVisible(false);
  if (project_ == nullptr || !activeTake_.has_value()) {
    takeLabel_->setText("No active take selected");
    refreshFrames();
    showLiveView();
    return;
  }
  const auto& take = *activeTake_;
  takeLabel_->setText(
      QString("Scene %1: %2  /  Shot %3: %4  /  Take %5")
          .arg(take.sceneIndex + 1, 4, 10, QLatin1Char('0'))
          .arg(project_->scenes()[take.sceneIndex])
          .arg(take.shotIndex + 1, 4, 10, QLatin1Char('0'))
          .arg(project_->shots(take.sceneIndex)[take.shotIndex])
          .arg(take.takeIndex + 1, 4, 10, QLatin1Char('0')));
  QString error;
  const int framesPerSecond = project_->takeFrameRate(
      take.sceneIndex, take.shotIndex, take.takeIndex, &error);
  {
    const QSignalBlocker blocker(fpsCombo_);
    fpsCombo_->setCurrentIndex(fpsCombo_->findData(framesPerSecond));
  }
  if (!error.isEmpty()) {
    showError(error);
  }
  refreshFrames();
  showLiveView();
}


void AnimationWidget::capture() {
  if (!captureCoordinator_->isReady() || project_ == nullptr ||
      !activeTake_.has_value() || pendingCapture_.has_value() ||
      captureCoordinator_->isCapturing()) {
    return;
  }
  stopPlayback();
  QString error;
  const auto take = *activeTake_;
  const QString takePath = project_->takeDirectory(
      take.sceneIndex, take.shotIndex, take.takeIndex, &error);
  if (takePath.isEmpty()) {
    showError(error);
    return;
  }
  pendingCapture_ = PendingCapture{0, contextRevision_, project_,
                                    project_->directory(), takePath, take};
  statusLabel_->setText("Capturing frame...");
  statusLabel_->setVisible(true);
  errorLabel_->setVisible(false);
  updateControls();
  const quint64 requestId = captureCoordinator_->requestCapture(
      CapturePurpose::AnimationFrame, &error);
  if (requestId == 0) {
    pendingCapture_.reset();
    showError(error);
    updateControls();
    return;
  }
  pendingCapture_->requestId = requestId;
}


void AnimationWidget::importCapture(const QString& filePath) {
  if (!pendingCapture_.has_value()) {
    return;
  }
  const PendingCapture capture = *pendingCapture_;
  pendingCapture_.reset();
  QString error;
  if (contextRevision_ != capture.contextRevision || project_ != capture.project ||
      project_->directory() != capture.projectDirectory ||
      project_->takeDirectory(capture.take.sceneIndex, capture.take.shotIndex,
                              capture.take.takeIndex, &error) !=
          capture.takeDirectory) {
    showError("The active take changed before capture completed. The image "
              "remains at " + filePath);
    updateControls();
    return;
  }
  const auto frame = project_->importFrame(
      capture.take.sceneIndex, capture.take.shotIndex, capture.take.takeIndex,
      filePath, QDateTime::currentDateTimeUtc(), &error);
  if (!frame.has_value()) {
    showError(error + " The image remains at " + filePath);
    updateControls();
    return;
  }
  QFile::remove(filePath);
  statusLabel_->setText("Captured frame " + frameLabel(frame->number));
  statusLabel_->setVisible(true);
  refreshFrames(frame->number);
  showLiveView();
  updateControls();
}


void AnimationWidget::refreshFrames(int selectedNumber) {
  const QSignalBlocker blocker(frameStrip_);
  frameStrip_->clear();
  frames_.clear();
  if (project_ == nullptr || !activeTake_.has_value()) {
    new QListWidgetItem(liveViewIcon(), "Live view", frameStrip_);
    frameStrip_->setCurrentRow(0);
    frameCountLabel_->setText("No active take");
    updateControls();
    return;
  }
  const auto take = *activeTake_;
  QString error;
  frames_ = project_->frames(take.sceneIndex, take.shotIndex, take.takeIndex,
                             &error);
  for (const AnimationFrame& frame : frames_) {
    const QImage thumbnail(frame.lowResPath);
    auto* item = new QListWidgetItem(
        QIcon(QPixmap::fromImage(thumbnail)), frameLabel(frame.number),
        frameStrip_);
    if (frame.number == selectedNumber) {
      frameStrip_->setCurrentItem(item);
    }
  }
  auto* liveItem =
      new QListWidgetItem(liveViewIcon(), "Live view", frameStrip_);
  if (showingLive_) {
    frameStrip_->setCurrentItem(liveItem);
  }
  frameCountLabel_->setText(
      QString("%1 frame%2").arg(frames_.size()).arg(frames_.size() == 1 ? "" : "s"));
  if (!error.isEmpty()) {
    showError(error);
  }
  updateOnionFrame();
  updateControls();
}


void AnimationWidget::selectFrame(int row) {
  if (row == static_cast<int>(frames_.size())) {
    showLiveView();
    return;
  }
  if (row < 0 || row >= static_cast<int>(frames_.size())) {
    updateControls();
    return;
  }
  stopPlayback(false);
  showFrame(row);
  updateControls();
}


void AnimationWidget::showLiveView() {
  stopPlayback(false);
  showingLive_ = true;
  const QSignalBlocker blocker(frameStrip_);
  frameStrip_->setCurrentRow(static_cast<int>(frames_.size()));
  static_cast<AnimationCanvas*>(canvas_)->setBaseImage(
      liveImage_, !captureCoordinator_->isReady()
                      ? "Connect a camera in Cinematography"
                      : "Waiting for live view...");
  updateOnionFrame();
  frameCountLabel_->setText(
      QString("%1 frame%2").arg(frames_.size()).arg(frames_.size() == 1 ? "" : "s"));
  updateControls();
}


void AnimationWidget::updateOnionFrame() {
  QImage onion;
  if (showingLive_ && onionCheck_->isChecked() && !frames_.empty()) {
    onion = QImage(frames_.back().lowResPath);
  }
  static_cast<AnimationCanvas*>(canvas_)->setOnionImage(
      onion, onionOpacity_->value() / 100.0);
}


void AnimationWidget::deleteSelectedFrame() {
  const int row = frameStrip_->currentRow();
  if (project_ == nullptr || !activeTake_.has_value() || row < 0 ||
      row >= static_cast<int>(frames_.size())) {
    return;
  }
  QMessageBox confirmation(QMessageBox::Warning, "Delete Frame",
                            "Delete the selected frame? This cannot be undone.",
                            QMessageBox::Cancel, this);
  auto* action = confirmation.addButton("Delete", QMessageBox::DestructiveRole);
  confirmation.exec();
  if (confirmation.clickedButton() != action) {
    return;
  }
  const auto take = *activeTake_;
  QString error;
  if (!project_->deleteFrame(take.sceneIndex, take.shotIndex, take.takeIndex,
                             frames_[row].number, &error)) {
    showError(error);
    return;
  }
  refreshFrames();
  showLiveView();
}


void AnimationWidget::togglePlayback() {
  if (playbackTimer_->isActive()) {
    stopPlayback();
    return;
  }
  if (frames_.empty()) {
    return;
  }
  playbackImages_.clear();
  playbackImages_.reserve(frames_.size() + 1);
  const bool highResolution = playbackQuality_->currentIndex() == 1;
  for (const AnimationFrame& frame : frames_) {
    QImageReader reader(highResolution ? frame.highResPath : frame.lowResPath);
    if (highResolution) {
      const QSize sourceSize = reader.size();
      if (sourceSize.isValid()) {
        reader.setScaledSize(sourceSize.scaled(canvas_->size(),
                                               Qt::KeepAspectRatio));
      }
    }
    playbackImages_.push_back(reader.read());
  }
  if (!liveImage_.isNull()) {
    playbackImages_.push_back(liveImage_);
  }
  showingLive_ = false;
  static_cast<AnimationCanvas*>(canvas_)->setOnionImage({}, 0.0);
  playbackRow_ = frameStrip_->currentRow();
  if (playbackRow_ < 0 || playbackRow_ >= static_cast<int>(frames_.size())) {
    playbackRow_ = 0;
  }
  playbackStartRow_ = playbackRow_;
  playbackTransition_ = 0;
  showPlaybackFrame(playbackRow_);
  playbackClock_.start();
  schedulePlaybackFrame();
  playButton_->setText("Pause");
  updateControls();
}


void AnimationWidget::stopPlayback(bool returnToLive) {
  playbackTimer_->stop();
  playbackImages_.clear();
  playButton_->setText("Play");
  playbackRow_ = -1;
  if (returnToLive && !showingLive_) {
    showLiveView();
  }
}


void AnimationWidget::advancePlayback() {
  if (playbackImages_.empty()) {
    stopPlayback();
    return;
  }

  const qint64 elapsedNs = playbackClock_.nsecsElapsed();
  const qint64 expectedTransition =
      PlaybackTiming::transitionAt(elapsedNs, fpsCombo_->currentData().toInt(),
                                   playbackTransition_);
  if (!loopCheck_->isChecked() &&
      expectedTransition >=
          static_cast<qint64>(playbackImages_.size()) - playbackStartRow_) {
    stopPlayback();
    return;
  }

  playbackTransition_ = expectedTransition;
  playbackRow_ = static_cast<int>(
      (playbackStartRow_ + playbackTransition_) %
      static_cast<qint64>(playbackImages_.size()));
  showPlaybackFrame(playbackRow_);
  schedulePlaybackFrame();
}


void AnimationWidget::schedulePlaybackFrame() {
  playbackTimer_->start(PlaybackTiming::delayMilliseconds(
      playbackClock_.nsecsElapsed(), fpsCombo_->currentData().toInt(),
      playbackTransition_));
}


void AnimationWidget::navigate(int offset) {
  if (frames_.empty()) {
    return;
  }
  stopPlayback(false);
  int row = frameStrip_->currentRow();
  if (offset < -1) {
    row = 0;
  } else if (offset > 1) {
    row = static_cast<int>(frames_.size()) - 1;
  } else {
    row = std::clamp(row < 0 ? (offset > 0 ? 0 : static_cast<int>(frames_.size()) - 1)
                             : row + offset,
                     0, static_cast<int>(frames_.size()) - 1);
  }
  frameStrip_->setCurrentRow(row);
  showFrame(row);
}


void AnimationWidget::showFrame(int row) {
  if (row < 0 || row >= static_cast<int>(frames_.size())) {
    return;
  }
  showingLive_ = false;
  const QSignalBlocker blocker(frameStrip_);
  frameStrip_->setCurrentRow(row);
  static_cast<AnimationCanvas*>(canvas_)->setOnionImage({}, 0.0);
  const QImage image(frames_[row].highResPath);
  static_cast<AnimationCanvas*>(canvas_)->setBaseImage(
      image, image.isNull() ? "Could not load frame" : QString{});
  frameCountLabel_->setText(
      QString("Frame %1 of %2").arg(row + 1).arg(frames_.size()));
}


void AnimationWidget::showPlaybackFrame(int row) {
  if (row < 0 || row >= static_cast<int>(playbackImages_.size())) {
    return;
  }
  if (row < static_cast<int>(frames_.size())) {
    const QSignalBlocker blocker(frameStrip_);
    frameStrip_->setCurrentRow(row);
    frameCountLabel_->setText(
        QString("Frame %1 of %2").arg(row + 1).arg(frames_.size()));
  } else {
    const QSignalBlocker blocker(frameStrip_);
    frameStrip_->setCurrentRow(static_cast<int>(frames_.size()));
    frameCountLabel_->setText("Live view");
  }
  static_cast<AnimationCanvas*>(canvas_)->setBaseImage(
      playbackImages_[row], playbackImages_[row].isNull()
                                ? "Could not load frame"
                                : QString{});
}


void AnimationWidget::updateControls() {
  const bool hasTake = project_ != nullptr && activeTake_.has_value();
  const bool capturing = pendingCapture_.has_value();
  captureButton_->setEnabled(hasTake && captureCoordinator_->isReady() &&
                              !capturing && !captureCoordinator_->isCapturing() &&
                              !playbackTimer_->isActive());
  playButton_->setEnabled(!frames_.empty() && !capturing);
  playbackQuality_->setEnabled(!playbackTimer_->isActive());
  deleteButton_->setEnabled(
      frameStrip_->currentRow() >= 0 &&
      frameStrip_->currentRow() < static_cast<int>(frames_.size()) &&
      !capturing && !playbackTimer_->isActive());
  onionCheck_->setEnabled(hasTake && !frames_.empty());
  onionOpacity_->setEnabled(onionCheck_->isEnabled() && onionCheck_->isChecked());
}


void AnimationWidget::showError(const QString& message) {
  statusLabel_->setVisible(false);
  errorLabel_->setText(message);
  errorLabel_->setVisible(true);
}
