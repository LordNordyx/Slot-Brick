# Slot. for TrimUI Brick — v0.44

An unofficial port of **Slot.**, Brandon T. Kowalski's GB/GBC/GBA frontend for the Anbernic RG SP,
to the **original TrimUI Brick** on stock firmware, with NES and SNES added. Port by **LordNordyx**.

> **Note:** Report problems at <https://github.com/LordNordyx/slot-brick>, not to the upstream project.

This is a complete SD card install. You don't need any earlier release, and copying the card
doesn't flash anything to the device.

---

## Installation

1. **Back up.** Power off the Brick and copy your entire current SD card to your PC.
2. **Prepare a card.** Use an empty SD card; a spare is safest. If you reuse your current card,
   verify the backup first, then delete everything on it (no reformat needed).
3. **Copy Slot.** Copy everything **inside** the `SD-CARD` folder to the root of the card,
   including the hidden `.slot-boot` folder. Don't copy the `SD-CARD` folder itself.
4. **Restore your files** from the backup into the matching folders:
   - `Slot/Games`, `Slot/Saves`, `Slot/States`, `Slot/BIOS`
   - `Slot/Config`, `Slot/Labels`, `Slot/Wallpapers`
   - Any `Slot/BootLogo/original*` files

   ⚠️ Do **not** restore old `Slot/System`, `trimui`, `Tools` or `Apps` folders.
5. **Start.** Insert the card and power on. If the stock menu appears instead of Slot, open
   **Apps → Slot** once to set up automatic startup.

---

## Controls

`HOME` is the left small button below the screen (labelled Menu) and `SELECT` is the middle one.
`A + B` means hold A and press B; `A / B` means either button.

### On the carousel

| Button           | Action                                  |
| ---------------- | --------------------------------------- |
| `Left` / `Right` | Browse games                            |
| `Up` / `Down`    | Previous / next letter                  |
| `L1` / `R1`      | Switch shelves (GBA, GB, GBC, NES, SNES) |
| Tap `A`          | Resume from the last save state         |
| Hold `A`         | Start the game fresh                    |
| `START`          | Cart colour, finish, shape and label (GBA: and core) |
| `HOME`           | Menu (Screen, Gameplay, Wi-Fi, Cart Art, RetroAchievements, Update, and System: LEDs, Date & Time, Auto Clock, About) |

### In game

| Button                      | Action                                              |
| --------------------------- | --------------------------------------------------- |
| Hold `HOME`                 | Save, eject and return to the carousel              |
| Double-tap `HOME`           | Save state switcher                                 |
| `HOME + B` or `SELECT + B`  | LEDs on / off                                       |
| `SELECT + HOME`             | Game menu (multiplayer Host / Join)                 |
| `SELECT + R1`               | Save state                                          |
| `SELECT + L1`               | Load the latest save state                          |
| `HOME + A` or `SELECT + A`  | Next shader                                         |
| `SELECT + X`                | GBA/SNES: original or full screen (saved per game)  |
| `SELECT + Y`                | Colour correction on / off (GB, GBC, GBA)           |
| `HOME + L1` / `R1`          | Previous / next overlay                             |
| `SELECT + L2` / `R2`        | GB: previous / next colour palette                  |
| `SELECT + Up` / `Down`      | Brightness                                          |
| `SELECT + Left` / `Right`   | Blue light filter                                   |
| `L1` / `R1`                 | GB/GBC/NES: stretched / original picture            |
| Hold `L2`                   | Rewind                                              |
| Hold `R2`                   | Fast-forward (double-tap to lock it on)             |

### System

| Button             | Action                                 |
| ------------------ | -------------------------------------- |
| Volume `+` / `−`   | Volume (press both together to mute)   |
| `Fn` slider        | Mute / unmute                          |
| Press `Power`      | Sleep / wake                           |
| Hold `Power`       | Shut down, or exit to the stock OS     |

**Cart colour:** press `START` on a game on the shelf to change its cartridge: any colour and a
solid, see-through or glitter finish for Game Boy and GBA carts, and the shape: original or Color
for Game Boy carts, NES or Famicom for NES carts, US or Europe / Japan for SNES carts. NES and
SNES carts are always their system's grey, like the real ones; a NES game on the Famicom cart
can have any colour, as Famicom carts came in many. On a GBA cart `START` opens the core picker: `Left` / `Right` swap the core, `Up` changes the colour
and `Down` the finish. The cart changes as you go; `A` saves, `B` puts it back. Choices are kept
in `Slot/Labels/cart_shell.ini`.

**Choosing a label:** with Wi-Fi on, press `Y` in the cart menu (or in the GBA core picker) to
search: the keyboard opens with the game's name filled in, so `START` searches right away, or
change it first (each word only has to begin a word of the name, so `yoshi island` finds
*Super Mario World 2: Yoshi's Island*). It looks through every game of that console in the
places Cart Art uses. The labels found appear in a grid: move with the d-pad, `A` picks one,
`Y` searches again and `B` goes back. Then frame the label to fit the cart's cutout: the d-pad
moves the picture, hold `L1` and use the d-pad to make the frame bigger or smaller, `R1` evens
out its shape and `Y` puts it back as it was. `A` keeps it in place of the label the game had;
`B` goes back to the grid. The labels not picked are deleted when the menu closes. NES games
keep the NES and the Famicom cart's label apart: a label picked goes to the cart shape showing.

**NES:** put `.nes` ROMs in `Slot/Games/NES`; they get their own shelf of NES cartridges and run
on FCEUmm. No BIOS is needed. NES carts are grey; a ROM named as a Japanese release, such as
`Rockman (Japan).nes`, or on a chip only Famicom carts used, gets the small, landscape Famicom
cart instead, in a colour picked from its name (change it under `START` → Colour). NES ROMs don't say where they were
sold, so for other Japanese games choose Famicom under `START` → Shape. Cart Art fetches NES labels too (see below), or put your own picture in
`Slot/Labels/NES`. The picture is the TV's 256 x 224, padded to 4:3 at actual size or
stretched to fill the screen with `L1` / `R1`. RetroAchievements work for NES too.

**SNES:** put `.sfc` or `.smc` ROMs in `Slot/Games/SNES`; they get a shelf of SNES cartridges and
run on Snes9x. No BIOS is needed. European and Japanese games get the rounder PAL / Super Famicom
cart; Slot reads where a game was sold from the ROM, so a ripped `Terranigma.sfc` works too. `L1` / `R1` and `X` / `Y` are the SNES's own buttons (no turbo
on SNES), so `SELECT + X` switches the picture between original and full screen. Cart Art fetches
SNES labels too, and RetroAchievements work.

**Cart art:** connect to Wi-Fi, then open **HOME → Cart Art** (greyed out while Wi-Fi is off).
**GET CART ART** gives every game with no label a scan of its real cartridge label; labels you
already have are never replaced. The scans come from the slot art set behind
[Cart Studio](https://studio.slot-cfw.fyi) (scans from ScreenScraper), downloaded over HTTPS.
A game is found by the ROM's checksum, or for a trimmed or patched ROM by its file name
(helped by the GBA game code in its header). ROM hacks and homebrew keep the label with their
file name, so they never look like the game they are based on. Each ROM is read once and its
checksum kept in `Slot/System/Cache/crc32.txt`.

NES and SNES labels come from the [LaunchBox Games Database](https://gamesdb.launchbox-app.com)'s
pictures of the cartridges: Slot finds the game by its ROM's file name (No-Intro names work best,
for example `Mega Man 2 (USA).nes`) and cuts the label out of a picture of the standard US cart.
Games on the Famicom cart get a Famicom cart's label (from the Japanese release), kept apart in
`Slot/Labels/NES/Famicom` so a NES game can have both: swapping its shape swaps the label. While
a game has only one, both carts show it. SNES games on the rounder Super Famicom / PAL cart get
that cart's label (from the Japanese or European release), and either kind of SNES cart falls
back to the other's label when its own has none. Carts of another shape (Tengen, gold Zelda,
unlicensed) keep their file-name label, as do games LaunchBox has no cart picture of.

`Slot/Logs/cart-art.txt` lists each game and what it got. Below GET CART ART, every game with a
label is listed: press A twice on one to delete its label, then GET CART ART fetches a new one.

**RetroAchievements:** **HOME → RetroAchievements** (greyed out while Wi-Fi is off) logs in to
your [RetroAchievements](https://retroachievements.org) account and tests the connection.
Your password is sent once, over HTTPS, and never stored: Slot keeps only the login token, in `Slot/Config/retroachievements.txt`; delete that file or choose **LOG OUT** to forget
it. While logged in, games earn achievements in softcore mode (save states, rewind and
fast-forward stay allowed): a small toast slides into the top-right corner with how many a
game has when it starts, and with each unlock and its badge as it happens. A game's badges
are fetched when it starts and kept in `Slot/System/Cache/Badges`. Unlocks made offline are sent once the Brick is back online, while the game is
still running. Hardcore mode isn't available.

**Update:** **HOME → Update** (greyed out while Wi-Fi is off) checks for a newer release of Slot
and shows what is new in it. Choose INSTALL, then press A again: Slot downloads it, checks it,
puts it in place and starts again. It needs 20% battery or a charger. Only Slot's own files
are replaced (`Slot/System`, the `Apps` it came with, `.slot-boot`, the bundled overlays and
this README); your games, saves, states, labels, BIOS files, settings and wallpapers are left
as they are. Every update is signed, and one whose signature or checksum doesn't match is
never installed. If the new version doesn't start, the previous one is put back.

**Auto Clock:** **HOME → System → Auto Clock** (on to start with) sets the clock from an
internet time server whenever Wi-Fi connects, and every six hours while it stays connected.
The time zone is still yours to choose under **Date & Time**, and summer time is a change you
make there. On the clock screen shown at first start, the date and time fill themselves in
once Wi-Fi connects, unless you have already changed them.

**Brick Pro (experimental):** Slot tells a Brick Pro from a Brick by itself. Its left stick
works as the D-pad, in menus and in games, and **HOME → System → LEDs** gains a **Sticks** row
for the lights around the sticks (**Triggers** sets the shoulder lights at the back). The right
stick, the stick clicks and the F1/F2 keys do nothing yet. Not yet tested on a real Brick Pro.

**Wi-Fi status:** the icon beside the clock shows whether Wi-Fi is connected, on but not
reaching the network, or off.

**Overlays:** pick one under **HOME → Screen →** the system **→ Overlay**. Slot comes with
KrutzOtrem's overlays for the Brick (the *Vanilla* set for GB, GBC and GBA, the *CRT* set for NES
and SNES; MIT, see `Slot/System/licenses`). Add your own 1024×768 PNGs to `Slot/Overlays/GB`,
`GBC`, `GBA`, `NES` or `SNES`. The game is fitted into the part of the overlay that isn't fully
opaque; while one is on, the picture size shortcuts do nothing.
In game, `HOME + L1` / `R1` steps through them and Off. More of KrutzOtrem's
sets are at <https://github.com/KrutzOtrem/Trimui-Brick-Overlays>.

**Auto-sleep:** the Brick sleeps after 5 minutes idle in menus or 15 minutes in a game.
To turn this off, create an empty file named `Slot/DISABLE_AUTO_SLEEP`.

---

## Apps

These appear in the stock OS under **Apps**.

| App       | What it does                                                         |
| --------- | -------------------------------------------------------------------- |
| **Slot**  | Starts Slot and sets it to start automatically                       |
| **Logo**  | Sets your own boot logo, or restores the original (see below)        |
| **Check** | Reports why Slot did or did not start, in `startup-report.txt`       |
| **Stock** | Turns off Slot autostart and restores the stock startup              |

### Custom boot logo

1. Put one image (`.png`, `.jpg`, `.jpeg` or `.bmp`) in `Apps/BootLogo/Image`.
2. Open **Apps → Logo**, then restart.

To restore the original, empty the `Image` folder and open **Logo** again.

> **Keep a backup:** the first run saves the original logo to `Slot/BootLogo/original.bmp`.
> Copy that file to your PC; it's the only way back.

---

## Multiplayer (experimental)

1. Install this same version on both Bricks, with the same game, core and BIOS.
2. In game, press `SELECT + HOME`.
3. Choose **Host** on one Brick first, then **Join** on the other.

No router is needed. This hasn't been tested on two physical devices yet. If it fails, send
`Slot/Logs/multiplayer.log` and `Slot/Logs/slot.log`.

---

## Troubleshooting

| Problem                               | Fix                                                                 |
| ------------------------------------- | ------------------------------------------------------------------- |
| Stock menu appears instead of Slot    | Open **Apps → Check** and read `startup-report.txt` on the card     |
| You want to skip Slot at startup      | Create an empty file `Slot/DISABLE_AUTOBOOT`; delete it to re-enable |
| You need stock emulators or RetroArch | They aren't included; keep your original card backup                |
