#!/bin/sh
# Build the Slot OS SD card image: TrimUI's boot chain and kernel, a small system in RAM that
# starts Slot directly, and a FAT partition with Slot's card files. Needs Docker.
# Usage: brick/os/build.sh [card files, default dist-brick] [FAT size in MiB, default 2560; at least
# about 2304, or the FAT32 has too few clusters]
# SLOT_OS_VERSION names the build in Slot OS's log (the release workflows set the tag, or
# "dev <commit>"); left unset, it is "dev <commit>".
# SLOT_OS_MODEL=brickpro builds the Brick Pro's image (experimental) from its own firmware.
# Output: dist-os/slot-os.img.xz, for Raspberry Pi Imager, balenaEtcher or dd; and
# dist-os/slot-os-update.zip, what HOME → Update installs on a card already running Slot OS
# (Slot, slot-os/ with the new boot.img and u-boot settings, the boot logo; brick/update-manifest.sh describes it
# as update-os.txt). The Brick Pro's are slot-os-brickpro.img.xz and slot-os-brickpro-update.zip
# (update-os-brickpro.txt).
set -eu
cd "$(dirname "$0")/../.."
CARD_FILES=${1:-dist-brick}
FAT_MIB=${2:-2560}
IMAGE=slot-brick-os
SLOT_OS_VERSION=${SLOT_OS_VERSION:-dev $(git rev-parse --short HEAD 2>/dev/null)}
SLOT_OS_MODEL=${SLOT_OS_MODEL:-brick}
case "$SLOT_OS_MODEL" in
    brick) NAME=slot-os ;;
    brickpro) NAME=slot-os-brickpro ;;
    *) echo "SLOT_OS_MODEL is brick or brickpro, not $SLOT_OS_MODEL" >&2; exit 1 ;;
esac
[ -x "$CARD_FILES/Slot/System/slot" ] || { echo "No card files in $CARD_FILES: run brick/docker-build.sh first." >&2; exit 1; }
sh brick/os/fetch.sh
docker build -q -t "$IMAGE" -f brick/os/Dockerfile brick/os > /dev/null
mkdir -p dist-os
docker run --rm -u "$(id -u):$(id -g)" -v "$PWD":/src -w /src -e SLOT_OS_VERSION="$SLOT_OS_VERSION" \
    -e SLOT_OS_MODEL="$SLOT_OS_MODEL" "$IMAGE" sh -euc "
    sh brick/os/make-image.sh '$CARD_FILES' dist-os/$NAME.img '$FAT_MIB'
    rm -f dist-os/$NAME.img.xz
    xz -T0 -6 dist-os/$NAME.img
    cat /opt/arm64-debs/VERSIONS > dist-os/debian-packages.txt
"
rm -rf dist-os/update && mkdir -p dist-os/update/slot-os
cp -r "$CARD_FILES/Slot" dist-os/update/
cp -r dist-os/slot-os/. dist-os/update/slot-os/
cp dist-os/boot.img dist-os/update/slot-os/boot.img
cp dist-os/env.img dist-os/update/slot-os/env.img
cp dist-os/bootlogo.bmp dist-os/update/bootlogo.bmp
rm -f "dist-os/$NAME-update.zip"
python3 brick/zip-card.py dist-os/update "dist-os/$NAME-update.zip" > /dev/null
rm -rf dist-os/update
ls -l "dist-os/$NAME.img.xz" "dist-os/$NAME-update.zip"
