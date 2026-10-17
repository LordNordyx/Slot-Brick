<p align="center">
  <img src="assets/slot-brick-banner.png" alt="Slot. for TrimUI Brick — retro handheld and cartridge artwork" width="100%">
</p>

<h1 align="center">Slot. for TrimUI Brick</h1>

<p align="center">
  A lightweight frontend for your Game Boy, Game Boy Color, Game Boy Advance, NES, and SNES collection.<br>
  Built for the <strong>original TrimUI Brick</strong> running <strong>stock firmware</strong>.
</p>

<p align="center">
  <strong>GB · GBC · GBA · NES · SNES</strong> &nbsp;|&nbsp; Stock firmware &nbsp;|&nbsp; Work in progress
</p>

<p align="center">
  <a href="../../releases"><strong>Download</strong></a> ·
  <a href="#installation">Installation</a> ·
  <a href="#controls">Controls</a> ·
  <a href="#troubleshooting">Troubleshooting</a> ·
  <a href="brick/CHANGELOG.md">Changelog</a>
</p>

---

A port of [Slot.](https://github.com/BrandonKowalski/slot), originally developed by **Brandon T. Kowalski** for the Anbernic RG SP. This version adapts Slot. to the original TrimUI Brick, including its screen, controls, LEDs, and stock firmware startup.

> [!IMPORTANT]
> **Unofficial port · Active development**
>
> This port may contain bugs, incomplete functionality, or missing features. Further improvements are planned, particularly to button shortcuts.
>
> Please report port-specific bugs in **this repository**, rather than in the upstream Slot. project.

## Features

| Feature | Details |
| :--- | :--- |
| **NES and SNES** | Their own shelves of NES and SNES cartridges, running on FCEUmm and Snes9x. No BIOS needed. |
| **Custom cartridges** | Change any cart's colour, finish, and shape from the shelf. |
| **Brick integration** | Screen and control support, plus LED power, brightness, colour, and effects. |
| **Sleep support** | Deep sleep with low battery drain. Auto-sleep after 5 minutes idle in menus or 15 minutes in game. |
| **Low battery warning** | F1/F2 stay red at 15% or below and blink rapidly at 10% or below (500 ms cycle), even with decorative LEDs off. Charging restores your lights; at 5%, Slot saves and shuts down. |
| **Adaptive performance** | Adaptive CPU clocks for smooth audio. |
| **Wireless connectivity** | Wi-Fi support and an experimental two-player link over Wi-Fi. |
| **Startup handling** | Support for slow-mounting SD cards. See the known startup issue below. |

## Installation

### 1. Back up your SD card

Copy any important files from your existing SD card to your computer.

> [!WARNING]
> Formatting the SD card erases **all existing data**. Make a backup before continuing.

### 2. Download the latest release

Download `slot-brick-<version>-sd-card.zip` from the [Releases](../../releases) page and extract it on your computer.

### 3. Format the SD card

Use a formatting tool such as [Rufus](https://rufus.ie/) with these settings:

| Setting | Value |
| :--- | :--- |
| File system | `FAT32` |
| Volume label | `SLOT` |

### 4. Copy the installation files

Copy **all contents** of the extracted `SD-CARD` folder to the **root of your SD card**.

> [!IMPORTANT]
> Include hidden files and folders, especially `.slot-boot`. Copy the contents of `SD-CARD`, rather than the folder itself.

### 5. Add your games and saves

Copy your games, save files, save states, and BIOS files from your backup into their respective folders inside `Slot/`.

> [!TIP]
> Create game labels (cartridge artwork) with [Cart Studio](https://slot-cfw.fyi/) by Brandon T. Kowalski.

### 6. Launch Slot.

Insert the SD card into your TrimUI Brick and power on the device. Slot. should launch automatically.

> [!NOTE]
> **Known startup issue:** On first launch or after a restart, the stock firmware menu may appear instead of Slot.
>
> Open **Apps → Slot** to launch it manually. This should restore automatic startup.

## Controls

The three small buttons on the bottom of the Brick are **HOME** (left, labelled Menu), **SELECT** (middle), and **START** (right).

**Notation:** `/` means either button. `+` means hold the first button and press the second.

### Carousel

| Input | Action |
| :--- | :--- |
| `Left` / `Right` | Browse the carousel |
| `Up` / `Down` | Jump to the previous / next letter |
| `L1` / `R1` | Switch between the GB, GBC, GBA, NES, and SNES shelves |
| Tap `A` | Resume the last save state |
| Hold `A` | Start the game fresh |
| `START` | Customise the cartridge (GBA: also choose the emulator core) |
| `HOME` | Open the menu: settings, Wi-Fi, and About |

### In game

| Input | Action |
| :--- | :--- |
| Hold `HOME` | Save state, eject the cartridge, and return to the carousel |
| Double-tap `HOME` | Open the save state switcher: load, delete, or undo within 30 seconds |
| `SELECT` + `B` | Toggle LEDs |
| `SELECT` + `HOME` | Open multiplayer Host / Join |
| `SELECT` + `R1` | Save state |
| `SELECT` + `L1` | Load the most recent save state |
| `SELECT` + `A` | Cycle shaders (next) |
| `SELECT` + `X` | **GBA / SNES:** switch between original and full screen; saved per game |
| `SELECT` + `Y` | Toggle colour correction |
| `SELECT` + `Up` / `Down` | Adjust brightness |
| `SELECT` + `Left` / `Right` | Adjust the blue light filter |
| `L1` / `R1` | **GB / GBC / NES:** switch between stretched / original picture. On SNES they are the game's own buttons |
| Hold `L2` | Rewind |
| Hold `R2` | Fast-forward |
| Double-tap `R2` | Lock fast-forward on; press again to unlock |

### System

| Input | Action |
| :--- | :--- |
| Volume `+` / `−` | Adjust volume |
| `Fn` slider | Mute / unmute |
| Press `Power` | Sleep / wake; game state is saved before sleep |
| Hold `Power` | Shut down or exit to the stock OS |

## Customising cartridges

Press `START` on a game on the shelf to change its cartridge:

- **Colour** (Game Boy, GBA and Famicom): the original, plain colours, or special-edition plastics.
- **Finish** (Game Boy and GBA): solid, see-through, or glitter.
- **Shape:** Game Boy carts can use the original or the Color shape, NES carts the NES or a Famicom cart, and SNES carts the US or the Europe / Japan cart.
- **Label:** with Wi-Fi on, `Y` opens a search with the game's name filled in: press `START` to search as it is, or change it first. `Left` / `Right` then flips through every label found, shown on the cart as you go.

NES and SNES carts are always their system's grey, like the real ones. Famicom carts came in many colours, so a NES game on the Famicom cart can have any colour.

The cart changes as you go. `A` saves, `B` puts it back, and **Reset to original** restores the game's own look. On a GBA cart, `START` opens the core picker instead: `Left` / `Right` change the core, `Up` the colour, `Down` the finish, `Y` searches for a label, and `L1` / `R1` flips through the ones found.

## NES and SNES

- Put `.nes` ROMs in `Slot/Games/NES` and `.sfc` / `.smc` ROMs in `Slot/Games/SNES`.
- No BIOS is needed for either.
- European and Japanese SNES games get the rounder PAL / Super Famicom cart. Slot reads this from the ROM, so ripped ROMs without a region in their name work too.
- Japanese NES games get the small, coloured Famicom cart. NES ROMs don't say where they were sold, so Slot goes by the name, such as `(Japan)`, and by chips only Famicom carts used. For any other game, choose **Famicom** under `START` → **Shape**.
- On SNES, `X`, `Y`, `L1`, and `R1` go to the game.

## Stock firmware apps

These utilities appear under **Apps** in the stock OS:

| App | Purpose |
| :--- | :--- |
| **Slot** | Launch Slot. and enable automatic startup. |
| **Logo** | Set a custom boot logo or restore the original. |
| **Check** | Report why Slot. did or did not start, without changing anything. |
| **Stock** | Disable Slot. autostart and restore stock startup. |

## Multiplayer

> [!WARNING]
> **Experimental:** Multiplayer has not yet been tested on two physical devices.

1. Install the **same Slot. version** on both Bricks, using the **same game, emulator core, and BIOS**.
2. In game, press `SELECT` + `HOME` to open the game menu, then choose **Host** or **Join**.
3. Start **Host** on one Brick first, then **Join** on the other.

If the connection fails, attach both logs to your issue:

```text
Slot/Logs/multiplayer.log
Slot/Logs/slot.log
```

## Troubleshooting

| Issue or preference | What to do |
| :--- | :--- |
| Stock menu appears instead of Slot. | Launch **Apps → Slot**. For diagnostics, run **Apps → Check** and read `startup-report.txt` on the SD card. |
| Skip Slot. at startup | Create an empty file named `Slot/DISABLE_AUTOBOOT`. |
| Disable auto-sleep | Create an empty file named `Slot/DISABLE_AUTO_SLEEP`. |
| Restore stock startup | Run **Apps → Stock**. |

## Development

Brick-specific code is gated behind the `brick` Cargo feature.

- **Device scripts and apps:** [`brick/`](brick/)
- **Simulator:** [`brick/sim/README.md`](brick/sim/README.md) — the real Brick build running on a PC in Docker, with its screen and buttons in a browser. Needs no device, and is how a frontend change is checked before a card is written.
- **Change history:** [`brick/CHANGELOG.md`](brick/CHANGELOG.md)
- **Dev builds:** every change to `main` is built automatically and published as the [Dev build](../../releases/tag/dev) release, replacing the previous one.

## Credits

| Project / contributor | Contribution |
| :--- | :--- |
| [Brandon T. Kowalski](https://github.com/BrandonKowalski/slot) | Original Slot. frontend and Cart Studio |

## AI Disclosure
This port was developed with assistance from [Claude Opus](https://claude.ai/). 

---

<p align="center"><sub>Slot. for TrimUI Brick · An unofficial port by LordNordyx</sub></p>
