#!/bin/sh
# The Brick simulator: the real Brick build of slot, on a PC, in Docker, no GPU or device needed.
# Usage: brick/sim.sh serve [PORT]     run it, with its screen and buttons at http://127.0.0.1:PORT/
#        brick/sim.sh test [NAME...]   run brick/sim/scenarios (all, or those named); fail on any
#                                      screenshot that differs from brick/sim/golden
#        brick/sim.sh update [NAME...] the same, writing the screenshots as the new goldens
#        brick/sim.sh unit             the sim build's own unit tests
#        brick/sim.sh stop             stop a running `serve`
# MODEL=pro makes it a Brick Pro (serve); scenarios named pro-*.sim always run as one.
# Screenshots land in target-sim/out/<scenario>/, with NAME.diff.png marking what changed.
set -eu
cd "$(dirname "$0")/.."
IMAGE=slot-brick-sim
NAME=slot-brick-sim
SIM=target-sim
BIN=$SIM/release/slot
# 2026-01-01 00:00 UTC: the clock every scenario sees, so screenshots match from run to run.
SIM_TIME=1767225600

cmd="${1:-serve}"
[ $# -eq 0 ] || shift

docker_run() {
    docker run --rm -u "$(id -u):$(id -g)" -v "$PWD":/src -w /src \
        -e HOME=/tmp -e CARGO_HOME=/src/.docker-cache/cargo "$@"
}

# The simulator's image and the Brick's five cores (from the same sources, for this PC; built
# once, then kept): what both the unit tests and the scenarios need.
prepare() {
    docker build -q -t "$IMAGE" -f brick/sim/Dockerfile brick/sim > /dev/null
    mkdir -p .docker-cache/cargo
    docker_run "$IMAGE" sh brick/sim/build-cores.sh
}

# And Slot itself, for the scenarios and serve (the unit tests build their own).
build() {
    prepare
    docker_run "$IMAGE" cargo build --locked --release --target-dir "$SIM" \
        -p slot --no-default-features --features sim
}

# A fresh SD card and fresh stand-in hardware in $1: the card layout slot makes itself, the
# Brick card's data files and the cores, and a cartridge per shelf. Only Pocket Test and Disc
# Test are real games (brick/sim/make-rom.py, make-ps1.py); the others are stand-ins that fill
# the shelf, which the cores refuse. Put your own games in target-sim/live/card/Slot/Games to play them under `serve`.
fresh() {
    rm -rf "$1"
    mkdir -p "$1/hw"
    docker_run "$IMAGE" "$BIN" --init-root "$1/card/Slot" > /dev/null
    cp brick/card/Slot/System/*.txt "$SIM"/cores/*_libretro.so "$1/card/Slot/System/"
    cp -r brick/card/Slot/System/Fonts "$1/card/Slot/System/"
    for cart in "GBA/Sim Quest.gba" "GBA/Pixel Racer.gba" "GB/Pocket Test.gb" \
                "GBC/Colour Check.gbc" "NES/Famicom Probe.nes" "SNES/Super Sample.sfc"; do
        printf 'slot simulator stand-in cartridge\n' > "$1/card/Slot/Games/$cart"
    done
    python3 -I brick/sim/make-rom.py "$1/card/Slot/Games/GB/Pocket Test.gb"
    python3 -I brick/sim/make-ps1.py "$1/card/Slot/Games/PS1/Disc Test.exe"
    # A game of two discs, each painting its bands in its own order, in a folder of its own.
    two="$1/card/Slot/Games/PS1/Two Disc Test"
    mkdir -p "$two"
    for n in 1 2; do
        python3 -I brick/sim/make-ps1.py "$two/Two Disc Test (Disc $n).iso" "$n"
        printf 'Two Disc Test (Disc %s).iso\n' "$n" >> "$two/Two Disc Test.m3u"
    done
    python3 -I brick/sim/make-overlay.py "$1/card/Slot/Overlays/GB/Sim Frame.png"
    cp "$1/card/Slot/Overlays/GB/Sim Frame.png" \
        "$1/card/Slot/Overlays/GBA/Sim Frame with a name far too long to fit beside its label.png"
    # A release to update to, signed with the simulator's key, which only a simulator build takes.
    python3 -I brick/sim/make-update.py "$1/releases"
    openssl pkeyutl -sign -rawin -inkey brick/sim/update-key.pem \
        -in "$1/releases/latest/download/update.txt" -out "$1/releases/latest/download/update.txt.sig"
}

# Where the cart art and label servers live. Docker looks names up with this machine's DNS;
# where that cannot answer for one of them, resolve.py does and the address is passed in, so
# the label search and Get Cart Art can be tried in the sim.
art_hosts() {
    for host in art.slot-cfw.fyi images.launchbox-app.com; do
        addr=$(python3 -I brick/sim/resolve.py "$host" 2>/dev/null) || continue
        # An address and nothing else: what this prints is split into docker's arguments.
        case "$addr" in
            *[!0-9.]* | '') continue ;;
        esac
        printf -- '--add-host %s:%s ' "$host" "$addr"
    done
}

scenarios() {
    if [ $# -eq 0 ]; then
        ls brick/sim/scenarios/*.sim
    else
        for n in "$@"; do echo "brick/sim/scenarios/$n.sim"; done
    fi
}

# One scenario, on a card of its own: PASS or FAIL (with the last lines of what it said) in
# $SIM/out/NAME.result, its log beside it.
run_one() {
    update="$1" script="$2"
    name=$(basename "$script" .sim)
    run="$SIM/run/$name"
    fresh "$run"
    model=; case "$name" in pro-*) model=pro ;; esac
    if docker_run -e SLOT_ROOT="/src/$run/card/Slot" -e SLOT_HW_ROOT="/src/$run/hw" -e SLOT_SIM_MODEL="$model" \
        -e SLOT_SIM_TIME="$SIM_TIME" -e SLOT_SIM_SCRIPT="/src/$script" \
        -e SLOT_UPDATE_URL="file:///src/$run/releases" -e SLOT_NTP_SERVER= -e SLOT_SIM_VERSION=v0.1 -e SLOT_SIM_CARD="12.3/63.9 GB free" \
        -e SLOT_SIM_OUT="/src/$SIM/out" -e SLOT_SIM_GOLDEN=/src/brick/sim/golden \
        -e SLOT_SIM_UPDATE="$update" "$IMAGE" timeout 300 "$BIN" > "$SIM/out/$name.log" 2>&1
    then
        echo "PASS $name" > "$SIM/out/$name.result"
    else
        {
            echo "FAIL $name (log: $SIM/out/$name.log)"
            grep "^sim: " "$SIM/out/$name.log" | tail -3 | sed "s|/src/||g; s/^/    /"
        } > "$SIM/out/$name.result"
    fi
}

# The scenarios, JOBS at a time (as many as there are cores, unless set): each has its own
# card, log and screenshots, so they do not get in each other's way.
run_scenarios() {
    update="$1"; shift
    build
    rm -rf "$SIM/out"
    mkdir -p "$SIM/out"
    jobs=${JOBS:-$(nproc 2>/dev/null || echo 2)}
    # Each runs a game at full speed: more at once than there are cores to spare and they
    # fall behind their own timings (a held button let go late, a screenshot mid-animation).
    list=$(scenarios "$@")
    for script in $list; do
        [ -f "$script" ] || { echo "no scenario $script"; exit 2; }
    done
    for script in $list; do
        # One at a time, each runs in the foreground, as it always did.
        if [ "$jobs" -le 1 ]; then
            run_one "$update" "$script"
            continue
        fi
        while [ "$(jobs -p | wc -l)" -ge "$jobs" ]; do sleep 0.2; done
        run_one "$update" "$script" &
    done
    wait
    failed=""
    for script in $list; do
        name=$(basename "$script" .sim)
        cat "$SIM/out/$name.result"
        grep -q '^PASS' "$SIM/out/$name.result" || failed="$failed $name"
        rm -f "$SIM/out/$name.result"
    done
    [ -z "$failed" ] || { echo "failed:$failed"; exit 1; }
}

case "$cmd" in
    serve)
        port="${1:-8765}"
        build
        live="$SIM/live"
        [ -d "$live/card" ] && [ "${FRESH:-0}" != 1 ] || fresh "$live"
        docker rm -f "$NAME" > /dev/null 2>&1 || true
        echo "Brick simulator at http://127.0.0.1:$port/ (card kept in $live/card; FRESH=1 starts over)"
        # Published on this machine's loopback only.
        # art_hosts prints flags that are meant to be split into words.
        # shellcheck disable=SC2046
        docker_run --name "$NAME" $(art_hosts) -p "127.0.0.1:$port:$port" \
            -e SLOT_ROOT="/src/$live/card/Slot" -e SLOT_HW_ROOT="/src/$live/hw" -e SLOT_SIM_MODEL="${MODEL:-}" \
            -e SLOT_SIM_HTTP="0.0.0.0:$port" ${SLOT_SIM_TIME:+-e SLOT_SIM_TIME="$SLOT_SIM_TIME"} \
            -e SLOT_UPDATE_URL="${SLOT_UPDATE_URL:-file:///src/$live/releases}" \
            "$IMAGE" "$BIN"
        ;;
    stop) docker rm -f "$NAME" > /dev/null 2>&1 || true ;;
    test) run_scenarios 0 "$@" ;;
    update) run_scenarios 1 "$@" ;;
    unit)
        prepare
        docker_run "$IMAGE" cargo test --locked --release --target-dir "$SIM" \
            -p slot -p slot-power --no-default-features --features sim --lib --test led
        ;;
    *) sed -n '2,10p' "$0"; exit 2 ;;
esac
