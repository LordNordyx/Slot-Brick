#!/bin/sh
# Wi-Fi helper for TrimUI Brick (tg5040).
# slot (brick_wifi.rs) always runs this file. Brick firmware may ship its own
# /etc/wifi/wifi_init.sh, but it calls an rfkill.elf that firmware lacks, so the
# radio stays blocked; this helper unblocks it through sysfs and checks the result.
# Usage: brick_wifi.sh start|stop|radio|ensure|dhcp
# start:  radio on, wpa_supplicant control socket up, DHCP client waiting.
# stop:   DHCP + supplicant stopped, radio off.
# ensure: for a supplicant someone else started (the firmware does at boot, with
#         the radio still blocked): radio on, wlan0 up, DHCP client waiting.
# dhcp:   a DHCP client for wlan0 if none is running (after joining a network).
# Never blocks for long and never leaves daemons attached to our stdout,
# because slot waits for this script to exit. Everything is logged to
# Slot/Logs/wifi.log (or /tmp/slot/wifi.log when the card is read-only).
PATH=${SLOT_WIFI_PATH:+$SLOT_WIFI_PATH:}/usr/trimui/bin:/usr/sbin:/sbin:/usr/bin:/bin
ROOT=${SLOT_ROOT:-/mnt/SDCARD/Slot}
RUN=${SLOT_WIFI_RUN:-/tmp/slot}
# Test hooks; default to the real system paths.
ETC=${SLOT_WIFI_ETC:-/etc}
NET=${SLOT_WIFI_NET:-/sys/class/net}
VARRUN=${SLOT_WIFI_VARRUN:-/var/run}
RFKILL=${SLOT_WIFI_RFKILL:-/sys/class/rfkill}
IFACE=wlan0
LOG="$ROOT/Logs/wifi.log"
mkdir -p "$RUN" "$ROOT/Logs" 2>/dev/null
# $RUN holds a DHCP script run as root: nobody else may write there.
chmod 700 "$RUN" 2>/dev/null
[ ! -f "$LOG" ] || [ "$(wc -c < "$LOG")" -le 65536 ] || mv -f "$LOG" "$LOG.previous" 2>/dev/null
# An unwritable card must not lose the log: slot tells the user to read it.
( : >> "$LOG" ) 2>/dev/null || LOG="$RUN/wifi.log"
exec >> "$LOG" 2>&1 < /dev/null
echo "--- $(date) brick_wifi.sh $*"

radio() { # radio block|unblock
    if command -v rfkill.elf >/dev/null 2>&1; then rfkill.elf "$1" wifi
    elif command -v rfkill >/dev/null 2>&1; then rfkill "$1" wifi
    fi
    # Set the kernel switch directly too: Brick firmware has neither tool.
    soft=1; [ "$1" = block ] || soft=0
    for r in "$RFKILL"/rfkill*; do
        [ "$(cat "$r/type" 2>/dev/null)" = wlan ] || continue
        echo "$soft" > "$r/soft" 2>/dev/null
        echo "radio $1: $(basename "$r") soft=$(cat "$r/soft" 2>/dev/null) hard=$(cat "$r/hard" 2>/dev/null)"
    done
}

iface_up() { # the kernel's IFF_UP bit, so a failed ifconfig is caught
    flags=$(cat "$NET/$IFACE/flags" 2>/dev/null) || return 1
    [ $((flags & 1)) = 1 ]
}

# A socket file alone can be left over from a dead supplicant: ask it.
supplicant_ready() {
    dir=$(find_socket) || return 1
    if command -v wpa_cli >/dev/null 2>&1; then
        [ "$(wpa_cli -p "$dir" -i "$IFACE" ping 2>/dev/null)" = PONG ]
    else
        pidof wpa_supplicant >/dev/null 2>&1
    fi
}

# Control-socket directories slot knows how to talk to, in its search order.
find_socket() {
    for dir in "$ETC/wifi/sockets" "$VARRUN/wpa_supplicant" /run/wpa_supplicant; do
        [ -S "$dir/$IFACE" ] && { echo "$dir"; return 0; }
    done
    return 1
}

wait_for() { # wait_for <tenths> <test...>
    n=$1; shift
    while [ "$n" -gt 0 ]; do
        "$@" && return 0
        sleep 0.1
        n=$((n - 1))
    done
    "$@"
}

pick_conf() {
    # Prefer the stock profile store so networks saved in TrimUI/NextUI work,
    # but only if it exposes a control socket slot can reach.
    stock="$ETC/wifi/wpa_supplicant.conf"
    if [ -f "$stock" ] && grep -Eq '^[[:space:]]*ctrl_interface=(DIR=)?(/etc/wifi/sockets|/var/run/wpa_supplicant|/run/wpa_supplicant)' "$stock"; then
        echo "$stock"; return
    fi
    own="$ROOT/Config/wpa_supplicant.conf"
    if [ ! -f "$own" ]; then
        mkdir -p "$ROOT/Config"
        printf 'ctrl_interface=%s/wpa_supplicant\nupdate_config=1\n' "$VARRUN" > "$own"
    fi
    echo "$own"
}

write_dhcp_script() { # minimal busybox udhcpc event script
    cat > "$1" <<'EOS'
#!/bin/sh
PATH=/usr/sbin:/sbin:/usr/bin:/bin
case "$1" in
deconfig) ifconfig "$interface" 0.0.0.0 ;;
bound|renew)
    ifconfig "$interface" "$ip" ${subnet:+netmask "$subnet"} ${broadcast:+broadcast "$broadcast"}
    if [ -n "$router" ]; then
        while route del default gw 0.0.0.0 dev "$interface" 2>/dev/null; do :; done
        for r in $router; do route add default gw "$r" dev "$interface"; break; done
    fi
    if [ -n "$dns" ]; then
        { [ -z "$domain" ] || echo "search $domain"; for d in $dns; do echo "nameserver $d"; done; } > /etc/resolv.conf 2>/dev/null
    fi ;;
esac
exit 0
EOS
    chmod 755 "$1"
}

start_dhcp() {
    if [ -f "$RUN/udhcpc.pid" ] && kill -0 "$(cat "$RUN/udhcpc.pid")" 2>/dev/null; then return; fi
    # Firmware may already run one for wlan0.
    for p in $(pidof udhcpc 2>/dev/null); do
        tr '\000' ' ' < "/proc/$p/cmdline" 2>/dev/null | grep -q "$IFACE" && return
    done
    script=/usr/share/udhcpc/default.script
    if [ ! -x "$script" ]; then
        script="$RUN/udhcpc.script"
        write_dhcp_script "$script"
    fi
    command -v udhcpc >/dev/null 2>&1 || { echo 'udhcpc not found; no IP address will be assigned'; return; }
    # Foreground-retrying client in the background: keeps asking until the
    # supplicant associates, then renews. stdio already points at the log.
    udhcpc -i "$IFACE" -R -t 3 -T 2 -A 3 -p "$RUN/udhcpc.pid" -s "$script" &
}

stop_dhcp() {
    [ ! -f "$RUN/udhcpc.pid" ] || kill "$(cat "$RUN/udhcpc.pid")" 2>/dev/null
    for p in $(pidof udhcpc 2>/dev/null); do
        tr '\000' ' ' < "/proc/$p/cmdline" 2>/dev/null | grep -q "$IFACE" && kill "$p" 2>/dev/null
    done
    rm -f "$RUN/udhcpc.pid"
}

# Some firmware ships its own helper. Try it first for start/stop, with its output in
# this log, but never trust it blindly: start only counts once wlan0 is up and a
# supplicant answers, and stop still runs ours so nothing is left powered.
firmware() { # firmware <verb>
    fw="$ETC/wifi/wifi_init.sh"
    [ -f "$fw" ] || return 1
    echo "running firmware helper: $fw $1"
    /bin/sh "$fw" "$1"
    rc=$?
    echo "firmware helper $1 exited $rc"
    return "$rc"
}

case "${1:-}" in
start)
    # Before the firmware helper too, whose own unblock needs the missing rfkill.elf.
    radio unblock
    if firmware start && wait_for 50 iface_up && wait_for 30 supplicant_ready; then
        echo "control socket: $(find_socket)"
        start_dhcp
        exit 0
    fi
    [ ! -f "$ETC/wifi/wifi_init.sh" ] || echo 'firmware helper did not bring Wi-Fi up; using slot helper'
    radio unblock
    if ! wait_for 50 [ -d "$NET/$IFACE" ]; then
        echo "no $IFACE interface (driver not loaded?)"; exit 1
    fi
    ifconfig "$IFACE" up && wait_for 20 iface_up || { echo "cannot bring $IFACE up"; exit 1; }
    if ! supplicant_ready; then
        command -v wpa_supplicant >/dev/null 2>&1 || { echo 'wpa_supplicant not found'; exit 1; }
        # Any supplicant still running cannot be reached; replace it and its stale socket.
        for p in $(pidof wpa_supplicant 2>/dev/null); do kill "$p" 2>/dev/null; done
        wait_for 20 sh -c '! pidof wpa_supplicant >/dev/null 2>&1'
        for dir in "$ETC/wifi/sockets" "$VARRUN/wpa_supplicant"; do rm -f "$dir/$IFACE"; done
        conf=$(pick_conf)
        echo "starting wpa_supplicant with $conf"
        wpa_supplicant -B -D nl80211 -i "$IFACE" -c "$conf" -P "$RUN/wpa_supplicant.pid" \
            || wpa_supplicant -B -D wext -i "$IFACE" -c "$conf" -P "$RUN/wpa_supplicant.pid" \
            || { echo 'wpa_supplicant failed to start'; exit 1; }
        wait_for 50 supplicant_ready || { echo 'wpa_supplicant did not answer on its control socket'; exit 1; }
    fi
    echo "control socket: $(find_socket)"
    ! command -v wpa_cli >/dev/null 2>&1 || echo "state: $(wpa_cli -p "$(find_socket)" -i "$IFACE" status 2>&1 | grep -E '^(wpa_state|ssid)=' | tr '\n' ' ')"
    start_dhcp
    exit 0
    ;;
radio)
    # Radio and interface only, for slotlink.sh's private AP/join daemons.
    radio unblock
    wait_for 50 [ -d "$NET/$IFACE" ] || { echo "no $IFACE interface (driver not loaded?)"; exit 1; }
    ifconfig "$IFACE" up && wait_for 20 iface_up || { echo "cannot bring $IFACE up"; exit 1; }
    exit 0
    ;;
ensure)
    supplicant_ready || { echo 'no supplicant answering; nothing to repair'; exit 1; }
    # A supplicant started under a blocked radio or a down wlan0 answers but never
    # recovers (scans fail): restart it the way turning Wi-Fi off and on does.
    blocked=0
    for r in "$RFKILL"/rfkill*; do
        [ "$(cat "$r/type" 2>/dev/null)" = wlan ] && [ "$(cat "$r/soft" 2>/dev/null)" = 1 ] && blocked=1
    done
    if [ "$blocked" = 1 ] || ! iface_up; then
        echo 'supplicant running with the radio off; restarting Wi-Fi'
        /bin/sh "$0" stop
        exec /bin/sh "$0" start
    fi
    radio unblock
    iface_up || { ifconfig "$IFACE" up && wait_for 20 iface_up; } || { echo "cannot bring $IFACE up"; exit 1; }
    start_dhcp
    exit 0
    ;;
dhcp)
    # Joined a network but no address: make sure something is asking for one.
    start_dhcp
    exit 0
    ;;
stop)
    firmware stop || :
    stop_dhcp
    if dir=$(find_socket) && command -v wpa_cli >/dev/null 2>&1; then
        wpa_cli -p "$dir" -i "$IFACE" terminate >/dev/null 2>&1
    fi
    for p in $(pidof wpa_supplicant 2>/dev/null); do kill "$p" 2>/dev/null; done
    wait_for 20 sh -c '! pidof wpa_supplicant >/dev/null 2>&1'
    rm -f "$RUN/wpa_supplicant.pid"
    ifconfig "$IFACE" 0.0.0.0 down 2>/dev/null
    radio block
    exit 0
    ;;
*)
    echo "usage: $0 start|stop|radio|ensure|dhcp"; exit 2
    ;;
esac
