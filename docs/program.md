# Brick

Brick is a cross-platform stop motion application for Linux, Windows, and macOS.

## Program

R1 - Program will run natively on Linux, macOS, and Windows. The supported Linux package formats are deb, rpm, and AppImage.
R2 - Program will support capturing images from both webcams and DSLRs.
R3 - Program must never delete captured images without explicit input from the user.
R4 - Program must be written in safe Rust.
R5 - The program is open source with the MIT license.
R6 - No GPL/LGPL/AGPL code, not even reading it: `cargo xtask licences --check`
R7 - The program must never export video footage. Exported footage will be frames only.
R8 - All project data must be self contained in the project save directory.
R9 - When the program is opened by default it will open the last project that was opened.
R10 - DSLR support for Canon is done with the EDSDK. The source code for which must not be included in this project. Documentation about how do take a download from Canon and install it into the build should be provided.

### Workflow

W1 - The only supported animation frame rates are 8, 12, 15, 24, and 30 fps.
W2 - The program works you through different tabs that are specifically designed for different sets of the project.
W3 - The first tab is the Direct tab. This tab is used to break apart of the film into scenes and manage what take is actively being worked on.
W4 - The second tab is Cinematography which is for adjusting camera settings and taking test photos to play with lighting and camera settings.
W5 - The third tab is Animation which is used for actually animating the take. It has tools like onion screening and playback controls.

#### Direct Tab

DT1 - The direct tab supports adding scenes, shots, and takes. 

#### Cinematography Tab

CT1 - DSLR control settings should be shutter speed, aperature, and ISO.

#### Animation Tab

AT1 - Animation preview must always playback smoothly and match the set project frame rate. This is designed to the animator can preview the final animation and it being correct is critical.
AT2 - The enter key should capture a frame.
AT3 - The space key should playback the current animation at the set framerate.
AT4 - The default animation preview should playback a capture of the live preview so it can be used to see what the current frame would look like in the animation without capturing it yet.
AT5 - A high resolution playback mode is selectable with a toggle which will playback the captured frames rather than the live view frames. It will not be a full resolution preview because playback must always match the projects fps.

### UI

U1 - The UI design must be that of a professional piece of software. The interface design is flat.
U2 - The only supported theme is black. Color accents should be the color of a brick, dark red.
