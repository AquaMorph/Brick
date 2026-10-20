#pragma once

#include <QImage>
#include <QSize>
#include <QString>

// Read standard images or develop Canon RAW through the isolated SDK helper.
QImage loadCaptureImage(const QString& path, const QSize& targetSize = {},
                        QString* error = nullptr);
