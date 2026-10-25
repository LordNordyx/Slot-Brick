#!/bin/sh
# Install/repair only; safe to call from the working stock-menu app.
CARD=/mnt/SDCARD
BIN=/usr/trimui/bin
ROOT="$CARD/Slot"
# Runs on every start: when the hook is already in place and current, with a good backup
# and no installer left to retire, there is nothing to do, so skip the full checks.
if [ ! -f "$ROOT/DISABLE_AUTOBOOT" ] && [ ! -d "$CARD/trimui" ] && [ -s "$BIN/runtrimui-slot-backup.sh" ] \
    && ! grep -q '^# SLOT_BRICK_BOOT_V[12]$' "$BIN/runtrimui-slot-backup.sh" \
    && "$ROOT/System/slot-filecheck" compare "$ROOT/System/runtrimui-slot.sh" "$BIN/runtrimui.sh" 2>/dev/null; then
    exit 0
fi
mkdir -p "$ROOT/Logs" 2>/dev/null
if ( : >> "$ROOT/Logs/install.log" ) 2>/dev/null; then exec >> "$ROOT/Logs/install.log" 2>&1; else exec >> /tmp/slot-install.log 2>&1; echo 'WARNING: SD card is not writable'; fi
echo "slot autoboot installer v0.5: $(date)"
fail() { echo "INSTALL FAILED: $1"; echo "$1" > "$ROOT/Logs/autoboot-status.txt" 2>/dev/null; exit 1; }
[ ! -f "$ROOT/DISABLE_AUTOBOOT" ] || { echo 'Autoboot disabled by marker.'; exit 0; }
strings "$BIN/MainUI" 2>/dev/null | grep -qx 'Trimui Brick' || fail 'Device not identified as original TrimUI Brick.'
[ -s "$BIN/runtrimui.sh" ] || fail 'Original startup script missing.'
[ -s "$ROOT/System/runtrimui-slot.sh" ] || fail 'Packaged startup hook missing.'
[ -s "$CARD/.slot-boot/boot.sh" ] || fail 'Missing hidden .slot-boot folder.'
[ -s "$ROOT/System/slot" ] || fail 'Missing slot executable.'
BACKUP="$BIN/runtrimui-slot-backup.sh"
NEW="$BIN/runtrimui-slot-new-$$.sh"
trap 'rm -f "$NEW"' 0
if grep -q '^# SLOT_BRICK_BOOT_V[12]$' "$BIN/runtrimui.sh"; then
    [ -s "$BACKUP" ] || fail 'Installed hook has no fallback backup.'
else
    # A firmware update or reinstall replaces the startup script but can leave an
    # older backup behind. The active script is not a Slot hook, so it is the
    # current original: keep the old backup aside and back up the current one.
    if [ -e "$BACKUP" ] && ! "$ROOT/System/slot-filecheck" compare "$BIN/runtrimui.sh" "$BACKUP"; then
        echo 'Startup script changed since the last install (firmware update?); refreshing the backup.'
        mv -f "$BACKUP" "$BIN/runtrimui-slot-backup.previous.sh" || fail 'Could not set the old backup aside.'
    fi
    cp -p "$BIN/runtrimui.sh" "$BACKUP" || fail 'Could not save original startup.'
    "$ROOT/System/slot-filecheck" compare "$BIN/runtrimui.sh" "$BACKUP" || fail 'Backup verification failed.'
fi
if grep -q '^# SLOT_BRICK_BOOT_V[12]$' "$BACKUP"; then fail 'Fallback is another slot hook; refusing recursive startup.'; fi
if ! "$ROOT/System/slot-filecheck" compare "$ROOT/System/runtrimui-slot.sh" "$BIN/runtrimui.sh"; then
    cp "$ROOT/System/runtrimui-slot.sh" "$NEW" || fail 'Could not stage hook.'
    chmod 755 "$NEW" || fail 'Could not set hook permissions.'
    "$ROOT/System/slot-filecheck" compare "$ROOT/System/runtrimui-slot.sh" "$NEW" || fail 'Hook verification failed.'
    mv -f "$NEW" "$BIN/runtrimui.sh" || fail 'Could not activate hook.'
fi
# Retire discovery bootstrap so the previous launcher cannot rediscover it.
if [ -d "$CARD/trimui" ]; then
    mkdir -p "$ROOT/Installer" || fail 'Could not create installer archive.'
    mv "$CARD/trimui" "$ROOT/Installer/trimui-installed-$$" || fail 'Could not retire bootstrap.'
fi
sync
printf 'Installed v0.5. Next boot starts slot automatically.\n' > "$ROOT/Logs/autoboot-status.txt" 2>/dev/null
echo 'Installed v0.5. Next boot starts slot automatically.'
