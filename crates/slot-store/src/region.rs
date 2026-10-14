//! Where a NES or SNES ROM was sold, read from the ROM itself: a ripped cart's file name often
//! has no "(Japan)" or "(Europe)" in it.
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// The SNES header's destination code (0x00 Japan, 0x01 North America, 0x02 Europe, ...), from
/// whichever of the LoROM, HiROM and ExHiROM headers checks out. A copier's 512-byte header
/// before the ROM is skipped.
pub fn snes_destination(rom: &Path) -> Option<u8> {
    let mut f = File::open(rom).ok()?;
    let len = f.metadata().ok()?.len();
    let skip = if len % 1024 == 512 { 512 } else { 0 };
    let mut fallback = None;
    for at in [0x7fc0u64, 0xffc0, 0x40_ffc0] {
        if skip + at + 0x20 > len {
            continue;
        }
        let mut h = [0u8; 0x20];
        f.seek(SeekFrom::Start(skip + at)).ok()?;
        f.read_exact(&mut h).ok()?;
        let complement = u16::from_le_bytes([h[0x1c], h[0x1d]]);
        let checksum = u16::from_le_bytes([h[0x1e], h[0x1f]]);
        let destination = h[0x19];
        if destination > 0x14 {
            continue;
        }
        if checksum ^ complement == 0xffff {
            return Some(destination);
        }
        // A hack or a bad dump can carry a stale checksum: a sane map mode will do.
        if fallback.is_none() && h[0x15] & 0xe0 == 0x20 {
            fallback = Some(destination);
        }
    }
    fallback
}

/// Sold in Japan or a PAL country, so on the rounder PAL / Super Famicom cart: None when the
/// header says nothing either way.
pub fn snes_pal_or_japan(rom: &Path) -> Option<bool> {
    // Japan, Europe and its countries (0x02..=0x0a), Australia.
    snes_destination(rom).map(|d| matches!(d, 0x00 | 0x02..=0x0a | 0x11))
}

/// Mappers only Japanese (Famicom) carts used: Konami's VRC chips, Namco 163, Jaleco, Irem,
/// Sunsoft-3 and Taito boards that never came west.
const FAMICOM_ONLY: &[u16] = &[18, 19, 21, 22, 23, 24, 25, 26, 65, 67, 73, 75, 80, 82, 85, 88, 95, 210];

/// The iNES mapper number, None when the file has no iNES header.
pub fn nes_mapper(rom: &Path) -> Option<u16> {
    let mut h = [0u8; 16];
    File::open(rom).ok()?.read_exact(&mut h).ok()?;
    if &h[..4] != b"NES\x1a" {
        return None;
    }
    let nes2 = h[7] & 0x0c == 0x08;
    // An old dumper's text in bytes 7..16 ("DiskDude!") is no mapper: only the low nibble counts.
    let high = if nes2 || h[12..].iter().all(|b| *b == 0) { h[7] & 0xf0 } else { 0 };
    let mut mapper = (h[6] >> 4) as u16 | high as u16;
    if nes2 {
        mapper |= ((h[8] & 0x0f) as u16) << 8;
    }
    Some(mapper)
}

/// A board only Famicom carts had. Most Famicom games share boards with NES ones, so this
/// spots only some; the rest go by name or the cart menu.
pub fn nes_famicom_board(rom: &Path) -> bool {
    nes_mapper(rom).is_some_and(|m| FAMICOM_ONLY.contains(&m))
}
