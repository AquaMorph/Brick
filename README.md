# Brick

Brick is a cross-platform stop motion application for Linux, Windows, and
macOS. The current project is the initial application shell: it opens an empty
black window ready for the program's work-mode interface.

## Requirements

- A C++20 compiler
- CMake 3.21 or newer
- Qt 6.5 or newer with the Widgets component

## Build

The same commands work on all supported operating systems from a terminal with
Qt available to CMake:

```sh
cmake -S . -B build -DCMAKE_BUILD_TYPE=Release
cmake --build build --config Release --parallel
```

Run the application from the generated build directory. With a single-config
generator it is `build/Brick` on Linux, `build/Brick.exe` on Windows, or
`build/Brick.app` on macOS. Visual Studio places the Windows executable at
`build/Release/Brick.exe`.

If CMake cannot locate Qt, provide its installation prefix:

```sh
cmake -S . -B build -DCMAKE_PREFIX_PATH=/path/to/Qt/6.x/compiler
```

## Install

To copy the built application into a staging directory:

```sh
cmake --install build --config Release --prefix staging
```

Qt's platform deployment tools (`windeployqt`, `macdeployqt`, or
`qt_generate_deploy_app_script`) will be added when distributable packages are
introduced. The continuous integration workflow compiles the application on
Linux, Windows, and macOS for every change.
