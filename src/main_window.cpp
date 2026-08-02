#include "main_window.h"

#include <QAction>
#include <QApplication>
#include <QFileDialog>
#include <QFont>
#include <QHBoxLayout>
#include <QInputDialog>
#include <QKeySequence>
#include <QLabel>
#include <QLineEdit>
#include <QListWidget>
#include <QMenu>
#include <QMenuBar>
#include <QMessageBox>
#include <QPushButton>
#include <QSettings>
#include <QStatusBar>
#include <QTabWidget>
#include <QVBoxLayout>
#include <QWidget>

#include <algorithm>
#include <utility>

namespace {

constexpr auto kLastProjectKey = "Projects/lastOpenedDirectory";

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

  auto* producerTab = new QWidget(tabs);
  auto* producerLayout = new QVBoxLayout(producerTab);
  producerLayout->setContentsMargins(24, 24, 24, 24);
  producerLayout->setSpacing(12);

  auto* heading = new QLabel("Scenes", producerTab);
  QFont headingFont = heading->font();
  headingFont.setPointSize(18);
  headingFont.setBold(true);
  heading->setFont(headingFont);
  producerLayout->addWidget(heading);

  sceneList_ = new QListWidget(producerTab);
  sceneList_->setObjectName("sceneList");
  sceneList_->setAlternatingRowColors(true);
  producerLayout->addWidget(sceneList_, 1);

  auto* sceneActions = new QHBoxLayout;
  newSceneButton_ = new QPushButton("New Scene...", producerTab);
  newSceneButton_->setObjectName("newSceneButton");
  deleteSceneButton_ = new QPushButton("Delete", producerTab);
  deleteSceneButton_->setObjectName("deleteSceneButton");
  moveSceneUpButton_ = new QPushButton("Move Up", producerTab);
  moveSceneUpButton_->setObjectName("moveSceneUpButton");
  moveSceneDownButton_ = new QPushButton("Move Down", producerTab);
  moveSceneDownButton_->setObjectName("moveSceneDownButton");
  sceneActions->addWidget(newSceneButton_);
  sceneActions->addWidget(deleteSceneButton_);
  sceneActions->addStretch();
  sceneActions->addWidget(moveSceneUpButton_);
  sceneActions->addWidget(moveSceneDownButton_);
  producerLayout->addLayout(sceneActions);

  connect(newSceneButton_, &QPushButton::clicked, this,
          [this] { createScene(); });
  connect(deleteSceneButton_, &QPushButton::clicked, this,
          [this] { deleteScene(); });
  connect(moveSceneUpButton_, &QPushButton::clicked, this,
          [this] { moveScene(-1); });
  connect(moveSceneDownButton_, &QPushButton::clicked, this,
          [this] { moveScene(1); });
  connect(sceneList_, &QListWidget::currentRowChanged, this,
          [this] { updateSceneActions(); });

  tabs->addTab(producerTab, "Producer");
  tabs->addTab(new QWidget(tabs), "Cinematography");
  tabs->addTab(new QWidget(tabs), "Animation");
  setCentralWidget(tabs);

  updateSceneActions();
  statusBar()->showMessage("No project open");

  const QString lastProjectDirectory =
      QSettings().value(kLastProjectKey).toString();
  if (!lastProjectDirectory.isEmpty()) {
    QString error;
    auto project = Project::open(lastProjectDirectory, &error);
    if (project.has_value()) {
      setProject(std::move(*project));
    } else {
      statusBar()->showMessage("Could not reopen last project: " + error);
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
  deleteSceneButton_->setEnabled(hasProject && row >= 0);
  moveSceneUpButton_->setEnabled(hasProject && row > 0);
  moveSceneDownButton_->setEnabled(hasProject && row >= 0 && row + 1 < count);
}


void MainWindow::setProject(Project project) {
  project_ = std::move(project);
  QSettings().setValue(kLastProjectKey, project_->directory());
  setWindowTitle(project_->name() + " - " +
                  QApplication::applicationDisplayName());
  statusBar()->showMessage(project_->directory());
  refreshScenes(project_->scenes().empty() ? -1 : 0);
}
