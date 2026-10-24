#!/bin/sh
# Runs inside brick/os/Dockerfile's image (build.sh starts it): makes the SD card image.
#   $1  card files (docker-build.sh's output)   $2  output image   $3  FAT size in MiB
set -eu
CARD_FILES=$1
IMG=$2
FAT_MIB=${3:-1024}
SRC=.docker-cache/os
OS=brick/os
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' 0

# --- The RAM system (initramfs) -------------------------------------------------------------
R=$WORK/root
mkdir -p "$R"
for deb in /opt/arm64-debs/*.deb; do dpkg-deb -x "$deb" "$R"; done
rm -rf "$R/usr/share/doc" "$R/usr/share/lintian" "$R/usr/share/man" "$R/usr/share/locale" \
    "$R/usr/lib/aarch64-linux-gnu/gconv" "$R/usr/share/bug" "$R/etc/ld.so.conf.d" \
    "$R/lib/systemd" "$R/usr/lib/udev" "$R/usr/share/sounds" "$R/usr/share/alsa/speaker-test"
# Of alsa-utils, only what sets up and tests the mixer.
find "$R/usr/bin" "$R/usr/sbin" -type f ! -name amixer ! -name alsactl ! -name aplay ! -name speaker-test -delete
mkdir -p "$R/etc" "$R/bin" "$R/sbin" "$R/usr/bin" "$R/usr/sbin" "$R/proc" "$R/sys" "$R/dev" "$R/tmp" \
    "$R/run" "$R/root" "$R/mnt/SDCARD" "$R/lib/modules/4.9.191" "$R/lib/firmware" "$R/usr/lib"
# busybox-static's applets, as links.
for applet in $(grep -v '^#' "$OS/busybox-applets.txt"); do
    ln -sf /bin/busybox "$R/bin/$applet"
done
# glibc's loader where every aarch64 binary looks for it.
ln -sf aarch64-linux-gnu/ld-linux-aarch64.so.1 "$R/lib/ld-linux-aarch64.so.1"
for f in libEGL.so libGLESv2.so libIMGegl.so libglslcompiler.so libpvrNULL_WSEGL.so libsrv_um.so libusc.so; do
    install -m 755 "$SRC/$f" "$R/usr/lib/$f"
done
ln -sf libEGL.so "$R/usr/lib/libEGL.so.1"
ln -sf libGLESv2.so "$R/usr/lib/libGLESv2.so.2"
install -m 755 "$SRC/pvrsrvctl" "$R/usr/bin/pvrsrvctl"
# The GPU module as shipped carries 20 MB of debug sections the kernel never reads.
aarch64-linux-gnu-strip --strip-debug -o "$R/lib/modules/4.9.191/pvrsrvkm.ko" "$SRC/pvrsrvkm.ko"
aarch64-linux-gnu-strip --strip-debug -o "$R/lib/modules/4.9.191/dc_sunxi.ko" "$SRC/dc_sunxi.ko"
cp "$SRC"/rgx.fw.22.102.54.38 "$SRC"/rgx.sh.22.102.54.38 "$R/lib/firmware/"
install -m 755 "$OS/init" "$R/init"
# u-boot's built-in settings, should it not read ours, start /rdinit or /sbin/init.
ln -sf /init "$R/rdinit"
ln -sf /init "$R/sbin/init"
cat "$OS/VERSION" > "$R/etc/slot-os-version"
install -m 644 "$OS/asound.conf" "$R/etc/asound.conf"
install -m 755 "$OS/mixer.sh" "$R/etc/mixer.sh"
# Every library Slot, its cores and the GPU driver ask for (and what Slot opens itself:
# EGL, GLES, ALSA) must be here, or the card boots to a black screen.
missing=0
for elf in "$R"/usr/lib/*.so "$R"/usr/bin/* "$R"/usr/sbin/* "$CARD_FILES"/Slot/System/slot "$CARD_FILES"/Slot/System/*.so; do
    for lib in $(aarch64-linux-gnu-readelf -d "$elf" | sed -n 's/.*(NEEDED).*\[\(.*\)\]/\1/p') libEGL.so.1 libGLESv2.so.2 libasound.so.2; do
        [ -e "$R/usr/lib/$lib" ] || [ -e "$R/lib/aarch64-linux-gnu/$lib" ] || [ -e "$R/usr/lib/aarch64-linux-gnu/$lib" ] \
            || { echo "missing $lib (for $(basename "$elf"))"; missing=1; }
    done
done
[ "$missing" = 0 ]
# Only busybox and this init stay in RAM, to keep what u-boot loads and the kernel unpacks
# small. Every other file (glibc, ALSA, the GPU's libraries, modules and firmware) goes to
# the card's slot-os folder, once; in RAM, /lib and /usr keep their folders and links, and
# each file becomes a link to its copy on the card. FAT has no links, and a link copied as a
# second file is a second library: libGLESv2.so.2 beside libGLESv2.so gave Slot a GL with
# no context, as EGL had made it current in the other.
SYS=$WORK/slot-os
mkdir -p "$SYS"
( cd "$R" && find lib usr -type f ) | while read -r f; do
    mkdir -p "$SYS/$(dirname "$f")"
    mv "$R/$f" "$SYS/$f"
    ln -s "/mnt/SDCARD/slot-os/$f" "$R/$f"
done
( cd "$R" && find . | LC_ALL=C sort | cpio -o -H newc --quiet ) | gzip -9 -n > "$WORK/initramfs.gz"
echo "initramfs: $(du -h "$WORK/initramfs.gz" | cut -f1) (unpacked $(du -sh "$R" | cut -f1)); on the card: $(du -sh "$SYS" | cut -f1)"

# --- boot.img: TrimUI's kernel (from Knulli's boot.img) with our initramfs -------------------
python3 "$OS/bootimg.py" "$SRC/knulli-boot.img" "$WORK/initramfs.gz" "$WORK/boot.img"
# Knulli's u-boot settings (env.txt), with the kernel's console messages turned down
# (loglevel 1, was 8): at 115200 baud, printing them held up the boot.
mkenvimage -r -p 0 -s 0x20000 -o "$WORK/env.img" "$OS/env.txt"

# --- The FAT partition: Slot and the player's folders ----------------------------------------
dd if=/dev/zero of="$WORK/fat.img" bs=1M count=0 seek="$FAT_MIB" 2>/dev/null
mkfs.vfat -F 32 -n SLOT "$WORK/fat.img" >/dev/null
export MTOOLS_SKIP_CHECK=1
mcopy -s -p -i "$WORK/fat.img" "$CARD_FILES/Slot" ::/
mcopy -s -p -i "$WORK/fat.img" "$SYS" ::/
# u-boot shows this from the boot-resource partition as soon as it starts.
python3 "$OS/bootlogo.py" "$WORK/bootlogo.bmp"
mcopy -i "$WORK/fat.img" "$WORK/bootlogo.bmp" ::/bootlogo.bmp

# --- The card -------------------------------------------------------------------------------
# Knulli's layout for the Brick: boot0 at 128 KiB (the GPT's entries moved to 80 KiB out of
# its way), u-boot's package at 16 400 KiB, then the partitions u-boot reads by name.
MIB=1048576
# The boot partition is 15 MiB, as Knulli's: u-boot reads all of it into memory at
# 0x45000000, and a bigger one may overwrite what u-boot still needs above it.
BOOT_MIB=15
[ "$(stat -c %s "$WORK/boot.img")" -le $((BOOT_MIB * MIB)) ] || { echo "boot.img is over $BOOT_MIB MiB" >&2; exit 1; }
FAT_START=$(((36 + BOOT_MIB + 2) * MIB))
TOTAL=$((FAT_START + FAT_MIB * MIB + MIB))
rm -f "$IMG"
dd if=/dev/zero of="$IMG" bs=1 count=0 seek="$TOTAL" 2>/dev/null
sgdisk -o -j 160 \
    -n 1:73728:+${BOOT_MIB}M -c 1:boot \
    -n 2:0:+1M -c 2:env \
    -n 3:0:+1M -c 3:env-redund \
    -n 4:$((FAT_START / 512)):+${FAT_MIB}M -c 4:boot-resource -t 4:0700 -A 4:set:2 \
    "$IMG" >/dev/null
dd if="$SRC/boot0.img" of="$IMG" bs=1024 seek=128 conv=notrunc 2>/dev/null
dd if="$SRC/boot_package.fex" of="$IMG" bs=1024 seek=16400 conv=notrunc 2>/dev/null
dd if="$WORK/boot.img" of="$IMG" bs=$MIB seek=36 conv=notrunc 2>/dev/null
dd if="$WORK/env.img" of="$IMG" bs=$MIB seek=$((36 + BOOT_MIB)) conv=notrunc 2>/dev/null
dd if="$WORK/env.img" of="$IMG" bs=$MIB seek=$((37 + BOOT_MIB)) conv=notrunc 2>/dev/null
dd if="$WORK/fat.img" of="$IMG" bs=$MIB seek=$((FAT_START / MIB)) conv=notrunc 2>/dev/null
sgdisk -p "$IMG" | tail -6
