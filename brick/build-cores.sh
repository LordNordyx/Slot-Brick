#!/bin/sh
set -eu
: "${BRICK_TOOLCHAIN:?Set BRICK_TOOLCHAIN to the extracted TrimUI toolchain directory}"
cd "$(dirname "$0")/.."
export PATH="$BRICK_TOOLCHAIN/bin:$PATH"
MGBA=e31759b24e7a4e3899285ff720d7b573ac328ae7
GPSP=7b2dd37394e2822110cacef321e9bb291a305b77
FCEUMM=7a542dab1e87679921962a9f056186eca425c0c2
SNES9X=fae2fea08f74180759ef540ee94259213f503480
mkdir -p vendor brick/core-src
[ -f vendor/mgba-src.tar.gz ] || curl -fL "https://github.com/libretro/mgba/archive/$MGBA.tar.gz" -o vendor/mgba-src.tar.gz
[ -f vendor/gpsp-src.tar.gz ] || curl -fL "https://github.com/libretro/gpsp/archive/$GPSP.tar.gz" -o vendor/gpsp-src.tar.gz
[ -f vendor/fceumm-src.tar.gz ] || curl -fL "https://github.com/libretro/libretro-fceumm/archive/$FCEUMM.tar.gz" -o vendor/fceumm-src.tar.gz
[ -f vendor/snes9x-src.tar.gz ] || curl -fL "https://github.com/libretro/snes9x/archive/$SNES9X.tar.gz" -o vendor/snes9x-src.tar.gz
# The archives are committed; a fresh download must match them byte for byte.
sha256sum -c - <<'SUMS'
396d749cce8fe3358b29cbb1db479b1816a151bd688ee45b1d241503cbc40243  vendor/mgba-src.tar.gz
665876b87cb2d1c23264462be72ff2b5bd7dff7e0efb0c79d66bee87b72114ec  vendor/gpsp-src.tar.gz
f80b5c1df39e22d78791b1867ea79f3ec8c0823e4b1f82b3f89e432c614a6c53  vendor/fceumm-src.tar.gz
0d4b0c4181d66668ec0040eb17f90a559f7b7fa6cbd9198593c3bdbe79acd029  vendor/snes9x-src.tar.gz
SUMS
mgba_src="$PWD/brick/core-src/mgba-$MGBA"
gpsp_src="$PWD/brick/core-src/gpsp-$GPSP"
fceumm_src="$PWD/brick/core-src/libretro-fceumm-$FCEUMM"
snes9x_src="$PWD/brick/core-src/snes9x-$SNES9X"
if [ ! -d "$mgba_src" ]; then
    tar --no-same-owner -xzf vendor/mgba-src.tar.gz -C brick/core-src
    for p in cores/mgba/*.patch; do git -C "$mgba_src" apply "$PWD/$p"; done
fi
if [ ! -d "$gpsp_src" ]; then
    tar --no-same-owner -xzf vendor/gpsp-src.tar.gz -C brick/core-src
    for p in cores/gpsp/*.patch brick/gpsp-old-binutils.patch; do git -C "$gpsp_src" apply "$PWD/$p"; done
fi
[ -d "$fceumm_src" ] || tar --no-same-owner -xzf vendor/fceumm-src.tar.gz -C brick/core-src
[ -d "$snes9x_src" ] || tar --no-same-owner -xzf vendor/snes9x-src.tar.gz -C brick/core-src
cmake -S "$mgba_src" -B brick/core-build/mgba \
    -DCMAKE_SYSTEM_NAME=Linux -DCMAKE_SYSTEM_PROCESSOR=aarch64 \
    -DCMAKE_C_COMPILER=aarch64-linux-gnu-gcc -DCMAKE_CXX_COMPILER=aarch64-linux-gnu-g++ \
    -DLIBMGBA_ONLY=ON -DBUILD_LIBRETRO=ON -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_C_FLAGS=-mcpu=cortex-a53 \
    -DCMAKE_DISABLE_FIND_PACKAGE_PNG=ON -DCMAKE_DISABLE_FIND_PACKAGE_ZLIB=ON \
    -DCMAKE_DISABLE_FIND_PACKAGE_LibZip=ON -DCMAKE_POLICY_VERSION_MINIMUM=3.5
cmake --build brick/core-build/mgba --target mgba_libretro --parallel 8
make -C "$gpsp_src" platform=arm64 CC=aarch64-linux-gnu-gcc CXX=aarch64-linux-gnu-g++ AR=aarch64-linux-gnu-ar -j8
# rpi3_64 is FCEUmm's Cortex-A53 target, the Brick's CPU.
make -C "$fceumm_src" -f Makefile.libretro platform=rpi3_64 CC=aarch64-linux-gnu-gcc CXX=aarch64-linux-gnu-g++ AR=aarch64-linux-gnu-ar -j8
# snes9x is C++: its runtime is linked in, as the firmware's libstdc++ may be too old or absent.
CXXFLAGS="-mcpu=cortex-a53 -mtune=cortex-a53" LDFLAGS="-static-libstdc++ -static-libgcc" \
    make -C "$snes9x_src/libretro" platform=unix CC=aarch64-linux-gnu-gcc CXX=aarch64-linux-gnu-g++ AR=aarch64-linux-gnu-ar -j8
cp brick/core-build/mgba/mgba_libretro.so vendor/
cp "$gpsp_src/gpsp_libretro.so" vendor/
cp "$fceumm_src/fceumm_libretro.so" vendor/
cp "$snes9x_src/libretro/snes9x_libretro.so" vendor/
