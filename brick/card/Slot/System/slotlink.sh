#!/bin/sh
# Temporary two-Brick network. Never writes saved Wi-Fi profiles.
set -eu
PATH=/usr/trimui/bin:/usr/sbin:/sbin:/usr/bin:/bin
ROOT=${SLOT_ROOT:-/mnt/SDCARD/Slot}
RUN=${SLOT_LINK_RUN:-/tmp/slot-brick-link}
ETC=${SLOT_LINK_ETC:-/etc}
# Slot's own helper first: Brick firmware's wifi_init.sh cannot unblock the radio.
CARD_HELPER="$ROOT/System/brick_wifi.sh"
HELPER=${SLOT_WIFI_HELPER:-$CARD_HELPER}
[ ! -f "$CARD_HELPER" ] && [ -f "$ETC/wifi/wifi_init.sh" ] && [ -z "${SLOT_WIFI_HELPER:-}" ] && HELPER="$ETC/wifi/wifi_init.sh"
mkdir -p "$ROOT/Logs"
LOG="$ROOT/Logs/multiplayer.log"
[ ! -f "$LOG" ] || [ "$(wc -c < "$LOG")" -le 131072 ] || mv -f "$LOG" "$LOG.previous"
exec >> "$LOG" 2>&1
stop_owned() {
    for kind in hostapd wpa_supplicant; do
        if [ -f "$RUN/$kind.pid" ]; then
            pid=$(cat "$RUN/$kind.pid")
            case "$pid" in ''|*[!0-9]*) continue;; esac
            # Protect against stale PID reuse.
            if [ -r "/proc/$pid/cmdline" ] && tr '\000' ' ' < "/proc/$pid/cmdline" | grep -Fq "$RUN/"; then
                kill "$pid" 2>/dev/null || :
                n=0
                while kill -0 "$pid" 2>/dev/null && [ "$n" -lt 10 ]; do sleep 0.1; n=$((n+1)); done
                kill -KILL "$pid" 2>/dev/null || :
            fi
        fi
    done
    ifconfig wlan0 0.0.0.0 down 2>/dev/null || :
    if [ -f "$HELPER" ]; then sh "$HELPER" stop || :; fi
    if command -v rfkill.elf >/dev/null 2>&1; then rfkill.elf block wifi || :
    elif command -v rfkill >/dev/null 2>&1; then rfkill block wifi || :; fi
    rm -f "$RUN/hostapd.pid" "$RUN/wpa_supplicant.pid" "$RUN/hostapd.conf" "$RUN/join.conf"
}
# Set while a link session owns wlan0; RESTORE remembers that normal Wi-Fi was on before.
ACTIVE="$RUN/active"
RESTORE="$RUN/restore-wifi"
case "${2:-}" in
    warm) exit 0;;
    down|cool)
        # Slot asks for this whenever a game or its menu closes. Only a link session's own
        # radio setup is undone; Wi-Fi nobody linked over stays exactly as it is.
        [ -e "$ACTIVE" ] || { echo 'No link session; Wi-Fi left alone'; exit 0; }
        stop_owned
        rm -f "$ACTIVE"
        if [ -e "$RESTORE" ]; then
            rm -f "$RESTORE"
            echo 'Link session over; turning Wi-Fi back on'
            ( sh "$HELPER" start ) < /dev/null > /dev/null 2>&1 &
        else
            echo 'Wi-Fi off'
        fi
        exit 0;;
    host|join) role=$2;;
    *) echo 'Invalid multiplayer action'; exit 2;;
esac
[ "${1:-}" = link ] || exit 2
umask 077
mkdir -p "$RUN"
chmod 700 "$RUN"
# Firmware helper loads/unblocks its driver. Stop DHCP and station mode before
# taking over wlan0, then unblock again for our private temporary daemons.
trap 'stop_owned' EXIT
trap 'exit 1' HUP INT TERM
[ -f "$HELPER" ] || { echo 'Missing firmware Wi-Fi helper'; exit 1; }
# From here on wlan0 belongs to the link session until "down".
if [ ! -e "$ACTIVE" ] && pgrep -x wpa_supplicant > /dev/null 2>&1; then : > "$RESTORE"; fi
: > "$ACTIVE"
stop_owned
if [ "$HELPER" = "$CARD_HELPER" ]; then
    # Slot's own helper: station mode is already stopped; just power the radio.
    sh "$HELPER" radio
else
    sh "$HELPER" start
    sh "$HELPER" stop
fi
# The helper's "radio" has unblocked it already, through sysfs where there is no rfkill tool
# (Slot OS has none).
if command -v rfkill.elf >/dev/null 2>&1; then rfkill.elf unblock wifi
elif command -v rfkill >/dev/null 2>&1; then rfkill unblock wifi; fi
ifconfig wlan0 up
if [ "$role" = host ]; then
    command -v hostapd >/dev/null || { echo 'Firmware hostapd unavailable'; exit 1; }
    cat > "$RUN/hostapd.conf" <<EOF
interface=wlan0
driver=nl80211
ctrl_interface=$RUN/host
ssid=slotlink-brick
hw_mode=g
channel=6
max_num_sta=1
wpa=2
wpa_key_mgmt=WPA-PSK
rsn_pairwise=CCMP
wpa_passphrase=slot-brick-link
EOF
    hostapd -B -P "$RUN/hostapd.pid" "$RUN/hostapd.conf"
    ifconfig wlan0 10.42.0.1 netmask 255.255.255.0 up
else
    command -v wpa_supplicant >/dev/null || { echo 'Firmware supplicant unavailable'; exit 1; }
    cat > "$RUN/join.conf" <<EOF
ctrl_interface=$RUN/join
update_config=0
network={
    ssid="slotlink-brick"
    key_mgmt=WPA-PSK
    psk="slot-brick-link"
    scan_ssid=1
    scan_freq=2437
}
EOF
    wpa_supplicant -B -D nl80211 -i wlan0 -c "$RUN/join.conf" -P "$RUN/wpa_supplicant.pid"
    ifconfig wlan0 10.42.0.2 netmask 255.255.255.0 up
fi
# Rust verifies AP-ENABLED/COMPLETED using the daemon control socket.
echo "Started $role; waiting for driver readiness"
trap - EXIT HUP INT TERM
