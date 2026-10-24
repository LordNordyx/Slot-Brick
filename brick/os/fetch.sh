#!/bin/sh
# Download what sources.txt lists into .docker-cache/os/, checking each file's sha256.
set -eu
cd "$(dirname "$0")/../.."
OUT=.docker-cache/os
mkdir -p "$OUT"
grep -v '^#' brick/os/sources.txt | while read -r sum name url; do
    [ -n "$sum" ] || continue
    if ! echo "$sum  $OUT/$name" | sha256sum -c - >/dev/null 2>&1; then
        echo "fetching $name"
        curl -fsSL -o "$OUT/$name.part" "$url"
        echo "$sum  $OUT/$name.part" | sha256sum -c - >/dev/null || { echo "$name: sha256 differs" >&2; rm -f "$OUT/$name.part"; exit 1; }
        mv "$OUT/$name.part" "$OUT/$name"
    fi
done
