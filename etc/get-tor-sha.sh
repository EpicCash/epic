#!/bin/bash


# -----------------------------------------------------------------------------
# get-tor-sha.sh
#
# This script automates the download, signature verification, and SHA256
# checksum calculation for official Tor expert bundle archives for multiple
# platforms (macOS, Linux, Windows). It ensures that only authentic and
# untampered Tor binaries are used in the project by verifying GPG signatures
# and recording checksums. The Tor version is read from ../tor-version.txt.
#
# Usage: Run this script from the etc/ directory.
# -----------------------------------------------------------------------------

### Before running this script, read:
# https://support.torproject.org/tbb/how-to-verify-signature/
#
# 

set -e

TOR_VERSION=$(cat ../tor-version.txt)
BASEURL="https://archive.torproject.org/tor-package-archive/torbrowser/${TOR_VERSION}/"
FILES=(
  "tor-expert-bundle-macos-x86_64-${TOR_VERSION}.tar.gz"
  "tor-expert-bundle-macos-aarch64-${TOR_VERSION}.tar.gz"
  "tor-expert-bundle-linux-x86_64-${TOR_VERSION}.tar.gz"
  "tor-expert-bundle-windows-x86_64-${TOR_VERSION}.tar.gz"
)

# Truncate or create tor_signatures.txt at the start
: > tor_signatures.txt

for ARCHIVE in "${FILES[@]}"; do
  ASC="$ARCHIVE.asc"
  echo "=============================="
  echo "Processing $ARCHIVE"
  echo "=============================="

  # Download archive
  if [ ! -f "$ARCHIVE" ]; then
    echo "Downloading $ARCHIVE..."
    curl -LO "${BASEURL}${ARCHIVE}"
  else
    echo "$ARCHIVE already exists."
  fi

  # Download .asc signature
  if [ ! -f "$ASC" ]; then
    echo "Downloading $ASC..."
    curl -LO "${BASEURL}${ASC}"
  else
    echo "$ASC already exists."
  fi

  # Verify signature
  echo "Verifying signature for $ARCHIVE..."
  gpg --verify "$ASC" "$ARCHIVE"

  # Print and save SHA256
  if command -v shasum >/dev/null 2>&1; then
    SHA256=$(shasum -a 256 "$ARCHIVE" | awk '{print $1}')
  elif command -v sha256sum >/dev/null 2>&1; then
    SHA256=$(sha256sum "$ARCHIVE" | awk '{print $1}')
  else
    echo "No SHA256 tool found!"
    exit 1
  fi

  echo "$ARCHIVE $SHA256"
  echo "$ARCHIVE $SHA256" >> tor_signatures.txt

  echo ""
done