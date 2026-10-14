use slot_store::region::{nes_famicom_board, nes_mapper, snes_destination, snes_pal_or_japan};

fn snes_rom(dir: &tempfile::TempDir, name: &str, hirom: bool, copier: bool, destination: u8) -> std::path::PathBuf {
    let mut rom = vec![0u8; 0x10000];
    let at = if hirom { 0xffc0 } else { 0x7fc0 };
    rom[at + 0x15] = if hirom { 0x21 } else { 0x20 };
    rom[at + 0x19] = destination;
    rom[at + 0x1c..at + 0x20].copy_from_slice(&[0x34, 0x12, 0xcb, 0xed]);
    if copier {
        rom.splice(0..0, [0u8; 512]);
    }
    let path = dir.path().join(name);
    std::fs::write(&path, rom).unwrap();
    path
}

#[test]
fn a_snes_rom_says_where_it_was_sold() {
    let d = tempfile::tempdir().unwrap();
    assert_eq!(snes_destination(&snes_rom(&d, "lo.sfc", false, false, 0x02)), Some(0x02));
    assert_eq!(snes_destination(&snes_rom(&d, "hi.sfc", true, false, 0x09)), Some(0x09));
    assert_eq!(snes_destination(&snes_rom(&d, "smc.smc", true, true, 0x00)), Some(0x00));
    assert_eq!(snes_pal_or_japan(&snes_rom(&d, "eu.sfc", false, false, 0x02)), Some(true));
    assert_eq!(snes_pal_or_japan(&snes_rom(&d, "jp.sfc", true, false, 0x00)), Some(true));
    assert_eq!(snes_pal_or_japan(&snes_rom(&d, "us.sfc", false, false, 0x01)), Some(false));
    let blank = d.path().join("blank.sfc");
    std::fs::write(&blank, [0u8; 0x200]).unwrap();
    assert_eq!(snes_pal_or_japan(&blank), None);
    assert_eq!(snes_pal_or_japan(&d.path().join("missing.sfc")), None);
}

#[test]
fn a_nes_rom_on_a_famicom_only_board_is_spotted() {
    let d = tempfile::tempdir().unwrap();
    let rom = |name: &str, h6: u8, h7: u8, tail: &[u8]| {
        let mut h = vec![b'N', b'E', b'S', 0x1a, 2, 1, h6, h7, 0, 0, 0, 0, 0, 0, 0, 0];
        h[12..12 + tail.len()].copy_from_slice(tail);
        h.resize(16 + 0x8000, 0);
        let p = d.path().join(name);
        std::fs::write(&p, h).unwrap();
        p
    };
    assert_eq!(nes_mapper(&rom("vrc6.nes", 0x80, 0x10, &[])), Some(24));
    assert!(nes_famicom_board(&rom("vrc6b.nes", 0x80, 0x10, &[])));
    assert!(!nes_famicom_board(&rom("mmc3.nes", 0x40, 0x00, &[])));
    assert_eq!(nes_mapper(&rom("dude.nes", 0x40, b'D', b"ude!")), Some(4), "dumper text read as a mapper");
    let junk = d.path().join("junk.nes");
    std::fs::write(&junk, b"not a rom").unwrap();
    assert_eq!(nes_mapper(&junk), None);
}
