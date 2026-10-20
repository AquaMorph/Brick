# Keep the RAW SDK separate from the camera-control SDK. Canon's Mac RAW
# development libraries are Intel-only even though camera control is universal.
if(NOT APPLE AND NOT WIN32)
  message(FATAL_ERROR "Canon's RAW SDK is supplied for macOS and Windows only.")
endif()
if(NOT EXISTS "${BRICK_CANON_RAW_ROOT}/Header/EDSDK.h")
  message(FATAL_ERROR "Set BRICK_CANON_RAW_ROOT to EDSDK in Canon's RAW-processing package.")
endif()

add_executable(brick_canon_raw src/camera/canon/raw_decoder_main.cpp)
set_target_properties(brick_canon_raw PROPERTIES
  AUTOMOC OFF AUTOUIC OFF AUTORCC OFF
  RUNTIME_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}/canon-raw/$<CONFIG>"
)
target_include_directories(brick_canon_raw PRIVATE "${BRICK_CANON_RAW_ROOT}/Header")
if(APPLE)
  set(raw_frameworks "${BRICK_CANON_RAW_ROOT}/Framework")
  if(NOT EXISTS "${raw_frameworks}/EDSDK.framework/Versions/A/DppCore.bundle")
    message(FATAL_ERROR "Canon RAW development requires the RAW SDK's DppCore.bundle.")
  endif()
  set_target_properties(brick_canon_raw PROPERTIES
    OSX_ARCHITECTURES x86_64
    BUILD_RPATH "@executable_path"
    INSTALL_RPATH "@executable_path"
    BUILD_WITH_INSTALL_RPATH TRUE
  )
  target_compile_definitions(brick_canon_raw PRIVATE __MACOS__=1)
  target_link_libraries(brick_canon_raw PRIVATE
    "${raw_frameworks}/EDSDK.framework")
  add_custom_command(TARGET brick_canon_raw POST_BUILD
    COMMAND /usr/bin/ditto "${raw_frameworks}/EDSDK.framework"
            "$<TARGET_FILE_DIR:brick_canon_raw>/EDSDK.framework"
    VERBATIM)
  option(BRICK_CANON_RAW_ADHOC_SIGN
         "Clear quarantine and ad-hoc-sign only the build-local RAW helper and SDK copy" OFF)
  if(BRICK_CANON_RAW_ADHOC_SIGN)
    add_custom_command(TARGET brick_canon_raw POST_BUILD
      COMMAND /usr/bin/xattr -cr "$<TARGET_FILE_DIR:brick_canon_raw>/EDSDK.framework"
      COMMAND /usr/bin/codesign --force --deep --sign -
              "$<TARGET_FILE_DIR:brick_canon_raw>/EDSDK.framework"
      COMMAND /usr/bin/codesign --force --sign - "$<TARGET_FILE:brick_canon_raw>"
      VERBATIM)
  endif()
  add_custom_command(TARGET brick POST_BUILD
    COMMAND /usr/bin/ditto "$<TARGET_FILE_DIR:brick_canon_raw>"
            "$<TARGET_BUNDLE_DIR:brick>/Contents/Helpers/CanonRaw"
    VERBATIM)
  install(DIRECTORY "$<TARGET_FILE_DIR:brick_canon_raw>/"
          DESTINATION "Brick.app/Contents/Helpers/CanonRaw" USE_SOURCE_PERMISSIONS)
else()
  if(CMAKE_SIZEOF_VOID_P EQUAL 8)
    set(raw_library "${BRICK_CANON_RAW_ROOT}/../EDSDK_64/Library/EDSDK.lib")
    set(raw_runtime "${BRICK_CANON_RAW_ROOT}/../EDSDK_64/Dll")
  else()
    set(raw_library "${BRICK_CANON_RAW_ROOT}/Library/EDSDK.lib")
    set(raw_runtime "${BRICK_CANON_RAW_ROOT}/Dll")
  endif()
  target_link_libraries(brick_canon_raw PRIVATE "${raw_library}")
  add_custom_command(TARGET brick_canon_raw POST_BUILD
    COMMAND ${CMAKE_COMMAND} -E copy_directory "${raw_runtime}"
            "$<TARGET_FILE_DIR:brick_canon_raw>"
    VERBATIM)
  add_custom_command(TARGET brick POST_BUILD
    COMMAND ${CMAKE_COMMAND} -E copy_directory "$<TARGET_FILE_DIR:brick_canon_raw>"
            "$<TARGET_FILE_DIR:brick>/canon-raw"
    VERBATIM)
  install(DIRECTORY "$<TARGET_FILE_DIR:brick_canon_raw>/" DESTINATION bin/canon-raw)
endif()

add_dependencies(brick brick_canon_raw)
set_property(TARGET brick APPEND PROPERTY LINK_DEPENDS "$<TARGET_FILE:brick_canon_raw>")
add_dependencies(brick_core brick_canon_raw)
target_compile_definitions(brick_core PRIVATE
  BRICK_CANON_RAW_HELPER="$<TARGET_FILE:brick_canon_raw>")
