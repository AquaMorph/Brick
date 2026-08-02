#include <QApplication>
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
  palette.setColor(QPalette::Text, Qt::white);
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
  )");

  MainWindow window;
  window.show();

  return application.exec();
}
