#!/bin/bash
# The Updates release (tag `updates`): what Slot's Update screen downloads, for every channel,
# so the version releases hold only what people download (crates/slot/src/update.rs). Made the
# first time it is needed; a pre-release, so GitHub never shows it as the latest release.
# Usage: brick/updates-release.sh (with GH_TOKEN and GH_REPO set)
set -eu
gh release view updates > /dev/null 2>&1 && exit 0
gh release create updates --target "${GITHUB_SHA:?}" --title "Updates" --prerelease --notes \
    "For Slot's Update screen: the files it checks and installs updates by. There is nothing to download here; Slot itself is on the version releases and the Dev build."
