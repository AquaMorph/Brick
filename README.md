# Brick

Brick is a cross-platform stop motion application for Linux, Windows, and
macOS. The current project is the initial application shell: it opens an empty
black window ready for the program's work-mode interface.

## Requirements

- A C++20 compiler
- CMake 3.21 or newer
- Qt 6.5 or newer with the Multimedia and Widgets components

## Build

The same commands work on all supported operating systems from a terminal with
Qt available to CMake:

```sh
cmake -S . -B build -DCMAKE_BUILD_TYPE=Release
cmake --build build --config Release --parallel
```

Webcams use Qt Multimedia and require no vendor SDK. Canon DSLR support is an
optional local build because Canon distributes EDSDK under its own license.
Brick does not redistribute the SDK, its headers, libraries, archives, or
sample code. After obtaining EDSDK directly from Canon, extract the Linux or
Windows package so that `third_party/canon/EDSDK/Header/EDSDK.h` exists, then
configure with:

```sh
cmake -S . -B build -DBRICK_ENABLE_CANON_EDSDK=ON
```

Alternatively set `BRICK_EDSDK_ROOT` to the extracted `EDSDK` directory. On
macOS, set it to the `EDSDK.framework` directory obtained from Canon's disk
image. SDK archives and `third_party/canon` are ignored by Git. Anyone distributing a
Canon-enabled binary must independently confirm that their Canon agreement
permits distributing the EDSDK runtime and must include Canon's required
notices. The EDSDK readme also requires executable documentation to state:
"this software is based in part on the work of the Independent JPEG Group".

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
