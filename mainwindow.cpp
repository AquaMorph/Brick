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
  EdsError err = EDS_ERR_OK;
  EdsCameraRef camera = NULL;
  bool isSDKLoaded = false;
  // Initialize SDK
  err = EdsInitializeSDK();
  if(err == EDS_ERR_OK) {
    isSDKLoaded = true;
  }
  // Get first camera
  if(err == EDS_ERR_OK) {
    // See Sample 2.
    err = getFirstCamera (&camera);
  }
  // Set Object event handler
/*  if(err == EDS_ERR_OK) {
    err = EdsSetObjectEventHandler(camera, kEdsObejctEvent_All,
    handleObjectEvent, NULL);
  }
  // Set Property event handler
  if(err == EDS_ERR_OK) {
    err = EdsSetPropertyEventHandler(camera, kEdsPropertyEvent_All,
    handlePropertyEvent, NULL);
  }
  // Set State event handler
  if(err == EDS_ERR_OK) {
    err = EdsSetCameraStateEventHandler(camera, kEdsStateEvent_All,
    handleStateEvent, NULL);
  }*/
  // Open session with camera
  if(err == EDS_ERR_OK) {
    err = EdsOpenSession(camera);
  }
  //Take a photo
  //if(err == EDS_ERR_OK) {
  //  err = takePicture(camera);
  //}

  // Set camera name in status bar
  statusBar()->showMessage(getCameraName(camera));

  // Close session with camera
  if(err == EDS_ERR_OK) {
    err = EdsCloseSession(camera);
  }
  // Release camera
  if(camera != NULL) {
    EdsRelease(camera);
  }
  // Terminate SDK
  if(isSDKLoaded) {
    EdsTerminateSDK();
  }
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
