#!/bin/sh
# Build a deckd binary that actually runs on the Raspberry Pi.
#
# Why not `cross`:
#   1. cross publishes linux/amd64 images only, so on an Apple Silicon Mac every
#      tag fails with "no matching manifest for linux/arm64/v8".
#   2. Its default image links against glibc 2.39. The Pi runs Debian Bookworm
#      with glibc 2.36, so that binary will not start:
#        /lib/aarch64-linux-gnu/libc.so.6: version `GLIBC_2.39' not found
#
# Building natively inside a Bookworm arm64 container sidesteps both: the glibc
# matches the Pi exactly and no cross-compilation is involved. On an arm64 host
# this runs at native speed.
#
# Usage: deploy/build-pi.sh [output-dir]
#   Default output dir is target/pi.

set -eu

REPO_DIR=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
OUT_DIR=${1:-"$REPO_DIR/target/pi"}
TARGET_DIR="$OUT_DIR/cargo-target"

# Matches the Pi: Debian 12 (Bookworm), glibc 2.36, aarch64.
IMAGE=debian:bookworm-slim

mkdir -p "$TARGET_DIR"

echo "Building deckd for aarch64 Bookworm in $IMAGE ..."
docker run --rm --platform linux/arm64 \
    -v "$REPO_DIR":/src -w /src \
    -v "$TARGET_DIR":/target \
    -e CARGO_TARGET_DIR=/target \
    "$IMAGE" sh -c '
set -eu
apt-get update -qq
apt-get install -y -qq curl build-essential pkg-config libudev-dev >/dev/null
curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable >/dev/null 2>&1
. "$HOME/.cargo/env"
cargo build --release
'

BIN="$TARGET_DIR/release/deckd"
if [ ! -f "$BIN" ]; then
    echo "ERROR: build reported success but $BIN is missing" >&2
    exit 1
fi

cp "$BIN" "$OUT_DIR/deckd"

# Verify the artifact before anyone tries to deploy it. A wrong-arch or
# too-new-glibc binary fails only at service start, which is a much worse
# place to discover it.
echo
echo "Artifact: $OUT_DIR/deckd"
file "$OUT_DIR/deckd" | grep -q 'ARM aarch64' || {
    echo "ERROR: not an aarch64 binary" >&2
    exit 1
}

MAX_GLIBC=$(objdump -T "$OUT_DIR/deckd" 2>/dev/null \
    | grep -oE 'GLIBC_2\.[0-9]+' | sort -uV | tail -1)
echo "Max glibc requirement: ${MAX_GLIBC:-none detected} (Pi provides 2.36)"

echo "OK"
