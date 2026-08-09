#include "core/project.h"

#include "storage/project_storage_internal.h"

#include <utility>


Project::Project(QString name, QString directory, std::vector<QString> scenes,
                 std::vector<std::vector<QString>> shots,
                 std::vector<std::vector<int>> takeCounts,
                 std::optional<ActiveTake> activeTake)
    : name_(std::move(name)),
      directory_(std::move(directory)),
      scenes_(std::move(scenes)),
      shots_(std::move(shots)),
      takeCounts_(std::move(takeCounts)),
      activeTake_(activeTake) {}


const QString& Project::name() const { return name_; }


const QString& Project::directory() const { return directory_; }


const std::vector<QString>& Project::scenes() const { return scenes_; }


const std::vector<QString>& Project::shots(int sceneIndex) const {
  return shots_.at(sceneIndex);
}


int Project::takeCount(int sceneIndex, int shotIndex) const {
  return takeCounts_.at(sceneIndex).at(shotIndex);
}


const std::optional<Project::ActiveTake>& Project::activeTake() const {
  return activeTake_;
}


bool Project::selectTake(int sceneIndex, int shotIndex, int takeIndex,
                         QString* error) {
  if (sceneIndex < 0 || sceneIndex >= static_cast<int>(scenes_.size()) ||
      shotIndex < 0 || shotIndex >= static_cast<int>(shots_[sceneIndex].size()) ||
      takeIndex < 0 || takeIndex >= takeCounts_[sceneIndex][shotIndex]) {
    project_storage::setError(error, "The selected take does not exist.");
    return false;
  }

  const auto previous = activeTake_;
  activeTake_ = ActiveTake{sceneIndex, shotIndex, takeIndex};
  if (!saveActiveTake(error)) {
    activeTake_ = previous;
    return false;
  }
  return true;
}
