#include <QApplication>
#include <QMainWindow>
#include <QPalette>

int main(int argc, char* argv[]) {
  QApplication application(argc, argv);
  QApplication::setApplicationDisplayName("Brick");
  QApplication::setApplicationName("Brick");
  QApplication::setApplicationVersion("0.1.0");
  QApplication::setOrganizationName("Brick");

  auto palette = application.palette();
  palette.setColor(QPalette::Window, Qt::black);
  application.setPalette(palette);

  QMainWindow window;
  window.setWindowTitle(QApplication::applicationDisplayName());
  window.resize(1280, 720);
  window.show();

  return application.exec();
}
