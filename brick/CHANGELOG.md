# Changelog — Slot. for TrimUI Brick

## Unreleased
- Choose a game's label yourself: the cart menu's Label row (L1/R1 in the GBA core picker)
  finds every label for the game, other regions included, and shows each on the cart to pick from.
- Wi-Fi comes back on at startup as it was left: Slot remembers whether Wi-Fi was on, and
  starts it itself when the firmware does not (it may not, with Slot in place of its menu).
- Wi-Fi only counts as online once it has an address, and at startup Slot waits for the
  firmware's Wi-Fi service to answer and tries again if fixing it fails, so Cart Art and
  RetroAchievements work without opening the Wi-Fi menu first.
- Cart colours: START on a game changes its cartridge's colour, finish (solid, see-through,
  glitter) and, for Game Boy carts, its shape, previewed live. NES and SNES carts only change
  shape (NES or Famicom; US or PAL), as the real ones came in their system's grey; a Famicom
  cart can have any colour. On GBA carts START opens the core
  picker, where Up/Down change the colour and finish.
- SNES: European and Japanese games get the rounder PAL / Super Famicom cart, read from the ROM's
  header so ripped ROMs with no region in their name work too.
- NES: Famicom games get the real Famicom cart, small, landscape and in colour, instead of a
  coloured NES cart. Cart Art fetches Famicom labels for them from LaunchBox's pictures of
  Japanese carts, kept in `Labels/NES/Famicom` beside the NES label, so swapping a game between
  the NES and Famicom cart swaps its label too. A tall NES label on a Famicom cart is shown whole
  instead of cut down.
- Wi-Fi comes up at startup even when the firmware starts its Wi-Fi service after Slot: Slot
  waits for it, then checks a network joined and repairs once more if not.
- NES: a ROM on a chip only Famicom carts used (Konami VRC, Namco 163 and others) gets a Famicom
  plastic even without "(Japan)" in its name.
- The core picker shows the cart in its real plastic instead of the default black, and its
  legend fits the Brick's screen.
- SNES: `.sfc`/`.smc` ROMs in `Slot/Games/SNES` get a shelf of SNES cartridges and run on Snes9x,
  with save states, rewind, fast-forward and RetroAchievements. X, Y, L and R are the game's own;
  SELECT + X switches the picture. Cart Art fetches SNES labels from LaunchBox cart pictures.
- Slot creates the NES and SNES folders on the card itself at startup.
- Wi-Fi comes up on its own at startup: the radio the firmware leaves blocked is repaired in the
  background, so saved networks are joined without opening the Wi-Fi menu.
- After connecting, the Wi-Fi menu makes sure an address is asked for and waits for it, then
  checks every couple of seconds while one is still on its way.
- The Wi-Fi menu opens showing whether Wi-Fi is on, instead of OFF until it has checked.
- NES: `.nes` ROMs in `Slot/Games/NES` get a shelf of NES cartridges (grey; Famicom colours for
  Japanese releases) and run on FCEUmm (GPL-2.0), with save states, rewind, fast-forward and
  RetroAchievements. The emulator picture canvas is now sized per system (256x240 for NES); the
  Game Boys and GBA are unchanged.
- The NES cart is drawn after the real one: a full-height grip on the left, the label recess
  open to the top on the right, the arrow below it.
- Cart Art fetches NES labels, cut out of the LaunchBox Games Database's pictures of standard NES
  carts, found by the ROM's name.
- New HOME > Cart Art (greyed out while Wi-Fi is off): GET CART ART gives games with no label a
  real cartridge label scan (by ROM checksum, from the slot art set behind Cart Studio, over
  HTTPS). Existing labels are never replaced. Labels can be deleted there one by one. Trimmed or patched ROMs are found
  by file name and GBA game code; ROM hacks never get their base game's label.
- The Wi-Fi keyboard is QWERTY under a number row, and briefly shows the last character typed.
- New HOME > RetroAchievements (greyed out while Wi-Fi is off): log in, test the connection, log out. Over HTTPS (rustls);
  only the login token is kept. While logged in, games earn achievements (softcore), with a
  small corner toast for the game's count and each unlock, with its badge like Steam's. Built on rcheevos 12.5 (MIT), vendored in
  crates/slot-cheevos.
- Wi-Fi that was already on (the firmware starts it at boot with the radio blocked) is restarted
  properly when the Wi-Fi menu opens, so scanning works without turning it off and on first.
- Fixed Wi-Fi never turning on: the firmware's wifi_init.sh needs an rfkill.elf the Brick
  lacks, so the radio stayed blocked. Slot now unblocks it itself and checks the result.
- Wi-Fi errors always leave Slot/Logs/wifi.log, and B really cancels a stuck toggle.
- Closing a game no longer turns Wi-Fi off: multiplayer's radio cleanup now only runs after a
  real link session, and brings normal Wi-Fi back if it was on before.

## v0.23
- Settings > About credits the port to LordNordyx.
- Short app names in the stock Apps list: Slot, Logo, Check and Stock, each with its own icon.
  Folder names are unchanged, so existing Apps/BootLogo/Image pictures keep working.
- Card README rewritten with the Brick's actual button layout (HOME = left button, SELECT = middle).
- Removed leftovers: the old Tools/tg5040/Slot.pak launcher and the RetroAchievements research note.

## v0.22.2
- Fixed "works once, then stock menu": the startup hook now waits until the real SD card is
  mounted instead of reading a stray Slot folder on internal storage.
- Check lists files hidden under the card mount on internal storage.

## v0.22.1
- The startup hook waits up to 30 s (was 10 s) for slow-mounting cards and logs each decision
  to Slot/Logs/hook.log.

## v0.22
- A read-only or damaged card no longer drops silently to the stock menu; Slot shows
  "Can't write to SD card".
- Check writes startup-report.txt at the top of the card.

## v0.21
- Fixed autostart failing after a firmware update or reinstall.
- New Check app: read-only startup report.

## v0.17 – v0.20
- SELECT + X switches GBA between 3:2 and 4:3 full screen, remembered per game.
- About screen describes the Brick port.

## v0.16
- Waking no longer needs two Power presses; Wi-Fi reconnects in the background.

## v0.15
- Merged upstream Slot 1.6.0: GB cartridge clocks kept in saves, read-only card alarm, faster
  library loading, shuffled wallpapers, crash logs.
- Boot Logo app (now Logo) replaces the separate install/restore logo apps.

## v0.14
- Lower sleep drain: radios off during suspend, real suspend-to-RAM where available.

## v0.13
- Auto-sleep after 5 min idle in menus or 15 min in game (Slot/DISABLE_AUTO_SLEEP turns it off).

## v0.12
- Wi-Fi works on stock firmware; minimum CPU clock in games fixes crackling audio.

Earlier versions: Brick screen, input, LEDs, power menu and autostart (see SOURCES.md).
