# Sources and provenance

- slot: user-supplied slot-1.5.0.zip, commit 118c695758127703f9ca5f61ab41d91034a93375; https://github.com/BrandonKowalski/slot. GPL-3.0-or-later. Original README, LICENSE, assets, notices, and core patches preserved.
- TrimUI toolchain: https://github.com/trimui/toolchain_sdk_smartpro/releases/tag/20231018
  - `aarch64-linux-gnu-7.5.0-linaro.tgz`, SHA256 `604d97c1f5fadc88d6ebff577ea648d0fbc650411af98a3bd8783dbe4e3ed154`.
  - GE8300 SDL source used as a reference for EGL native-window handling: `SDL2-2.26.1.GE8300.tgz`, SHA256 `a37fbed6c8771f1050e6520fa9c48354e01519de813fe176ae53089f2087bb87`. See `src/video/mali-fbdev/SDL_malivideo.c` and `include/SDL_events.h`. SDL is dynamically loaded from the user's firmware; not redistributed here.
- NextUI platform reference: https://github.com/LoveRetro/NextUI/tree/5921134e5752d85073b1de0dc0bb27a2429cd1ca/workspace/tg5040
  - `platform/platform.h`: Brick SDL button indices, screen dimensions and SD root.
  - `platform/platform.c`: battery sysfs names.
  - `libmsettings/msettings.c`: brightness ioctl 0x102 and argument layout.
  - `PAKS.md`: tools package directory convention.
- mGBA: https://github.com/libretro/mgba/tree/e31759b24e7a4e3899285ff720d7b573ac328ae7; MPL-2.0. Original source archive and upstream slot patches included.
- gpSP: https://github.com/libretro/gpsp/tree/7b2dd37394e2822110cacef321e9bb291a305b77; GPL-2.0, with per-file notices. Original source archive, upstream slot patch, and extra assembler patch included.
- FCEUmm: https://github.com/libretro/libretro-fceumm/tree/7a542dab1e87679921962a9f056186eca425c0c2; GPL-2.0. Unmodified; source archive `vendor/fceumm-src.tar.gz`, built with its `rpi3_64` (Cortex-A53) target by `brick/build-cores.sh`. Licence ships as `Slot/System/licenses/fceumm-GPL-2.0.txt`.
- Snes9x: https://github.com/libretro/snes9x/tree/fae2fea08f74180759ef540ee94259213f503480; Snes9x licence (free for non-commercial use only; Slot is free and must stay so while it ships Snes9x). Unmodified; source archive `vendor/snes9x-src.tar.gz`, built for the Cortex-A53 with its C++ runtime linked in by `brick/build-cores.sh`. Licence ships as `Slot/System/licenses/snes9x-LICENSE.txt`.
- PCSX ReARMed: https://github.com/libretro/pcsx_rearmed/tree/c8816799b50388e61cfe237fe2cdbb7d8175f20a; GPL-2.0. Source archive `vendor/pcsx_rearmed-src.tar.gz` (its `deps/` are in-tree, so the archive is complete), with `brick/pcsx-old-binutils.patch` (spells the link register `x30` for the toolchain's older assembler); built with its `h5` (Allwinner Cortex-A53) target by `brick/build-cores.sh`. Licence ships as `Slot/System/licenses/pcsx_rearmed-GPL-2.0.txt`.
- SNES cart shape: drawn from a front photo of a US Super Mario Kart cartridge (PJ's Games product image), consulted 2026-10-14; `crates/slot-ui/assets/snes_cart*.svg` are original drawings.
- NES and SNES cart labels: LaunchBox Games Database (https://gamesdb.launchbox-app.com), "Cart - Front" pictures of NES-region carts, listed in `Slot/System/{nes,snes}-cart-labels.txt` by `brick/make-cart-labels.py` from its public Metadata.zip; Cart Art downloads only the pictures of the user's own games from images.launchbox-app.com and crops the label on the device. Pictures are community scans; none are shipped.
- PlayStation disc pictures: the same LaunchBox Games Database Metadata.zip, its "Disc" pictures of PlayStation games, listed in `Slot/System/ps1-disc-labels.txt` by `brick/make-cart-labels.py ps1`; Cart Art downloads only the pictures of the user's own games and keeps each whole, cut to the disc. None are shipped. The disc and disc slot drawn on the shelf are original.
- NES cart shape: drawn from a front photo of a replacement NES shell (Retrotainment Games product image), consulted 2026-10-14; the SVGs in `crates/slot-ui/assets/nes_cart*.svg` are original drawings.

Port additions are GPL-3.0-or-later as part of slot. Rust dependencies are pinned by Cargo.lock. Core and asset notices are in the original source and the runtime `Slot/System/licenses` directory.

v0.2 bootstrap references: the same pinned NextUI commit's `skeleton/BOOT/trimui/app/MainUI`, `runtrimui.sh`, and `keymon`. These establish stock firmware discovery via `trimui/app/MainUI` and the internal `runtrimui.sh` hook. The new implementation uses a separate `.slot-boot` directory and its own backup, rather than installing NextUI's updater or destructive shutdown path. No NextUI kernel, emulators or firmware image are bundled.

v0.3 modifies frontend display geometry/scaling, startup error reporting and Brick input device selection; adds installation from the working stock-menu entry point and bounded startup retry. Emulator core binaries are unchanged from v0.1. Complete updated source is SOURCE/slot-brick-v0.3.tar.gz.

v0.3.1 selects display defaults by platform: GB/GBC Actual, GBA Stretch. Its source archive is SOURCE/slot-brick-v0.3.1.tar.gz.

v0.4: input-daemon startup follows the pinned NextUI skeleton/SYSTEM/tg5040/paks/MinUI.pak/launch.sh reference. Shutdown requests use firmware poweroff/reboot after slot saves and releases its runtime; no forced shutdown flags are used. New source archive: SOURCE/slot-brick-v0.4.tar.gz.

v0.5 restores the held-Power menu, maps its first choice to save-and-exit, retains real shutdown for the second choice, and updates labels/tests.

LED references for v0.6:
- LoveRetro/NextUI, pinned commit 5921134e5752d85073b1de0dc0bb27a2429cd1ca, workspace/tg5040/platform/platform.c: brightness paths, f1/f2/m/lr zones, RGB/effect/cycle/duration sysfs attributes.
- Same commit, workspace/all/ledcontrol/ledcontrol.c: effect IDs (2=Breathe, 4=Static).
- https://github.com/pierceTee/TrimuiLEDController — driver-interface documentation and brightness-off behavior. Consulted 2026-10-10.
New Rust controller and menu integration are original port additions under GPL-3.0-or-later. No external UI or daemon is redistributed. Source: SOURCE/slot-brick-v0.6.tar.gz.

v0.8 sleep reference: the same pinned NextUI commit's skeleton/SYSTEM/tg5040/bin/suspend, for transient Wi-Fi/Bluetooth service handling, /sys/power/state mem suspend and the false-error-after-wake workaround. Slot uses its own one-attempt helper with service rollback and standby fallback. No firmware power-management binary or kernel is distributed. Sleep LED snapshots, input guarding and render-loop changes are original Slot port additions.

v0.8.1: Brick Home+A shader and Home+B LED chords, Select+B LED alias, persistent LED mode toggle and sleep interaction tests. Original port additions; no new third-party code.

v0.9: adaptive clock controller, unchanged-frame suppression, suspend verification and launcher policy restoration are original Rust/shell implementations. Research references (consulted 2026-10-11):
- Linux CPUFreq interface: https://docs.kernel.org/admin-guide/pm/cpufreq.html
- MinUI Zero design/measurement notes: https://github.com/Zero-Labs-CFW/MinUI-Zero/blob/main/docs/DECISIONS.md (conceptual reference, no code copied).
- Stock input-daemon handshake documented in https://github.com/LoveRetro/NextUI/pull/836 (original Brick tested by contributor; PR was open when consulted). Our marker ownership and cleanup implementation is original.
- Original Brick power measurements: https://github.com/big-due/TrimUI-Brick-Hardware-Testing (measurements do not establish Slot's battery life).


v0.10: original Rust Wi-Fi settings, keyboard, asynchronous firmware-service wrapper and supplicant control client. Protocol reference: https://w1.fi/wpa_supplicant/devel/ctrl_iface_page.html . Firmware helper/socket layout checked against the pinned NextUI skeleton/SYSTEM/tg5040/etc/wifi/wifi_init.sh. Password handling follows upstream wpa_config_parse_psk (literal bytes between first/last quotes). No NextUI networking UI, daemon or external Wi-Fi program is bundled.

RetroAchievements: rcheevos v12.5.0 (https://github.com/RetroAchievements/rcheevos, MIT) is vendored, unmodified apart from leaving out disc, archive and encryption hashing, in crates/slot-cheevos/rcheevos (see its VERSION); its licence ships as Slot/System/licenses/rcheevos-MIT.txt. libretro.h there is the libretro API header from the pinned gpSP source. The slot_cheevos.c shim, the HTTP(S) client (rustls with ring and webpki-roots, pinned by Cargo.lock) and the menus are original port additions. Protocol: dorequest.php `login2`, as used by rcheevos and RetroArch.
