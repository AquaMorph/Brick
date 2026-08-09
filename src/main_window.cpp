#include "main_window.h"

#include "animation_widget.h"
#include "camera/capture_coordinator.h"
#include "cinematography_widget.h"

#include <QAction>
#include <QApplication>
#include <QFileDialog>
#include <QFont>
#include <QFrame>
#include <QHBoxLayout>
#include <QInputDialog>
#include <QKeySequence>
#include <QLabel>
#include <QLineEdit>
#include <QListWidget>
#include <QMenu>
#include <QMenuBar>
#include <QMessageBox>
#include <QPainter>
#include <QPen>
#include <QPixmap>
#include <QPushButton>
#include <QSettings>
#include <QTabBar>
#include <QTabWidget>
#include <QVBoxLayout>
#include <QWidget>

#include <algorithm>
#include <utility>

namespace {

constexpr auto kLastProjectKey = "Projects/lastOpenedDirectory";

QIcon takeStatusIcon(bool active) {
  auto drawCheck = [active](const QColor& color) {
    QPixmap pixmap(12, 12);
    pixmap.fill(Qt::transparent);
    if (!active) {
      return pixmap;
    }

    QPainter painter(&pixmap);
    painter.setRenderHint(QPainter::Antialiasing);
    QPen pen(color, 1.8, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin);
    painter.setPen(pen);
    painter.drawPolyline(QPolygonF{{2.0, 6.0}, {5.0, 9.0}, {10.0, 2.5}});
    return pixmap;
  };

  QIcon icon;
  icon.addPixmap(drawCheck(QColor("#d75a4e")), QIcon::Normal);
  icon.addPixmap(drawCheck(Qt::white), QIcon::Selected);
  return icon;
}

}  // namespace


MainWindow::MainWindow(QWidget* parent) : QMainWindow(parent) {
  setWindowTitle(QApplication::applicationDisplayName());
  resize(1280, 720);

  auto* fileMenu = menuBar()->addMenu("&File");
  auto* newProjectAction = fileMenu->addAction("&New Project...");
  newProjectAction->setShortcut(QKeySequence::New);
  connect(newProjectAction, &QAction::triggered, this,
          [this] { createProject(); });

  auto* openProjectAction = fileMenu->addAction("&Open Project...");
  openProjectAction->setShortcut(QKeySequence::Open);
  connect(openProjectAction, &QAction::triggered, this,
          [this] { openProject(); });

  fileMenu->addSeparator();
  auto* exitAction = fileMenu->addAction("E&xit");
  exitAction->setShortcut(QKeySequence::Quit);
  connect(exitAction, &QAction::triggered, qApp, &QApplication::quit);

  auto* tabs = new QTabWidget(this);
  tabs->setDocumentMode(true);
  tabs->tabBar()->setFocusPolicy(Qt::NoFocus);

  auto* directTab = new QWidget(tabs);
  auto* directLayout = new QVBoxLayout(directTab);
  directLayout->setContentsMargins(24, 24, 24, 24);
  directLayout->setSpacing(18);

  filmTitleLabel_ = new QLabel("No film open", directTab);
  QFont filmTitleFont = filmTitleLabel_->font();
  filmTitleFont.setPointSize(24);
  filmTitleFont.setBold(true);
  filmTitleLabel_->setFont(filmTitleFont);
  directLayout->addWidget(filmTitleLabel_);

  auto* filmLayout = new QHBoxLayout;
  filmLayout->setSpacing(24);

  auto addSectionDivider = [filmLayout, directTab] {
    auto* divider = new QFrame(directTab);
    divider->setObjectName("directSectionDivider");
    divider->setFrameShape(QFrame::VLine);
    divider->setFixedWidth(1);
    filmLayout->addWidget(divider);
  };

  auto* scenePanel = new QVBoxLayout;
  scenePanel->setSpacing(12);

  auto* sceneHeading = new QLabel("Scenes", directTab);
  QFont headingFont = sceneHeading->font();
  headingFont.setPointSize(18);
  headingFont.setBold(true);
  sceneHeading->setFont(headingFont);
  scenePanel->addWidget(sceneHeading);

  sceneList_ = new QListWidget(directTab);
  sceneList_->setObjectName("sceneList");
  sceneList_->setAlternatingRowColors(true);
  scenePanel->addWidget(sceneList_, 1);

  auto* sceneActions = new QHBoxLayout;
  newSceneButton_ = new QPushButton("New Scene...", directTab);
  newSceneButton_->setObjectName("newSceneButton");
  renameSceneButton_ = new QPushButton("Rename...", directTab);
  renameSceneButton_->setObjectName("renameSceneButton");
  deleteSceneButton_ = new QPushButton("Delete", directTab);
  deleteSceneButton_->setObjectName("deleteSceneButton");
  moveSceneUpButton_ = new QPushButton("Move Up", directTab);
  moveSceneUpButton_->setObjectName("moveSceneUpButton");
  moveSceneDownButton_ = new QPushButton("Move Down", directTab);
  moveSceneDownButton_->setObjectName("moveSceneDownButton");
  sceneActions->addWidget(newSceneButton_);
  sceneActions->addWidget(renameSceneButton_);
  sceneActions->addWidget(deleteSceneButton_);
  sceneActions->addStretch();
  sceneActions->addWidget(moveSceneUpButton_);
  sceneActions->addWidget(moveSceneDownButton_);
  scenePanel->addLayout(sceneActions);
  filmLayout->addLayout(scenePanel, 1);
  addSectionDivider();

  auto* shotPanel = new QVBoxLayout;
  shotPanel->setSpacing(12);

  auto* shotHeading = new QLabel("Shots", directTab);
  shotHeading->setFont(headingFont);
  shotPanel->addWidget(shotHeading);

  shotList_ = new QListWidget(directTab);
  shotList_->setObjectName("shotList");
  shotList_->setAlternatingRowColors(true);
  shotPanel->addWidget(shotList_, 1);

  auto* shotActions = new QHBoxLayout;
  newShotButton_ = new QPushButton("New Shot...", directTab);
  newShotButton_->setObjectName("newShotButton");
  renameShotButton_ = new QPushButton("Rename...", directTab);
  renameShotButton_->setObjectName("renameShotButton");
  deleteShotButton_ = new QPushButton("Delete", directTab);
  deleteShotButton_->setObjectName("deleteShotButton");
  moveShotUpButton_ = new QPushButton("Move Up", directTab);
  moveShotUpButton_->setObjectName("moveShotUpButton");
  moveShotDownButton_ = new QPushButton("Move Down", directTab);
  moveShotDownButton_->setObjectName("moveShotDownButton");
  shotActions->addWidget(newShotButton_);
  shotActions->addWidget(renameShotButton_);
  shotActions->addWidget(deleteShotButton_);
  shotActions->addStretch();
  shotActions->addWidget(moveShotUpButton_);
  shotActions->addWidget(moveShotDownButton_);
  shotPanel->addLayout(shotActions);
  filmLayout->addLayout(shotPanel, 1);
  addSectionDivider();

  auto* takePanel = new QVBoxLayout;
  takePanel->setSpacing(12);

  auto* takeHeading = new QLabel("Takes", directTab);
  takeHeading->setFont(headingFont);
  takePanel->addWidget(takeHeading);

  takeList_ = new QListWidget(directTab);
  takeList_->setObjectName("takeList");
  takeList_->setAlternatingRowColors(true);
  takeList_->setIconSize(QSize(12, 12));
  takePanel->addWidget(takeList_, 1);

  auto* takeActions = new QHBoxLayout;
  newTakeButton_ = new QPushButton("New Take", directTab);
  newTakeButton_->setObjectName("newTakeButton");
  deleteTakeButton_ = new QPushButton("Delete", directTab);
  deleteTakeButton_->setObjectName("deleteTakeButton");
  selectTakeButton_ = new QPushButton("Select", directTab);
  selectTakeButton_->setObjectName("selectTakeButton");
  takeActions->addWidget(newTakeButton_);
  takeActions->addWidget(deleteTakeButton_);
  takeActions->addStretch();
  takeActions->addWidget(selectTakeButton_);
  takePanel->addLayout(takeActions);
  filmLayout->addLayout(takePanel, 1);
  directLayout->addLayout(filmLayout, 1);

  connect(newSceneButton_, &QPushButton::clicked, this,
          [this] { createScene(); });
  connect(renameSceneButton_, &QPushButton::clicked, this,
          [this] { renameScene(); });
  connect(deleteSceneButton_, &QPushButton::clicked, this,
          [this] { deleteScene(); });
  connect(moveSceneUpButton_, &QPushButton::clicked, this,
          [this] { moveScene(-1); });
  connect(moveSceneDownButton_, &QPushButton::clicked, this,
          [this] { moveScene(1); });
  connect(sceneList_, &QListWidget::currentRowChanged, this,
          [this] {
            updateSceneActions();
            refreshShots();
          });
  connect(newShotButton_, &QPushButton::clicked, this,
          [this] { createShot(); });
  connect(renameShotButton_, &QPushButton::clicked, this,
          [this] { renameShot(); });
  connect(deleteShotButton_, &QPushButton::clicked, this,
          [this] { deleteShot(); });
  connect(moveShotUpButton_, &QPushButton::clicked, this,
          [this] { moveShot(-1); });
  connect(moveShotDownButton_, &QPushButton::clicked, this,
          [this] { moveShot(1); });
  connect(shotList_, &QListWidget::currentRowChanged, this,
           [this] {
             updateShotActions();
             refreshTakes();
             updateCinematographyShot();
           });
  connect(newTakeButton_, &QPushButton::clicked, this,
          [this] { createTake(); });
  connect(deleteTakeButton_, &QPushButton::clicked, this,
           [this] { deleteTake(); });
  connect(selectTakeButton_, &QPushButton::clicked, this,
          [this] { selectTake(); });
  connect(takeList_, &QListWidget::itemDoubleClicked, this,
          [this] { selectTake(); });
  connect(takeList_, &QListWidget::currentRowChanged, this,
          [this] { updateTakeActions(); });

  tabs->addTab(directTab, "Direct");
  captureCoordinator_ = new CaptureCoordinator(tabs);
  cinematographyWidget_ = new CinematographyWidget(captureCoordinator_, tabs);
  tabs->addTab(cinematographyWidget_, "Cinematography");
  animationWidget_ = new AnimationWidget(captureCoordinator_, tabs);
  tabs->addTab(animationWidget_, "Animation");
  setCentralWidget(tabs);

  updateSceneActions();
  updateShotActions();
  updateTakeActions();

  const QString lastProjectDirectory =
      QSettings().value(kLastProjectKey).toString();
  if (!lastProjectDirectory.isEmpty()) {
    QString error;
    auto project = Project::open(lastProjectDirectory, &error);
    if (project.has_value()) {
      setProject(std::move(*project));
    } else {
      QMessageBox::warning(this, "Could Not Reopen Project", error);
    }
  }
}


void MainWindow::createProject() {
  const QString parentDirectory = QFileDialog::getExistingDirectory(
      this, "Choose Project Location", QString(),
      QFileDialog::ShowDirsOnly | QFileDialog::DontResolveSymlinks);
  if (parentDirectory.isEmpty()) {
    return;
  }

  bool accepted = false;
  const QString name = QInputDialog::getText(
      this, "New Project", "Project name:", QLineEdit::Normal, QString(),
      &accepted);
  if (!accepted) {
    return;
  }

  QString error;
  auto project = Project::create(parentDirectory, name, &error);
  if (!project.has_value()) {
    QMessageBox::critical(this, "Could Not Create Project", error);
    return;
  }

  setProject(std::move(*project));
}


void MainWindow::openProject() {
  const QString directory = QFileDialog::getExistingDirectory(
      this, "Open Project", QString(),
      QFileDialog::ShowDirsOnly | QFileDialog::DontResolveSymlinks);
  if (directory.isEmpty()) {
    return;
  }

  QString error;
  auto project = Project::open(directory, &error);
  if (!project.has_value()) {
    QMessageBox::critical(this, "Could Not Open Project", error);
    return;
  }

  setProject(std::move(*project));
}


void MainWindow::createScene() {
  if (!project_.has_value()) {
    return;
  }

  bool accepted = false;
  const QString name = QInputDialog::getText(
      this, "New Scene", "Scene name:", QLineEdit::Normal, QString(),
      &accepted);
  if (!accepted) {
    return;
  }

  QString error;
  if (!project_->createScene(name, &error)) {
    QMessageBox::critical(this, "Could Not Create Scene", error);
    return;
  }
  refreshScenes(static_cast<int>(project_->scenes().size()) - 1);
}


void MainWindow::renameScene() {
  const int row = sceneList_->currentRow();
  if (!project_.has_value() || row < 0) {
    return;
  }

  bool accepted = false;
  const QString name = QInputDialog::getText(
      this, "Rename Scene", "Scene name:", QLineEdit::Normal,
      project_->scenes()[row], &accepted);
  if (!accepted) {
    return;
  }

  QString error;
  if (!project_->renameScene(row, name, &error)) {
    QMessageBox::critical(this, "Could Not Rename Scene", error);
    return;
  }
  refreshScenes(row);
  updateActiveTakeViews();
}


void MainWindow::deleteScene() {
  const int row = sceneList_->currentRow();
  if (!project_.has_value() || row < 0) {
    return;
  }

  const QString& name = project_->scenes()[row];
  QMessageBox confirmation(QMessageBox::Warning, "Delete Scene",
                           QString("Delete scene \"%1\" and all of its "
                                   "contents? This cannot be undone.")
                               .arg(name),
                           QMessageBox::Cancel, this);
  auto* deleteButton = confirmation.addButton(
      "Delete", QMessageBox::DestructiveRole);
  confirmation.exec();
  if (confirmation.clickedButton() != deleteButton) {
    return;
  }

  QString error;
  if (!project_->deleteScene(row, &error)) {
    QMessageBox::critical(this, "Could Not Delete Scene", error);
    return;
  }
  refreshScenes(std::min(row, static_cast<int>(project_->scenes().size()) - 1));
  updateActiveTakeViews();
}


void MainWindow::moveScene(int offset) {
  const int from = sceneList_->currentRow();
  const int to = from + offset;
  if (!project_.has_value() || from < 0) {
    return;
  }

  QString error;
  if (!project_->moveScene(from, to, &error)) {
    QMessageBox::critical(this, "Could Not Move Scene", error);
    return;
  }
  refreshScenes(to);
  updateActiveTakeViews();
}


void MainWindow::refreshScenes(int selectedRow) {
  sceneList_->clear();
  if (project_.has_value()) {
    const auto& scenes = project_->scenes();
    for (int index = 0; index < static_cast<int>(scenes.size()); ++index) {
      sceneList_->addItem(
          QString("%1  %2").arg(index + 1, 4, 10, QLatin1Char('0')).arg(
              scenes[index]));
    }
    if (selectedRow >= 0 && selectedRow < sceneList_->count()) {
      sceneList_->setCurrentRow(selectedRow);
    }
  }
  updateSceneActions();
}


void MainWindow::updateSceneActions() {
  const bool hasProject = project_.has_value();
  const int row = sceneList_->currentRow();
  const int count = sceneList_->count();
  newSceneButton_->setEnabled(hasProject);
  renameSceneButton_->setEnabled(hasProject && row >= 0);
  deleteSceneButton_->setEnabled(hasProject && row >= 0);
  moveSceneUpButton_->setEnabled(hasProject && row > 0);
  moveSceneDownButton_->setEnabled(hasProject && row >= 0 && row + 1 < count);
}


void MainWindow::createShot() {
  const int sceneRow = sceneList_->currentRow();
  if (!project_.has_value() || sceneRow < 0) {
    return;
  }

  bool accepted = false;
  const QString name = QInputDialog::getText(
      this, "New Shot", "Shot name:", QLineEdit::Normal, QString(), &accepted);
  if (!accepted) {
    return;
  }

  QString error;
  if (!project_->createShot(sceneRow, name, &error)) {
    QMessageBox::critical(this, "Could Not Create Shot", error);
    return;
  }
  refreshShots(static_cast<int>(project_->shots(sceneRow).size()) - 1);
}


void MainWindow::renameShot() {
  const int sceneRow = sceneList_->currentRow();
  const int shotRow = shotList_->currentRow();
  if (!project_.has_value() || sceneRow < 0 || shotRow < 0) {
    return;
  }

  bool accepted = false;
  const QString name = QInputDialog::getText(
      this, "Rename Shot", "Shot name:", QLineEdit::Normal,
      project_->shots(sceneRow)[shotRow], &accepted);
  if (!accepted) {
    return;
  }

  QString error;
  if (!project_->renameShot(sceneRow, shotRow, name, &error)) {
    QMessageBox::critical(this, "Could Not Rename Shot", error);
    return;
  }
  refreshShots(shotRow);
  updateActiveTakeViews();
}


void MainWindow::deleteShot() {
  const int sceneRow = sceneList_->currentRow();
  const int shotRow = shotList_->currentRow();
  if (!project_.has_value() || sceneRow < 0 || shotRow < 0) {
    return;
  }

  const QString& name = project_->shots(sceneRow)[shotRow];
  QMessageBox confirmation(
      QMessageBox::Warning, "Delete Shot",
      QString("Delete shot \"%1\" and all of its contents? This cannot be "
              "undone.")
          .arg(name),
      QMessageBox::Cancel, this);
  auto* deleteButton = confirmation.addButton(
      "Delete", QMessageBox::DestructiveRole);
  confirmation.exec();
  if (confirmation.clickedButton() != deleteButton) {
    return;
  }

  QString error;
  if (!project_->deleteShot(sceneRow, shotRow, &error)) {
    QMessageBox::critical(this, "Could Not Delete Shot", error);
    return;
  }
  refreshShots(std::min(
      shotRow, static_cast<int>(project_->shots(sceneRow).size()) - 1));
  updateActiveTakeViews();
}


void MainWindow::moveShot(int offset) {
  const int sceneRow = sceneList_->currentRow();
  const int from = shotList_->currentRow();
  const int to = from + offset;
  if (!project_.has_value() || sceneRow < 0 || from < 0) {
    return;
  }

  QString error;
  if (!project_->moveShot(sceneRow, from, to, &error)) {
    QMessageBox::critical(this, "Could Not Move Shot", error);
    return;
  }
  refreshShots(to);
  updateActiveTakeViews();
}


void MainWindow::refreshShots(int selectedRow) {
  shotList_->clear();
  const int sceneRow = sceneList_->currentRow();
  if (project_.has_value() && sceneRow >= 0) {
    const auto& shots = project_->shots(sceneRow);
    for (int index = 0; index < static_cast<int>(shots.size()); ++index) {
      shotList_->addItem(
          QString("%1  %2").arg(index + 1, 4, 10, QLatin1Char('0')).arg(
              shots[index]));
    }
    if (selectedRow >= 0 && selectedRow < shotList_->count()) {
      shotList_->setCurrentRow(selectedRow);
    }
  }
  updateShotActions();
}


void MainWindow::updateShotActions() {
  const bool hasScene = project_.has_value() && sceneList_->currentRow() >= 0;
  const int row = shotList_->currentRow();
  const int count = shotList_->count();
  newShotButton_->setEnabled(hasScene);
  renameShotButton_->setEnabled(hasScene && row >= 0);
  deleteShotButton_->setEnabled(hasScene && row >= 0);
  moveShotUpButton_->setEnabled(hasScene && row > 0);
  moveShotDownButton_->setEnabled(hasScene && row >= 0 && row + 1 < count);
}


void MainWindow::createTake() {
  const int sceneRow = sceneList_->currentRow();
  const int shotRow = shotList_->currentRow();
  if (!project_.has_value() || sceneRow < 0 || shotRow < 0) {
    return;
  }

  QString error;
  if (!project_->createTake(sceneRow, shotRow, &error)) {
    QMessageBox::critical(this, "Could Not Create Take", error);
    return;
  }
  refreshTakes(project_->takeCount(sceneRow, shotRow) - 1);
}


void MainWindow::deleteTake() {
  const int sceneRow = sceneList_->currentRow();
  const int shotRow = shotList_->currentRow();
  const int takeRow = takeList_->currentRow();
  if (!project_.has_value() || sceneRow < 0 || shotRow < 0 || takeRow < 0) {
    return;
  }

  QMessageBox confirmation(
      QMessageBox::Warning, "Delete Take",
      QString("Delete take %1 and all of its contents? This cannot be undone.")
          .arg(takeRow + 1, 4, 10, QLatin1Char('0')),
      QMessageBox::Cancel, this);
  auto* deleteButton = confirmation.addButton(
      "Delete", QMessageBox::DestructiveRole);
  confirmation.exec();
  if (confirmation.clickedButton() != deleteButton) {
    return;
  }

  QString error;
  if (!project_->deleteTake(sceneRow, shotRow, takeRow, &error)) {
    QMessageBox::critical(this, "Could Not Delete Take", error);
    return;
  }
  refreshTakes(std::min(takeRow,
                        project_->takeCount(sceneRow, shotRow) - 1));
  updateActiveTakeViews();
}


void MainWindow::selectTake() {
  const int sceneRow = sceneList_->currentRow();
  const int shotRow = shotList_->currentRow();
  const int takeRow = takeList_->currentRow();
  if (!project_.has_value() || sceneRow < 0 || shotRow < 0 || takeRow < 0) {
    return;
  }

  QString error;
  if (!project_->selectTake(sceneRow, shotRow, takeRow, &error)) {
    QMessageBox::critical(this, "Could Not Select Take", error);
    return;
  }
  refreshTakes(takeRow);
  updateActiveTakeViews();
}


void MainWindow::refreshTakes(int selectedRow) {
  takeList_->clear();
  const int sceneRow = sceneList_->currentRow();
  const int shotRow = shotList_->currentRow();
  if (project_.has_value() && sceneRow >= 0 && shotRow >= 0) {
    const int count = project_->takeCount(sceneRow, shotRow);
    for (int index = 0; index < count; ++index) {
      auto* item = new QListWidgetItem(QString("Take %1").arg(
          index + 1, 4, 10, QLatin1Char('0')), takeList_);
      item->setIcon(takeStatusIcon(
          project_->activeTake() ==
          Project::ActiveTake{sceneRow, shotRow, index}));
    }
    const int row = selectedRow >= 0 ? selectedRow : 0;
    if (row < takeList_->count()) {
      takeList_->setCurrentRow(row);
    }
  }
  updateTakeActions();
}


void MainWindow::updateTakeActions() {
  const bool hasShot = project_.has_value() &&
                       sceneList_->currentRow() >= 0 &&
                       shotList_->currentRow() >= 0;
  newTakeButton_->setEnabled(hasShot);
  deleteTakeButton_->setEnabled(hasShot && takeList_->currentRow() >= 0);
  selectTakeButton_->setEnabled(hasShot && takeList_->currentRow() >= 0);
}


void MainWindow::updateActiveTakeViews() {
  animationWidget_->setActiveTake(project_ ? &*project_ : nullptr,
                                  project_ ? project_->activeTake()
                                           : std::nullopt);
}


void MainWindow::updateCinematographyShot() {
  const int sceneIndex = sceneList_->currentRow();
  const int shotIndex = shotList_->currentRow();
  cinematographyWidget_->setShot(project_ ? &*project_ : nullptr, sceneIndex,
                                 shotIndex);
}


void MainWindow::setProject(Project project) {
  project_ = std::move(project);
  QSettings().setValue(kLastProjectKey, project_->directory());
  setWindowTitle(project_->name() + " - " +
                  QApplication::applicationDisplayName());
  filmTitleLabel_->setText(project_->name());
  const int sceneRow = project_->activeTake().has_value()
                           ? project_->activeTake()->sceneIndex
                           : (project_->scenes().empty() ? -1 : 0);
  refreshScenes(sceneRow);
  if (project_->activeTake().has_value()) {
    refreshShots(project_->activeTake()->shotIndex);
    refreshTakes(project_->activeTake()->takeIndex);
  }
  updateActiveTakeViews();
  updateCinematographyShot();
}
