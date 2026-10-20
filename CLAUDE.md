# Slot. for TrimUI Brick — notes for agents

Port of Slot. (a GB/GBC/GBA frontend, plus NES and SNES) to the TrimUI Brick and Brick Hammer.
Rust workspace in `crates/`; everything Brick-specific is behind the `brick` Cargo feature.
`brick/` holds the card's files (`brick/card/`), its build scripts, the simulator and the
changelog. The root `README.md` has a Development section; read it before a release.

## Working on main

- `main` takes pull requests only. Branch, push, open a PR; never push to `main` directly.
- PR titles say what changes for players in plain words ("Auto Save interval setting"),
  not how ("Refactor save timer").

## Changelog — required

Release notes are not generated from commits or PR titles. They are `brick/CHANGELOG.md`:

- A PR that changes anything on the card (`crates/*/src`, `crates/*/assets`, `brick/card/`,
  `vendor/`, `Cargo.lock`) adds a line under `## Unreleased`. The `Changelog` CI check
  (`brick/changelog-check.sh`) fails without it.
- Write it for players: what they can now do or what was fixed, and where it is in the menus
  (e.g. "Gameplay → Auto Save: save every 1, 5 or 10 minutes, or never. It was every minute.").
  Not commit-speak, not internals.
- Changes players won't notice (refactors, tests, CI, docs) get the `no-changelog` label.
- The `## Unreleased` section is what the Dev build release shows (`brick/dev-notes.sh`), and
  becomes the numbered release's notes when it is moved under `## vX.Y`.

## Releases

Follow "Making a release" in `README.md`: bump the version in all five places it lists, move
`## Unreleased` under `## vX.Y` (leave an empty `## Unreleased`), merge that as a `Release vX.Y`
PR, then tag `vX.Y` on `main` and push the tag. `.github/workflows/release.yml` checks, tests,
builds and publishes. Every merge to `main` also replaces the `dev` pre-release.

## Checking a change

- `cargo test --workspace`, `cargo fmt --all -- --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`.
- Brick simulator (Docker): `sh brick/sim.sh unit`, `sh brick/sim.sh test` (screenshot
  scenarios vs `brick/sim/golden`; `update` rewrites goldens — only when the change is meant
  to alter the screen). See `brick/sim/README.md`.
- A card build: `sh brick/docker-build.sh`, output in `dist-brick/`.

## Card contents

- Anything shipped on the SD card lives under `brick/card/`; `brick/package-card.sh` copies
  it and adds the binaries. Bundled third-party assets (e.g. KrutzOtrem's overlays in
  `brick/card/Slot/Overlays/`) need their licence in `licenses/` and a credit in both READMEs.
- `brick/overlays-local/` is git-ignored and never reaches a published card.
