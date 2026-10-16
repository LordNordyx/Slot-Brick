#!/bin/sh
# The four cores for the simulator: the same pinned sources (vendor/*-src.tar.gz) and patches
# as the Brick's, built for x86_64. Run inside the sim box by brick/sim.sh; skipped when
# target-sim/cores already holds this exact set.
set -eu
cd "$(dirname "$0")/../.."
MGBA=e31759b24e7a4e3899285ff720d7b573ac328ae7
GPSP=7b2dd37394e2822110cacef321e9bb291a305b77
FCEUMM=7a542dab1e87679921962a9f056186eca425c0c2
SNES9X=fae2fea08f74180759ef540ee94259213f503480
OUT=target-sim/cores
SRC=target-sim/core-src

stamp=$(cat "$0" vendor/*-src.tar.gz cores/mgba/*.patch cores/gpsp/*.patch | sha256sum | cut -d' ' -f1)
if [ "$(cat "$OUT/stamp" 2>/dev/null)" = "$stamp" ]; then
    exit 0
fi
sha256sum -c - <<'SUMS'
396d749cce8fe3358b29cbb1db479b1816a151bd688ee45b1d241503cbc40243  vendor/mgba-src.tar.gz
665876b87cb2d1c23264462be72ff2b5bd7dff7e0efb0c79d66bee87b72114ec  vendor/gpsp-src.tar.gz
f80b5c1df39e22d78791b1867ea79f3ec8c0823e4b1f82b3f89e432c614a6c53  vendor/fceumm-src.tar.gz
0d4b0c4181d66668ec0040eb17f90a559f7b7fa6cbd9198593c3bdbe79acd029  vendor/snes9x-src.tar.gz
SUMS
rm -rf "$SRC" "$OUT"
mkdir -p "$SRC" "$OUT"
for core in mgba gpsp fceumm snes9x; do
    tar --no-same-owner -xzf "vendor/$core-src.tar.gz" -C "$SRC"
done
mgba="$SRC/mgba-$MGBA"
gpsp="$SRC/gpsp-$GPSP"
fceumm="$SRC/libretro-fceumm-$FCEUMM"
snes9x="$SRC/snes9x-$SNES9X"
# Each its own repository, so git apply reads the patches' paths from there.
git init -q "$mgba" && git init -q "$gpsp"
for p in cores/mgba/*.patch; do git -C "$mgba" apply "$PWD/$p"; done
for p in cores/gpsp/*.patch; do git -C "$gpsp" apply "$PWD/$p"; done
jobs=$(nproc)

cmake -S "$mgba" -B "$SRC/mgba-build" -DLIBMGBA_ONLY=ON -DBUILD_LIBRETRO=ON \
    -DCMAKE_BUILD_TYPE=Release -DCMAKE_DISABLE_FIND_PACKAGE_PNG=ON \
    -DCMAKE_DISABLE_FIND_PACKAGE_ZLIB=ON -DCMAKE_DISABLE_FIND_PACKAGE_LibZip=ON \
    -DCMAKE_POLICY_VERSION_MINIMUM=3.5 > /dev/null
cmake --build "$SRC/mgba-build" --target mgba_libretro --parallel "$jobs" > /dev/null
make -C "$gpsp" platform=unix -j"$jobs" > /dev/null
make -C "$fceumm" -f Makefile.libretro platform=unix -j"$jobs" > /dev/null
make -C "$snes9x/libretro" platform=unix -j"$jobs" > /dev/null

cp "$SRC/mgba-build/mgba_libretro.so" "$gpsp/gpsp_libretro.so" \
    "$fceumm/fceumm_libretro.so" "$snes9x/libretro/snes9x_libretro.so" "$OUT/"
echo "$stamp" > "$OUT/stamp"
echo "sim cores built in $OUT"
