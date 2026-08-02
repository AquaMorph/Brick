#include "main_window.h"

#include <QAction>
#include <QApplication>
#include <QFileDialog>
#include <QInputDialog>
#include <QKeySequence>
#include <QLineEdit>
#include <QMenu>
#include <QMenuBar>
#include <QMessageBox>
#include <QStatusBar>
#include <QTabWidget>
#include <QWidget>

#include <utility>

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
  tabs->addTab(new QWidget(tabs), "Producer");
  tabs->addTab(new QWidget(tabs), "Cinematography");
  tabs->addTab(new QWidget(tabs), "Animation");
  setCentralWidget(tabs);

  statusBar()->showMessage("No project open");
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


void MainWindow::setProject(Project project) {
  project_ = std::move(project);
  setWindowTitle(project_->name() + " - " +
                 QApplication::applicationDisplayName());
  statusBar()->showMessage(project_->directory());
}
