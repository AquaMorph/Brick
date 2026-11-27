# Brick

Brick is a cross-platform stop motion application for Linux, Windows, and macOS.

## Program

R1 - Program will run natively on Linux, macOS, and Windows. The supported Linux package formats are deb, rpm, and AppImage. Windows is distributed as an MSI installer and macOS as a DMG containing an app bundle.
R2 - Program will support capturing images from both webcams and DSLRs.
R3 - Program must never delete captured images without explicit input from the user.
R4 - Program must be written in safe Rust. The only exception is foreign function interface code needed to talk to camera SDKs and platform camera APIs. That code must live in its own crate, expose a safe API, and document the invariants each `unsafe` block relies on.
R5 - The program is open source with the MIT license.
R6 - No GPL/LGPL/AGPL code, not even reading it: `cargo xtask licences --check`
R7 - The program must never export video footage. Exported footage will be frames only.
R8 - All project data must be self contained in the project save directory.
R9 - When the program is opened by default it will open the last project that was opened. If that project no longer exists or cannot be opened, the program shows the project picker (P2) with an error explaining why.
R10 - DSLR support for Canon is done with the EDSDK. The source code for which must not be included in this project. Documentation about how do take a download from Canon and install it into the build should be provided.
R11 - The program must build and run without the EDSDK installed. In that case DSLR support is compiled out and only webcams are available.
R12 - The program makes no network connections and collects no telemetry.
R13 - A crash or power loss must never lose a frame that was shown as captured, and must never leave a project in a state the program cannot open.

### Workflow

W1 - The only supported animation frame rates are 8, 12, 15, 24, and 30 fps.
W2 - The program guides you through different tabs, each designed for a different stage of the project.
W3 - The first tab is the Direct tab. This tab is used to break the film into scenes and manage which take is actively being worked on.
W4 - The second tab is Cinematography which is for adjusting camera settings and taking test photos to play with lighting and camera settings.
W5 - The third tab is Animation which is used for actually animating the take. It has tools like onion skinning and playback controls.
W6 - The fourth tab is Export, which writes frames out of the project (EX1-EX6).
W7 - Exactly one take is active at a time. The Cinematography window and captured test frames is tied to the active shot. Animation tabs always operate on the active take. The active scene and take is shown in the status bar on every tab.

### Projects

P1 - A project is organized as Project > Scene > Shot > Take > Frames.
P2 - The project picker lists recent projects and has actions to create a new project and open an existing one. It is shown on first launch and when R9 cannot open the last project.
P3 - Creating a project asks for a name, a save location, and a frame rate (W1). The default frame rate is 12 fps. The frame rate can be changed later in project settings.
P4 - The project directory contains a single human readable project file (`project.toml`) that holds the scene/shot/take structure, frame order, camera settings, and project settings.
P5 - Captured images are stored under `frames/` with file names that are unique IDs. They are never renamed or modified after capture. Frame order is defined only by the project file.
P6 - Every change is saved automatically. There is no save button. The project file is written atomically (write to a temporary file, then rename) so a crash can never leave a partially written project file.
P7 - A captured image is written and flushed to disk before it appears in the timeline.
P8 - Removing a frame, take, shot, or scene moves its images to a trash folder inside the project. Images are only permanently deleted when the user empties the project trash and confirms (R3).
P9 - Generated data such as thumbnails and playback proxies is stored in `.cache/` inside the project. It can be deleted at any time and is rebuilt on demand.
P10 - Only one instance of the program may have a project open at a time. Opening a project that is already open shows an error instead of opening it twice.
P11 - Application settings that are not project data (recent projects, window layout, last used camera) are stored in the operating system's standard config directory.

### Cameras

C1 - The camera abstraction supports live view, capture, and reading and writing settings. DSLRs and webcams both implement it.
C2 - Webcams are supported on Linux through V4L2, on Windows through Media Foundation, and on macOS through AVFoundation.
C3 - Canon DSLRs are supported through the EDSDK on all three platforms. Captured images are transferred directly to the computer, not saved to the camera's memory card.
C4 - When a DSLR is set to RAW+JPEG, both files are kept. The JPEG is used for display, playback, and export by default. The RAW file is kept alongside it for the user.
C5 - Only one camera is active at a time. The user picks the camera from a list of detected cameras, and the program reconnects to the last used camera automatically when it is available.
C6 - Connecting and disconnecting a camera while the program is running is detected and shown in the status bar. A disconnect must not crash the program or affect frames already captured.
C7 - A failed capture shows an error and leaves the take unchanged. It never adds a blank or partial frame.
C8 - For webcams, auto exposure, auto white balance, and auto focus are turned off by default where the device allows it, to prevent flicker between frames.
C9 - A simulated camera is available in development builds so the program can be developed and tested without hardware.

#### Direct Tab

DT1 - The direct tab supports adding scenes, shots, and takes.
DT2 - Scenes and shots can be renamed and reordered. Takes are numbered automatically within their shot.
DT3 - Scenes, shots, and takes each have an optional free text notes field.
DT4 - Selecting a take and making it active switches the Cinematography and Animation tabs to that take and scene.
DT5 - Each take shows its frame count, its duration at the project frame rate, and a thumbnail of its first frame.
DT6 - One take per shot can be marked as the chosen take. Export uses the chosen take by default (EX2).
DT7 - Removing a scene, shot, or take requires confirmation and follows P8.

#### Cinematography Tab

CT1 - DSLR control settings should be shutter speed, aperture, and ISO.
CT2 - The tab shows a live view of the active camera, filling as much of the available area as possible while keeping the camera's aspect ratio.
CT3 - Live view can be magnified to check focus.
CT4 - Optional overlays include a rule of thirds grid, a center mark, and safe area guides for common aspect ratios.
CT5 - Test shots can be captured from this tab. They are stored in a separate test shot folder for the active shot and are never part of the take's animation.
CT6 - Test shots can be viewed full size and compared side by side with live view.
CT7 - Camera settings are saved per shot. When a take is made active, the saved settings for its shot are applied to the camera, so every take of a shot uses the same exposure.
CT8 - Camera settings can only be changed on this tab. The Animation tab does not show them and cannot change them, so settings are never bumped by accident while animating.

#### Animation Tab

AT1 - Animation preview must always playback smoothly and match the set project frame rate. This is designed so the animator can preview the final animation and it being correct is critical.
AT2 - The enter key should capture a frame.
AT3 - The space key should playback the current animation at the set framerate.
AT4 - The default animation preview should playback a capture of the live preview so it can be used to see what the current frame would look like in the animation without capturing it yet.
AT5 - A high resolution playback mode is selectable with a toggle which will playback the captured frames rather than the live view frames. It will not be a full resolution preview because playback must always match the projects fps.
AT6 - All frames needed for playback are decoded into memory before playback starts. Frame timing uses a monotonic clock, not the UI refresh rate.
AT7 - Playback can loop. Playback can be limited to the last N frames, with N set by the user, so the animator can review only the most recent motion.
AT8 - Onion skinning overlays previous frames on the live view. The number of frames (0-5) and their opacity are adjustable.
AT9 - A timeline shows thumbnails of every frame in the take. Clicking a frame shows it in the viewer. The viewer returns to live view when the user captures or presses Escape.
AT10 - Frames can be removed (P8), reordered, and held. A held frame repeats an existing image and does not copy the image file.
AT11 - Frame edits (remove, reorder, hold) can be undone and redone.
AT12 - Capturing never blocks the UI. Live view keeps running and the new frame appears in the timeline as soon as P7 is satisfied.
AT13 - New frames are inserted where the capture frame is in the timeline. By default this is the end of the timeline unless the capture point is moved.

### Export

EX1 - Export writes a numbered image sequence (for example `frame_00001.jpg`) to a directory chosen by the user outside the project.
EX2 - Export can be run for a single take, a shot, a scene, or the whole film. For shots, scenes, and the film, the chosen take of each shot is used (DT6).
EX3 - Held frames are written as separate files so the sequence plays correctly at the project frame rate in other software.
EX4 - The output format is either the original captured files unchanged or JPEG/PNG at a user selected resolution.
EX5 - Export never changes anything in the project.
EX6 - Export writes a small text file with the sequence describing the project frame rate and frame count.

### Keyboard

K1 - Keyboard shortcuts work on the Animation tab without needing to focus a specific control. Shortcuts are disabled while a text field has focus.
K2 - Default shortcuts on the Animation tab:
- Enter: capture frame (AT2)
- Space: play/stop (AT3)
- Left/Right: step to the previous/next frame
- Home/End: go to the first/last frame
- Escape: return to live view
- Delete: remove the selected frame (P8)
- O: toggle onion skinning
- L: toggle loop
- H: toggle high resolution playback (AT5)
- Ctrl+Z / Ctrl+Shift+Z: undo / redo (Cmd on macOS)

### UI

U1 - The UI design must be that of a professional piece of software. The interface design is flat.
U2 - The only supported theme is black. Color accents should be the color of a brick, dark red.
U3 - The main window has the tab bar at the top and a status bar at the bottom. The status bar shows the active camera and its connection state, the active scene/shot/take, the project frame rate, and the frame count.
U4 - The minimum supported window size is 1280x720. Layouts must scale correctly on high DPI displays.
U5 - Errors are shown inside the program window and never only in a log.

### Build and Release

B1 - Development tasks run through `cargo xtask`, including the licence check (R6), packaging, and EDSDK setup.
B2 - Documentation in `docs/edsdk.md` explains how to download the EDSDK from Canon and where to place it so the build finds it (R10). The EDSDK location is ignored by git.
B3 - Continuous integration builds and tests on Linux, Windows, and macOS, with and without EDSDK support where the SDK is available.
B4 - Logs are written to the operating system's standard log directory to help with bug reports.
