#!/bin/sh
HERE="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
SD="$(CDPATH= cd -- "$HERE/../.." && pwd)"
ROOT="$SD/Slot"
mkdir -p "$ROOT/Logs" 2>/dev/null
# Uptime at each step of startup, up to Slot's first frame (issue #44).
[ ! -f "$ROOT/Logs/boot-times.log" ] || mv -f "$ROOT/Logs/boot-times.log" "$ROOT/Logs/boot-times.previous.log" 2>/dev/null
stamp() { echo "$(cut -d' ' -f1 /proc/uptime 2>/dev/null) $*" >> "$ROOT/Logs/boot-times.log" 2>/dev/null; }
stamp started from Apps
export SLOT_BOOT_TIMES="$ROOT/Logs/boot-times.log"
sh "$SD/Slot/System/install-autoboot.sh"
stamp startup hook checked
exec sh "$SD/Slot/System/launch.sh"
