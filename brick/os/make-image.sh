#!/bin/sh
# Runs inside brick/os/Dockerfile's image (build.sh starts it): makes the SD card image.
#   $1  card files (docker-build.sh's output)   $2  output image   $3  FAT size in MiB
set -eu
CARD_FILES=$1
IMG=$2
FAT_MIB=${3:-2560}
SRC=.docker-cache/os
OS=brick/os
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' 0

# --- TrimUI's own files: their kernel, its drivers, the GPU libraries and firmware ----------
# From TrimUI's firmware release (sources.txt): the boot image as it is, and from their
# system, each file below.
TRIMUI=$WORK/trimui
python3 "$OS/trimui-firmware.py" "$SRC/TG3040_Brick_firmware_v1.1.1.zip" "$TRIMUI"
for f in lib/modules/4.9.191/pvrsrvkm.ko lib/modules/4.9.191/dc_sunxi.ko \
         lib/modules/4.9.191/xradio_mac.ko lib/modules/4.9.191/xradio_core.ko lib/modules/4.9.191/xradio_wlan.ko \
         lib/firmware/rgx.fw.22.102.54.38 lib/firmware/rgx.sh.22.102.54.38 \
         lib/firmware/boot_xr829.bin lib/firmware/fw_xr829.bin lib/firmware/sdd_xr829.bin lib/firmware/etf_xr829.bin \
         usr/lib/libEGL.so usr/lib/libGLESv2.so usr/lib/libIMGegl.so usr/lib/libglslcompiler.so \
         usr/lib/libpvrNULL_WSEGL.so usr/lib/libsrv_um.so usr/lib/libusc.so usr/bin/pvrsrvctl; do
    debugfs -R "dump /$f $TRIMUI/$(basename "$f")" "$TRIMUI/rootfs.fex" 2>/dev/null
    [ -s "$TRIMUI/$(basename "$f")" ] || { echo "not in TrimUI's system: /$f" >&2; exit 1; }
done
rm -f "$TRIMUI/rootfs.fex"

# --- The RAM system (initramfs) -------------------------------------------------------------
R=$WORK/root
mkdir -p "$R"
for deb in /opt/arm64-debs/*.deb; do dpkg-deb -x "$deb" "$R"; done
# Each Debian package's copyright and licence file, for the card's slot-os/licenses (the
# docs folders go below). libgcc-s1 and libstdc++6 point theirs to gcc-12-base's.
LIC=$WORK/licenses
mkdir -p "$LIC/debian"
for deb in /opt/arm64-debs/*.deb; do
    pkg=$(dpkg-deb -f "$deb" Package)
    [ ! -e "$R/usr/share/doc/$pkg/copyright" ] || cp -L "$R/usr/share/doc/$pkg/copyright" "$LIC/debian/$pkg.txt"
done
ls /opt/arm64-debs/*.deb | xargs -n1 basename > "$LIC/debian/packages.txt"
cp "$OS/NOTICE.txt" "$LIC/NOTICE.txt"
cp licenses/fceumm-GPL-2.0.txt "$LIC/GPL-2.0.txt"
rm -rf "$R/usr/share/doc" "$R/usr/share/lintian" "$R/usr/share/man" "$R/usr/share/locale" \
    "$R/usr/lib/aarch64-linux-gnu/gconv" "$R/usr/share/bug" "$R/etc/ld.so.conf.d" \
    "$R/lib/systemd" "$R/usr/lib/udev" "$R/usr/share/sounds" "$R/usr/share/alsa/speaker-test"
# The card's tools stay in RAM, in /sbin: they run while the card is not mounted (their
# libraries are copied to RAM first).
mv "$R/usr/sbin/fsck.exfat" "$R/sbin/fsck.exfat"
# wpa_supplicant and wpa_cli to the card with the rest (only /lib and /usr go there).
mv "$R/sbin/wpa_supplicant" "$R/sbin/wpa_cli" "$R/usr/sbin/"
find "$R/sbin" ! -type d ! -name fsck.fat ! -name fsck.exfat ! -name sgdisk -delete
# Of alsa-utils, only what sets up and tests the mixer; of wpasupplicant, the daemon and
# wpa_cli, which Slot's Wi-Fi helper (brick_wifi.sh) asks whether it answers; of hostapd,
# the daemon, for the host of a two-Brick link (slotlink.sh).
find "$R/usr/bin" "$R/usr/sbin" -type f ! -name amixer ! -name alsactl ! -name aplay ! -name speaker-test \
    ! -name wpa_supplicant ! -name wpa_cli ! -name hostapd -delete
rm -rf "$R/var/run" && mkdir -p "$R/var" && ln -s /run "$R/var/run"
mkdir -p "$R/etc" "$R/bin" "$R/sbin" "$R/usr/bin" "$R/usr/sbin" "$R/proc" "$R/sys" "$R/dev" "$R/tmp" \
    "$R/run" "$R/root" "$R/mnt/SDCARD" "$R/lib/modules/4.9.191" "$R/lib/firmware" "$R/usr/lib"
# busybox-static's applets, as links.
for applet in $(grep -v '^#' "$OS/busybox-applets.txt"); do
    ln -sf /bin/busybox "$R/bin/$applet"
done
# glibc's loader where every aarch64 binary looks for it.
ln -sf aarch64-linux-gnu/ld-linux-aarch64.so.1 "$R/lib/ld-linux-aarch64.so.1"
for f in libEGL.so libGLESv2.so libIMGegl.so libglslcompiler.so libpvrNULL_WSEGL.so libsrv_um.so libusc.so; do
    install -m 755 "$TRIMUI/$f" "$R/usr/lib/$f"
done
ln -sf libEGL.so "$R/usr/lib/libEGL.so.1"
ln -sf libGLESv2.so "$R/usr/lib/libGLESv2.so.2"
install -m 755 "$TRIMUI/pvrsrvctl" "$R/usr/bin/pvrsrvctl"
# The GPU module as shipped carries 20 MB of debug sections the kernel never reads.
aarch64-linux-gnu-strip --strip-debug -o "$R/lib/modules/4.9.191/pvrsrvkm.ko" "$TRIMUI/pvrsrvkm.ko"
aarch64-linux-gnu-strip --strip-debug -o "$R/lib/modules/4.9.191/dc_sunxi.ko" "$TRIMUI/dc_sunxi.ko"
cp "$TRIMUI"/rgx.fw.22.102.54.38 "$TRIMUI"/rgx.sh.22.102.54.38 "$R/lib/firmware/"
for m in xradio_mac xradio_core xradio_wlan; do
    aarch64-linux-gnu-strip --strip-debug -o "$R/lib/modules/4.9.191/$m.ko" "$TRIMUI/$m.ko"
done
cp "$TRIMUI"/boot_xr829.bin "$TRIMUI"/fw_xr829.bin "$TRIMUI"/sdd_xr829.bin "$TRIMUI"/etf_xr829.bin "$R/lib/firmware/"
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
for elf in "$R"/usr/lib/*.so "$R"/usr/bin/* "$R"/usr/sbin/* "$R"/sbin/fsck.* "$R"/sbin/sgdisk "$CARD_FILES"/Slot/System/slot "$CARD_FILES"/Slot/System/*.so; do
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
cp -r "$LIC" "$SYS/licenses"
( cd "$R" && find . | LC_ALL=C sort | cpio -o -H newc --quiet ) | gzip -9 -n > "$WORK/initramfs.gz"
echo "initramfs: $(du -h "$WORK/initramfs.gz" | cut -f1) (unpacked $(du -sh "$R" | cut -f1)); on the card: $(du -sh "$SYS" | cut -f1)"

# --- boot.img: TrimUI's kernel (their firmware's boot.fex) with our initramfs ---------------
python3 "$OS/bootimg.py" "$TRIMUI/boot.fex" "$WORK/initramfs.gz" "$WORK/boot.img"
# TrimUI's u-boot settings (their firmware's env.fex: a CRC, a flag byte, then name=value
# strings), with the kernel's console messages turned down
# (loglevel 1, was 8): at 115200 baud, printing them held up the boot.
python3 -c '
import sys
data = open(sys.argv[1], "rb").read()[5:]
for line in filter(None, data.split(b"\0")):
    print(("loglevel=1" if line.startswith(b"loglevel=") else line.decode()))
' "$TRIMUI/env.fex" > "$WORK/env.txt"
mkenvimage -r -p 0 -s 0x20000 -o "$WORK/env.img" "$WORK/env.txt"

# --- The FAT partition: Slot and the player's folders ----------------------------------------
# Made for a 1 TiB partition (32 KiB clusters, 256 MiB of FAT), then told it is $FAT_MIB MiB:
# on first boot the Brick grows the partition to the card's end and only has to write the
# new size into the boot sector (init's grow). $FAT_MIB must leave it at least 65525
# clusters, or it would not count as FAT32.
dd if=/dev/zero of="$WORK/fat.img" bs=1M count=0 seek=$((1024 * 1024)) 2>/dev/null
mkfs.vfat -F 32 -s 64 -n SLOT "$WORK/fat.img" >/dev/null
python3 "$OS/fatsize.py" "$WORK/fat.img" $((FAT_MIB * 2048))
truncate -s "${FAT_MIB}M" "$WORK/fat.img"
export MTOOLS_SKIP_CHECK=1
mcopy -s -p -i "$WORK/fat.img" "$CARD_FILES/Slot" ::/
mcopy -s -p -i "$WORK/fat.img" "$SYS" ::/
# u-boot shows this from the boot-resource partition as soon as it starts.
python3 "$OS/bootlogo.py" "$WORK/bootlogo.bmp"
mcopy -i "$WORK/fat.img" "$WORK/bootlogo.bmp" ::/bootlogo.bmp
fsck.fat -n "$WORK/fat.img" >/dev/null

# --- The card -------------------------------------------------------------------------------
# The layout: boot0 at 128 KiB (the GPT's entries moved to 80 KiB out of
# its way), u-boot's package at 16 400 KiB, then the partitions u-boot reads by name.
MIB=1048576
# The boot partition is 15 MiB: u-boot reads all of it into memory at
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
dd if="$TRIMUI/boot0_sdcard.fex" of="$IMG" bs=1024 seek=128 conv=notrunc 2>/dev/null
dd if="$TRIMUI/boot_package.fex" of="$IMG" bs=1024 seek=16400 conv=notrunc 2>/dev/null
dd if="$WORK/boot.img" of="$IMG" bs=$MIB seek=36 conv=notrunc 2>/dev/null
dd if="$WORK/env.img" of="$IMG" bs=$MIB seek=$((36 + BOOT_MIB)) conv=notrunc 2>/dev/null
dd if="$WORK/env.img" of="$IMG" bs=$MIB seek=$((37 + BOOT_MIB)) conv=notrunc 2>/dev/null
dd if="$WORK/fat.img" of="$IMG" bs=$MIB seek=$((FAT_START / MIB)) conv=notrunc 2>/dev/null
sgdisk -p "$IMG" | tail -6
# For a card already running Slot OS: its system folder, and the boot image that init
# writes to the boot partition when it finds it at slot-os/boot.img.
OUT=$(dirname "$IMG")
rm -rf "$OUT/slot-os" && cp -r "$SYS" "$OUT/slot-os" && cp "$WORK/boot.img" "$WORK/bootlogo.bmp" "$OUT/"
