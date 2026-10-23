mod common;

use std::fs::File;
use std::io::Read;

use common::{fixture, zip_of};
use slot_store::archive::zip;

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
