#!/bin/sh
# The Brick simulator: the real Brick build of slot, on a PC, in Docker, no GPU or device needed.
# Usage: brick/sim.sh serve [PORT]     run it, with its screen and buttons at http://127.0.0.1:PORT/
#        brick/sim.sh test [NAME...]   run brick/sim/scenarios (all, or those named); fail on any
#                                      screenshot that differs from brick/sim/golden
#        brick/sim.sh update [NAME...] the same, writing the screenshots as the new goldens
#        brick/sim.sh unit             the sim build's own unit tests
#        brick/sim.sh stop             stop a running `serve`
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

build() {
    docker build -q -t "$IMAGE" -f brick/sim/Dockerfile brick/sim > /dev/null
    mkdir -p .docker-cache/cargo
    docker_run "$IMAGE" cargo build --locked --release --target-dir "$SIM" \
        -p slot --no-default-features --features sim
    # The Brick's four cores, from the same sources, for this PC. Built once, then kept.
    docker_run "$IMAGE" sh brick/sim/build-cores.sh
}

# A fresh SD card and fresh stand-in hardware in $1: the card layout slot makes itself, the
# Brick card's data files and the cores, and a cartridge per shelf. Only Pocket Test is a real
# game (brick/sim/make-rom.py); the others are stand-ins that fill the shelf, which the cores
# refuse. Put your own games in target-sim/live/card/Slot/Games to play them under `serve`.
fresh() {
    rm -rf "$1"
    mkdir -p "$1/hw"
    docker_run "$IMAGE" "$BIN" --init-root "$1/card/Slot" > /dev/null
    cp brick/card/Slot/System/*.txt "$SIM"/cores/*_libretro.so "$1/card/Slot/System/"
    for cart in "GBA/Sim Quest.gba" "GBA/Pixel Racer.gba" "GB/Pocket Test.gb" \
                "GBC/Colour Check.gbc" "NES/Famicom Probe.nes" "SNES/Super Sample.sfc"; do
        printf 'slot simulator stand-in cartridge\n' > "$1/card/Slot/Games/$cart"
    done
    python3 -I brick/sim/make-rom.py "$1/card/Slot/Games/GB/Pocket Test.gb"
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

run_scenarios() {
    update="$1"; shift
    build
    rm -rf "$SIM/out"
    mkdir -p "$SIM/out"
    failed=""
    for script in $(scenarios "$@"); do
        [ -f "$script" ] || { echo "no scenario $script"; exit 2; }
        name=$(basename "$script" .sim)
        run="$SIM/run/$name"
        fresh "$run"
        if docker_run -e SLOT_ROOT="/src/$run/card/Slot" -e SLOT_HW_ROOT="/src/$run/hw" \
            -e SLOT_SIM_TIME="$SIM_TIME" -e SLOT_SIM_SCRIPT="/src/$script" \
            -e SLOT_SIM_OUT="/src/$SIM/out" -e SLOT_SIM_GOLDEN=/src/brick/sim/golden \
            -e SLOT_SIM_UPDATE="$update" "$IMAGE" timeout 300 "$BIN" > "$SIM/out/$name.log" 2>&1
        then
            echo "PASS $name"
        else
            echo "FAIL $name (log: $SIM/out/$name.log)"
            grep "^sim: " "$SIM/out/$name.log" | tail -3 | sed "s|/src/||g; s/^/    /"
            failed="$failed $name"
        fi
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
            -e SLOT_ROOT="/src/$live/card/Slot" -e SLOT_HW_ROOT="/src/$live/hw" \
            -e SLOT_SIM_HTTP="0.0.0.0:$port" ${SLOT_SIM_TIME:+-e SLOT_SIM_TIME="$SLOT_SIM_TIME"} \
            "$IMAGE" "$BIN"
        ;;
    stop) docker rm -f "$NAME" > /dev/null 2>&1 || true ;;
    test) run_scenarios 0 "$@" ;;
    update) run_scenarios 1 "$@" ;;
    unit)
        build
        docker_run "$IMAGE" cargo test --locked --release --target-dir "$SIM" \
            -p slot -p slot-power --no-default-features --features sim --lib --test led
        ;;
    *) sed -n '2,10p' "$0"; exit 2 ;;
esac
