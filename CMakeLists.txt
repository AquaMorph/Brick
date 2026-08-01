cmake_minimum_required(VERSION 3.21)

project(
  Brick
  VERSION 0.1.0
  DESCRIPTION "Cross-platform stop motion software"
  LANGUAGES CXX
)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

find_package(Qt6 6.5 REQUIRED COMPONENTS Widgets)

qt_standard_project_setup()

qt_add_executable(brick
  WIN32
  MACOSX_BUNDLE
  src/main.cpp
)

target_link_libraries(brick PRIVATE Qt6::Widgets)

set_target_properties(brick PROPERTIES
  MACOSX_BUNDLE_BUNDLE_NAME "Brick"
  MACOSX_BUNDLE_BUNDLE_VERSION "${PROJECT_VERSION}"
  MACOSX_BUNDLE_GUI_IDENTIFIER "org.brickanimation.brick"
  MACOSX_BUNDLE_SHORT_VERSION_STRING "${PROJECT_VERSION}"
  OUTPUT_NAME "Brick"
)

install(TARGETS brick
  BUNDLE DESTINATION .
  RUNTIME DESTINATION bin
)
