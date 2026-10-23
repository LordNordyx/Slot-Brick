# The Brick simulator

The real Brick build of slot (`--features sim`), running on a PC in Docker. Mesa draws it in
software into an offscreen 1024×768 buffer, through the same EGL and GLES2 path the Brick uses.
No GPU, no display and no device: if you can run Docker, you can see the Brick's screen and
press its buttons.

It exists so a change to the frontend can be seen and tested in seconds rather than by writing
a card and booting hardware. Nothing about it is a mock of the app — it *is* the app, with its
screen, buttons and sysfs files stood in for.

## Getting started

You need **Docker** and **python3**. The `task` runner is optional; every `task sim:*` below
has an `sh brick/sim.sh ...` equivalent.

```sh
task sim                      # or: sh brick/sim.sh serve [PORT]
```

Then open **http://127.0.0.1:8765/**. The first run builds the image, compiles slot and builds
the four cores, which takes several minutes; later runs start in seconds.

The page shows the screen live and takes the keyboard: arrows, X=A, Z=B, S=X, A=Y, Q/W=L1/R1,
1/2=L2/R2, Enter=Start, Shift=Select, M=Menu, P=Power, `-`/`=` volume.

The card lives in `target-sim/live/card`, and is kept between runs. Put your own games in
`target-sim/live/card/Slot/Games/<GB|GBC|GBA|NES|SNES>/` and restart. `FRESH=1 task sim` throws
the card away and starts over.

**On WSL**, clone into the Linux filesystem rather than under `/mnt/c`, or the build will crawl
and file modes will not stick. If `localhost:8765` does not answer from Windows, put
`networkingMode=mirrored` under `[wsl2]` in `%UserProfile%\.wslconfig` and `wsl --shutdown`.
Do not reach it by the WSL IP: it answers to localhost only, on purpose.

## Brick Pro

`MODEL=pro task sim` (or `MODEL=pro sh brick/sim.sh serve`) makes the simulator a Brick Pro: its
MainUI names it one, its LED driver has the Pro's rear zone, and it has a left stick, pushed with
<kbd>I</kbd> <kbd>J</kbd> <kbd>K</kbd> <kbd>L</kbd> on the page, `POST /stick?x=&y=` (-32767..32767), or `stick DIR [ms]` in a scenario.
Scenarios whose names start with `pro-` always run as a Brick Pro.

## What is real, and what is not

Real: the whole frontend, menus, shelf, LED driver writes, battery and charger handling, the
mute switch, sleep and wake, settings, saves, and the games themselves. The four cores (mGBA,
gpSP, FCEUmm, Snes9x) are built for the PC from the same pinned sources and patches as the
Brick's, so games really run, on the CPU, at full speed.

Stood in for:

| | |
| :--- | :--- |
| Screen | an offscreen buffer, read out as PNG |
| Buttons | a queue fed over HTTP or by a scenario |
| Hardware files | plain files under `SLOT_HW_ROOT`: `/sys/class/led_anim`, `power_supply`, `cpufreq`, `gpio243`, `/proc/net/route` |
| Sound | played into nothing |
| Sleep | suspends until Power is pressed, without running `sleep.sh` |

**It cannot tell you** about speed or CPU clocks on the H700, real audio underruns, or Wi-Fi:
there is no supplicant, so the Wi-Fi menu cannot connect. Test those on the Brick.

The one real game on the test card is Pocket Test, a tiny Game Boy ROM made by `make-rom.py`;
the other carts are stand-ins that the cores refuse.

## Checking a change you have made

The usual loop. Say you changed something on the cart menu:

```sh
task sim                                                     # leave it running
curl -XPOST -H 'X-Slot-Sim: 1' '127.0.0.1:8765/press?btn=a'  # set the clock
curl -XPOST -H 'X-Slot-Sim: 1' '127.0.0.1:8765/press?btn=start'
curl -XPOST -H 'X-Slot-Sim: 1' '127.0.0.1:8765/idle'         # wait for it to settle
curl -o shot.png 127.0.0.1:8765/frame.png                    # then look at shot.png
```

Rebuild after editing: stop it (Ctrl+C, or `sh brick/sim.sh stop`) and start it again.

Searching for a label needs the `online` knob, since the app will not look without Wi-Fi:

```sh
curl -XPOST -H 'X-Slot-Sim: 1' '127.0.0.1:8765/set?knob=online&value=1'
```

## HTTP API

Answers on localhost only. Every POST needs the header `X-Slot-Sim: 1`.

| Call | Does |
| :--- | :--- |
| `GET /frame.png` | the screen now |
| `GET /state` | phase, frame count, idle time, LED files, backlight, battery, charging, mute, online |
| `POST /press?btn=a&ms=80` | press and release |
| `POST /down?btn=menu`, `POST /up?btn=menu` | for chords and held keys |
| `POST /idle?settle=400&timeout=10000` | returns once the screen holds still |
| `POST /set?knob=battery&value=15` | knobs: `battery` 0–100, `charging` 0/1, `mute` 0/1, `online` 0/1 |
| `POST /quit` | stop |

Buttons: `up down left right a b x y l1 r1 l2 r2 start select menu volup voldown power`.

Playwright works the same way: open the page and use the keys above.

## Scenarios

A scenario drives the Brick and checks what it did. `scenarios/NAME.sim` holds one step a line;
`#` starts a comment.

```sh
task sim:test                 # every scenario
sh brick/sim.sh test boot     # one of them
task sim:unit                 # the sim build's own unit tests
```

| Step | Does |
| :--- | :--- |
| `press BTN [MS]` | press and release, 80 ms by default |
| `hold BTN [MS]` | hold it down for longer |
| `down BTN` / `up BTN` | for chords |
| `idle [SETTLE] [TIMEOUT]` | wait until the screen holds still (400 ms / 15 s) |
| `wait [MS]` | wait regardless |
| `phase NAME [TIMEOUT]` | fail unless the Brick reaches that phase, eg `Shelf`, `QuickMenu` |
| `set KNOB VALUE` | as the HTTP knobs |
| `led FILE VALUE` | fail unless an LED file holds that value, eg `led effect_rgb_hex_m FFFFFF` |
| `snap NAME` | settle, save the screen, and compare it with the golden |
| `grab NAME` | save the screen as it is this moment, unsettled and with no golden: for animations |

Each scenario starts on a fresh card with six stand-in carts and the clock fixed at
2026-01-01 00:00, so a run is the same every time. A scenario that needs more on its card gets it
from `scenarios/NAME.card.sh`, run with the card's `Slot` folder once the card is made: see
`compressed.card.sh`, which adds zipped and 7z'd copies of Pocket Test with
`brick/sim/make-archive.py`. Screenshots land in
`target-sim/out/SCENARIO/`. A `snap` that differs writes `NAME.diff.png` with the changed
pixels in magenta.

CI runs the unit tests and every scenario on each pull request (`.github/workflows/sim.yml`)
and keeps the screenshots as an artifact.

### Goldens, and when not to have one

`task sim:update` accepts the current screens as the new goldens. **Look at them before you
commit them** — that is the only step that stops a bug being blessed as correct.

Only `boot` and `quick-menu` keep goldens. The rest check what the Brick did rather than what
it looked like: the phase it reached, the LED files it wrote, how it answered a flat battery.
A screen still being designed changes every few hours, and a golden that is only ever accepted
unread asserts nothing while looking like it does. Add a golden once a screen has settled.

A screen whose content comes off the network cannot have one at all.

## Gotchas

- **A battery of 5% or less is a real shutdown**, which ends the run. The battery is read every
  10 s, so a scenario needs `wait 10500` after `set battery` before the Brick has noticed.
- **L1/R1 change the picture mode** inside a GB, GBC or NES game, and switch shelves outside one.
- **In a scenario, a label search never leaves the machine**: it finds one made-up label, so the
  label screens can be walked through. Under `serve` it is the real search.
- **Cart art and label searches really go to the internet.** They need DNS that can answer for
  `art.slot-cfw.fyi` and `images.launchbox-app.com`. Where the machine's resolver cannot — a
  resolver that does not handle the `.fyi` TLD is the usual cause — `serve` looks the hosts up
  itself (`resolve.py`) and passes their addresses to the container.
- **The desktop build** (`default` features) wants libudev and ALSA headers. That is nothing to
  do with the sim.

## How it is put together

- **`--features sim`** is the Brick build with the device loop hooked: the screen is an
  offscreen EGL pbuffer (`FbdevSurface::offscreen`, `read_rgba`), input comes from `SimInput`,
  sleep waits for Power, and `sim.tick(...)` runs each frame. See `crates/slot/src/device_app.rs`.
- **`slot_power::hw(path)`** is the one place a hardware path is named. Under sim it prefixes
  `SLOT_HW_ROOT`, so nothing reaches the real machine.
- **`crates/slot/src/sim/`** — `mod.rs` holds the shared state and settle detection, `server.rs`
  the HTTP API and the page, `script.rs` the scenario runner and golden compare, `fixture.rs`
  the stand-in sysfs files and the knobs, `image.rs` PNG encode, decode and compare.
- **`brick/sim.sh`** — `serve`, `test`, `update`, `unit`, `stop`. Beside it: `Dockerfile`
  (Debian bookworm, Rust, Mesa), `build-cores.sh`, `make-rom.py`, `resolve.py`, `scenarios/`
  and `golden/`.

To add a knob, add it to `fixture::set` and to the report in `fixture::report`. To add a
scenario step, add it to the match in `script.rs`.

Environment: `SLOT_ROOT`, `SLOT_HW_ROOT`, `SLOT_SIM_HTTP`, `SLOT_SIM_SCRIPT`, `SLOT_SIM_OUT`,
`SLOT_SIM_GOLDEN`, `SLOT_SIM_UPDATE`, `SLOT_SIM_TIME`.
