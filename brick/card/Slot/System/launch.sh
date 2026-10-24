#!/bin/sh
# Experimental app launcher. Does not install firmware or replace boot files.
set -u
SYS="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
export SLOT_ROOT="$(dirname "$SYS")"
mkdir -p "$SLOT_ROOT/Logs" 2>/dev/null
# An unwritable card must not stop Slot; it shows its own warning.
[ ! -f "$SLOT_ROOT/Logs/slot.log" ] || mv -f "$SLOT_ROOT/Logs/slot.log" "$SLOT_ROOT/Logs/slot.previous.log" 2>/dev/null
if ( : >> "$SLOT_ROOT/Logs/slot.log" ) 2>/dev/null; then
    exec > "$SLOT_ROOT/Logs/slot.log" 2>&1
else
    exec > /tmp/slot.log 2>&1
    echo 'WARNING: SD card is not writable'
fi
# Uptime at each step of startup (issue #44); boot.sh or Apps/Slot started the log.
export SLOT_BOOT_TIMES="${SLOT_BOOT_TIMES:-$SLOT_ROOT/Logs/boot-times.log}"
stamp() { echo "$(cut -d' ' -f1 /proc/uptime 2>/dev/null) $*" >> "$SLOT_BOOT_TIMES" 2>/dev/null; }
stamp launch.sh started
printf 'slot Brick v0.44\n'
date
uname -a
# Keep inherited NextUI libraries ahead of the stock fallback.
export LD_LIBRARY_PATH="${LD_LIBRARY_PATH:+$LD_LIBRARY_PATH:}/usr/trimui/lib:/usr/lib:/lib"
export PATH=/usr/trimui/bin:/usr/sbin:/sbin:$PATH
export SDL_JOYSTICK_ALLOW_BACKGROUND_EVENTS=1
# Stock starts this before Apps; cold-boot replacement must start it too.
INPUT_PID=
if ! pidof trimui_inputd >/dev/null 2>&1; then
    if [ -x /usr/trimui/bin/trimui_inputd ]; then
        /usr/trimui/bin/trimui_inputd >> "$SLOT_ROOT/Logs/input-daemon.log" 2>&1 &
        INPUT_PID=$!
        echo "Started trimui_inputd PID $INPUT_PID"
    else
        echo 'WARNING: trimui_inputd executable missing'
    fi
fi
# Capture inherited clocks for cleanup even if the frontend crashes.
CPU_SYS=/sys/devices/system/cpu/cpu0/cpufreq
CPU_MIN=$(cat "$CPU_SYS/scaling_min_freq" 2>/dev/null || :)
CPU_MAX=$(cat "$CPU_SYS/scaling_max_freq" 2>/dev/null || :)
CPU_GOV=$(cat "$CPU_SYS/scaling_governor" 2>/dev/null || :)
CPU_SPEED=$(cat "$CPU_SYS/scaling_setspeed" 2>/dev/null || :)
cleanup() {
    sh "$SYS/slotlink.sh" link down || :
    if [ -n "$CPU_MIN" ] && [ -n "$CPU_MAX" ] && [ -n "$CPU_GOV" ]; then
        printf '%s\n' "$CPU_MAX" > "$CPU_SYS/scaling_max_freq" 2>/dev/null || :
        printf '%s\n' "$CPU_MIN" > "$CPU_SYS/scaling_min_freq" 2>/dev/null || :
        printf '%s\n' "$CPU_GOV" > "$CPU_SYS/scaling_governor" 2>/dev/null || :
        if [ "$CPU_GOV" = userspace ] && [ -n "$CPU_SPEED" ]; then
            printf '%s\n' "$CPU_SPEED" > "$CPU_SYS/scaling_setspeed" 2>/dev/null || :
        fi
    fi
    [ -z "$INPUT_PID" ] || kill "$INPUT_PID" 2>/dev/null || :
}
trap cleanup 0
# No wait here for the daemon's virtual joystick (/dev/input/js0): Slot sets up its screen
# and library meanwhile, and only then waits for the joystick itself.
if [ -f "$SLOT_ROOT/DEBUG_INPUT" ]; then
    export SLOT_TRACE_INPUT=1
else
    unset SLOT_TRACE_INPUT
fi
# CPU policy is temporary and restored by cleanup; no firmware changes.
# The hardware report only changes with the firmware, so it is not redone on every start:
# only when missing, after an update of Slot (a newer diagnose.sh), or after a crash (which
# removes it). Then in the background, once Slot is up, so it never delays startup.
DIAG="$SLOT_ROOT/Logs/diagnostics.txt"
if [ ! -s "$DIAG" ] || [ "$SYS/diagnose.sh" -nt "$DIAG" ]; then
    ( sleep 20; nice -n 19 sh "$SYS/diagnose.sh" "$DIAG" ) > /dev/null 2>&1 &
fi
if [ "$(uname -m)" != aarch64 ]; then
    echo 'This build requires aarch64 TrimUI Brick firmware.'
    exit 1
fi
chmod +x "$SYS/slot"
sh "$SYS/slotlink.sh" link down || :
# Slot makes its folders itself as it starts (root::ensure), so it is loaded only once.
cd "$SLOT_ROOT"
stamp starting slot
"$SYS/slot"
status=$?
echo "slot exit code: $status"
# Killed by a signal (crash): keep this run's log, which ends with the
# "slot: crash:" line, for bug reports. Newest five are kept.
if [ "$status" -ge 128 ]; then
    CRASH="$SLOT_ROOT/Logs/crash"
    mkdir -p "$CRASH" && cp "$SLOT_ROOT/Logs/slot.log" "$CRASH/slot-$(date '+%Y-%m-%d_%H-%M-%S').log" \
        && echo "Crash log kept in Logs/crash"
    # Redo the hardware report on the next start, for the bug report.
    rm -f "$DIAG"
    ls -1t "$CRASH"/slot-*.log 2>/dev/null | tail -n +6 | while read -r old; do rm -f "$old"; done
fi
sync
# An update swapped in (see crates/slot/src/update.rs): Slot removes .slot-update/pending once
# the new one has shown a frame. Still there when it ends, the new one never started, so the
# files it replaced go back, and the old one starts instead.
UPDATE="$(dirname "$SLOT_ROOT")/.slot-update"
if [ "$status" != 66 ] && [ -f "$UPDATE/pending" ] && [ -d "$UPDATE/old" ]; then
    echo 'The update did not start: putting the previous version back'
    ( cd "$UPDATE/old" && find . -type f ) | while IFS= read -r f; do
        mkdir -p "$(dirname "$(dirname "$SLOT_ROOT")/$f")"
        mv -f "$UPDATE/old/$f" "$(dirname "$SLOT_ROOT")/$f"
    done
    rm -rf "$UPDATE"
    sync
    status=66
fi
case "$status" in
    64) action=poweroff ;;
    65) action=reboot ;;
    66)
        # Start again on the files just swapped in; this script may be one of them.
        cleanup
        trap - 0
        echo 'Starting Slot again on the updated files'
        exec sh "$SYS/launch.sh"
        ;;
    *) exit "$status" ;;
esac
# The frontend has settled saves and exited, releasing audio/video handles.
cleanup
INPUT_PID=
echo "Requesting system $action"
if ! "$action"; then
    echo "FAILED: $action command; returning to launcher for recovery."
    exit 1
fi
# A successful power command is asynchronous. Never fall through to stock.
while :; do sleep 1; done
