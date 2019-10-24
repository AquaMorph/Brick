#ifndef MAINWINDOW_H
#define MAINWINDOW_H

#include <QMainWindow>
#include "edsdkwrapper.h"

QT_BEGIN_NAMESPACE
namespace Ui { class MainWindow; }
QT_END_NAMESPACE

class MainWindow : public QMainWindow {
  Q_OBJECT

public:
  MainWindow(QWidget *parent = nullptr);
  ~MainWindow();

private:
  Ui::MainWindow *ui;
  void OnFileNew();
  void OnFileOpen();
  void OnFileSave();
  void OnHelpAbout();
};


#endif // MAINWINDOW_H
