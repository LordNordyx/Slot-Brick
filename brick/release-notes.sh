#!/bin/bash
# The notes for a numbered release: the changelog's section for that version, as it was
# written there. Nothing is generated, so what people read is what was meant for them.
# Usage: brick/release-notes.sh v0.24 > notes.md
set -eu
cd "$(dirname "$0")/.."
tag=${1:?Usage: release-notes.sh TAG}

awk -v want="## $tag" '
    $0 == want { inside = 1; next }
    inside && /^## / { exit }
    inside { print }
' brick/CHANGELOG.md
