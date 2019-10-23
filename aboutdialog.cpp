#include "aboutdialog.h"
#include "ui_aboutdialog.h"

AboutDialog::AboutDialog(QWidget *parent) :
  QDialog(parent),
  ui(new Ui::AboutDialog) {
  // Remove question mark from the title bar
  setWindowFlags(windowFlags() & ~Qt::WindowContextHelpButtonHint);

  ui->setupUi(this);
}

AboutDialog::~AboutDialog(){
  delete ui;
}
