# Slot OS (prototype)

Slot OS boots the TrimUI Brick from the SD card straight into Slot, without TrimUI's own
system. It never writes to the Brick's internal storage: take the card out, and the Brick
starts as before. Build it with `sh brick/docker-build.sh && sh brick/os/build.sh`; the image is
`dist-os/slot-os.img.xz`.

## Trying it on the card you use now

Writing the image **erases the whole card**. These steps save everything first, and put it
back afterwards. They are for Windows; they take about half an hour, most of it copying.

### 1. Save everything on the card

1. Put the card in the PC. In File Explorer, open **View → Show → Hidden items**, so the
   hidden `.slot-boot` folder shows too.
2. Make a folder on the PC, for example `Documents\Brick backup`, and copy **everything** on
   the card into it: `Slot`, `Apps`, `.slot-boot` and anything else there.
3. Check the copy is complete: right-click the card and the backup folder, **Properties**;
   the sizes and file counts should match.

Your saves, save states, settings, games, labels and BIOS are all in `Slot`. Keep this
backup until you are back on the card as it was.

### 2. Write Slot OS to the card

1. Install **Raspberry Pi Imager** (raspberrypi.com/software).
2. **Choose Device**: *No filtering*. **Choose OS**: scroll down to *Use custom*, and pick
   `slot-os-prototype.img.xz`. **Choose Storage**: the SD card. Check its size; picking the
   wrong drive erases that drive.
3. **Next**, then *No* to customisation, then *Yes* to erase. It writes and then verifies.
4. Windows may now say *You need to format the disk in drive X: before you can use it*,
   once or several times. **Always click Cancel.** Those are Slot OS's boot partitions;
   formatting them breaks the card.

The card now shows one drive in Windows, named **SLOT**, with a `Slot` folder. It holds
1 GB: room for a few games, not your whole collection.

### 3. Put a few games on it

From the backup, copy into the **SLOT** drive:

- a few games, into the matching `Slot\Games\GB`, `GBA`, ... folders;
- `Slot\BIOS` (needed for PlayStation games);
- if you want your saves: `Slot\Saves`, `Slot\States` and `Slot\Config`.

Eject the card in Windows before taking it out.

### 4. Start the Brick

Put the card in and turn the Brick on.

- **Slot appears:** try every button (A, B, X, Y in particular), the sound, and
  HOME → Power Off. Then put the card in the PC and send
  `Slot\Logs\boot-times.log`, `Slot\Logs\os.log` and `Slot\Logs\slot.log`.
- **The screen stays black for 30 seconds:** hold Power until the Brick is off. Put the
  card in the PC and look for `Slot\Logs\os.log`. Send it if it is there; if there is no
  `Logs` folder at all, say so (Slot OS never got as far as starting).

### 5. Back to how it was

1. Copy anything you want to keep from the **SLOT** drive (saves made while trying Slot OS
   are in `Slot\Saves` and `Slot\States`) into the backup's `Slot` folder, replacing the
   older files.
2. Erase the card back to one ordinary drive. Either:
   - **SD Card Formatter** (sdcard.org, the SD Association's tool): pick the card, *Quick
     format*, **Format**. It formats a card over 32 GB as exFAT and a smaller one as FAT32,
     as cards come from the shop. Or:
   - **Raspberry Pi Imager**: **Choose OS** → *Erase* → the card → **Next**. It formats the
     card FAT32. That is fine, unless a single game file is over 4 GB.
3. Copy everything from the backup folder back onto the card, `.slot-boot` included.
4. Eject, put it in the Brick, and turn it on: it starts Slot as before. Nothing on the
   Brick itself was changed, so there is nothing else to undo.
