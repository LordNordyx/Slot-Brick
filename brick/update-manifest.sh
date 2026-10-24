#!/bin/bash
# update.txt for a card zip: what Slot's Update screen reads to find and check an update
# (crates/slot/src/update.rs). With UPDATE_SIGNING_KEY set (the release key, PEM), it is also
# signed, as update.txt.sig beside it; Slot installs nothing whose signature does not match.
# NAME is update.txt for the stock card zip (the default), update-os.txt for Slot OS's.
# Usage: brick/update-manifest.sh release|dev VERSION ZIP OUT_DIR [NAME]
set -eu
cd "$(dirname "$0")/.."
channel=${1:?Usage: update-manifest.sh release|dev VERSION ZIP OUT_DIR}
version=${2:?}
zip=${3:?}
out=${4:?}
name=${5:-update.txt}
case "$channel" in release | dev) ;; *) echo "unknown channel $channel" >&2; exit 2 ;; esac

# The notes Slot shows: the release's changelog section, or for the Dev build, Unreleased.
section=$([ "$channel" = release ] && echo "## $version" || echo "## Unreleased")
notes=$(awk -v want="$section" '
    $0 == want { inside = 1; next }
    inside && /^## / { exit }
    inside && /^- / { print }
' brick/CHANGELOG.md)

mkdir -p "$out"
{
    echo "slot-update 1"
    echo "channel=$channel"
    echo "version=$version"
    echo "commit=$(git rev-parse HEAD)"
    echo "zip=$(basename "$zip")"
    echo "size=$(stat -c %s "$zip")"
    echo "sha256=$(sha256sum "$zip" | cut -d' ' -f1)"
    echo
    printf '%s\n' "$notes"
} > "$out/$name"

if [ -z "${UPDATE_SIGNING_KEY:-}" ]; then
    echo "UPDATE_SIGNING_KEY is not set: $name is not signed, and Slot will not install it" >&2
    exit 0
fi
key=$(mktemp)
trap 'rm -f "$key"' EXIT
printf '%s\n' "$UPDATE_SIGNING_KEY" > "$key"
openssl pkeyutl -sign -rawin -inkey "$key" -in "$out/$name" -out "$out/$name.sig"
# The key must be the one Slot checks with (RELEASE_KEY in crates/slot/src/update.rs): signed
# with any other, every Brick would refuse the update, and only a Brick would find out.
ours=$(openssl pkey -in "$key" -pubout -outform DER | tail -c 32 | od -An -tx1 | tr -d ' \n')
built_in=$(sed -n '/^const RELEASE_KEY/,/^];/p' crates/slot/src/update.rs | grep -o '0x[0-9a-f][0-9a-f]' | sed 's/0x//' | tr -d '\n')
if [ "$ours" != "$built_in" ]; then
    echo "UPDATE_SIGNING_KEY is not the release key Slot checks with (RELEASE_KEY in crates/slot/src/update.rs)" >&2
    exit 1
fi
openssl pkey -in "$key" -pubout > "$key.pub"
openssl pkeyutl -verify -rawin -pubin -inkey "$key.pub" -in "$out/$name" -sigfile "$out/$name.sig" > /dev/null
rm -f "$key.pub"
echo "wrote $out/$name and its signature"
