#!/bin/sh
# Build the Brick SD card files in Docker; no Rust or cross toolchain needed on the host.
# Usage: brick/docker-build.sh [--cores] [OUT]
#   --cores  also rebuild mGBA and gpSP (otherwise the prebuilt vendor/*.so are used)
#   OUT      output directory, default dist-brick (replaced if it exists)
set -eu
cd "$(dirname "$0")/.."
CORES=0
[ "${1:-}" != --cores ] || { CORES=1; shift; }
OUT="${1:-dist-brick}"
IMAGE=slot-brick-build

docker build -t "$IMAGE" -f brick/Dockerfile brick
mkdir -p .docker-cache/cargo
rm -rf "$OUT"
# Host user, so build output is not owned by root. Cargo's download cache stays
# in .docker-cache between runs.
docker run --rm -u "$(id -u):$(id -g)" -v "$PWD":/src -w /src \
    -e HOME=/tmp -e CARGO_HOME=/src/.docker-cache/cargo -e CORES="$CORES" -e OUT="$OUT" \
    -e SLOT_CHANNEL="${SLOT_CHANNEL:-}" \
    "$IMAGE" sh -euc '
        [ "$CORES" = 0 ] || sh brick/build-cores.sh
        sh brick/build-frontend.sh
        sh brick/package-card.sh "$OUT"
        file "$OUT/Slot/System/slot"
    '
echo "Copy everything inside $OUT/ (including the hidden .slot-boot folder) to the SD card root."
