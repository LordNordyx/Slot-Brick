//! ROMs kept in a .zip or .7z. The one ROM inside is read, checked and unpacked here, so the
//! rest of Slot goes on passing a cart's path around.
mod sevenz;
pub mod zip;

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::platform::Platform;

/// Where a zipped game's ROM is unpacked to, under Slot/. Only the last one is kept.
pub const UNPACKED_DIR: &str = "System/Cache/Unpacked";
const FROM_FILE: &str = "from.txt";

/// The ROM inside an archive, as its index lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub size: u64,
    pub crc: u32,
}

/// Why an archive is not on the shelf.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Skip {
    NoRom,
    SeveralRoms,
    Encrypted,
    Method,
    Damaged,
}

impl std::fmt::Display for Skip {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Skip::NoRom => "holds no ROM for this shelf",
            Skip::SeveralRoms => "holds more than one ROM",
            Skip::Encrypted => "has a password",
            Skip::Method => "is packed in a way Slot cannot unpack",
            Skip::Damaged => "is damaged, or is a zip64",
        })
    }
}

/// A file inside an archive: `at` is its place in the archive's own list.
struct Member {
    at: usize,
    name: String,
    size: u64,
    crc: Option<u32>,
    dir: bool,
    encrypted: bool,
    supported: bool,
}

impl Member {
    fn entry(&self) -> Entry {
        Entry {
            name: self.name.clone(),
            size: self.size,
            crc: self.crc.unwrap_or(0),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Zip,
    SevenZ,
}

fn kind(path: &Path) -> Option<Kind> {
    let ext = path.extension()?.to_str()?;
    if ext.eq_ignore_ascii_case("zip") {
        Some(Kind::Zip)
    } else if ext.eq_ignore_ascii_case("7z") {
        Some(Kind::SevenZ)
    } else {
        None
    }
}

pub fn is_archive(path: &Path) -> bool {
    kind(path).is_some()
}

fn members(path: &Path) -> Result<Vec<Member>, Skip> {
    match kind(path) {
        Some(Kind::Zip) => {
            let mut file = File::open(path).map_err(|_| Skip::Damaged)?;
            Ok(zip::index(&mut file)
                .map_err(|_| Skip::Damaged)?
                .into_iter()
                .enumerate()
                .map(|(at, e)| Member {
                    at,
                    dir: e.is_dir(),
                    encrypted: e.encrypted(),
                    supported: matches!(e.method, 0 | 8),
                    size: e.size,
                    crc: Some(e.crc),
                    name: e.name,
                })
                .collect())
        }
        Some(Kind::SevenZ) => sevenz::members(path),
        None => Err(Skip::NoRom),
    }
}

/// A file that may be a ROM: not a folder, nor the hidden files and resource forks a Mac adds,
/// and named as a ROM of some cart shelf.
fn rom_like(m: &Member) -> bool {
    !m.dir
        && !m
            .name
            .split(['/', '\\'])
            .any(|c| c.starts_with('.') || c == "__MACOSX")
        && Platform::ALL
            .iter()
            .any(|p| p.takes_archives() && p.accepts(Path::new(&m.name)))
}

/// The archive's one ROM. A ROM for another shelf counts too, so it is the same ROM whichever
/// shelf asks.
fn rom(members: Vec<Member>) -> Result<Member, Skip> {
    let mut roms = members.into_iter().filter(rom_like);
    let rom = roms.next().ok_or(Skip::NoRom)?;
    if roms.next().is_some() {
        return Err(Skip::SeveralRoms);
    }
    if rom.encrypted {
        return Err(Skip::Encrypted);
    }
    if !rom.supported {
        return Err(Skip::Method);
    }
    if rom.crc.is_none() {
        return Err(Skip::Damaged);
    }
    Ok(rom)
}

/// The ROM an archive holds for `platform`'s shelf.
pub fn entry(path: &Path, platform: Platform) -> Result<Entry, Skip> {
    let rom = rom(members(path)?)?;
    if !platform.takes_archives() || !platform.accepts(Path::new(&rom.name)) {
        return Err(Skip::NoRom);
    }
    Ok(rom.entry())
}

fn refused(path: &Path, why: Skip) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("{} {why}", path.display()),
    )
}

fn rom_of(path: &Path) -> io::Result<Member> {
    members(path)
        .and_then(rom)
        .map_err(|why| refused(path, why))
}

/// Runs `f` on the archive's ROM, unpacked as `f` reads it.
fn with_rom<T>(
    path: &Path,
    f: impl FnOnce(&Entry, &mut dyn Read) -> io::Result<T>,
) -> io::Result<T> {
    let rom = rom_of(path)?;
    let entry = rom.entry();
    match kind(path) {
        Some(Kind::Zip) => {
            let mut file = File::open(path)?;
            let found = zip::index(&mut file)?
                .into_iter()
                .nth(rom.at)
                .ok_or_else(|| refused(path, Skip::Damaged))?;
            let mut r = zip::reader(&mut file, &found)?;
            f(&entry, &mut r)
        }
        Some(Kind::SevenZ) => sevenz::with_member(path, rom.at, |r| f(&entry, r)),
        None => Err(refused(path, Skip::NoRom)),
    }
}

/// `buf.len()` bytes from `off` in a ROM, plain or inside an archive. An archive is unpacked
/// from its start up to there, and no further.
pub fn read_at(path: &Path, off: u64, buf: &mut [u8]) -> io::Result<()> {
    if !is_archive(path) {
        let mut file = File::open(path)?;
        file.seek(SeekFrom::Start(off))?;
        return file.read_exact(buf);
    }
    with_rom(path, |_, r| {
        io::copy(&mut (&mut *r).take(off), &mut io::sink())?;
        r.read_exact(buf)
    })
}

/// A ROM's size: an archive's from its index.
pub fn len(path: &Path) -> io::Result<u64> {
    match is_archive(path) {
        true => rom_of(path).map(|r| r.size),
        false => Ok(std::fs::metadata(path)?.len()),
    }
}

/// The CRC32 No-Intro and the cart art set list ROMs by: an archive's from its index, without
/// unpacking.
pub fn crc32(path: &Path) -> io::Result<u32> {
    if is_archive(path) {
        return rom_of(path).map(|r| r.crc.unwrap_or(0));
    }
    let mut file = File::open(path)?;
    let mut hasher = crc32fast::Hasher::new();
    let mut buf = vec![0u8; 256 << 10];
    loop {
        match file.read(&mut buf)? {
            0 => return Ok(hasher.finalize()),
            n => hasher.update(&buf[..n]),
        }
    }
}

/// The file a core should load: a plain ROM itself, or an archive's ROM unpacked into `dir` as
/// `<archive's name>.<ROM's extension>`. The same archive, unchanged, is not unpacked again;
/// any other unpacked ROM is deleted first.
pub fn playable(path: &Path, dir: &Path) -> io::Result<PathBuf> {
    if !is_archive(path) {
        return Ok(path.to_path_buf());
    }
    let rom = rom_of(path)?;
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| refused(path, Skip::Damaged))?;
    let ext = Path::new(&rom.name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("rom")
        .to_ascii_lowercase();
    let out = dir.join(format!("{stem}.{ext}"));
    let from = from_line(path, &rom.entry())?;
    let kept = std::fs::read_to_string(dir.join(FROM_FILE)).is_ok_and(|t| t == from)
        && std::fs::metadata(&out).is_ok_and(|m| m.len() == rom.size);
    if kept {
        return Ok(out);
    }
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir)?;
    if crate::free_space(dir).is_some_and(|free| free < rom.size) {
        return Err(io::Error::new(
            io::ErrorKind::StorageFull,
            format!(
                "{} needs {} MB free on the SD card",
                path.display(),
                rom.size.div_ceil(1 << 20)
            ),
        ));
    }
    let part = dir.join(format!(".{stem}.{ext}.part"));
    let unpacked = with_rom(path, |entry, r| {
        let mut file = File::create(&part)?;
        let mut hasher = crc32fast::Hasher::new();
        let mut buf = vec![0u8; 256 << 10];
        let mut written = 0u64;
        loop {
            let n = r.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
            file.write_all(&buf[..n])?;
            written += n as u64;
        }
        if written != entry.size || hasher.finalize() != entry.crc {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "{} is damaged (its checksum does not match)",
                    path.display()
                ),
            ));
        }
        file.sync_all()
    });
    if let Err(e) = unpacked {
        let _ = std::fs::remove_file(&part);
        return Err(e);
    }
    std::fs::rename(&part, &out)?;
    crate::atomic_write(&dir.join(FROM_FILE), from.as_bytes())?;
    Ok(out)
}

/// What the unpacked ROM came from: the archive, its size and age, and the ROM's checksum.
fn from_line(path: &Path, rom: &Entry) -> io::Result<String> {
    let meta = std::fs::metadata(path)?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos());
    Ok(format!(
        "{}\t{}\t{mtime}\t{:08X}\n",
        path.display(),
        meta.len(),
        rom.crc
    ))
}
