#!/bin/bash
# Release notes for the Dev build: the changelog's Unreleased section, the same notes the next
# numbered release will carry, so what people read is what was written for them.
# Usage: brick/dev-notes.sh > notes.md
set -eu
cd "$(dirname "$0")/.."

release=$(git tag --list 'v*' --sort=-v:refname | head -1)
notes=$(awk '
    $0 == "## Unreleased" { inside = 1; next }
    inside && /^## / { exit }
    inside && NF { print }
' brick/CHANGELOG.md)

echo "Test build of the latest changes, $(date -u '+%-d %B %Y'). It may have bugs."
echo
echo "**New since ${release:-the start}:**"
echo "${notes:-- Nothing yet}"
