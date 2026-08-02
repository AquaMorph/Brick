# Brick

Brick is an open source stop motion program. It runs on Linux, Windows, and MacOS. The supported package types for Linux are deb and rpm. The program is primarily designed to support DSLRs, specifically those made by Canon. But webcams are supported as well.

## Code Style

Follow the [C++ Core Guidelines](https://isocpp.github.io/CppCoreGuidelines/CppCoreGuidelines).

### Formatting

- Use 2-space indentation (not tabs)
- Line length: Keep to reasonable length (~80-120 chars) where practical
- Blank lines: 2 between top-level definitions, 1 between method definitions

## Code Structure

TODO

## Commit Guidelines

Read `.agents/COMMITS.md` for documentation on how to commit.

## Program Structure

## Film File System Structure

The current stop motion program is saved in a directory that named with project name. Each project will contain a group of scenes. Each scene will contain a group of shots. Each shot still contain a group of takes.

Config files for each type contain metadata needed each.

Project_Name
∟ project.conf
∟ 0001_SCENE_ONE
  ∟ scene.conf
  ∟ 0001_SHOT_ONE
    ∟ shot.conf
    ∟ audio
      ∟ scene.wav
    ∟ 0001_TAKE_ONE
      ∟ take.conf
      ∟ frames
        ∟ preview
          ∟ high_res
            ∟ 0001_0001_0001_000001.jpg
            ∟ ...
            ∟ 0001_0001_0001_000001.jpg
          ∟ low_res
            ∟ 0001_0001_0001_000001.jpg
            ∟ ...
            ∟ 0001_0001_0001_000001.jpg
        ∟ RAW
          ∟ 0001_0001_0001_000001.cr3
          ∟ ...
          ∟ 0001_0001_0001_000001.cr3


## UI

The UI of this program is flat and professional. The only supported theme is black. Color accents should be the color of a brick, dark red.

### Tabs

The entire UI of the program is based on tabbed work modes. 

The first tab is the Producer tab. This tab is used to break apart of the film into scenes and manage what take is actively being worked on.

The second tab is Cinematography which is for adjusting camera settings and taking test photos to play with lighting and camera settings.

The third tab is Animation which is used for actually animating the take. It has tools like onion screening and playback controls.

## Tests

TODO

## Common Pitfalls

TODO
