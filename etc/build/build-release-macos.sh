#!/usr/bin/env bash

# Code used to build the releases for the project
# The final output is the tar.gz file and the checksum with sha256 for macos
# This must be executed from the root of the project

# Initialize the project variables
NAME=$(awk -F "=" '/name/ {print $2}' Cargo.toml | head -1 | tr -d '"' | tr -d ' ')
VERSION=$(awk -F "=" '/version/ {print $2}' Cargo.toml | head -1 | tr -d '"' | tr -d ' ')
OS=macos

OUTPUTPATH=release
BINARY=./target/release/epic
TARFILE=$NAME-$VERSION-$OS.tar.gz
SHAFILE=$NAME-$VERSION-$OS-sha256sum.txt

# Clean the Build the project
cargo clean
cargo build --release

# Create the output path and tar the files and dependencies
mkdir -p ./$OUTPUTPATH/$NAME
cp ./debian/foundation.json ./etc/README.MD $BINARY ./$OUTPUTPATH/$NAME
cd ./$OUTPUTPATH && tar -czvf $TARFILE epic/ && cd ../
rm -r ./$OUTPUTPATH/$NAME/

# Generate the sha256sum
shasum -a 256 ./$OUTPUTPATH/$TARFILE | sed 's, .*/, ,' > ./$OUTPUTPATH/$SHAFILE
