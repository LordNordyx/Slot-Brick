#![allow(dead_code)]

use tempfile::TempDir;

pub fn tmp_root() -> TempDir {
    let d = tempfile::tempdir().expect("tempdir");
    for sub in [
        "Games",
        "Games/GBA",
        "Games/GB",
        "Games/GBC",
        "Labels",
        "Saves",
        "States",
        "System",
        "Config",
    ] {
        std::fs::create_dir_all(d.path().join(sub)).expect("create content dir");
    }
    d
}

pub fn fixture(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// A zip of `files`, deflated or stored, as a plain zip tool writes one.
pub fn zip_of(files: &[(&str, &[u8])], deflate: bool) -> Vec<u8> {
    use std::io::Write;
    let (mut out, mut dir) = (Vec::new(), Vec::new());
    for (name, data) in files {
        let packed = if deflate {
            let mut e = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::best());
            e.write_all(data).unwrap();
            e.finish().unwrap()
        } else {
            data.to_vec()
        };
        let crc = crc32fast::hash(data);
        let method: u16 = if deflate { 8 } else { 0 };
        let local = out.len() as u32;
        out.extend(0x0403_4b50u32.to_le_bytes());
        out.extend(20u16.to_le_bytes());
        out.extend(0u16.to_le_bytes());
        out.extend(method.to_le_bytes());
        out.extend([0u8; 4]);
        out.extend(crc.to_le_bytes());
        out.extend((packed.len() as u32).to_le_bytes());
        out.extend((data.len() as u32).to_le_bytes());
        out.extend((name.len() as u16).to_le_bytes());
        out.extend(0u16.to_le_bytes());
        out.extend(name.as_bytes());
        out.extend(&packed);
        dir.extend(0x0201_4b50u32.to_le_bytes());
        dir.extend(((3u16 << 8) | 20).to_le_bytes());
        dir.extend(20u16.to_le_bytes());
        dir.extend(0u16.to_le_bytes());
        dir.extend(method.to_le_bytes());
        dir.extend([0u8; 4]);
        dir.extend(crc.to_le_bytes());
        dir.extend((packed.len() as u32).to_le_bytes());
        dir.extend((data.len() as u32).to_le_bytes());
        dir.extend((name.len() as u16).to_le_bytes());
        dir.extend([0u8; 8]);
        dir.extend((0o100644u32 << 16).to_le_bytes());
        dir.extend(local.to_le_bytes());
        dir.extend(name.as_bytes());
    }
    let at = out.len() as u32;
    out.extend(&dir);
    out.extend(0x0605_4b50u32.to_le_bytes());
    out.extend([0u8; 4]);
    out.extend((files.len() as u16).to_le_bytes());
    out.extend((files.len() as u16).to_le_bytes());
    out.extend((dir.len() as u32).to_le_bytes());
    out.extend(at.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    out
}

/// A 7z of `files`, LZMA2, as 7-Zip writes one by default.
pub fn sevenz_of(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut w = sevenz_rust2::ArchiveWriter::new(std::io::Cursor::new(Vec::new())).unwrap();
    for (name, data) in files {
        w.push_archive_entry(sevenz_rust2::ArchiveEntry::new_file(name), Some(*data))
            .unwrap();
    }
    w.finish().unwrap().into_inner()
}
