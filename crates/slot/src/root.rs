use std::path::{Path, PathBuf};

/// Every platform in Platform::ALL has its four folders here (tests/dist.rs checks).
pub const DIRS: [&str; 40] = [
    "BIOS",
    "Config",
    "Fonts",
    "Games",
    "Games/GBA",
    "Games/GB",
    "Games/GBC",
    "Games/NES",
    "Games/SNES",
    "Games/PS1",
    "Labels",
    "Labels/GBA",
    "Labels/GB",
    "Labels/GBC",
    "Labels/NES",
    "Labels/SNES",
    "Labels/PS1",
    "Overlays",
    "Overlays/GBA",
    "Overlays/GB",
    "Overlays/GBC",
    "Overlays/NES",
    "Overlays/SNES",
    "Overlays/PS1",
    "Saves",
    "Saves/GBA",
    "Saves/GB",
    "Saves/GBC",
    "Saves/NES",
    "Saves/SNES",
    "Saves/PS1",
    "States",
    "States/GBA",
    "States/GB",
    "States/GBC",
    "States/NES",
    "States/SNES",
    "States/PS1",
    "System",
    "Wallpapers",
];

pub fn ensure(root: &Path) {
    for sub in DIRS {
        let _ = std::fs::create_dir_all(root.join(sub));
    }
    if let Err(e) = slot_store::move_config(root) {
        eprintln!("slot: config: {e}");
    }
}

pub fn bios_dir(root: &Path) -> PathBuf {
    root.join("BIOS")
}

const BIOS_FILE: &str = "gba_bios.bin";

const BIOS_BYTES: u64 = 16 * 1024;
const BIOS_FIRST_BYTE: u8 = 0x18;

pub fn has_real_bios(root: &Path) -> bool {
    let Ok(mut f) = std::fs::File::open(bios_dir(root).join(BIOS_FILE)) else {
        return false;
    };
    if !f.metadata().is_ok_and(|m| m.len() == BIOS_BYTES) {
        return false;
    }
    let mut first = [0u8; 1];
    std::io::Read::read_exact(&mut f, &mut first).is_ok() && first[0] == BIOS_FIRST_BYTE
}

pub fn saves_dir(root: &Path) -> PathBuf {
    root.join("Saves")
}

/// Space on the card that holds `path`, in bytes: (free, size). Free is what the player can
/// still fill, the filesystem's own reserve left out.
pub fn card_space(path: &Path) -> Option<(u64, u64)> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: `c` is a NUL-terminated path and `st` a statvfs for the call to fill.
    if unsafe { libc::statvfs(c.as_ptr(), &mut st) } != 0 {
        return None;
    }
    let block = st.f_frsize as u64;
    Some(((st.f_bavail as u64).saturating_mul(block), (st.f_blocks as u64).saturating_mul(block)))
}
