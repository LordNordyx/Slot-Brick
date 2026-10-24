#!/bin/sh
# Build the Slot OS SD card image: TrimUI's boot chain and kernel, a small system in RAM that
# starts Slot directly, and a FAT partition with Slot's card files. Needs Docker.
# Usage: brick/os/build.sh [card files, default dist-brick] [FAT size in MiB, default 2560; at least
# about 2304, or the FAT32 has too few clusters]
# Output: dist-os/slot-os.img.xz, for Raspberry Pi Imager, balenaEtcher or dd.
set -eu
cd "$(dirname "$0")/../.."
CARD_FILES=${1:-dist-brick}
FAT_MIB=${2:-2560}
IMAGE=slot-brick-os
[ -x "$CARD_FILES/Slot/System/slot" ] || { echo "No card files in $CARD_FILES: run brick/docker-build.sh first." >&2; exit 1; }
sh brick/os/fetch.sh
docker build -q -t "$IMAGE" -f brick/os/Dockerfile brick/os > /dev/null
mkdir -p dist-os
docker run --rm -u "$(id -u):$(id -g)" -v "$PWD":/src -w /src "$IMAGE" sh -euc "
    sh brick/os/make-image.sh '$CARD_FILES' dist-os/slot-os.img '$FAT_MIB'
    rm -f dist-os/slot-os.img.xz
    xz -T0 -6 dist-os/slot-os.img
    cat /opt/arm64-debs/VERSIONS > dist-os/debian-packages.txt
"
ls -l dist-os/slot-os.img.xz
