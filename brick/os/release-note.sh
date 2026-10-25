#!/bin/sh
# The part of a release's notes about Slot OS. Usage: release-note.sh vX.Y|dev
set -eu
tag=${1:?Usage: release-note.sh vX.Y|dev}
cat <<NOTE

## INTRODUCING: Slot OS

Slot as its own custom firmware: the Brick boots straight from the SD card into Slot, in under
3 seconds instead of about 9. Otherwise it is the same Slot, with the same games, saves and
settings. Flash \`slot-os-$tag-sd-card.img.xz\` onto a card (it erases the card); the README has
the guide. It keeps itself up to date through HOME → Online → Update.
NOTE
