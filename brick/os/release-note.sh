#!/bin/sh
# The part of a release's notes about Slot OS. Usage: release-note.sh vX.Y|dev
set -eu
tag=${1:?Usage: release-note.sh vX.Y|dev}
cat <<NOTE

## INTRODUCING: Slot OS

Slot as its own custom firmware: the Brick boots straight from the SD card into Slot, in under
3 seconds instead of about 9. Otherwise it is the same Slot, with the same games, saves and
settings. Flash \`slot-os-brick-$tag.img.xz\` onto a card (it erases the card); the README has
the guide. It keeps itself up to date through HOME → Online → Update.

**Brick Pro (experimental, not yet tried on one):** flash \`slot-os-brickpro-$tag.img.xz\`
instead. It is built from TrimUI's Brick Pro firmware and reads the Pro's buttons and sticks. If you
try it, please report back in issue #65 with the card's \`Slot/Logs\` folder.
NOTE
