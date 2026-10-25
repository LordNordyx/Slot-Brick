# Changelog — Slot. for TrimUI Brick

## Unreleased
- Sounds: the HOME menu now makes a soft pop as you move, change a setting, open a page and go back; a GBA cart pops as it opens for the core picker, and snaps shut as it closes; and a label slaps down as it lands on its cart.
- System → System Sounds: set how loud Slot's own sounds are (Volume, 0–100%, on top of the Brick's volume), and turn each kind on or off: Menu, Favorite Star, Cart In & Out, Cart Opening and Label On. Sounds in quick succession no longer add up to louder and louder; each takes over from the last.
- Fixed: putting a label on a GBA cart in its own colour (such as Pokémon Ruby's red) showed the cart black while framing the label and as it flew on. And while a label flies on, the cart's empty label slot now shows its plastic, in the colour you picked, rather than black.
- Favorites: the star now pops onto the cart when you press Y, with one of five pop sounds at random, and pops off again with a lower, softer one. It's shaded too, with a lit and a dark side.
- Library → Systems: hide the shelves of systems you don't play. The last system with games can't be hidden.
- System → Reset Settings: puts every setting back to how it came (LEDs and font included). Press A, then A again when it asks "Sure?". Games, saves, states, play time, favorites, the clock, brightness and volume stay.
- System → About is now a page: the version, whether it is a Dev build or a release, the device, the firmware (stock or Slot OS), and how much of the SD card is free. The label is still there, under About → Label.
- Fixed: after waking from sleep, Wi-Fi could say it was connected while Cart Art, RetroAchievements and Update stayed greyed out until Wi-Fi was turned off and on. Slot now notices and gets an address again by itself.

## v0.50
**New since v0.44:**
- PlayStation: PS1 games from `Slot/Games/PS1` (`.cue`/`.bin`, `.chd`, `.pbp`, `.iso`, `.m3u`) on a shelf of discs that spin into the slot, games of several discs included.
- Rumble: games that rumble now drive the Brick's motor.
- Favorites (Y) and sorting (SELECT), with play time and a title card for the selected game.
- Brick Pro support: its left stick works as the D-pad, and its stick lights have their own LED row.
- Fonts: HOME → System → Font, with GohuFont included, and room for your own in `Slot/Fonts`.
- A reworked HOME menu: Emulation, Gameplay, Library, Online and System.
- Faster startup, and Wi-Fi fixes.

## v0.44
**New since v0.43:**
- HOME → Update: update Slot over Wi-Fi. It shows what's new (L1 / R1 scroll long notes), asks before installing, and leaves your games, saves, states, labels and settings alone. Updates are signed and checked before anything is replaced; if the new version doesn't start, the previous one is put back, and an update cut short by the Brick switching off is cleared away. Needs 20% battery or a charger.
- HOME → System: LEDs, Date & Time and About are now under System, leaving the menu shorter.
- System → Auto Clock: with Wi-Fi on, the clock now sets itself from an internet time server, on connecting and every six hours. On by default; the time zone is still set under Date & Time.

## v0.43
**New since v0.42:**
- Overlays for every system come with Slot: KrutzOtrem's *Vanilla* set for GB, GBC and GBA and *CRT* set for NES and SNES, made for the Brick. Thanks to @KrutzOtrem!
- Overlays with a grid or tint over the game (such as KrutzOtrem's LCD and DMG grids) now work: the game fills the part of the overlay that isn't fully opaque, at the size the overlay was drawn for.
- An overlay name too long for its row is shortened, and scrolls slowly while the row is selected.
- Gameplay → Auto Save: save the game every 1, 5 or 10 minutes, or never. It was every minute.
- Gameplay → Low Battery Shutdown: save and shut down at 3%, 5% or 10% battery, or never. It was 5%.

## v0.42
**New since v0.41:**
- Screen settings are sorted by system: Game Boy / GBC, Game Boy Advance, NES and SNES each have their own page. NES and SNES shaders can now be set there too.
- Colour correction is set separately for the Game Boys and the GBA. A setting saved before keeps working for both.
- Overlays: put your own 1024×768 PNG overlays in `Slot/Overlays/<system>` and pick one on the system's Screen page. The game fits itself into the overlay's window. In game, HOME + L1 / R1 switches between them.

## v0.41
**New since v0.40:**
- Choosing a label now ends with it peeling onto the cart like a sticker, on every cart. On GBA carts the front closes for it, then opens again.
- Low-battery warning on the F1/F2 lights: steady red at 15% or below, blinking at 10% or below, even with the lights turned off. Thanks to @wargenng!
- Fixed: the About screen showed the wrong version.

## v0.40
**New since v0.23:**
- Wifi status icon on the shelf footer. Icons may change.
- Label Picker. Press Y when in the edit screen on a game cart. It has a label editor built in, so you can resize the label and make it fit the cart cutout
- Finally fixed Wi-Fi
- Customize cartridges: color, finish and shape for all shelves.
- Add SNES support: SNES and PAL cartridges
- Add NES support: NES and FAMICOM Cartridges
- Add RetroAchievements support, softcore only
- Automatic label fetcher in main menu, it fetches labels for all carts at once, but it is really barebones, so refer to the label picker for more control

**FOR DEVELOPERS**
- The Brick simulator - for testing in the browser before real hardware testing. Refer to the Development part of the Readme.

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
