#include <QApplication>
#include <QMainWindow>
#include <QPalette>
#include <QTabWidget>
#include <QWidget>

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
  )");

  QMainWindow window;
  window.setWindowTitle(QApplication::applicationDisplayName());
  window.resize(1280, 720);

  auto* tabs = new QTabWidget(&window);
  tabs->setDocumentMode(true);
  tabs->addTab(new QWidget(tabs), "Producer");
  tabs->addTab(new QWidget(tabs), "Cinematography");
  tabs->addTab(new QWidget(tabs), "Animation");
  window.setCentralWidget(tabs);

  window.show();

  return application.exec();
}
