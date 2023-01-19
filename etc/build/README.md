# Helpers for the creation of releases

This path is used to build the releases for the epic projects.
Currently, the release creation must be done on the native systems.

Each build is composed of:

- The binaries zip or in a tar with:
  - [README.MD](../README.MD)
  - foundation.json
  - The binary itself
- A sha256sum of the zip or tar

## Building the releases for Linux

Currently, we have two options to build the project on Linux.
- Using a stable environment using a docker container
- Locally by executing the sh provided

The docker approach can be built in any system using docker as a dependency.

### Using docker as a build environment

The build process is using a docker image to generate the final binaries.

To build the project, first build the docker-image with:

```shell
docker build . -f ./etc/Dockerfile.build --tag epic-build
```

On Linux/macOS, you can execute the build using:

```shell
docker run -it -v $(pwd):/home/app epic-build /bin/bash -c "./etc/build/build-release-linux-amd64.sh"
```

Or the following for windows:

```shell
docker run -it -v %cd%:/home/app epic-build /bin/bash -c "./etc/build/build-release-linux-amd64.sh"
```

At the root of the project.

### Local build

To build the release files locally, run:

```shell
./etc/build/build-release-linux-amd64.sh
```

At the root of the project.

## Building the releases for macOS

Being on a native macOS system,
To build the release for macOS you must run:

```shell
./etc/build/build-release-macos.sh
```

At the root of the project.

## Building the releases for Windows

Windows releases following a semi-automatic approach, since the binaries must be generated first.

With the binaries on hand, create a `release` folder on the root of the project.
On there you can place the `epic.exe`, and run:

```shell
./etc/build/build-release-windows.sh
```