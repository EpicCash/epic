#!/usr/bin/env bash

# As a semi-automatic approach, this sh can generate the final zip and sha256 for windows builds
# This requires that you build the exe binary and place it on a release folder from the root of the project
# This must be executed from the root of the project

NAME=$(awk -F "=" '/name/ {print $2}' Cargo.toml | head -1 | tr -d '"' | tr -d ' ')
VERSION=$(awk -F "=" '/version/ {print $2}' Cargo.toml | head -1 | tr -d '"' | tr -d ' ')
OS=windows

OUTPUTPATH=release
BINARY=./release/epic.exe
ZIPFILE=$NAME-$VERSION-$OS.zip
SHAFILE=$NAME-$VERSION-$OS-sha256sum.txt

# Create the output path and tar the files and dependencies
mkdir -p ./$OUTPUTPATH/$NAME
cp ./debian/foundation.json ./etc/README.MD $BINARY ./$OUTPUTPATH/$NAME
cd ./$OUTPUTPATH && zip $ZIPFILE epic/* && cd ../
rm -r ./$OUTPUTPATH/$NAME/

# Generate the sha256sum
shasum -a 256 ./$OUTPUTPATH/$ZIPFILE | sed 's, .*/, ,' > ./$OUTPUTPATH/$SHAFILE