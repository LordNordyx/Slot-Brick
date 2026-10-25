# Slot OS

Slot OS boots the TrimUI Brick from the SD card straight into Slot, without TrimUI's own
system, and never writes to the Brick's internal storage. Installing it is in the main
README (Installation → Slot OS); this is how it is built and how it works.

## Building

    sh brick/docker-build.sh          # Slot's card files, in dist-brick/
    sh brick/os/build.sh dist-brick   # needs Docker
    SLOT_OS_MODEL=brickpro sh brick/os/build.sh dist-brick   # the Brick Pro's (experimental)

Out in `dist-os/`:

- `slot-os.img.xz`: the card image, for Raspberry Pi Imager, balenaEtcher or dd.
- `slot-os-update.zip`: what HOME → Update installs on a card already running Slot OS.
  `brick/update-manifest.sh <channel> <version> dist-os/slot-os-update.zip <dir> update-os.txt`
  describes and signs it.
- `boot.img`, `slot-os/`, `bootlogo.bmp`: the parts, for trying a change on a card without
  flashing it (below).

## How it fits together

- **The card:** TrimUI's boot0 and u-boot, the boot partition (TrimUI's 4.9 kernel with our
  initramfs, `bootimg.py`), u-boot's settings (TrimUI's) and one FAT32 partition holding
  `Slot/`, `slot-os/` (glibc, ALSA, wpa_supplicant, the GPU and Wi-Fi drivers) and the boot
  logo (`bootlogo.py`). What comes from where is pinned in `sources.txt` and `Dockerfile`, and
  listed for players in `NOTICE.txt` (on the card as `slot-os/licenses`).
- **The FAT32** is made for a 1 TiB partition and told it is 2.5 GiB; on its first start the
  Brick grows the partition to the card's end and rewrites the size (`init`'s `grow`).
- **`init`** is PID 1 and the only script in RAM: it feeds the watchdog, mounts the card
  (checking it first if it was left in use), installs a `slot-os/boot.img` it finds there,
  loads the GPU and Wi-Fi drivers, sets the mixer (`mixer.sh`) and runs Slot again and again.
  Slot's exit code says what next: 64 power off, 66 start again (an update).
- **The Brick Pro** has an image of its own (`slot-os-brickpro…`), built the same way from
  TrimUI's Brick Pro firmware (platform tg4040): the same boot0 and kernel version, but its own
  u-boot and device tree (another panel and backlight pin, other button pins, and the I2C bus its
  sticks are on) and its own kernel build. The image writes `brickpro` to `/etc/slot-os-model`:
  Slot then reads the Pro's button pins and its sticks (two converters on `/dev/i2c-3`,
  `crates/slot/src/input/pro_sticks.rs`), and `init` switches on its 5 V rail (PD11). Not yet
  tried on a Brick Pro.
- **Updates:** Slot reads `update-os.txt` instead of `update.txt` (`update-os-brickpro.txt` on a
  Brick Pro's Slot OS; `crates/slot/src/update.rs`).
  An update that brings a boot image keeps the old one with the files it replaced, and `init`
  puts them all back if the new Slot never shows a frame.

## Trying a change on a card

Copy `dist-brick/Slot/System/slot` to the card's `Slot/System/`, `dist-os/slot-os/` over the
card's `slot-os/`, and `dist-os/boot.img` to `slot-os/boot.img`: at the next start the Brick
writes the boot image (white LEDs) and restarts into it. A script at `slot-os/debug.sh` runs
before Slot, its output in `Slot/Logs/debug.log`. Startup times are in
`Slot/Logs/boot-times.log`, the kernel's log and the drivers' in `Slot/Logs/os.log`.
