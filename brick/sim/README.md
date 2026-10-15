# Brick simulator

The real Brick build of slot (`--features sim`), running on a PC in Docker. Mesa draws it in
software into an offscreen 1024×768 buffer, with the same EGL and GLES2 path the Brick uses.
It needs no GPU, no display and no device.

What is real: the whole frontend, menus, shelf, LED driver writes, battery and charger
handling, the mute switch, sleep and wake, settings, and saves.
What is stood in for:
- **Screen:** an offscreen buffer
- **Buttons:** a queue fed over HTTP or by scripts
- **Hardware files:** plain files under `SLOT_HW_ROOT` (`/sys/class/led_anim`, `power_supply`,
  `cpufreq`, `gpio243`, `/proc/net/route`), so nothing touches the real machine
- **Sound:** played into nothing
- **Sleep:** suspends until Power is pressed, without running `sleep.sh`

The four cores (mGBA, gpSP, FCEUmm, Snes9x) are built for the PC from the same pinned
sources and patches as the Brick's (`build-cores.sh`, once, then cached), so games really run,
on the CPU, at full speed. The test card has one real game, Pocket Test, a tiny Game Boy ROM
made by `make-rom.py`. The other carts are stand-ins that the cores refuse. To play your own
games under `task sim`, copy them into `target-sim/live/card/Slot/Games/<shelf>/`.

Cart art and label searches really go to the internet, so they need DNS that can answer for
`art.slot-cfw.fyi` and `images.launchbox-app.com`. Where the machine's resolver cannot (a
resolver that does not handle the `.fyi` TLD is the usual cause), `serve` looks the hosts up
itself and passes their addresses to the container.

What it can't tell you: speed and CPU clocks on the H700, real audio underruns, and Wi-Fi
(there's no supplicant). Test those on the Brick.

## Use

```sh
task sim            # or: sh brick/sim.sh serve [PORT]   -> http://127.0.0.1:8765/
task sim:test       # run every scenario, compare with the goldens
task sim:update     # accept the current screens as the new goldens
task sim:unit       # unit tests of the sim build
```

The page shows the screen live. Keys: arrows, X=A, Z=B, S=X, A=Y, Q/W=L1/R1, 1/2=L2/R2,
Enter=Start, Shift=Select, M=Menu, P=Power, -/= volume.

## HTTP API

Only answers on localhost. Every POST needs the header `X-Slot-Sim: 1`.

| Call | Does |
| :--- | :--- |
| `GET /frame.png` | the screen now |
| `GET /state` | phase, frame count, idle time, LED files, backlight, battery, charging, mute, online |
| `POST /press?btn=a&ms=80` | press and release (`up down left right a b x y l1 r1 l2 r2 start select menu volup voldown power`) |
| `POST /down?btn=menu`, `POST /up?btn=menu` | for chords |
| `POST /idle?settle=400&timeout=10000` | returns once the screen holds still |
| `POST /set?knob=battery&value=15` | knobs: `battery` 0–100, `charging` 0/1, `mute` 0/1, `online` 0/1 |
| `POST /quit` | stop |

```sh
curl -XPOST -H 'X-Slot-Sim: 1' '127.0.0.1:8765/press?btn=menu'
curl -XPOST -H 'X-Slot-Sim: 1' '127.0.0.1:8765/idle' && curl -o screen.png 127.0.0.1:8765/frame.png
```

## Scenarios

`scenarios/NAME.sim` holds one step per line; see `crates/slot/src/sim/script.rs` for the
full list. Each scenario starts on a fresh card with six stand-in carts and the clock fixed
at 2026-01-01 00:00. `snap NAME` waits for the screen to settle and saves
`target-sim/out/SCENARIO/NAME.png`. It fails if any pixel differs from `golden/SCENARIO/NAME.png`,
and writes `NAME.diff.png` with the changed pixels in magenta. CI runs all scenarios on
every pull request (`.github/workflows/sim.yml`) and keeps the screenshots as an artifact.

When a change is meant to alter the screen, run `task sim:update`, look at the new goldens, and
commit them with the change.
