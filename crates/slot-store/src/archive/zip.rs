//! The zips Slot reads: a ROM a player zipped, and the card zip an update brings. Only what
//! both need: the central directory, and each file's bytes, stored or deflated.
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};

const END: u32 = 0x0605_4b50;
const CENTRAL: u32 = 0x0201_4b50;
const LOCAL: u32 = 0x0403_4b50;
const END_LEN: usize = 22;
const COMMENT_MAX: usize = 0xffff;

#[derive(Clone, Debug)]
pub struct ZipEntry {
    pub name: String,
    pub method: u16,
    pub flags: u16,
    pub packed: u64,
    pub size: u64,
    pub crc: u32,
    pub local: u64,
    /// The system the zip was made on (3 is Unix) and the file's attributes there.
    pub made_by: u8,
    pub attrs: u32,
}

impl ZipEntry {
    pub fn encrypted(&self) -> bool {
        self.flags & 1 != 0
    }

    pub fn is_dir(&self) -> bool {
        self.name.ends_with('/')
    }
}

fn bad(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("damaged zip ({what})"))
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    b.get(at..at + 2).map(|b| u16::from_le_bytes([b[0], b[1]]))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    b.get(at..at + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// Every file in the zip's central directory. A zip64 or multi-disk zip is refused.
pub fn index(file: &mut File) -> io::Result<Vec<ZipEntry>> {
    let len = file.seek(SeekFrom::End(0))?;
    let tail_len = len.min((END_LEN + COMMENT_MAX) as u64);
    file.seek(SeekFrom::Start(len - tail_len))?;
    let mut tail = vec![0u8; tail_len as usize];
    file.read_exact(&mut tail)?;
    let end = (0..tail.len().saturating_sub(END_LEN - 1))
        .rev()
        .find(|&i| u32_at(&tail, i) == Some(END))
        .ok_or_else(|| bad("no directory"))?;
    let e = &tail[end..];
    let half = |off| u16_at(e, off).ok_or_else(|| bad("directory"));
    let word = |off| u32_at(e, off).ok_or_else(|| bad("directory"));
    let (disk, dir_disk, count) = (half(4)?, half(6)?, half(10)?);
    let (dir_len, dir_at) = (word(12)?, word(16)?);
    if disk != 0 || dir_disk != 0 {
        return Err(bad("split over several disks"));
    }
    if count == u16::MAX || dir_len == u32::MAX || dir_at == u32::MAX {
        return Err(bad("zip64"));
    }
    if dir_at as u64 + dir_len as u64 > len {
        return Err(bad("directory"));
    }
    file.seek(SeekFrom::Start(dir_at as u64))?;
    let mut dir = vec![0u8; dir_len as usize];
    file.read_exact(&mut dir)?;
    let mut at = 0;
    let mut out = Vec::with_capacity(count as usize);
    for _ in 0..count {
        if u32_at(&dir, at) != Some(CENTRAL) {
            return Err(bad("entry"));
        }
        let half = |off| u16_at(&dir, at + off).ok_or_else(|| bad("entry"));
        let word = |off| u32_at(&dir, at + off).ok_or_else(|| bad("entry"));
        let (name_len, extra_len, comment_len) =
            (half(28)? as usize, half(30)? as usize, half(32)? as usize);
        let (packed, size, local) = (word(20)?, word(24)?, word(42)?);
        if packed == u32::MAX || size == u32::MAX || local == u32::MAX {
            return Err(bad("zip64"));
        }
        let name = dir
            .get(at + 46..at + 46 + name_len)
            .ok_or_else(|| bad("entry"))?;
        out.push(ZipEntry {
            name: String::from_utf8_lossy(name).into_owned(),
            made_by: (half(4)? >> 8) as u8,
            flags: half(8)?,
            method: half(10)?,
            crc: word(16)?,
            packed: packed as u64,
            size: size as u64,
            attrs: word(38)?,
            local: local as u64,
        });
        at += 46 + name_len + extra_len + comment_len;
    }
    Ok(out)
}

/// The file's bytes as they were before zipping. It stops at the size the directory gives, so
/// a zip can never unpack to more than it says.
pub fn reader<'a>(file: &'a mut File, entry: &ZipEntry) -> io::Result<Box<dyn Read + 'a>> {
    if entry.encrypted() {
        return Err(bad("encrypted"));
    }
    file.seek(SeekFrom::Start(entry.local))?;
    let mut head = [0u8; 30];
    file.read_exact(&mut head)?;
    if u32_at(&head, 0) != Some(LOCAL) {
        return Err(bad("entry"));
    }
    let skip = u16_at(&head, 26).unwrap_or(0) as i64 + u16_at(&head, 28).unwrap_or(0) as i64;
    file.seek(SeekFrom::Current(skip))?;
    let raw = file.take(entry.packed);
    match entry.method {
        0 => Ok(Box::new(raw.take(entry.size))),
        8 => Ok(Box::new(
            flate2::read::DeflateDecoder::new(raw).take(entry.size),
        )),
        _ => Err(bad("method")),
    }
}
