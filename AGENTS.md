# Brick

Brick is an open source stop motion program. It runs on Linux, Windows, and MacOS. The supported package types for Linux are deb and rpm. The program is primarily designed to support DSLRs, specifically those made by Canon. But webcams are supported as well.

## Features

All features must have a spec written for it with a spec number. Currently all specs are in @docs/program.md but in future as the program grows the specs will be broken into several files.

## Commits

Follow @docs/commits.md for writing git commits.

## Common Pitfalls

- Brick is under active development. Do not add backward-compatibility code for obsolete settings, persisted data, or behavior unless a task explicitly requires it. Prefer removing obsolete paths and updating the current format.
