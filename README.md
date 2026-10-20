# Brick

Brick is a cross-platform stop motion application for Linux, Windows, and
macOS. The current project is the initial application shell: it opens an empty
black window ready for the program's work-mode interface.

## Capture Recovery

Captures are downloaded into persistent `capture-recovery` storage under Qt's
application-local data directory before being imported into a project. Each job
records its original project, shot, and take identity, capture time, and camera
metadata. Changing the displayed project or selection does not redirect a capture.
Renames and reordering preserve destination identity; deleting and recreating a
destination does not.

Use **File > Recover Captures** to retry import, save assets to another folder, or
explicitly discard them. The dialog also opens on startup when recovery jobs remain.
Saving elsewhere never overwrites files and retains recovery originals until you
discard them. Interrupted downloads remain available for saving, but may be incomplete.
If an import was interrupted or its rollback could not be verified, retry is disabled
to avoid duplicates; inspect the project and save or discard the retained assets.
This protects against application exit and import failures, not all power-loss or
filesystem failures.

Project, shot, and take configs now require persistent UUID identity fields. Projects
created before these fields were introduced are not automatically migrated.

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

### Canon RAW capture

RAW-only CR3/CR2 captures require Canon's separate **EDSDK for RAW Develop**
package. No third-party RAW decoder is used. Extract this licensed package
separately from the camera-control SDK, then configure:

```sh
cmake -S . -B build -DBRICK_ENABLE_CANON_RAW=ON \
  -DBRICK_CANON_RAW_ROOT=/path/to/raw-package/EDSDK
cmake --build build --config Release --parallel
```

On macOS, `BRICK_CANON_RAW_ROOT` contains `Header` and `Framework`; extract
these from the package's disk image. On Windows it contains `Header` and
`Library`, with `EDSDK_64` alongside it. All decoder runtime files are copied
into a private helper directory, not into system folders. The original SDKs
remain unmodified and must not be committed to Git.

Canon RAW Develop 13.20.10 is Intel-only on macOS. Brick remains native on Apple
Silicon and runs the isolated decoder through Rosetta, which must already be
available. If macOS rejects the extracted SDK's signature, the opt-in
`-DBRICK_CANON_RAW_ADHOC_SIGN=ON` clears quarantine and applies development
signatures **only to the build-local helper and SDK copy**. This does not change
system security settings or the original SDK. Distribution requires appropriate
signing and compliance with Canon's licensing terms.

Brick develops full-resolution RGB previews using the RAW's recorded settings,
while preserving the original RAW in `frames/RAW`. Cinematography test images
keep their original RAW format and use the same decoder for display. Existing
failed RAW imports can be retried using **File > Recover Captures**.

To run the optional real-RAW regression test, set
`-DBRICK_TEST_CANON_RAW_FILE=/path/to/sample.CR3`, build, then run
`ctest --test-dir build -C Release --output-on-failure`. The test uses temporary
project storage and verifies that RAW import preserves the source bytes.

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

## License

Brick is licensed under the [MIT License](LICENSE). Canon EDSDK is not included
and remains subject to Canon's own license terms.
