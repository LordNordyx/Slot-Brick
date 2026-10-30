#!/bin/sh
# Read-only hardware report; contains no ROM listings, Wi-Fi names or credentials.
set -u
OUT="${1:-./slot-diagnostics.txt}"
{
    echo 'slot Brick v0.60 diagnostics'
    date
    uname -a
    for f in /etc/os-release /proc/device-tree/model /sys/class/graphics/fb0/virtual_size /sys/class/graphics/fb0/modes; do
        if [ -r "$f" ]; then echo "--- $f"; cat "$f"; echo; fi
    done
    echo '--- input devices'
    for d in /sys/class/input/event*/device; do
        [ -d "$d" ] || continue
        echo "$d"
        for f in name capabilities/key capabilities/abs capabilities/sw; do
            [ ! -r "$d/$f" ] || { printf '%s: ' "$f"; cat "$d/$f"; }
        done
    done
    echo '--- audio devices'
    [ ! -r /proc/asound/cards ] || cat /proc/asound/cards
    echo '--- mixer'
    amixer contents 2>&1 | head -120
    echo '--- power supply'
    for d in /sys/class/power_supply/*; do
        [ -d "$d" ] || continue
        echo "$d"
        for f in type capacity status online; do
            [ ! -r "$d/$f" ] || { printf '%s: ' "$f"; cat "$d/$f"; }
        done
    done
    echo '--- wifi (file names only, no profiles)'
    ls /sys/class/net 2>&1
    ls -l /etc/wifi /etc/wifi/sockets /var/run/wpa_supplicant 2>&1
    ls /etc/init.d 2>&1 | grep -iE 'wifi|wpa|wlan|net|dhcp'
    for b in wpa_supplicant wpa_cli udhcpc hostapd rfkill rfkill.elf; do
        printf '%s: ' "$b"; command -v "$b" || echo missing
    done
    for r in /sys/class/rfkill/rfkill*; do
        [ -d "$r" ] && echo "$(basename "$r"): type=$(cat "$r/type" 2>/dev/null) soft=$(cat "$r/soft" 2>/dev/null) hard=$(cat "$r/hard" 2>/dev/null)"
    done
    [ ! -e /usr/share/udhcpc/default.script ] || echo 'udhcpc default.script present'
    ps 2>/dev/null | grep -E 'wpa_supplicant|udhcpc|hostapd' | grep -v grep
    echo '--- startup chain (what runs and draws before slot)'
    for f in /usr/trimui/bin/runtrimui-slot-backup.sh /etc/inittab; do
        [ ! -r "$f" ] || { echo "### $f"; head -c 8192 "$f"; echo; }
    done
    ls -l /etc/init.d 2>&1
    for f in /etc/init.d/*; do
        grep -qiE 'trimui|loading|logo|splash|fb0|disp' "$f" 2>/dev/null && { echo "### $f"; head -c 4096 "$f"; echo; }
    done
    echo '### images that look like boot/loading screens'
    find /usr/trimui /etc /usr/share -maxdepth 4 \( -iname '*load*' -o -iname '*splash*' -o -iname '*boot*' -o -iname '*logo*' \) \
        \( -iname '*.png' -o -iname '*.bmp' -o -iname '*.jpg' -o -iname '*.raw' \) 2>/dev/null | head -40
    for f in bits_per_pixel stride; do
        [ ! -r "/sys/class/graphics/fb0/$f" ] || echo "fb0 $f: $(cat /sys/class/graphics/fb0/$f)"
    done
    echo '--- runtime libraries'
    for d in /usr/trimui/lib /usr/lib /lib; do
        for f in "$d"/libEGL.so* "$d"/libGLESv2.so* "$d"/libSDL2*.so* "$d"/libasound.so* "$d"/libc.so* "$d"/ld-linux-aarch64.so*; do
            [ ! -e "$f" ] || ls -l "$f"
        done
    done
} > "$OUT" 2>&1
