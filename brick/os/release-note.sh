#!/bin/sh
# The part of a release's notes about Slot OS. Usage: release-note.sh vX.Y|dev
set -eu
tag=${1:?Usage: release-note.sh vX.Y|dev}
cat <<NOTE

## INTRODUCING: Slot OS

A custom firmware (experimental) that boots the Brick straight from the SD card into Slot, in
about 4 seconds. Flash \`slot-os-$tag-sd-card.img.xz\` onto a card (it erases the card); the
README has the guide. It keeps itself up to date through HOME → Update.
NOTE
