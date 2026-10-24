#!/bin/sh
# Rebuild crates/slot-ui/assets/SymbolsNerdFontMono-Slot.ttf: only the symbols Slot draws, out
# of the full Nerd Font (2.5 MB, thousands of glyphs, which the font library reads in full at
# startup: about 0.6 s on the Brick). Run it after using a new symbol; the slot-ui test
# `every_symbol_slot_draws_is_in_the_font` fails until then. Needs Docker.
set -eu
cd "$(dirname "$0")/.."
# Every private-use code point written in slot-ui's sources (\u{f028} and the like).
codes=$(grep -rhoE '\\u\{(e[0-9a-f]{3}|f[0-8][0-9a-f]{2}|f[0-9a-f]{4})\}' crates/slot-ui/src \
    | tr -d '\\u{}' | sort -u | sed 's/^/U+/' | paste -sd, -)
echo "symbols: $codes"
docker run --rm -u "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD":/src -w /src python:3.12-slim sh -euc "
    pip install -q --user fonttools==4.55.3
    /tmp/.local/bin/pyftsubset crates/slot-ui/assets/SymbolsNerdFontMono-Regular.ttf \
        --unicodes='$codes' --layout-features='' --no-hinting --name-IDs='*' --name-languages='*' \
        --notdef-outline --output-file=crates/slot-ui/assets/SymbolsNerdFontMono-Slot.ttf
"
ls -l crates/slot-ui/assets/SymbolsNerdFontMono-Slot.ttf
