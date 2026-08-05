#include <QApplication>
#include <QColor>
#include <QPalette>

#include "main_window.h"

int main(int argc, char* argv[]) {
  QApplication application(argc, argv);
  QApplication::setApplicationDisplayName("Brick");
  QApplication::setApplicationName("Brick");
  QApplication::setApplicationVersion("0.1.0");
  QApplication::setOrganizationName("Brick");

  auto palette = application.palette();
  palette.setColor(QPalette::Window, Qt::black);
  palette.setColor(QPalette::WindowText, Qt::white);
  palette.setColor(QPalette::Base, Qt::black);
  palette.setColor(QPalette::AlternateBase, QColor("#111111"));
  palette.setColor(QPalette::Text, Qt::white);
  palette.setColor(QPalette::Highlight, QColor("#8f3028"));
  palette.setColor(QPalette::HighlightedText, Qt::white);
  application.setPalette(palette);

  application.setStyleSheet(R"(
    QMainWindow,
    QMenuBar,
    QMenu,
    QStatusBar,
    QTabWidget::pane,
    QTabWidget QWidget {
      background-color: #090909;
    }

    QTabWidget::pane {
      border: 0;
    }

    QFrame#directSectionDivider {
      border: 0;
      background-color: #282828;
    }

    QFrame#cameraSidebar {
      border: 1px solid #2d2d2d;
      border-radius: 4px;
      background-color: #111111;
    }

    QLabel#cameraPreview {
      border: 1px solid #303030;
      background-color: #030303;
      color: #777777;
    }

    QLabel#cameraPreview[live="true"] {
      border-color: #67221d;
    }

    QLabel#cinematographyShotLabel,
    QLabel#cameraStatus {
      color: #999999;
    }

    QListWidget#testShotGallery {
      border: 1px solid #2d2d2d;
      background-color: #0d0d0d;
    }

    QListWidget#testShotGallery::item {
      border: 1px solid transparent;
      padding: 4px;
    }

    QListWidget#testShotGallery::item:selected {
      border-color: #a23b31;
      background-color: #2a1513;
    }

    QComboBox {
      min-height: 28px;
      padding: 2px 8px;
      border: 1px solid #444444;
      border-radius: 3px;
      background-color: #202020;
      color: #f2f2f2;
    }

    QSlider::groove:horizontal {
      height: 4px;
      border-radius: 2px;
      background-color: #454545;
    }

    QSlider::sub-page:horizontal {
      border-radius: 2px;
      background-color: #8f3028;
    }

    QSlider::handle:horizontal {
      width: 20px;
      margin: -8px 0;
      border: 1px solid #777777;
      border-radius: 5px;
      background-color: #d0d0d0;
    }

    QSlider::handle:horizontal:hover,
    QSlider::handle:horizontal:focus {
      border-color: #b3473c;
      background-color: #eeeeee;
    }

    QSlider::handle:horizontal:pressed {
      border-color: #c75a4e;
      background-color: #b3473c;
    }

    QSlider:horizontal {
      min-height: 32px;
    }

    QTabBar {
      background-color: #111111;
    }

    QTabBar::tab {
      min-width: 160px;
      padding: 12px 24px 10px;
      border: 0;
      border-bottom: 3px solid transparent;
      background-color: #111111;
      color: #a0a0a0;
    }

    QTabBar::tab:hover {
      background-color: #181818;
      color: #eeeeee;
    }

    QTabBar::tab:selected {
      background-color: #181818;
      border-bottom-color: #8f3028;
      color: #ffffff;
    }

    QMenuBar,
    QMenu,
    QStatusBar {
      background-color: #111111;
      color: #dddddd;
    }

    QMenuBar::item:selected,
    QMenu::item:selected {
      background-color: #8f3028;
      color: #ffffff;
    }

    QPushButton {
      padding: 7px 14px;
      border: 1px solid #444444;
      border-radius: 3px;
      background-color: #202020;
      color: #f2f2f2;
    }

    QPushButton:hover {
      border-color: #a23b31;
      background-color: #8f3028;
      color: #ffffff;
    }

    QPushButton:pressed {
      background-color: #67221d;
      color: #ffffff;
    }

    QPushButton:disabled {
      border-color: #292929;
      background-color: #141414;
      color: #707070;
    }
  )");

  MainWindow window;
  window.show();

  return application.exec();
}
