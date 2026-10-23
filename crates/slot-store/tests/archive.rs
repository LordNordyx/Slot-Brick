mod common;

use std::fs::File;
use std::io::Read;

use common::{fixture, zip_of};
use slot_store::archive::{self as ar, zip, Skip};
use slot_store::Platform;

fn unzipped(path: &std::path::Path) -> Vec<(String, Vec<u8>)> {
    let mut file = File::open(path).unwrap();
    zip::index(&mut file)
        .unwrap()
        .into_iter()
        .filter(|e| !e.is_dir())
        .map(|e| {
            let mut data = Vec::new();
            zip::reader(&mut file, &e)
                .unwrap()
                .read_to_end(&mut data)
                .unwrap();
            (e.name, data)
        })
        .collect()
}

#[test]
fn zips_from_every_tool_give_back_the_rom() {
    let rom = std::fs::read(fixture("rom.gb")).unwrap();
    for name in [
        "rom.zip",
        "rom-stored.zip",
        "rom-7zip.zip",
        "rom-descriptor.zip",
    ] {
        assert_eq!(
            unzipped(&fixture(name)),
            vec![("rom.gb".to_string(), rom.clone())],
            "{name}"
        );
    }
}

#[test]
fn the_index_gives_each_files_size_checksum_and_method() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("two.zip");
    std::fs::write(
        &path,
        zip_of(&[("a.gb", b"hello"), ("b.txt", b"there")], true),
    )
    .unwrap();
    let index = zip::index(&mut File::open(&path).unwrap()).unwrap();
    assert_eq!(index.len(), 2);
    assert_eq!(
        (
            index[0].name.as_str(),
            index[0].size,
            index[0].crc,
            index[0].method
        ),
        ("a.gb", 5, crc32fast::hash(b"hello"), 8)
    );
    assert!(!index[0].encrypted());
    let locked = zip::index(&mut File::open(fixture("rom-encrypted.zip")).unwrap()).unwrap();
    assert!(locked[0].encrypted());
}

#[test]
fn a_file_that_is_no_zip_or_is_cut_short_is_refused() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("x.zip");
    std::fs::write(&path, b"no zip here").unwrap();
    assert!(zip::index(&mut File::open(&path).unwrap()).is_err());
    let whole = std::fs::read(fixture("rom.zip")).unwrap();
    std::fs::write(&path, &whole[..whole.len() - 30]).unwrap();
    assert!(zip::index(&mut File::open(&path).unwrap()).is_err());
}

#[test]
fn deflated_data_longer_than_the_index_says_stops_at_the_declared_size() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("bomb.zip");
    let mut bytes = zip_of(&[("big.gb", &[0u8; 1 << 16])], true);
    let dir = bytes
        .windows(4)
        .position(|w| w == 0x0201_4b50u32.to_le_bytes())
        .unwrap();
    bytes[dir + 24..dir + 28].copy_from_slice(&100u32.to_le_bytes());
    std::fs::write(&path, bytes).unwrap();
    let mut file = File::open(&path).unwrap();
    let entry = zip::index(&mut file).unwrap().remove(0);
    let mut data = Vec::new();
    zip::reader(&mut file, &entry)
        .unwrap()
        .read_to_end(&mut data)
        .unwrap();
    assert_eq!(data.len(), 100);
}

fn rom() -> Vec<u8> {
    std::fs::read(fixture("rom.gb")).unwrap()
}

fn write(d: &tempfile::TempDir, name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let path = d.path().join(name);
    std::fs::write(&path, bytes).unwrap();
    path
}

#[test]
fn every_archive_a_tool_makes_names_its_rom_size_and_checksum() {
    for name in [
        "rom.zip",
        "rom-stored.zip",
        "rom-7zip.zip",
        "rom-descriptor.zip",
        "rom-mac.zip",
        "rom-lzma.7z",
        "rom-lzma2.7z",
    ] {
        let e = ar::entry(&fixture(name), Platform::Gb).unwrap_or_else(|s| panic!("{name}: {s}"));
        assert_eq!(
            (e.name.as_str(), e.size, e.crc),
            ("rom.gb", 0x400, crc32fast::hash(&rom())),
            "{name}"
        );
    }
}

#[test]
fn an_archive_slot_cannot_open_says_why() {
    for (name, why) in [
        ("rom-encrypted.zip", Skip::Encrypted),
        ("rom-encrypted.7z", Skip::Encrypted),
        ("rom-hidden.7z", Skip::Encrypted),
        ("rom-bzip2.zip", Skip::Method),
        ("rom-ppmd.7z", Skip::Method),
    ] {
        assert_eq!(ar::entry(&fixture(name), Platform::Gb), Err(why), "{name}");
    }
}

#[test]
fn an_archive_needs_exactly_one_rom_and_one_for_the_shelf() {
    let d = tempfile::tempdir().unwrap();
    let none = write(&d, "none.zip", &zip_of(&[("readme.txt", b"hi")], true));
    let two = write(
        &d,
        "two.zip",
        &zip_of(&[("a.gb", b"a"), ("b.gbc", b"b")], true),
    );
    let mixed = write(
        &d,
        "mixed.7z",
        &common::sevenz_of(&[("a.gb", b"a"), ("b.nes", b"b")]),
    );
    let notes = write(
        &d,
        "notes.zip",
        &zip_of(
            &[("readme.txt", b"hi"), ("game.gb", b"rom"), ("dir/", b"")],
            false,
        ),
    );
    assert_eq!(ar::entry(&none, Platform::Gb), Err(Skip::NoRom));
    assert_eq!(ar::entry(&two, Platform::Gb), Err(Skip::SeveralRoms));
    assert_eq!(ar::entry(&mixed, Platform::Gb), Err(Skip::SeveralRoms));
    assert_eq!(ar::entry(&notes, Platform::Gb).unwrap().name, "game.gb");
    assert_eq!(ar::entry(&notes, Platform::Nes), Err(Skip::NoRom));
    assert_eq!(ar::entry(&notes, Platform::Ps1), Err(Skip::NoRom));
}

#[test]
fn a_rom_reads_the_same_plain_zipped_or_7zd() {
    for path in [
        fixture("rom.gb"),
        fixture("rom.zip"),
        fixture("rom-stored.zip"),
        fixture("rom-lzma2.7z"),
        fixture("rom-lzma.7z"),
    ] {
        let mut title = [0u8; 7];
        ar::read_at(&path, 0x134, &mut title).unwrap();
        assert_eq!(&title, b"ZIPTEST", "{}", path.display());
        assert_eq!(ar::len(&path).unwrap(), 0x400);
        assert_eq!(ar::crc32(&path).unwrap(), crc32fast::hash(&rom()));
        let mut past = [0u8; 2];
        assert!(
            ar::read_at(&path, 0x3ff, &mut past).is_err(),
            "{}",
            path.display()
        );
    }
}

#[test]
fn a_crc_of_a_plain_file_is_the_one_no_intro_lists() {
    let d = tempfile::tempdir().unwrap();
    assert_eq!(
        ar::crc32(&write(&d, "check.gb", b"123456789")).unwrap(),
        0xCBF4_3926
    );
}

#[test]
fn zip_and_7z_are_archives_in_any_case_and_nothing_else_is() {
    for name in ["a.zip", "a.ZIP", "a.7z", "a.7Z"] {
        assert!(ar::is_archive(std::path::Path::new(name)), "{name}");
    }
    for name in ["a.gb", "a.zip.gb", "zip", "a.rar"] {
        assert!(!ar::is_archive(std::path::Path::new(name)), "{name}");
    }
}

#[test]
fn a_zipped_rom_is_unpacked_under_its_archives_name_and_kept_for_next_time() {
    let d = tempfile::tempdir().unwrap();
    let cache = d.path().join("Unpacked");
    let zip = write(
        &d,
        "Pocket Zip.zip",
        &std::fs::read(fixture("rom.zip")).unwrap(),
    );
    let out = ar::playable(&zip, &cache).unwrap();
    assert_eq!(out, cache.join("Pocket Zip.gb"));
    assert_eq!(std::fs::read(&out).unwrap(), rom());
    let when = std::fs::metadata(&out).unwrap().modified().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    assert_eq!(ar::playable(&zip, &cache).unwrap(), out);
    assert_eq!(
        std::fs::metadata(&out).unwrap().modified().unwrap(),
        when,
        "it unpacked again"
    );
}

#[test]
fn starting_another_archive_deletes_the_last_one_unpacked() {
    let d = tempfile::tempdir().unwrap();
    let cache = d.path().join("Unpacked");
    let zip = write(&d, "A.zip", &std::fs::read(fixture("rom.zip")).unwrap());
    let sevenz = write(&d, "B.7z", &std::fs::read(fixture("rom-lzma2.7z")).unwrap());
    let first = ar::playable(&zip, &cache).unwrap();
    let second = ar::playable(&sevenz, &cache).unwrap();
    assert!(!first.exists());
    assert_eq!(std::fs::read(&second).unwrap(), rom());
}

#[test]
fn a_plain_rom_is_played_where_it_is_and_leaves_the_cache_alone() {
    let d = tempfile::tempdir().unwrap();
    let cache = d.path().join("Unpacked");
    let zip = write(&d, "A.zip", &std::fs::read(fixture("rom.zip")).unwrap());
    let kept = ar::playable(&zip, &cache).unwrap();
    let plain = write(&d, "B.gb", &rom());
    assert_eq!(ar::playable(&plain, &cache).unwrap(), plain);
    assert!(kept.exists());
}

#[test]
fn an_archive_changed_since_is_unpacked_again() {
    let d = tempfile::tempdir().unwrap();
    let cache = d.path().join("Unpacked");
    let zip = write(&d, "A.zip", &zip_of(&[("a.gb", b"first")], true));
    ar::playable(&zip, &cache).unwrap();
    std::fs::write(&zip, zip_of(&[("a.gb", b"second!")], true)).unwrap();
    assert_eq!(
        std::fs::read(ar::playable(&zip, &cache).unwrap()).unwrap(),
        b"second!"
    );
}

#[test]
fn a_rom_that_fails_its_checksum_is_refused_and_leaves_nothing_behind() {
    let d = tempfile::tempdir().unwrap();
    let cache = d.path().join("Unpacked");
    let mut bytes = zip_of(&[("a.gb", b"some rom")], true);
    let dir = bytes
        .windows(4)
        .position(|w| w == 0x0201_4b50u32.to_le_bytes())
        .unwrap();
    bytes[dir + 16] ^= 0xff;
    let zip = write(&d, "A.zip", &bytes);
    assert!(ar::playable(&zip, &cache).is_err());
    assert_eq!(std::fs::read_dir(&cache).map(|r| r.count()).unwrap_or(0), 0);
}

#[test]
fn a_damaged_7z_is_refused() {
    let d = tempfile::tempdir().unwrap();
    let mut bytes = std::fs::read(fixture("rom-lzma2.7z")).unwrap();
    bytes[40] ^= 0xff;
    let sevenz = write(&d, "A.7z", &bytes);
    let cache = d.path().join("Unpacked");
    assert!(ar::playable(&sevenz, &cache).is_err());
    assert_eq!(std::fs::read_dir(&cache).map(|r| r.count()).unwrap_or(0), 0);
}

#[test]
fn free_space_is_known_for_a_folder_that_exists() {
    let d = tempfile::tempdir().unwrap();
    assert!(slot_store::free_space(d.path()).is_some_and(|n| n > 0));
}
