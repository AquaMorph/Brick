# Brick

Brick is a cross-platform stop motion application for Linux, Windows, and macOS.

## Program

- **R1** — Program will run natively on Linux, macOS, and Windows. The supported Linux package formats are deb, rpm, and AppImage. Windows is distributed as an MSI installer and macOS as a DMG containing an app bundle.
- **R2** — Program will support capturing images from both webcams and DSLRs.
- **R3** — Program must never delete captured images without explicit input from the user.
- **R4** — Program must be written in safe Rust. The only exception is foreign function interface code needed to talk to camera SDKs and platform camera APIs. That code must live in its own crate, expose a safe API, and document the invariants each `unsafe` block relies on.
- **R5** — The program is open source with the MIT license.
- **R6** — No GPL/LGPL/AGPL code, not even reading it: `cargo xtask licences --check`
- **R7** — The program must never export video footage. Exported footage will be frames only.
- **R8** — All project data must be self contained in the project save directory.
- **R9** — When the program is opened by default it will open the last project that was opened. If that project no longer exists or cannot be opened, the program shows the project picker (P2) with an error explaining why.
- **R10** — DSLR support for Canon is done with the EDSDK. The source code for which must not be included in this project. Documentation about how do take a download from Canon and install it into the build should be provided.
- **R11** — The program must build and run without the EDSDK installed. In that case DSLR support is compiled out and only webcams are available.
- **R12** — The program makes no network connections and collects no telemetry.
- **R13** — A crash or power loss must never lose a frame that was shown as captured, and must never leave a project in a state the program cannot open.

## Workflow

- **W1** — The only supported animation frame rates are 8, 12, 15, 24, and 30 fps.
- **W2** — The program guides you through different tabs, each designed for a different stage of the project.
- **W3** — The first tab is the Direct tab. This tab is used to break the film into scenes and manage which take is actively being worked on.
- **W4** — The second tab is Cinematography which is for adjusting camera settings and taking test photos to play with lighting and camera settings.
- **W5** — The third tab is Animation which is used for actually animating the take. It has tools like onion skinning and playback controls.
- **W6** — The fourth tab is Export, which writes frames out of the project (EX1-EX6).
- **W7** — Exactly one take is active at a time. The Cinematography window and captured test frames is tied to the active shot. Animation tabs always operate on the active take. The active scene and take is shown in the status bar on every tab.

## Projects

- **P1** — A project is organized as Project > Scene > Shot > Take > Frames.
- **P2** — The project picker lists recent projects and has actions to create a new project and open an existing one. It is shown on first launch, when R9 cannot open the last project, and after Close Project (F8).
- **P3** — Creating a project asks for a name, a save location, and a frame rate (W1). The default frame rate is 12 fps. The frame rate can be changed later in project settings.
- **P4** — The project directory is laid out as follows. Each level of the hierarchy (P1) is a directory containing a human readable config file describing that level.

  ```
  Project_Name/
  ├── project.conf
  └── 0001_SCENE_ONE/
      ├── scene.conf
      └── 0001_SHOT_ONE/
          ├── shot.conf
          ├── audio/
          │   └── scene.wav
          ├── tests/
          │   ├── 000001.jpg
          │   └── ...
          └── 0001_TAKE/
              ├── take.conf
              └── frames/
                  ├── preview/
                  │   ├── high_res/
                  │   │   ├── 000001.jpg
                  │   │   └── ...
                  │   └── low_res/
                  │       ├── 000001.jpg
                  │       └── ...
                  └── RAW/
                      ├── 000001.cr3
                      └── ...
  ```

- **P5** — Captured images are named with a six digit number (`000001`, `000002`, ...) assigned in capture order within their folder. A number is never reused, even after the image is moved to the trash (P8). Image files are never renamed or modified after capture. All files from one capture share the same number across `high_res/`, `low_res/`, and `RAW/`. Frame order and holds are defined only by `take.conf`, not by file numbers.
- **P6** — Every change is saved automatically. There is no save button. Config files are written atomically (write to a temporary file, then rename) so a crash can never leave a partially written config file.
- **P7** — A captured image is written and flushed to disk before it appears in the timeline.
- **P8** — Removing a frame, take, shot, or scene moves its images to a `.trash/` folder at the root of the project. Images are only permanently deleted when the user empties the project trash and confirms (R3).
- **P9** — Generated data such as thumbnails and playback proxies is stored in `.cache/` at the root of the project. It can be deleted at any time and is rebuilt on demand.
- **P10** — Only one instance of the program may have a project open at a time. Opening a project that is already open shows an error instead of opening it twice.
- **P11** — Application settings that are not project data (recent projects, window layout, last used camera) are stored in the operating system's standard config directory.
- **P12** — Config files are TOML. `project.conf` holds project settings such as the frame rate (P3). `scene.conf` holds the scene name and notes. `shot.conf` holds the shot name, notes, camera settings (CT7), chosen take (DT6), and the next take and test shot numbers so numbers are never reused (P5, DT2). `take.conf` holds the frame order, holds (AT10), and notes.
- **P13** — Scene and shot directories are named `NNNN_NAME`, where `NNNN` is the four digit position starting at `0001` and `NAME` is the user given name in upper case with spaces and characters that are not letters or digits replaced by underscores. Renaming or reordering a scene or shot renames its directory. Take directories are named `NNNN_TAKE`, where `NNNN` is the take number (DT2).
- **P14** — `preview/high_res/` holds the full resolution JPEG of each capture. `preview/low_res/` holds the live view frame grabbed at the moment of capture, used by the default animation preview (AT4). `RAW/` holds the camera RAW file when the camera captures RAW (C4) and keeps the camera's extension (for example `.cr3`).
- **P15** — Test shots (CT5) are stored in the shot's `tests/` folder, numbered the same way as frames (P5).
- **P16** — Audio for a shot is stored in the shot's `audio/` folder.

## File Menu

- **F1** — The main window has a File menu. On macOS it lives in the system menu bar; on Linux and Windows it sits in a menu bar above the tab bar (U3). It is available on every tab.
- **F2** — The File menu contains, in order: New Project, Open Project, Open Recent, Save Project As, Close Project, and Quit.
- **F3** — New Project shows the create project dialog (P3). On success the new project is opened.
- **F4** — Open Project shows the operating system's folder picker. The chosen folder must contain a `project.conf`; otherwise an error explains why it cannot be opened (U5). Opening a project that is already open in another instance follows P10.
- **F5** — Open Recent is a submenu listing the recent projects (P11), most recent first, showing each project's name and path. Projects that no longer exist are shown disabled. The submenu ends with an action to clear the list. Clearing the list never touches project data.
- **F6** — There is no Save action, because every change is saved automatically (P6). Save Project As asks for a name and location and copies the whole project directory there, including `.trash/` but excluding `.cache/` (P9). The copy is then opened. The original project is left unchanged.
- **F7** — Save Project As writes to a temporary directory next to the destination and renames it into place when the copy is complete, so a failed or interrupted copy never leaves a partial project at the destination (R13). A copy into a location that already exists is refused.
- **F8** — Close Project closes the current project and shows the project picker (P2).
- **F9** — Opening, creating, or closing a project while one is open first closes the current project. No save prompt is shown. Any capture in progress finishes and is written to disk (P7) before the project closes, and playback stops.
- **F10** — Default shortcuts (Cmd instead of Ctrl on macOS): <kbd>Ctrl</kbd>+<kbd>N</kbd> new project, <kbd>Ctrl</kbd>+<kbd>O</kbd> open project, <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>S</kbd> save project as, <kbd>Ctrl</kbd>+<kbd>W</kbd> close project, <kbd>Ctrl</kbd>+<kbd>Q</kbd> quit. These work on every tab, including while a text field has focus.

## Cameras

- **C1** — The camera abstraction supports live view, capture, and reading and writing settings. DSLRs and webcams both implement it.
- **C2** — Webcams are supported on Linux through V4L2, on Windows through Media Foundation, and on macOS through AVFoundation.
- **C3** — Canon DSLRs are supported through the EDSDK on all three platforms. Captured images are transferred directly to the computer, not saved to the camera's memory card.
- **C4** — When a DSLR is set to RAW+JPEG, both files are kept. The JPEG is stored in `preview/high_res/` and used for display, high resolution playback, and export by default. The RAW file is stored in `RAW/` for the user (P14).
- **C5** — Only one camera is active at a time. The user picks the camera from a list of detected cameras, and the program reconnects to the last used camera automatically when it is available.
- **C6** — Connecting and disconnecting a camera while the program is running is detected and shown in the status bar. A disconnect must not crash the program or affect frames already captured.
- **C7** — A failed capture shows an error and leaves the take unchanged. It never adds a blank or partial frame.
- **C8** — For webcams, auto exposure, auto white balance, and auto focus are turned off by default where the device allows it, to prevent flicker between frames.
- **C9** — A simulated camera is available in development builds so the program can be developed and tested without hardware.
- **C10** — Cameras capture at their native (largest) resolution by default. The Cinematography tab offers a drop down of the lower resolutions the camera supports. The resolution is saved per shot with the other camera settings (CT7).

## Direct Tab

- **DT1** — The direct tab supports adding scenes, shots, and takes.
- **DT2** — Scenes and shots can be renamed and reordered. Takes are numbered automatically within their shot.
- **DT3** — Scenes, shots, and takes each have an optional free text notes field.
- **DT4** — Selecting a take and making it active switches the Cinematography and Animation tabs to that take and scene.
- **DT5** — Each take shows its frame count, its duration at the project frame rate, and a thumbnail of its first frame.
- **DT6** — One take per shot can be marked as the chosen take. Export uses the chosen take by default (EX2).
- **DT7** — Removing a scene, shot, or take requires confirmation and follows P8.

## Cinematography Tab

- **CT1** — DSLR control settings should be resolution (C10), shutter speed, aperture, and ISO. Webcam settings are resolution (C10), exposure time (shown as a shutter speed), gain, white balance, and focus, where the device supports them. A webcam whose manual exposure only gives black frames keeps auto exposure and does not offer exposure time (C8).
- **CT2** — The tab shows a live view of the active camera, filling as much of the available area as possible while keeping the camera's aspect ratio. Live view always shows the newest frame from the camera; older frames are dropped so it never falls behind.
- **CT3** — Live view can be magnified to check focus.
- **CT4** — Optional overlays include a rule of thirds grid, a center mark, and safe area guides for common aspect ratios.
- **CT5** — Test shots can be captured from this tab. They are stored in the active shot's `tests/` folder (P15) and are never part of the take's animation.
- **CT6** — Test shots can be viewed full size and compared side by side with live view.
- **CT7** — Camera settings are saved per shot. When a take is made active, the saved settings for its shot are applied to the camera, so every take of a shot uses the same exposure. Settings a shot has not saved yet are taken from the camera and saved when the shot is first used with a camera.
- **CT8** — Camera settings can only be changed on this tab. The Animation tab does not show them and cannot change them, so settings are never bumped by accident while animating.
- **CT9** — Shutter speed, aperture, and ISO are shown as rulers stacked on one scale of stops, sliding under a fixed center line that marks the current values. A third of a stop is the same width on every ruler, so their marks line up. Aperture gets brighter to the right and shutter speed and ISO to the left, so neighbouring rulers traded against each other slide the same way. The part of a ruler past the setting's last value is shaded. Dragging a ruler or clicking a value on it changes the setting. Above the rulers the current values are shown, each opening a list of every value, beside the exposure in EV compared with the shot's exposure when it was first shown with the camera. Clicking the EV makes the current exposure 0 EV.
- **CT10** — One of the three settings can be locked, ISO by default. A locked setting cannot change, and moving either of the other two moves the other one the opposite way so the exposure stays the same. Values the other setting cannot make up for are skipped, and a drag stops as soon as either of the two runs out of values, even if the other could go further. With nothing locked, each setting changes the exposure on its own. Dragging the gap between two rulers trades those two the same way, whether or not a setting is locked, as long as neither of them is the locked one.

## Animation Tab

- **AT1** — Animation preview must always playback smoothly and match the set project frame rate. This is designed so the animator can preview the final animation and it being correct is critical.
- **AT2** — The enter key should capture a frame.
- **AT3** — The space key should playback the current animation at the set framerate.
- **AT4** — The default animation preview should playback a capture of the live preview so it can be used to see what the current frame would look like in the animation without capturing it yet.
- **AT5** — A high resolution playback mode is selectable with a toggle which will playback the captured frames rather than the live view frames. It will not be a full resolution preview because playback must always match the projects fps.
- **AT6** — All frames needed for playback are decoded into memory before playback starts. Frame timing uses a monotonic clock, not the UI refresh rate.
- **AT7** — Playback can loop. Playback can be limited to the last N frames, with N set by the user, so the animator can review only the most recent motion.
- **AT8** — Onion skinning overlays previous frames on the live view. The number of frames (0-5) and their opacity are adjustable.
- **AT9** — A timeline shows thumbnails of every frame in the take. Clicking a frame shows it in the viewer. The viewer returns to live view when the user captures or presses Escape.
- **AT10** — Frames can be removed (P8), reordered, and held. A held frame repeats an existing image and does not copy the image file.
- **AT11** — Frame edits (remove, reorder, hold) can be undone and redone.
- **AT12** — Capturing never blocks the UI. Live view keeps running and the new frame appears in the timeline as soon as P7 is satisfied.
- **AT13** — New frames are inserted where the capture frame is in the timeline. By default this is the end of the timeline unless the capture point is moved.

## Export

- **EX1** — Export writes a numbered image sequence (for example `frame_00001.jpg`) to a directory chosen by the user outside the project.
- **EX2** — Export can be run for a single take, a shot, a scene, or the whole film. For shots, scenes, and the film, the chosen take of each shot is used (DT6).
- **EX3** — Held frames are written as separate files so the sequence plays correctly at the project frame rate in other software.
- **EX4** — The output format is either the original captured files unchanged or JPEG/PNG at a user selected resolution.
- **EX5** — Export never changes anything in the project.
- **EX6** — Export writes a small text file with the sequence describing the project frame rate and frame count.

## Keyboard

- **K1** — Keyboard shortcuts work on the Animation tab without needing to focus a specific control. Shortcuts are disabled while a text field has focus.
- **K2** — Default shortcuts on the Animation tab:
  - <kbd>Enter</kbd>: capture frame (AT2)
  - <kbd>Space</kbd>: play/stop (AT3)
  - <kbd>Left</kbd>/<kbd>Right</kbd>: step to the previous/next frame
  - <kbd>Home</kbd>/<kbd>End</kbd>: go to the first/last frame
  - <kbd>Escape</kbd>: return to live view
  - <kbd>Delete</kbd>: remove the selected frame (P8)
  - <kbd>O</kbd>: toggle onion skinning
  - <kbd>L</kbd>: toggle loop
  - <kbd>H</kbd>: toggle high resolution playback (AT5)
  - <kbd>Ctrl</kbd>+<kbd>Z</kbd> / <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Z</kbd>: undo / redo (Cmd on macOS)

## UI

- **U1** — The UI design must be that of a professional piece of software. The interface design is flat.
- **U2** — The only supported theme is black. Color accents should be the color of a brick, dark red.
- **U3** — The main window has the File menu (F1) and tab bar at the top and a status bar at the bottom. The status bar shows the active camera and its connection state, the active scene/shot/take, the project frame rate, and the frame count.
- **U4** — The minimum supported window size is 1280x720. Layouts must scale correctly on high DPI displays.
- **U5** — Errors are shown inside the program window and never only in a log.

## Build and Release

- **B1** — Development tasks run through `cargo xtask`, including the licence check (R6), packaging, and EDSDK setup.
- **B2** — Documentation in `docs/edsdk.md` explains how to download the EDSDK from Canon and where to place it so the build finds it (R10). The EDSDK location is ignored by git.
- **B3** — Continuous integration builds and tests on Linux, Windows, and macOS, with and without EDSDK support where the SDK is available.
- **B4** — Logs are written to the operating system's standard log directory to help with bug reports.
