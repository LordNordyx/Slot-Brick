mod common;

use std::path::{Path, PathBuf};

use common::{sevenz_of, tmp_root, zip_of};
use slot_store::{gb, header_clean, header_title_code, region, scan, Platform};

fn put(root: &Path, rel: &str, bytes: &[u8]) -> PathBuf {
    let path = root.join("Games").join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, bytes).unwrap();
    path
}

fn stems(root: &Path, platform: Platform) -> Vec<String> {
    scan(root)
        .unwrap()
        .into_iter()
        .filter(|c| c.platform == platform)
        .map(|c| c.stem)
        .collect()
}

#[test]
fn a_zipped_or_7zd_rom_is_a_cart_named_after_its_archive() {
    let d = tmp_root();
    put(d.path(), "GB/Tetris.gb", b"plain");
    put(
        d.path(),
        "GB/Pocket Zip.zip",
        &zip_of(&[("Pocket Test.gb", b"rom")], true),
    );
    put(
        d.path(),
        "NES/Mario.7z",
        &sevenz_of(&[("Super Mario Bros. (World).nes", b"rom")]),
    );
    assert_eq!(stems(d.path(), Platform::Gb), ["Pocket Zip", "Tetris"]);
    assert_eq!(stems(d.path(), Platform::Nes), ["Mario"]);
    let cart = scan(d.path())
        .unwrap()
        .into_iter()
        .find(|c| c.stem == "Mario")
        .unwrap();
    assert!(cart.rom.ends_with("NES/Mario.7z"));
}

#[test]
fn archives_slot_cannot_play_are_left_off_the_shelf() {
    let d = tmp_root();
    put(
        d.path(),
        "GB/Two.zip",
        &zip_of(&[("a.gb", b"a"), ("b.gb", b"b")], true),
    );
    put(
        d.path(),
        "GB/Nothing.zip",
        &zip_of(&[("readme.txt", b"hi")], true),
    );
    put(
        d.path(),
        "GB/NES Game.zip",
        &zip_of(&[("game.nes", b"rom")], true),
    );
    put(d.path(), "GB/Broken.7z", b"not a 7z");
    put(d.path(), "GB/._Mac Fork.zip", b"\0\x05\x16\x07");
    assert!(stems(d.path(), Platform::Gb).is_empty());
}

#[test]
fn a_plain_rom_wins_over_an_archive_of_the_same_name_and_a_zip_over_a_7z() {
    let d = tmp_root();
    put(d.path(), "GB/Tetris.gb", b"plain");
    put(
        d.path(),
        "GB/tetris.zip",
        &zip_of(&[("t.gb", b"zipped")], true),
    );
    put(d.path(), "GB/Kirby.7z", &sevenz_of(&[("k.gb", b"7z")]));
    put(d.path(), "GB/Kirby.zip", &zip_of(&[("k.gb", b"zip")], true));
    let names: Vec<String> = scan(d.path())
        .unwrap()
        .into_iter()
        .filter(|c| c.platform == Platform::Gb)
        .map(|c| c.rom.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["Kirby.zip", "Tetris.gb"]);
}

#[test]
fn the_playstation_shelf_takes_no_archives() {
    let d = tmp_root();
    put(
        d.path(),
        "PS1/Game.zip",
        &zip_of(&[("game.cue", b"FILE")], true),
    );
    assert!(stems(d.path(), Platform::Ps1).is_empty());
    assert!(!Platform::Ps1.takes_archives());
    assert!(Platform::ALL
        .iter()
        .filter(|p| **p != Platform::Ps1)
        .all(|p| p.takes_archives()));
}

fn gb_rom(cgb: u8) -> Vec<u8> {
    let mut rom = vec![0u8; 0x8000];
    rom[0x134..0x13b].copy_from_slice(b"POCKETS");
    rom[0x143] = cgb;
    rom
}

fn gba_rom() -> Vec<u8> {
    let mut rom = vec![0u8; 0x400];
    rom[3] = 0xEA;
    rom[0xa0..0xac].copy_from_slice(b"POKEMON EMER");
    rom[0xac..0xb0].copy_from_slice(b"BPEE");
    rom[0xb2] = 0x96;
    rom
}

fn snes_rom(destination: u8) -> Vec<u8> {
    let mut rom = vec![0u8; 0x10000];
    let h = &mut rom[0xffc0..0xffe0];
    h[0x15] = 0x21;
    h[0x19] = destination;
    h[0x1e..0x20].copy_from_slice(&0xffffu16.to_le_bytes());
    rom
}

fn nes_rom(mapper: u8) -> Vec<u8> {
    let mut rom = vec![0u8; 0x4010];
    rom[..4].copy_from_slice(b"NES\x1a");
    rom[6] = mapper << 4;
    rom[7] = mapper & 0xf0;
    rom
}

#[test]
fn header_readers_see_inside_archives_as_they_see_the_plain_rom() {
    let d = tmp_root();
    for (ext, rom) in [
        ("gb", gb_rom(0xc0)),
        ("gba", gba_rom()),
        ("sfc", snes_rom(0x02)),
        ("nes", nes_rom(24)),
    ] {
        let name = format!("game.{ext}");
        let plain = put(d.path(), &format!("x/{name}"), &rom);
        let zipped = put(
            d.path(),
            &format!("x/{ext}.zip"),
            &zip_of(&[(name.as_str(), &rom)], true),
        );
        let sevenz = put(
            d.path(),
            &format!("x/{ext}.7z"),
            &sevenz_of(&[(name.as_str(), &rom)]),
        );
        for path in [&zipped, &sevenz] {
            let at = path.display();
            assert_eq!(gb::class(path), gb::class(&plain), "{at}");
            assert_eq!(gb::title(path), gb::title(&plain), "{at}");
            assert_eq!(header_title_code(path), header_title_code(&plain), "{at}");
            assert_eq!(header_clean(path), header_clean(&plain), "{at}");
            assert_eq!(
                region::snes_destination(path),
                region::snes_destination(&plain),
                "{at}"
            );
            assert_eq!(region::nes_mapper(path), region::nes_mapper(&plain), "{at}");
        }
    }
    let games = d.path().join("Games/x");
    assert_eq!(gb::class(&games.join("gb.7z")), gb::Class::ColourOnly);
    assert_eq!(gb::title(&games.join("gb.zip")).as_deref(), Some("POCKETS"));
    assert_eq!(
        header_title_code(&games.join("gba.zip")),
        Some(("POKEMON EMER".to_string(), "BPEE".to_string()))
    );
    assert!(header_clean(&games.join("gba.7z")));
    assert_eq!(
        region::snes_pal_or_japan(&games.join("sfc.zip")),
        Some(true)
    );
    assert!(region::nes_famicom_board(&games.join("nes.7z")));
}
