<p align="center">
  <img src="assets/slot-brick-banner.png" alt="Slot. for TrimUI Brick and Brick Hammer — retro handheld and cartridge artwork" width="100%">
</p>

<h1 align="center">Slot. for TrimUI Brick and Brick Hammer</h1>

<p align="center">
  A lightweight frontend for your Game Boy, Game Boy Color, Game Boy Advance, NES, and SNES collection.<br>
  Built for the <strong>TrimUI Brick</strong> and <strong>Brick Hammer</strong>, on <strong>stock firmware</strong> or as its own custom firmware, <strong>Slot OS</strong>.
</p>

<p align="center">
  <strong>GB · GBC · GBA · NES · SNES</strong> &nbsp;|&nbsp; Stock firmware or Slot OS &nbsp;|&nbsp; Work in progress
</p>

<p align="center">
  <a href="../../releases"><strong>Download</strong></a> ·
  <a href="#installation">Installation</a> ·
  <a href="#controls">Controls</a> ·
  <a href="#troubleshooting">Troubleshooting</a> ·
  <a href="brick/CHANGELOG.md">Changelog</a>
</p>

---

A port of [Slot.](https://github.com/BrandonKowalski/slot), originally developed by **Brandon T. Kowalski** for the Anbernic RG SP. This version adapts Slot. to the TrimUI Brick and Brick Hammer, including its screen, controls, LEDs, and stock firmware startup.

> [!IMPORTANT]
> **Unofficial port · Active development**
>
> This port may contain bugs, incomplete functionality, or missing features. Further improvements are planned, particularly to button shortcuts.
>
> Please report port-specific bugs in **this repository**, rather than in the upstream Slot. project.

<a name="features"></a>
<details>
<summary><h2>Features</h2></summary>

| Feature | Details |
| :--- | :--- |
| **NES and SNES** | Their own shelves of NES and SNES cartridges, running on FCEUmm and Snes9x. No BIOS needed. |
| **PlayStation (experimental)** | A shelf of discs that spin into a disc slot, on PCSX ReARMed: `.cue`/`.bin`, `.chd`, `.pbp`, `.iso` and `.m3u`, with a disc picker for games of several discs. Not yet tested on a Brick ([#41](../../issues/41)). |
| **Custom cartridges** | Change any cart's colour, finish, and shape from the shelf. |
| **Cart art** | Fetch real cartridge labels for every game at once, or search for one game's label and frame it to fit the cart. |
| **RetroAchievements** | Log in from the menu and earn achievements in softcore mode. |
| **Overlays** | Frames for GB, GBC, GBA, NES and SNES games, by KrutzOtrem, or your own 1024×768 ones; the game fits itself into the overlay's window. |
| **Brick and Brick Hammer integration** | Screen and control support, plus LED power, brightness, colour, and effects. |
| **Brick Pro (experimental)** | Recognised by itself: the left stick works as the D-pad, and its stick lights have their own LEDs setting. Not yet tested on a Brick Pro; reports welcome ([#45](../../issues/45)). |
| **Sleep support** | Deep sleep with low battery drain. Auto-sleep after 5 minutes idle in menus or 15 minutes in game. |
| **Low battery warning** | F1/F2 stay red at 15% or below and blink rapidly at 10% or below (500 ms cycle), even with decorative LEDs off. Charging restores your lights; at 5%, Slot saves and shuts down (3%, 5%, 10% or off under System → Low Battery Shutdown). |
| **Adaptive performance** | Adaptive CPU clocks for smooth audio. |
| **Updates** | Update Slot from the menu over Wi-Fi: signed and checked, with your games, saves and settings left alone. |
| **Library** | The selected game's name on a title card under its cart for a moment, play time beside the battery (minutes, or hours and minutes), favorites with a star, and shelves sorted A–Z, by last played or by most played, or showing only your favorites. |
| **Fonts** | HOME → System → Font: Slot's own font, GohuFont, Press Start 2P, or any `.ttf` / `.otf` you put in `Slot/Fonts`. |
| **Auto clock** | The clock sets itself from an internet time server whenever Wi-Fi is on. |
| **Wireless connectivity** | Wi-Fi, with its status shown beside the clock, and an experimental two-player link over Wi-Fi. |
| **Startup handling** | Support for slow-mounting SD cards. See the known startup issue below. |


</details>

<a name="installation"></a>
<details>
<summary><h2>Installation</h2></summary>

Slot. runs two ways. Pick one:

- **Stock firmware** (recommended): Slot. starts from TrimUI's own system. Copy files onto a card; the stock OS and its apps stay one button away.
- **Slot OS** (custom firmware, experimental): the Brick starts straight from the card into Slot., in about 4 seconds instead of 9. Flash an image onto a card. Nothing on the Brick itself is changed: take the card out and it starts as before.

<details>
<summary><h3>Stock firmware</h3></summary>

#### 1. Back up your SD card

Copy any important files from your existing SD card to your computer.

> [!WARNING]
> Formatting the SD card erases **all existing data**. Make a backup before continuing.

#### 2. Download the latest release

Download `slot-brick-<version>-sd-card.zip` from the [Releases](../../releases) page and extract it on your computer.

#### 3. Format the SD card

Use a formatting tool such as [Rufus](https://rufus.ie/) with these settings:

| Setting | Value |
| :--- | :--- |
| File system | `FAT32` |
| Volume label | `SLOT` |

#### 4. Copy the installation files

Copy **all contents** of the extracted `SD-CARD` folder to the **root of your SD card**.

> [!IMPORTANT]
> Include hidden files and folders, especially `.slot-boot`. Copy the contents of `SD-CARD`, rather than the folder itself.

#### 5. Add your games and saves

Copy your games, save files, save states, and BIOS files from your backup into their respective folders inside `Slot/`.

> [!TIP]
> Create game labels (cartridge artwork) with [Cart Studio](https://slot-cfw.fyi/) by Brandon T. Kowalski.

#### 6. Launch Slot.

Insert the SD card into your TrimUI Brick or Brick Hammer and power on the device. Slot. should launch automatically.

> [!NOTE]
> **Known startup issue:** On first launch or after a restart, the stock firmware menu may appear instead of Slot.
>
> Open **Apps → Slot** to launch it manually. This should restore automatic startup.

</details>

<details>
<summary><h3>Slot OS (custom firmware, experimental)</h3></summary>

#### 1. Back up your SD card

Flashing the image erases **the whole card**. Copy everything on it to your computer, hidden folders included (in File Explorer: **View → Show → Hidden items**). Your games, saves, states, settings, labels and BIOS are all in `Slot`.

#### 2. Download the image

Download `slot-os-<version>-sd-card.img.xz` from the [Releases](../../releases) page. There is no need to extract it.

#### 3. Flash it onto the card

1. Install [Raspberry Pi Imager](https://www.raspberrypi.com/software/) (balenaEtcher works too).
2. **Choose Device**: *No filtering*. **Choose OS**: *Use custom*, and pick the image. **Choose Storage**: the SD card. Check its size: the drive you pick is erased.
3. **Next**, *No* to customisation, *Yes* to erase. It writes the card and checks it.

> [!WARNING]
> Windows may then say *You need to format the disk in drive X:*, once or several times. **Always click Cancel.** Those are Slot OS's boot partitions; formatting one breaks the card.

#### 4. Start the Brick once

Put the card in the Brick and turn it on. On its first start, Slot OS grows the card to its full size (a few seconds, once), then Slot. starts. Turn it off by holding `Power`.

> [!TIP]
> Before that first start the card shows only 2.5 GB. Games copied on before it stay, but copy a large collection after it.

#### 5. Add your games and saves

The card now shows one drive in Windows, **SLOT**, with a `Slot` folder. From your backup, copy into `Slot/`: `Games`, `Saves`, `States`, `BIOS`, `Labels`, and `Config` for your settings. Join your Wi-Fi again under **HOME → Online → Wi-Fi**: networks saved on stock firmware are kept on the Brick, not on the card.

#### What is different on Slot OS

- Holding `Power` shuts down: there is no stock OS to exit to.
- The stock firmware apps (below) are not there; take the card out to use the stock OS.
- **HOME → Online → Update** updates Slot OS itself as well as Slot. If an update does not start, the previous version is put back.
- If the Brick loses power while running, the card is checked and repaired at the next start.

#### Going back to stock firmware

Copy what you want to keep from the card (`Slot/Saves`, `Slot/States`), format it with [SD Card Formatter](https://www.sdcard.org/downloads/formatter/) (*Quick format*), and follow the stock firmware guide above.

</details>


</details>

<a name="controls"></a>
<details>
<summary><h2>Controls</h2></summary>

The three small buttons on the bottom of the Brick and Brick Hammer are **HOME** (left, labelled Menu), **SELECT** (middle), and **START** (right).

**Notation:** `/` means either button. `+` means hold the first button and press the second.

### Carousel

| Input | Action |
| :--- | :--- |
| `Left` / `Right` | Browse the carousel |
| `Up` / `Down` | Jump to the previous / next letter |
| `L1` / `R1` | Switch between the GBA, GB, GBC, NES, SNES and PlayStation shelves |
| Tap `A` | Resume the last save state |
| Hold `A` | Start the game fresh |
| `START` | Customise the cartridge (GBA: also choose the emulator core) |
| `Y` | Mark the game as a favorite (a star on its cart), or unmark it |
| `SELECT` | Sort the shelves: A–Z, last played, most played |
| `HOME` | Open the menu: Emulation, Gameplay, Library, Online (Wi-Fi, Cart Art, RetroAchievements, Update), and System (LEDs, Clock, Font, Rumble, Low Battery Shutdown, About) |

### In game

| Input | Action |
| :--- | :--- |
| Hold `HOME` | Save state, eject the cartridge, and return to the carousel |
| Double-tap `HOME` | Open the save state switcher: load, delete, or undo within 30 seconds |
| `SELECT` + `B` or `HOME` + `B` | Toggle LEDs |
| `SELECT` + `HOME` | Open multiplayer Host / Join |
| `SELECT` + `R1` | Save state |
| `SELECT` + `L1` | Load the most recent save state |
| `SELECT` + `A` or `HOME` + `A` | Next shader |
| `SELECT` + `X` | **GBA / SNES:** switch between original and full screen; saved per game |
| `SELECT` + `Y` | Toggle colour correction |
| `HOME` + `L1` / `R1` | Previous / next overlay for this system |
| `SELECT` + `L2` / `R2` | **GB:** previous / next colour palette |
| `SELECT` + `Up` / `Down` | Adjust brightness |
| `SELECT` + `Left` / `Right` | Adjust the blue light filter |
| `L1` / `R1` | **GB / GBC / NES:** switch between stretched / original picture. On SNES and PlayStation they are the game's own buttons |
| Hold `L2` | Rewind (not on PlayStation, where `L2` is the game's) |
| Hold `R2` | Fast-forward (**PlayStation:** `HOME` + `R2`, as `R2` is the game's) |
| Double-tap `R2` | Lock fast-forward on; press again to unlock |

### System

| Input | Action |
| :--- | :--- |
| Volume `+` / `−` | Adjust volume |
| `Fn` slider | Mute / unmute |
| Press `Power` | Sleep / wake; game state is saved before sleep |
| Hold `Power` | Shut down or exit to the stock OS (Slot OS: shut down) |


</details>

<a name="customising-cartridges"></a>
<details>
<summary><h2>Customising cartridges</h2></summary>

Press `START` on a game on the shelf to change its cartridge:

- **Colour** (Game Boy, GBA and Famicom): the original, plain colours, or special-edition plastics.
- **Finish** (Game Boy and GBA): solid, see-through, or glitter.
- **Shape:** Game Boy carts can use the original or the Color shape, NES carts the NES or a Famicom cart, and SNES carts the US or the Europe / Japan cart.
- **Label:** with Wi-Fi on, press `Y` to choose a label. See [Choosing a label](#choosing-a-label).

NES and SNES carts are always their system's grey, like the real ones. Famicom carts came in many colours, so a NES game on the Famicom cart can have any colour.

The cart changes as you go. `A` saves, `B` puts it back, and **Reset to original** restores the game's own look. On a GBA cart, `START` opens the core picker instead: `Left` / `Right` change the core, `Up` the colour, `Down` the finish, and `Y` searches for a label.

### Choosing a label

1. Press `Y` in the cart menu or the GBA core picker. A keyboard opens with the game's name filled in: press `START` to search as it is, or change it first.
2. The labels found appear in a grid. Move with the d-pad, press `A` on the one you want, `Y` to search again, or `B` to go back.
3. Frame the label to fit the cart's cutout. The d-pad moves the picture; hold `L1` and use the d-pad to make the frame bigger or smaller; `R1` evens out its shape; `Y` puts it back as it was. `A` keeps it, `B` goes back to the grid.


</details>

<a name="cart-art-and-retroachievements"></a>
<details>
<summary><h2>Cart Art and RetroAchievements</h2></summary>

Both need Wi-Fi and are greyed out in the `HOME` menu while it is off.

- **Cart Art:** **GET CART ART** gives every game without a label a scan of its real cartridge label. Labels you already have are never replaced. GB, GBC and GBA labels come from the art set behind Cart Studio, NES and SNES labels from the LaunchBox Games Database. Below the button, every game with a label is listed: press `A` twice on one to delete its label. It is quick but basic; use [Choosing a label](#choosing-a-label) for more control.
- **RetroAchievements:** log in to your [RetroAchievements](https://retroachievements.org) account. Only the login token is kept, never your password. Games then earn achievements in softcore mode, with a small toast for each unlock. Hardcore mode isn't available.


</details>

<a name="nes-and-snes"></a>
<details>
<summary><h2>NES and SNES</h2></summary>

- Put `.nes` ROMs in `Slot/Games/NES` and `.sfc` / `.smc` ROMs in `Slot/Games/SNES`.
- No BIOS is needed for either.
- European and Japanese SNES games get the rounder PAL / Super Famicom cart. Slot reads this from the ROM, so ripped ROMs without a region in their name work too.
- Japanese NES games get the small, coloured Famicom cart. NES ROMs don't say where they were sold, so Slot goes by the name, such as `(Japan)`, and by chips only Famicom carts used. For any other game, choose **Famicom** under `START` → **Shape**.
- On SNES, `X`, `Y`, `L1`, and `R1` go to the game.


</details>

<a name="overlays"></a>
<details>
<summary><h2>Overlays</h2></summary>

Slot comes with [KrutzOtrem](https://github.com/KrutzOtrem/Trimui-Brick-Overlays)'s overlays, made for the Brick and Brick Hammer: the *Vanilla* set for GB, GBC and GBA, and the *CRT* set for NES and SNES. Open **HOME → Emulation**, choose the system, and pick one on its **Overlay** row. They're off until you do.

To add your own, put 1024×768 PNG overlays in `Slot/Overlays/GB`, `GBC`, `GBA`, `NES` or `SNES`. The game shows through the part of the overlay that isn't fully opaque.

The game is scaled to fill that area without stretching. While an overlay is on, the in-game picture size shortcuts do nothing. In game, `HOME` + `L1` / `R1` steps through the system's overlays and Off, naming each as it goes on.

More of KrutzOtrem's sets are in their [repository](https://github.com/KrutzOtrem/Trimui-Brick-Overlays): copy the `GB`, `GBC` and `GBA` folders from a handheld set into `Slot/Overlays`.


</details>

<a name="stock-firmware-apps"></a>
<details>
<summary><h2>Stock firmware apps</h2></summary>

These utilities appear under **Apps** in the stock OS:

| App | Purpose |
| :--- | :--- |
| **Slot** | Launch Slot. and enable automatic startup. |
| **Logo** | Set a custom boot logo or restore the original. |
| **Check** | Report why Slot. did or did not start, without changing anything. |
| **Stock** | Disable Slot. autostart and restore stock startup. |


</details>

<a name="multiplayer"></a>
<details>
<summary><h2>Multiplayer</h2></summary>

> [!WARNING]
> **Experimental:** Multiplayer has not yet been tested on two physical devices.

1. Install the **same Slot. version** on both devices (Brick or Brick Hammer), using the **same game, emulator core, and BIOS**.
2. In game, press `SELECT` + `HOME` to open the game menu, then choose **Host** or **Join**.
3. Start **Host** on one Brick or Brick Hammer first, then **Join** on the other.

If the connection fails, attach both logs to your issue:

```text
Slot/Logs/multiplayer.log
Slot/Logs/slot.log
```


</details>

<a name="troubleshooting"></a>
<details>
<summary><h2>Troubleshooting</h2></summary>

| Issue or preference | What to do |
| :--- | :--- |
| Stock menu appears instead of Slot. | Launch **Apps → Slot**. For diagnostics, run **Apps → Check** and read `startup-report.txt` on the SD card. |
| Skip Slot. at startup | Create an empty file named `Slot/DISABLE_AUTOBOOT`. |
| Disable auto-sleep | Create an empty file named `Slot/DISABLE_AUTO_SLEEP`. |
| Restore stock startup | Run **Apps → Stock**. |


</details>

<a name="development"></a>
<details>
<summary><h2>Development</h2></summary>

Brick and Brick Hammer code is gated behind the `brick` Cargo feature.

- **Device scripts and apps:** [`brick/`](brick/)
- **Simulator:** [`brick/sim/README.md`](brick/sim/README.md) — the real Brick and Brick Hammer build running on a PC in Docker, with its screen and buttons in a browser. Needs no device, and is how a frontend change is checked before a card is written.
- **Change history:** [`brick/CHANGELOG.md`](brick/CHANGELOG.md)
- **Dev builds:** every change to `main` is built automatically and published as the [Dev build](../../releases/tag/dev) release, replacing the previous one.

### Making a release

Pushing a `vX.Y` tag builds the card from that tag and publishes it as a release, with
`update.txt` and `update.txt.sig` beside the zip: what **HOME → Online → Update** finds the release by.
`update.txt` is signed with the release key, kept in the `UPDATE_SIGNING_KEY` repository
secret (an Ed25519 private key, PEM); its public half is in `crates/slot/src/update.rs`, and
Slot installs nothing it doesn't verify. Without the secret, a release refuses to build.
The same release carries Slot OS, built from the same card (`brick/os/build.sh`, see
[`brick/os/README.md`](brick/os/README.md)): `slot-os-vX.Y-sd-card.img.xz` to flash, and
`slot-os-vX.Y-update.zip` with its own signed `update-os.txt`, which Slot OS's Update reads.
Every merge to `main` does the same for the Dev build.
Pushing a tag needs write access, so only maintainers can start one. Before tagging:

1. Put the version at the top of [`brick/README.md`](brick/README.md) — it is what the card
   says it is — and in `brick/card/Apps/Slot/config.json`, `brick/card/Slot/System/launch.sh`,
   `brick/card/Slot/System/diagnose.sh` and `PORT_VERSION` in `crates/slot-ui/src/sticker.rs`
   (the About screen).
2. Move the `## Unreleased` notes in [`brick/CHANGELOG.md`](brick/CHANGELOG.md) under a
   `## vX.Y` heading (with `**New since vX.W:**` under it), leaving an empty `## Unreleased`
   above it. Those notes become the release's text.
3. Merge that as a pull request named `Release vX.Y`; `main` only takes pull requests.
4. `git tag vX.Y && git push origin vX.Y`, with the tag on `main`.

### Changelog

Every pull request that changes what is on the card adds a line under `## Unreleased` in
[`brick/CHANGELOG.md`](brick/CHANGELOG.md), written for players: what they can now do or what
was fixed, and where to find it. The Changelog check fails without one. Those lines are the
Dev build's release notes and, at the next tag, the release's. A change players won't notice
(a refactor, a test) takes the `no-changelog` label instead.

The run refuses to publish if the tag is not on `main`, if the card's README names another
version, if the changelog has no section for it, or if that release already exists — and it
runs the simulator's unit tests and scenarios before it builds anything.


</details>

<a name="credits"></a>
<details>
<summary><h2>Credits</h2></summary>

| Project / contributor | Contribution |
| :--- | :--- |
| [Brandon T. Kowalski](https://github.com/BrandonKowalski/slot) | Original Slot. frontend and Cart Studio |
| [wargenng](https://github.com/wargenng) | Low-battery LED warning |
| [KrutzOtrem](https://github.com/KrutzOtrem/Trimui-Brick-Overlays) | The bundled overlays: *Vanilla* (GB, GBC, GBA) and *CRT* (NES, SNES), MIT |
| [Hugo Chargois](https://font.gohu.org/) | GohuFont, one of the fonts under HOME → System → Font, WTFPL |
| [Cody "CodeMan38" Boisclair](https://github.com/google/fonts/tree/main/ofl/pressstart2p) | Press Start 2P, one of the fonts under HOME → System → Font, OFL |
| [TrimUI](https://github.com/trimui/firmware_brick), [Debian](https://www.debian.org) | Slot OS (experimental) runs TrimUI's boot chain, kernel, drivers and firmware from their firmware 1.1.1, and Debian 12 packages (glibc, ALSA, busybox, wpa_supplicant, hostapd); licences in `slot-os/licenses` on a Slot OS card |


</details>

<a name="ai-disclosure"></a>
<details>
<summary><h2>AI Disclosure</h2></summary>

This port was developed with assistance from [Claude Opus](https://claude.ai/). 


</details>

---

<p align="center"><sub>Slot. for TrimUI Brick and Brick Hammer · An unofficial port by LordNordyx</sub></p>
