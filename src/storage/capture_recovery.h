#pragma once

#include "core/project.h"

#include <QLockFile>

#include <memory>

enum class CapturePurpose { TestShot, AnimationFrame };

struct RecoveryCapture {
  QString directory;
  QString filePath;
  QString description;
  bool ready;
};

class CaptureRecovery {
 public:
  explicit CaptureRecovery(QString rootDirectory = {});
  QString directory() const;
  std::vector<RecoveryCapture> captures() const;
  bool retry(const QString& jobDirectory, QString* error);
  // Copies all assets to an existing directory, or a single asset to a file.
  // Never overwrites and retains the job for explicit discard.
  bool saveElsewhere(const QString& jobDirectory, const QString& destination,
                    QString* error);
  bool discard(const QString& jobDirectory, QString* error);

  // begin() returns the durable capture base, with its lock held until release().
  QString begin(CapturePurpose purpose, const CaptureDestination& destination,
                const TestShot& metadata, QString* error);
  bool markReady(const QString& jobDirectory, const QString& filePath, QString* error);
  // Call only once the backend has stopped writing, including on failure.
  void release(const QString& jobDirectory);

 private:
  bool validJob(const QString& path, QString* error) const;
  std::unique_ptr<QLockFile> lock(const QString& path, QString* error) const;
  QString root_;
  std::map<QString, std::unique_ptr<QLockFile>> active_;
};
