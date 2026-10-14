#!/bin/bash
# Release notes for the Dev build: what is in it since the last numbered release, as a short
# list of the merged PRs' titles. Changes that leave the card as it was (build scripts, tests,
# docs) are left out, unless they are all there is.
# Usage: brick/dev-notes.sh > notes.md
set -eu
cd "$(dirname "$0")/.."

# Files that end up on the card, or decide what does.
shipped() { grep -qE '^(crates/[^/]+/(src|assets)/|brick/card/|vendor/|Cargo\.lock$)'; }

# The latest numbered release; its tag need not be on main, so changes are counted from its date.
release=$(git tag --list 'v*' --sort=-v:refname | head -1)
range=(--first-parent HEAD)
[ -z "$release" ] || range+=(--since="$(git log -1 --format=%cI "$release")")

features=() other=()
while read -r c; do
    subject=$(git log -1 --format=%s "$c")
    if [[ $subject =~ ^Merge\ pull\ request ]]; then
        line=$(git log -1 --format=%b "$c" | sed '/^[[:space:]]*$/d' | head -1)
    else
        line=$subject
    fi
    line=${line%.}
    [ -n "$line" ] || continue
    if git diff --name-only "$c^1" "$c" 2>/dev/null | shipped; then features+=("- $line"); else other+=("- $line"); fi
done < <(git log --format=%H "${range[@]}")

echo "Test build of the latest changes, $(date -u '+%-d %B %Y'). It may have bugs."
echo
echo "**New since ${release:-the start}:**"
if [ ${#features[@]} -gt 0 ]; then printf '%s\n' "${features[@]}"; else printf '%s\n' "${other[@]:-- Nothing yet}"; fi
