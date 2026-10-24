#!/bin/sh
# The part of a release's notes about its Slot OS files. Usage: release-note.sh vX.Y|dev
set -eu
tag=${1:?Usage: release-note.sh vX.Y|dev}
cat <<NOTE

### Slot OS (experimental)

\`slot-os-$tag-sd-card.img.xz\` starts the Brick from the SD card straight into Slot, without
TrimUI's own system, in about 4 seconds. Flash it onto a card with Raspberry Pi Imager; it
erases the card. See README → Installation → Slot OS. A card already running Slot OS updates
itself from **HOME → Update** (\`slot-os-$tag-update.zip\`).
NOTE
