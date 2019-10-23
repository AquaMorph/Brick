#include "mainwindow.h"
#include "ui_mainwindow.h"
#include "aboutdialog.h"

MainWindow::MainWindow(QWidget *parent)
  : QMainWindow(parent)
  , ui(new Ui::MainWindow) {
  ui->setupUi(this);

  // == MENU BAR ==
  QMenuBar* menuBar = new QMenuBar(this);
  setMenuBar(menuBar);

  // -- File menu --
  QMenu* fileMenu = menuBar->addMenu("File");

    // New
    QMenu * newMenu = fileMenu->addMenu("New");
      // New Take
      QAction* action = newMenu->addAction( "Project" );
      connect(action, &QAction::triggered, this, &MainWindow::OnFileNew);
      newMenu->addAction(action);
      // New Scene
      action = newMenu->addAction( "Scene" );
      connect(action, &QAction::triggered, this, &MainWindow::OnFileNew);
      newMenu->addAction(action);
      // New Take
      action = newMenu->addAction( "Take" );
      connect(action, &QAction::triggered, this, &MainWindow::OnFileNew);
      newMenu->addAction(action);

    // Open
    QMenu * openMenu = fileMenu->addMenu("Open");
      // Open Project
      action = new QAction("Project", this);
      connect(action, &QAction::triggered, this, &MainWindow::OnFileOpen);
      openMenu->addAction(action);
      // Open Scene
      action = new QAction("Scene", this);
      connect(action, &QAction::triggered, this, &MainWindow::OnFileOpen);
      openMenu->addAction(action);
      // Open Project
      action = new QAction("Take", this);
      connect(action, &QAction::triggered, this, &MainWindow::OnFileOpen);
      openMenu->addAction(action);

    // Save
    action = new QAction("Save", this);
    connect(action, &QAction::triggered, this, &MainWindow::OnFileSave);
    fileMenu->addAction(action);

    // separator
    fileMenu->addSeparator();

    // Exit
    action = new QAction("Exit", this);
    connect(action, &QAction::triggered, this, &MainWindow::close);
    fileMenu->addAction(action);

  QMenu * editMenu = menuBar->addMenu("Edit");
  QMenu * viewMenu = menuBar->addMenu("View");
  QMenu * helpMenu = menuBar->addMenu("Help");
    action = new QAction("About Brick", this);
    connect(action, &QAction::triggered, this, &MainWindow::OnHelpAbout);
    helpMenu->addAction(action);
}

MainWindow::~MainWindow() {
  delete ui;
}

void MainWindow::OnFileNew() {
  statusBar()->showMessage("File -> New");
}

void MainWindow::OnFileOpen() {
  statusBar()->showMessage("File -> Open");
}

void MainWindow::OnFileSave() {
  statusBar()->showMessage("File -> Save");
}
void MainWindow::OnHelpAbout() {
  AboutDialog aboutdialog;
  aboutdialog.setModal(true);
  aboutdialog.exec();
}
