#!/bin/sh
set -eu
: "${BRICK_TOOLCHAIN:?Set BRICK_TOOLCHAIN}"
cd "$(dirname "$0")/.."
OUT="${1:-dist-brick}"
if [ -e "$OUT" ]; then echo "Output already exists: $OUT. Choose a new directory." >&2; exit 1; fi
mkdir -p "$OUT"
cp -R brick/card/. "$OUT/"

# Boot Logo app: picture converter plus its own copy of the file checker.
( cd brick/bootlogo-tool && cargo build --locked --release --target aarch64-unknown-linux-gnu )
cp brick/bootlogo-tool/target/aarch64-unknown-linux-gnu/release/slot-bootlogo "$OUT/Apps/BootLogo/"
cp brick/card/Slot/System/slot-filecheck "$OUT/Apps/BootLogo/"
cp target/aarch64-unknown-linux-gnu/device/slot "$OUT/Slot/System/"
cp vendor/mgba_libretro.so vendor/gpsp_libretro.so vendor/fceumm_libretro.so vendor/snes9x_libretro.so vendor/pcsx_rearmed_libretro.so "$OUT/Slot/System/"
for f in slot mgba_libretro.so gpsp_libretro.so fceumm_libretro.so snes9x_libretro.so pcsx_rearmed_libretro.so; do "$BRICK_TOOLCHAIN/bin/aarch64-linux-gnu-strip" "$OUT/Slot/System/$f"; done
for d in BIOS Config Games/GB Games/GBC Games/GBA Games/NES Games/SNES Labels/GB Labels/GBC Labels/GBA Labels/NES Labels/SNES Saves/GB Saves/GBC Saves/GBA Saves/NES Saves/SNES States/GB States/GBC States/GBA States/NES States/SNES Overlays/GB Overlays/GBC Overlays/GBA Overlays/NES Overlays/SNES Wallpapers Logs; do mkdir -p "$OUT/Slot/$d"; done
cp brick/README.md "$OUT/Slot/README.md"
# Overlays kept on this machine only (brick/overlays-local is not in git, so a card built by
# GitHub never has them): PNGs only, for a card built here for its own owner.
for d in GB GBC GBA NES SNES; do
    for f in brick/overlays-local/"$d"/*.png; do
        if [ -f "$f" ]; then cp "$f" "$OUT/Slot/Overlays/$d/"; fi
    done
done
cp -R licenses "$OUT/Slot/System/"
cp LICENSE "$OUT/Slot/System/licenses/slot-GPL-3.0.txt"
chmod +x "$OUT/Slot/System/slot" "$OUT/Slot/System/"*.sh "$OUT/Apps/"*/launch.sh "$OUT/Apps/BootLogo/slot-bootlogo" "$OUT/Apps/BootLogo/slot-filecheck"
echo "Card files written to $OUT"
