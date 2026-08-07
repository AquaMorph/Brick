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

find_package(Qt6 6.5 REQUIRED COMPONENTS Concurrent Multimedia Widgets)

option(BRICK_ENABLE_CANON_EDSDK
       "Build Canon DSLR support using a locally supplied Canon EDSDK" OFF)
set(BRICK_EDSDK_ROOT "${CMAKE_SOURCE_DIR}/third_party/canon/EDSDK" CACHE PATH
    "Path to the extracted Canon EDSDK directory")

qt_standard_project_setup()

qt_add_executable(brick
  WIN32
  MACOSX_BUNDLE
  src/main.cpp
  src/main_window.cpp
  src/main_window.h
  src/animation_widget.cpp
  src/animation_widget.h
  src/frame_rate.h
  src/playback_timing.h
  src/layout_constants.h
  src/camera.cpp
  src/camera.h
  src/cinematography_widget.cpp
  src/cinematography_widget.h
  src/project.cpp
  src/project.h
)

target_link_libraries(brick PRIVATE Qt6::Concurrent Qt6::Multimedia Qt6::Widgets)

if(BRICK_ENABLE_CANON_EDSDK)
  if(WIN32)
    set(BRICK_EDSDK_INCLUDE_DIR "${BRICK_EDSDK_ROOT}/Header")
    if(CMAKE_SIZEOF_VOID_P EQUAL 8)
      set(BRICK_EDSDK_LIBRARY "${BRICK_EDSDK_ROOT}/../EDSDK_64/Library/EDSDK.lib")
      set(BRICK_EDSDK_RUNTIME_DIR "${BRICK_EDSDK_ROOT}/../EDSDK_64/Dll")
    else()
      set(BRICK_EDSDK_LIBRARY "${BRICK_EDSDK_ROOT}/Library/EDSDK.lib")
      set(BRICK_EDSDK_RUNTIME_DIR "${BRICK_EDSDK_ROOT}/Dll")
    endif()
  elseif(APPLE)
    set(BRICK_EDSDK_INCLUDE_DIR "${BRICK_EDSDK_ROOT}/Headers")
    set(BRICK_EDSDK_LIBRARY "${BRICK_EDSDK_ROOT}/EDSDK")
  elseif(CMAKE_SYSTEM_PROCESSOR MATCHES "^(aarch64|arm64)$")
    set(BRICK_EDSDK_INCLUDE_DIR "${BRICK_EDSDK_ROOT}/Header")
    set(BRICK_EDSDK_LIBRARY "${BRICK_EDSDK_ROOT}/Library/ARM64/libEDSDK.so")
  elseif(CMAKE_SYSTEM_PROCESSOR MATCHES "^(arm|armv7)")
    set(BRICK_EDSDK_INCLUDE_DIR "${BRICK_EDSDK_ROOT}/Header")
    set(BRICK_EDSDK_LIBRARY "${BRICK_EDSDK_ROOT}/Library/ARM32/libEDSDK.so")
  else()
    set(BRICK_EDSDK_INCLUDE_DIR "${BRICK_EDSDK_ROOT}/Header")
    set(BRICK_EDSDK_LIBRARY "${BRICK_EDSDK_ROOT}/Library/x86_64/libEDSDK.so")
  endif()

  if(NOT EXISTS "${BRICK_EDSDK_INCLUDE_DIR}/EDSDK.h")
    message(FATAL_ERROR
      "Canon EDSDK headers were not found. Extract your licensed SDK and set "
      "BRICK_EDSDK_ROOT to its EDSDK directory (or EDSDK.framework on macOS).")
  endif()
  if(NOT BRICK_EDSDK_LIBRARY OR NOT EXISTS "${BRICK_EDSDK_LIBRARY}")
    message(FATAL_ERROR "A Canon EDSDK library was not found for this platform.")
  endif()
  target_sources(brick PRIVATE src/canon_camera.cpp src/canon_camera.h)
  target_include_directories(brick PRIVATE "${BRICK_EDSDK_INCLUDE_DIR}")
  target_link_libraries(brick PRIVATE "${BRICK_EDSDK_LIBRARY}")
  target_compile_definitions(brick PRIVATE BRICK_HAS_CANON_EDSDK)
  if(UNIX AND NOT APPLE)
    target_compile_definitions(brick PRIVATE TARGET_OS_LINUX=1)
  elseif(APPLE)
    target_compile_definitions(brick PRIVATE __MACOS__=1)
  endif()
  if(WIN32)
    add_custom_command(TARGET brick POST_BUILD
      COMMAND ${CMAKE_COMMAND} -E copy_if_different
              "${BRICK_EDSDK_RUNTIME_DIR}/EDSDK.dll"
              "${BRICK_EDSDK_RUNTIME_DIR}/EdsImage.dll"
              "$<TARGET_FILE_DIR:brick>"
      VERBATIM)
  endif()
endif()

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

include(CTest)

if(BUILD_TESTING)
  add_executable(project_tests
    tests/project_test.cpp
    src/project.cpp
    src/project.h
    src/frame_rate.h
    src/playback_timing.h
  )
  target_include_directories(project_tests PRIVATE src)
  target_link_libraries(project_tests PRIVATE Qt6::Core Qt6::Gui)
  add_test(NAME project_tests COMMAND project_tests)
endif()
