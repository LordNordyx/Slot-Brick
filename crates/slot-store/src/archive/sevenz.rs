//! 7z archives, through sevenz-rust2: what is in one, and one file out of it.
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use sevenz_rust2::{Archive, BlockDecoder, EncoderMethod, Error, Password};

use super::{Member, Skip};

/// What this build unpacks: LZMA and LZMA2, with the filters 7-Zip puts before them.
const METHODS: [&[u8]; 12] = [
    EncoderMethod::ID_COPY,
    EncoderMethod::ID_LZMA,
    EncoderMethod::ID_LZMA2,
    EncoderMethod::ID_DELTA,
    EncoderMethod::ID_BCJ_X86,
    EncoderMethod::ID_BCJ_ARM,
    EncoderMethod::ID_BCJ_ARM64,
    EncoderMethod::ID_BCJ_ARM_THUMB,
    EncoderMethod::ID_BCJ_PPC,
    EncoderMethod::ID_BCJ_IA64,
    EncoderMethod::ID_BCJ_SPARC,
    EncoderMethod::ID_BCJ_RISCV,
];

fn read(path: &Path) -> Result<(File, Archive), Error> {
    let mut file = File::open(path)?;
    let archive = Archive::read(&mut file, &Password::empty())?;
    Ok((file, archive))
}

fn io_error(e: Error) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("damaged 7z ({e})"))
}

pub(super) fn members(path: &Path) -> Result<Vec<Member>, Skip> {
    // With its names hidden too, the archive's own index is what is locked.
    let (_, archive) = read(path).map_err(|e| match e {
        Error::PasswordRequired => Skip::Encrypted,
        Error::UnsupportedCompressionMethod(m) if m.starts_with("AES") => Skip::Encrypted,
        _ => Skip::Damaged,
    })?;
    Ok(archive
        .files
        .iter()
        .enumerate()
        .map(|(at, f)| {
            let coders = archive.stream_map.file_block_index[at]
                .map_or(&[][..], |b| &archive.blocks[b].coders[..]);
            Member {
                at,
                name: f.name.clone(),
                size: f.size,
                crc: f.has_crc.then_some(f.crc as u32),
                dir: f.is_directory || !f.has_stream,
                encrypted: coders
                    .iter()
                    .any(|c| c.encoder_method_id() == EncoderMethod::ID_AES256_SHA256),
                supported: coders
                    .iter()
                    .all(|c| METHODS.contains(&c.encoder_method_id())),
            }
        })
        .collect())
}

/// Runs `f` on file `at`, unpacked as it is read, on one thread. The files before it in its
/// block are read through and dropped: in a solid block each follows on from the one before.
pub(super) fn with_member<T>(
    path: &Path,
    at: usize,
    f: impl FnOnce(&mut dyn Read) -> io::Result<T>,
) -> io::Result<T> {
    let (mut file, archive) = read(path).map_err(io_error)?;
    let block = archive
        .stream_map
        .file_block_index
        .get(at)
        .copied()
        .flatten()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "7z: the file has no data"))?;
    let name = archive.files[at].name.clone();
    let password = Password::empty();
    let mut f = Some(f);
    let mut out = None;
    let done = BlockDecoder::new(1, block, &archive, &password, &mut file).for_each_entries(
        &mut |entry, r| {
            if out.is_some() || entry.name != name {
                io::copy(r, &mut io::sink())?;
                return Ok(true);
            }
            out = f.take().map(|f| f(r));
            Ok(false)
        },
    );
    match (out, done) {
        (Some(result), _) => result,
        (None, Err(e)) => Err(io_error(e)),
        (None, Ok(_)) => Err(io::Error::new(
            io::ErrorKind::NotFound,
            "7z: file not found",
        )),
    }
}
