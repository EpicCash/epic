# Helpers for the creation of releases

This path is used to build the releases for the epic projects.
Currently, the release creation must be done on the native systems.

Each build is composed of:
- The binaries zip or in a tar with:
    - README.MD
    - foundation.json
    - The binary itself
- A sha256sum of the zip or tar

## Building the releases for Linux

The creation of releases for Linux can be done in any system.
The build process is using a docker image to generate the final binaries.

To build the project, run

```
./etc/build/build-release-linux-amd64.sh
```

At the root of the project.

## Building the releases for macOS

Being on a native macOS system,
To build the release for macOS you must run:

```
./etc/build/build-release-macos.sh
```

At the root of the project.

## Building the releases for Windows

Windows releases following a semi-automatic approach, since the binaries must be generated first.

To prepare the binaries for release, create a `release` folder on the root of the project.
On there you can place the `epic.exe`, and run:

```
./etc/build/build-release-windows.sh
```