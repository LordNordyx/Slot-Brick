#!/bin/sh
CARD=/mnt/SDCARD
ROOT="$CARD/Slot"
mkdir -p "$ROOT/Logs" 2>/dev/null
# Uptime at each step of startup, up to Slot's first frame (issue #44).
[ ! -f "$ROOT/Logs/boot-times.log" ] || mv -f "$ROOT/Logs/boot-times.log" "$ROOT/Logs/boot-times.previous.log" 2>/dev/null
stamp() { echo "$(cut -d' ' -f1 /proc/uptime 2>/dev/null) $*" >> "$ROOT/Logs/boot-times.log" 2>/dev/null; }
[ -z "${SLOT_HOOK_STARTED:-}" ] || echo "$SLOT_HOOK_STARTED hook started (cold boot)" >> "$ROOT/Logs/boot-times.log" 2>/dev/null
stamp boot.sh started
export SLOT_BOOT_TIMES="$ROOT/Logs/boot-times.log"
# Never let an unwritable card stop Slot from starting: a failed log redirect
# would end this script and drop the Brick into the stock menu with no trace.
# Slot itself then warns "Can't write to SD card".
[ ! -f "$ROOT/Logs/boot.log" ] || mv -f "$ROOT/Logs/boot.log" "$ROOT/Logs/boot.previous.log" 2>/dev/null
if ( : >> "$ROOT/Logs/boot.log" ) 2>/dev/null; then
    exec > "$ROOT/Logs/boot.log" 2>&1
else
    exec > /tmp/slot-boot.log 2>&1
    echo 'WARNING: SD card is not writable'
fi
echo "slot autoboot v0.6: $(date)"
export PATH=/usr/trimui/bin:$PATH
export LD_LIBRARY_PATH=/usr/trimui/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}
[ -s "$ROOT/System/slot" ] && [ -s "$ROOT/System/launch.sh" ] || exit 1
# A fresh full card on an already configured Brick must retire discovery
# before entering Slot, so Exit to stock does not rediscover the installer.
# Also keeps the Brick's startup hook up to date (no change when it already matches;
# the installer then returns at once).
if grep -q '^# SLOT_BRICK_BOOT_V[12]$' /usr/trimui/bin/runtrimui.sh; then
    sh "$ROOT/System/install-autoboot.sh" || echo 'Startup hook check failed; see install.log'
fi
stamp startup hook checked
count=0
while [ ! -e /dev/fb0 ] && [ "$count" -lt 100 ]; do sleep 0.1; count=$((count + 1)); done
stamp screen device ready
# In /tmp so the check also works when the card cannot be written.
export SLOT_READY_FILE=/tmp/slot-first-frame.ready
rm -f "$SLOT_READY_FILE"
sh "$ROOT/System/launch.sh"
status=$?
# Retry once only when initialization never displayed a frame.
if [ ! -f "$SLOT_READY_FILE" ]; then
    echo "Startup did not display a frame (status $status); retrying once."
    sleep 2
    sh "$ROOT/System/launch.sh"
    status=$?
fi
echo "slot ended ($status); returning to previous launcher."
exit "$status"
