#!/bin/bash
# Fails when a change reaches the card but tells players nothing: a pull request that touches
# what ends up on the card must add a line to brick/CHANGELOG.md. A release (one that adds a
# "## vX.Y" heading) passes, as it only moves the notes already there.
# Usage: brick/changelog-check.sh BASE   (BASE is the commit the change is compared with)
set -eu
cd "$(dirname "$0")/.."
base=${1:?Usage: changelog-check.sh BASE}

# Files that end up on the card, or decide what does.
if ! git diff --name-only "$base"...HEAD | grep -qE '^(crates/[^/]+/(src|assets)/|brick/card/|vendor/|Cargo\.lock$)'; then
    echo "Nothing on the card changes; no changelog line needed."
    exit 0
fi

added=$(git diff "$base"...HEAD -- brick/CHANGELOG.md | grep -E '^\+(- |## v)' || true)
if [ -z "$added" ]; then
    echo "::error file=brick/CHANGELOG.md::This changes what is on the card, but adds nothing under '## Unreleased' in brick/CHANGELOG.md. Add a line saying, for players, what is new or fixed. If players will not notice it, add the no-changelog label."
    exit 1
fi
echo "Changelog lines added:"
echo "$added"
