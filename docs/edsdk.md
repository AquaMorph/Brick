# Canon EDSDK

Brick talks to Canon DSLRs through Canon's EOS Digital SDK (EDSDK) (R10, C3). Canon does not allow the SDK to be redistributed, so it is not part of this repository. Brick builds and runs without it; DSLR support is compiled out and only webcams are available (R11).

## Download

1. Register with the Canon developer program for your region (for example the Canon USA Developer Community at <https://developercommunity.usa.canon.com>) and request access to the EDSDK.
2. Download the EDSDK archive for each platform you build for. Each platform is a separate zip:
   - `EDSDK_v<version>_Linux.zip`
   - `EDSDK_v<version>_Windows.zip`
   - `EDSDK_v<version>_Macintosh.zip`

## Install

Put the archives in the root of the repository and run:

```sh
cargo xtask edsdk
```

Archives somewhere else can be passed directly:

```sh
cargo xtask edsdk ~/Downloads/EDSDK_v13.20.21_Linux.zip
```

The task detects which platform each archive is for and unpacks only the headers and libraries into `edsdk/`. Running it again replaces the installed SDK for that platform, which is how to upgrade. The archives themselves are left alone.

Unpacking the macOS SDK needs `hdiutil` on macOS, or [7-Zip](https://www.7-zip.org) (`7z` on the `PATH`) on Linux and Windows, because Canon ships it as a disk image.

Both `edsdk/` and the `EDSDK*.zip` downloads are ignored by git (B2). Never commit them.

## Layout

Directory names match Cargo's `target_os` and `target_arch` values, so a build script finds the SDK for the target it builds at `edsdk/<target_os>/`, with libraries under `<target_arch>/`:

```
edsdk/
├── readme.txt                 Canon's notice, see Distribution
├── linux/
│   ├── VERSION
│   ├── include/               EDSDK.h, EDSDKErrors.h, EDSDKTypes.h
│   ├── x86_64/libEDSDK.so
│   ├── aarch64/libEDSDK.so
│   └── arm/libEDSDK.so
├── windows/
│   ├── VERSION
│   ├── include/
│   ├── x86_64/                EDSDK.dll, EdsImage.dll, EDSDK.lib
│   └── x86/                   EDSDK.dll, EdsImage.dll, EDSDK.lib
└── macos/
    ├── VERSION
    ├── include/
    └── EDSDK.framework/       universal (x86_64 and arm64)
```

Headers are kept per platform because Canon's copies differ slightly between platforms.

## Distribution

The EDSDK includes code from the Independent JPEG Group. Packages that ship the EDSDK libraries must state that "this software is based in part on the work of the Independent JPEG Group", as described in `edsdk/readme.txt`.
