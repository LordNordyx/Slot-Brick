//! Updating Slot from its GitHub releases. Each release carries `update.txt`, naming the card
//! zip and its SHA-256, and `update.txt.sig`, its Ed25519 signature by the release key. A zip
//! is only unpacked once both check out. Slot's own files are unpacked beside the old ones and
//! swapped in, with the old ones kept until the new Slot has shown its first frame: if it never
//! does, launch.sh puts them back. The player's games, saves and settings are never touched.
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use ring::signature::{UnparsedPublicKey, ED25519};

/// Where releases are downloaded from. SLOT_UPDATE_URL points elsewhere (the simulator, a test).
pub const RELEASES: &str = "https://github.com/LordNordyx/Slot-Brick/releases";

/// The public half of the release key; the release workflows sign update.txt with the other.
const RELEASE_KEY: [u8; 32] = [
    0xe5, 0xad, 0xf6, 0x0e, 0x30, 0xd1, 0x3f, 0x29, 0x84, 0xec, 0x8d, 0x91, 0xa1, 0x0c, 0xc8, 0x3d,
    0x72, 0x08, 0x61, 0x74, 0x0d, 0x11, 0x21, 0x6e, 0x24, 0xd8, 0xbb, 0x70, 0x76, 0xf0, 0x71, 0x42,
];

/// The simulator's key (brick/sim/update-key.pem, not secret): a simulator build also takes
/// updates signed with it, so its scenarios can update from a folder. The Brick's never does.
#[cfg(any(test, feature = "sim"))]
const SIM_KEY: [u8; 32] = [
    0x82, 0xd5, 0x43, 0x4c, 0x75, 0x91, 0xdf, 0x83, 0x88, 0x52, 0x97, 0x35, 0xe4, 0xff, 0xee, 0x28,
    0x47, 0x58, 0x82, 0xa0, 0x1a, 0x18, 0xe4, 0x8f, 0x0c, 0xf2, 0xc2, 0x81, 0x4c, 0x9a, 0x44, 0xc3,
];

fn keys() -> Vec<[u8; 32]> {
    #[allow(unused_mut)]
    let mut keys = vec![RELEASE_KEY];
    #[cfg(any(test, feature = "sim"))]
    keys.push(SIM_KEY);
    keys
}

/// Beside Slot/ on the card: the download, the files unpacked from it, and those they replaced.
pub const DIR: &str = ".slot-update";
/// Every file is unpacked: the swap may start (or finish, after a power cut).
const STAGED: &str = "staged";
/// The swap has started and the new Slot has not shown a frame yet. launch.sh, finding this
/// after Slot ended, puts the old files back.
const PENDING: &str = "pending";

/// The exit status that asks launch.sh to start Slot again, on the files just swapped in.
pub const EXIT_UPDATED: i32 = 66;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Channel {
    Release,
    Dev,
}

impl Channel {
    fn name(self) -> &'static str {
        match self {
            Channel::Release => "release",
            Channel::Dev => "dev",
        }
    }
}

/// update.txt: a header line, `key=value` lines, a blank line, then the release's notes.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Manifest {
    pub channel: Channel,
    pub version: String,
    pub commit: String,
    pub zip: String,
    pub size: u64,
    pub sha256: [u8; 32],
    pub notes: Vec<String>,
}

const HEADER: &str = "slot-update 1";

impl Manifest {
    pub fn parse(text: &str) -> Result<Manifest, String> {
        let bad = || "The update's description is damaged".to_string();
        let mut lines = text.lines();
        if lines.next() != Some(HEADER) {
            return Err("This update needs a newer Slot: install it from a computer".into());
        }
        let (mut channel, mut version, mut commit, mut zip, mut size, mut sha256) = (None, None, None, None, None, None);
        for line in lines.by_ref() {
            if line.is_empty() {
                break;
            }
            let (key, value) = line.split_once('=').ok_or_else(bad)?;
            match key {
                "channel" => {
                    channel = Some(match value {
                        "release" => Channel::Release,
                        "dev" => Channel::Dev,
                        _ => return Err(bad()),
                    })
                }
                "version" => version = Some(value.to_string()),
                "commit" => commit = Some(value.to_string()),
                "zip" => zip = Some(value.to_string()),
                "size" => size = value.parse().ok(),
                "sha256" => sha256 = unhex(value),
                _ => {}
            }
        }
        let zip = zip.ok_or_else(bad)?;
        // A file name, never a path: it goes into a URL and onto the card.
        if zip.is_empty() || !zip.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)) {
            return Err(bad());
        }
        let version = version.ok_or_else(bad)?;
        if version_of(&version).is_none() {
            return Err(bad());
        }
        let commit = commit.ok_or_else(bad)?;
        if commit.len() < 7 || !commit.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(bad());
        }
        let notes = lines.map(str::to_string).filter(|l| !l.trim().is_empty()).collect();
        Ok(Manifest {
            channel: channel.ok_or_else(bad)?,
            version,
            commit,
            zip,
            size: size.ok_or_else(bad)?,
            sha256: sha256.ok_or_else(bad)?,
            notes,
        })
    }

    /// Seven hex digits of its commit, as `git log --oneline` prints them.
    pub fn short(&self) -> &str {
        &self.commit[..7]
    }
}

fn unhex(s: &str) -> Option<[u8; 32]> {
    if s.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(s.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    Some(out)
}

/// `v0.43` as [0, 43].
fn version_of(v: &str) -> Option<Vec<u32>> {
    let v = v.strip_prefix(['v', 'V']).unwrap_or(v);
    v.split('.').map(|p| p.parse().ok()).collect()
}

pub fn verified(text: &[u8], signature: &[u8], keys: &[[u8; 32]]) -> bool {
    keys.iter().any(|k| UnparsedPublicKey::new(&ED25519, k).verify(text, signature).is_ok())
}

/// What is installed: its version, the commit it was built from, and whether it is a Dev build.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Installed {
    pub version: String,
    pub commit: String,
    pub channel: Channel,
}

impl Installed {
    pub fn current() -> Installed {
        Installed {
            version: slot_ui::PORT_VERSION.to_lowercase(),
            commit: crate::build_info::Build::current().hash.to_string(),
            channel: match option_env!("SLOT_CHANNEL") {
                Some("dev") => Channel::Dev,
                _ => Channel::Release,
            },
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Offer {
    UpToDate,
    /// Newer than what is installed.
    Newer,
    /// A release, offered to a Dev build: older, perhaps, but finished.
    Back,
}

pub fn offer(installed: &Installed, m: &Manifest) -> Offer {
    match m.channel {
        Channel::Dev => {
            let same = installed.commit.len() >= 7 && m.commit.starts_with(&installed.commit);
            match installed.channel == Channel::Dev && same {
                true => Offer::UpToDate,
                false => Offer::Newer,
            }
        }
        Channel::Release if installed.channel == Channel::Dev => Offer::Back,
        Channel::Release => match (version_of(&m.version), version_of(&installed.version)) {
            (Some(new), Some(old)) if new > old => Offer::Newer,
            _ => Offer::UpToDate,
        },
    }
}

/// Where update.txt and the zip are, for each channel.
fn manifest_url(base: &str, channel: Channel) -> String {
    match channel {
        Channel::Release => format!("{base}/latest/download/update.txt"),
        Channel::Dev => format!("{base}/download/dev/update.txt"),
    }
}

fn zip_url(base: &str, m: &Manifest) -> String {
    let tag = match m.channel {
        Channel::Release => m.version.as_str(),
        Channel::Dev => "dev",
    };
    format!("{base}/download/{tag}/{}", m.zip)
}

pub fn base() -> String {
    std::env::var("SLOT_UPDATE_URL").unwrap_or_else(|_| RELEASES.to_string())
}

/// A download, as it comes. A simulator build also reads `file://` folders, standing in for
/// GitHub in its scenarios.
fn open(url: &str) -> Result<crate::net::Body, String> {
    #[cfg(any(test, feature = "sim"))]
    if let Some(path) = url.strip_prefix("file://") {
        let file = fs::File::open(path).map_err(|_| "server said 404".to_string())?;
        let len = file.metadata().ok().map(|m| m.len());
        return Ok(crate::net::Body { len, reader: Box::new(file) });
    }
    crate::net::get_stream(url)
}

fn fetch(url: &str, limit: usize) -> Result<Vec<u8>, String> {
    let body = open(url)?;
    let mut out = Vec::new();
    body.reader
        .take(limit as u64 + 1)
        .read_to_end(&mut out)
        .map_err(|e| format!("download failed: {e}"))?;
    if out.len() > limit {
        return Err("reply too large".into());
    }
    Ok(out)
}

/// The channel's latest update, checked against the keys.
pub fn check(base: &str, channel: Channel) -> Result<Manifest, String> {
    let url = manifest_url(base, channel);
    let text = fetch(&url, 256 * 1024).map_err(|e| match e.as_str() {
        "server said 404" => "No update has been published here yet".to_string(),
        _ => e,
    })?;
    let signature = fetch(&format!("{url}.sig"), 1024).map_err(|e| match e.as_str() {
        "server said 404" => "This update is not signed. Not installing it".to_string(),
        _ => e,
    })?;
    if !verified(&text, &signature, &keys()) {
        return Err("This update's signature does not match. Not installing it".into());
    }
    let text = std::str::from_utf8(&text).map_err(|_| "The update's description is damaged".to_string())?;
    let m = Manifest::parse(text)?;
    if m.channel != channel {
        return Err("The update's description is damaged".into());
    }
    Ok(m)
}

/// How an install is getting on, shared with the screen showing it.
#[derive(Default)]
pub struct Progress {
    pub downloaded: AtomicU64,
    /// Downloaded and checked: unpacking and swapping, which cannot be cancelled.
    pub unpacking: AtomicBool,
    pub cancel: AtomicBool,
}

/// Download, check, unpack and swap in `m`. `card` is the SD card's root (Slot/'s parent).
/// Fails with the card as it was, but for a swap cut short by a broken card, which
/// launch.sh then undoes.
pub fn install(base: &str, m: &Manifest, card: &Path, progress: &Progress) -> Result<(), String> {
    let dir = card.join(DIR);
    let _ = fs::remove_dir_all(&dir);
    let need = m.size * 3 + (16 << 20);
    if let Some(free) = free_space(card) {
        if free < need {
            return Err(format!("Needs {} MB free on the SD card", need.div_ceil(1 << 20)));
        }
    }
    fs::create_dir_all(&dir).map_err(|e| format!("Cannot write to the SD card: {e}"))?;
    let result = download(base, m, &dir, progress).and_then(|zip| {
        progress.unpacking.store(true, Ordering::Relaxed);
        stage(&zip, &dir.join("new")).map_err(|e| format!("Cannot unpack the update: {e}"))?;
        let _ = fs::remove_file(&zip);
        fs::write(dir.join(STAGED), "").map_err(|e| format!("Cannot write to the SD card: {e}"))
    });
    if let Err(e) = result {
        let _ = fs::remove_dir_all(&dir);
        return Err(e);
    }
    apply(card).map_err(|e| format!("Cannot swap in the update: {e}"))
}

fn download(base: &str, m: &Manifest, dir: &Path, progress: &Progress) -> Result<PathBuf, String> {
    let body = open(&zip_url(base, m))?;
    if body.len.is_some_and(|n| n != m.size) {
        return Err("The download is not the size it should be".into());
    }
    let path = dir.join("download.zip");
    let mut file = fs::File::create(&path).map_err(|e| format!("Cannot write to the SD card: {e}"))?;
    let mut reader = body.reader.take(m.size + 1);
    let mut hash = ring::digest::Context::new(&ring::digest::SHA256);
    let mut buf = vec![0u8; 64 * 1024];
    let mut got = 0u64;
    loop {
        if progress.cancel.load(Ordering::Relaxed) {
            return Err("Cancelled".into());
        }
        let n = match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof && got == m.size => break,
            Err(e) => return Err(format!("Download failed: {e}")),
        };
        hash.update(&buf[..n]);
        file.write_all(&buf[..n]).map_err(|e| format!("Cannot write to the SD card: {e}"))?;
        got += n as u64;
        progress.downloaded.store(got, Ordering::Relaxed);
    }
    if got != m.size {
        return Err("The download was cut short. Try again".into());
    }
    if hash.finish().as_ref() != &m.sha256[..] {
        return Err("The download is damaged (its checksum does not match). Try again".into());
    }
    file.sync_all().map_err(|e| format!("Cannot write to the SD card: {e}"))?;
    Ok(path)
}

fn free_space(path: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    match unsafe { libc::statvfs(c.as_ptr(), &mut st) } {
        0 => Some(st.f_bavail as u64 * st.f_frsize as u64),
        _ => None,
    }
}

/// Slot's own files on the card, which an update replaces. Everything else is the player's:
/// games, saves, states, labels, BIOS, settings, wallpapers, and the boot picture they chose.
/// Slot/AUTOBOOT says whether Slot starts by itself, so it is left as it is; trimui/ is only
/// for installing from a computer.
pub fn ours(rel: &str) -> bool {
    const OURS: [&str; 7] = [
        ".slot-boot/",
        "Apps/Slot/",
        "Apps/SlotBootRestore/",
        "Apps/StartupCheck/",
        "Apps/BootLogo/",
        "Slot/System/",
        "Slot/Overlays/",
    ];
    if rel.starts_with("Apps/BootLogo/Image/") {
        return rel == "Apps/BootLogo/Image/README.txt";
    }
    rel == "Slot/README.md" || OURS.iter().any(|p| rel.starts_with(p))
}

/// A path inside the zip's SD-CARD/ that stays inside it.
fn safe(rel: &str) -> bool {
    !rel.is_empty()
        && !rel.contains('\\')
        && rel.split('/').all(|c| !c.is_empty() && c != "." && c != "..")
}

/// Unpacks Slot's own files from the card zip (zip-card.py's: everything under SD-CARD/,
/// stored or deflated) into `into`.
pub fn stage(zip: &Path, into: &Path) -> std::io::Result<usize> {
    let bad = |what: &str| std::io::Error::new(std::io::ErrorKind::InvalidData, format!("damaged zip ({what})"));
    let data = fs::read(zip)?;
    let u16_at = |at: usize| data.get(at..at + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as usize);
    let u32_at = |at: usize| data.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize);
    let tail = data.len().saturating_sub(22 + 65535);
    let end = (tail..data.len().saturating_sub(21))
        .rev()
        .find(|&i| u32_at(i) == Some(0x0605_4b50))
        .ok_or_else(|| bad("no directory"))?;
    let count = u16_at(end + 10).ok_or_else(|| bad("directory"))?;
    let mut at = u32_at(end + 16).ok_or_else(|| bad("directory"))?;
    let mut written = 0;
    for _ in 0..count {
        if u32_at(at) != Some(0x0201_4b50) {
            return Err(bad("entry"));
        }
        let field = |off: usize| u16_at(at + off).ok_or_else(|| bad("entry"));
        let made_by = field(4)? >> 8;
        let method = field(10)?;
        let packed = u32_at(at + 20).ok_or_else(|| bad("entry"))?;
        let size = u32_at(at + 24).ok_or_else(|| bad("entry"))?;
        let (name_len, extra_len, comment_len) = (field(28)?, field(30)?, field(32)?);
        let attrs = u32_at(at + 38).ok_or_else(|| bad("entry"))?;
        let local = u32_at(at + 42).ok_or_else(|| bad("entry"))?;
        let name = data.get(at + 46..at + 46 + name_len).ok_or_else(|| bad("entry"))?;
        let name = std::str::from_utf8(name).map_err(|_| bad("name"))?.to_string();
        at += 46 + name_len + extra_len + comment_len;
        let Some(rel) = name.strip_prefix("SD-CARD/") else { continue };
        if rel.is_empty() || rel.ends_with('/') || !ours(rel) {
            continue;
        }
        if !safe(rel) {
            return Err(bad("name"));
        }
        if u32_at(local) != Some(0x0403_4b50) {
            return Err(bad("entry"));
        }
        let start = local + 30 + u16_at(local + 26).ok_or_else(|| bad("entry"))? + u16_at(local + 28).ok_or_else(|| bad("entry"))?;
        let raw = data.get(start..start + packed).ok_or_else(|| bad("entry"))?;
        let bytes = match method {
            0 => raw.to_vec(),
            8 => miniz_oxide::inflate::decompress_to_vec_with_limit(raw, size).map_err(|_| bad("data"))?,
            _ => return Err(bad("method")),
        };
        if bytes.len() != size {
            return Err(bad("size"));
        }
        let path = into.join(rel);
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut file = fs::File::create(&path)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        if made_by == 3 {
            use std::os::unix::fs::PermissionsExt;
            let mode = (attrs >> 16) as u32 & 0o777;
            if mode != 0 {
                let _ = fs::set_permissions(&path, fs::Permissions::from_mode(mode));
            }
        }
        written += 1;
    }
    if written == 0 || !into.join("Slot/System/slot").exists() {
        return Err(bad("no Slot in it"));
    }
    Ok(written)
}

/// Every file under `dir`, as paths relative to it.
fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut todo = vec![PathBuf::new()];
    while let Some(rel) = todo.pop() {
        let Ok(entries) = fs::read_dir(dir.join(&rel)) else { continue };
        for entry in entries.flatten() {
            let rel = rel.join(entry.file_name());
            match entry.file_type() {
                Ok(t) if t.is_dir() => todo.push(rel),
                Ok(_) => out.push(rel),
                Err(_) => {}
            }
        }
    }
    out.sort();
    out
}

/// Swaps the unpacked files in, each old one moved aside to old/ first. Run again after a
/// power cut, it carries on where it stopped.
pub fn apply(card: &Path) -> std::io::Result<()> {
    let dir = card.join(DIR);
    let (new, old) = (dir.join("new"), dir.join("old"));
    fs::write(dir.join(PENDING), "")?;
    for rel in files_under(&new) {
        let (dest, keep) = (card.join(&rel), old.join(&rel));
        if dest.exists() && !keep.exists() {
            fs::create_dir_all(keep.parent().unwrap_or(&old))?;
            fs::rename(&dest, &keep)?;
        }
        fs::create_dir_all(dest.parent().unwrap_or(card))?;
        fs::rename(new.join(&rel), &dest)?;
    }
    fs::remove_dir_all(&new)?;
    fs::remove_file(dir.join(STAGED))?;
    let _ = std::process::Command::new("sync").status();
    Ok(())
}

/// At startup: an update unpacked but not (or not all) swapped in, as a power cut leaves it,
/// is finished. True when it was, and Slot should start again on the new files.
pub fn resume(card: &Path) -> bool {
    if !card.join(DIR).join(STAGED).exists() {
        return false;
    }
    match apply(card) {
        Ok(()) => {
            eprintln!("slot: update: finished swapping in an update that was cut short");
            true
        }
        Err(e) => {
            eprintln!("slot: update: cannot finish swapping in the update: {e}");
            false
        }
    }
}

/// The new Slot is up: the old files are no longer needed.
pub fn settle(card: &Path) {
    let dir = card.join(DIR);
    if dir.join(PENDING).exists() && !dir.join(STAGED).exists() {
        match fs::remove_dir_all(&dir) {
            Ok(()) => eprintln!("slot: update: installed"),
            Err(e) => eprintln!("slot: update: cannot tidy {}: {e}", dir.display()),
        }
    }
}

/// The card's root: Slot/'s parent.
pub fn card_of(root: &Path) -> PathBuf {
    root.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("/mnt/SDCARD"))
}

/// The update channel kept in Config: Dev once chosen, else Release.
const CHANNEL_FILE: &str = "Config/update-channel.txt";

pub fn saved_channel(root: &Path) -> Channel {
    match fs::read_to_string(root.join(CHANNEL_FILE)).map(|s| s.trim().to_string()).as_deref() {
        Ok("dev") => Channel::Dev,
        _ => Channel::Release,
    }
}

pub fn save_channel(root: &Path, channel: Channel) -> std::io::Result<()> {
    match channel {
        Channel::Release => match fs::remove_file(root.join(CHANNEL_FILE)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        },
        Channel::Dev => fs::write(root.join(CHANNEL_FILE), format!("{}\n", channel.name())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::signature::{Ed25519KeyPair, KeyPair};

    fn manifest_text(channel: &str, version: &str, size: u64, sha: &str) -> String {
        format!(
            "{HEADER}\nchannel={channel}\nversion={version}\ncommit=0123456789abcdef0123456789abcdef01234567\n\
             zip=slot-brick-{version}-sd-card.zip\nsize={size}\nsha256={sha}\n\n- Something new.\n\n- And a fix.\n"
        )
    }

    fn installed(version: &str, commit: &str, channel: Channel) -> Installed {
        Installed { version: version.into(), commit: commit.into(), channel }
    }

    fn sim_pair() -> Ed25519KeyPair {
        let pem = fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../brick/sim/update-key.pem")).unwrap();
        let b64: String = pem.lines().filter(|l| !l.starts_with("-----")).collect();
        let der = base64(&b64);
        // A raw Ed25519 PKCS#8 v1 key: its 32-byte seed is the last thing in it.
        Ed25519KeyPair::from_seed_unchecked(&der[der.len() - 32..]).unwrap()
    }

    fn base64(s: &str) -> Vec<u8> {
        let table = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut bits = 0u32;
        let mut n = 0;
        let mut out = Vec::new();
        for c in s.bytes().filter(|&c| c != b'=') {
            bits = bits << 6 | table.iter().position(|&t| t == c).unwrap() as u32;
            n += 6;
            if n >= 8 {
                n -= 8;
                out.push((bits >> n) as u8);
            }
        }
        out
    }

    #[test]
    fn the_sim_key_in_the_code_is_the_one_on_file() {
        assert_eq!(sim_pair().public_key().as_ref(), SIM_KEY);
    }

    #[test]
    fn a_manifest_reads_its_fields_and_notes() {
        let m = Manifest::parse(&manifest_text("release", "v0.44", 12, &"ab".repeat(32))).unwrap();
        assert_eq!((m.channel, m.version.as_str(), m.size, m.short()), (Channel::Release, "v0.44", 12, "0123456"));
        assert_eq!(m.zip, "slot-brick-v0.44-sd-card.zip");
        assert_eq!(m.notes, ["- Something new.", "- And a fix."]);
    }

    #[test]
    fn a_manifest_with_a_path_for_its_zip_or_no_checksum_is_refused() {
        let good = manifest_text("release", "v0.44", 12, &"ab".repeat(32));
        assert!(Manifest::parse(&good.replace("zip=slot", "zip=../slot")).is_err());
        assert!(Manifest::parse(&good.replace("zip=slot", "zip=a/slot")).is_err());
        assert!(Manifest::parse(&good.replace(&"ab".repeat(32), "abc")).is_err());
        assert!(Manifest::parse(&good.replace("channel=release", "channel=beta")).is_err());
        assert!(Manifest::parse(&good.replace(HEADER, "slot-update 2")).unwrap_err().contains("newer Slot"));
    }

    #[test]
    fn only_a_signature_by_a_known_key_is_good() {
        let pair = sim_pair();
        let text = b"slot-update 1\n";
        let sig = pair.sign(text);
        assert!(verified(text, sig.as_ref(), &[SIM_KEY]));
        assert!(!verified(b"slot-update 1\nx", sig.as_ref(), &[SIM_KEY]));
        assert!(!verified(text, sig.as_ref(), &[RELEASE_KEY]));
        assert!(!verified(text, &[0; 64], &keys()));
    }

    #[test]
    fn newer_releases_are_offered_and_dev_builds_by_commit() {
        let m = |channel, version| Manifest {
            channel,
            version: String::from(version),
            commit: "abcdef0123456789".into(),
            zip: "z.zip".into(),
            size: 1,
            sha256: [0; 32],
            notes: Vec::new(),
        };
        let release = installed("v0.43", "1111111", Channel::Release);
        assert_eq!(offer(&release, &m(Channel::Release, "v0.44")), Offer::Newer);
        assert_eq!(offer(&release, &m(Channel::Release, "v0.43")), Offer::UpToDate);
        assert_eq!(offer(&release, &m(Channel::Release, "v0.9")), Offer::UpToDate);
        assert_eq!(offer(&installed("v0.9", "1", Channel::Release), &m(Channel::Release, "v0.10")), Offer::Newer);
        assert_eq!(offer(&release, &m(Channel::Dev, "v0.43")), Offer::Newer);
        let dev = installed("v0.43", "abcdef0", Channel::Dev);
        assert_eq!(offer(&dev, &m(Channel::Dev, "v0.43")), Offer::UpToDate);
        assert_eq!(offer(&installed("v0.43", "2222222", Channel::Dev), &m(Channel::Dev, "v0.43")), Offer::Newer);
        assert_eq!(offer(&dev, &m(Channel::Release, "v0.43")), Offer::Back);
    }

    #[test]
    fn only_slots_own_files_are_replaced() {
        for rel in ["Slot/System/slot", "Slot/System/licenses/a.txt", ".slot-boot/boot.sh", "Apps/Slot/launch.sh",
                    "Apps/BootLogo/Image/README.txt", "Slot/Overlays/GB/A.png", "Slot/README.md"] {
            assert!(ours(rel), "{rel}");
        }
        for rel in ["Slot/Games/GB/a.gb", "Slot/Saves/GB/a.sav", "Slot/Config/wifi.txt", "Slot/BIOS/gba_bios.bin",
                    "Slot/AUTOBOOT", "Apps/BootLogo/Image/boot.png", "trimui/app/MainUI", "Slot/Wallpapers/a.png"] {
            assert!(!ours(rel), "{rel}");
        }
        assert!(!safe("Slot/System/../../x") && !safe("Slot//x") && !safe("Slot\\x") && safe("Slot/System/slot"));
    }

    /// A card zip as zip-card.py writes one: deflated, under SD-CARD/, Unix modes kept.
    fn card_zip(files: &[(&str, &[u8])]) -> Vec<u8> {
        let (mut out, mut dir) = (Vec::new(), Vec::new());
        for (name, data) in files {
            let packed = miniz_oxide::deflate::compress_to_vec(data, 6);
            let offset = out.len() as u32;
            let head = |buf: &mut Vec<u8>, central: bool| {
                buf.extend_from_slice(&(if central { 0x0201_4b50u32 } else { 0x0403_4b50 }).to_le_bytes());
                if central {
                    buf.extend_from_slice(&(0x0314u16).to_le_bytes());
                }
                buf.extend_from_slice(&[20, 0, 0, 0, 8, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
                buf.extend_from_slice(&(packed.len() as u32).to_le_bytes());
                buf.extend_from_slice(&(data.len() as u32).to_le_bytes());
                buf.extend_from_slice(&(name.len() as u16).to_le_bytes());
                buf.extend_from_slice(&[0, 0]);
                if central {
                    buf.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
                    buf.extend_from_slice(&(0o100755u32 << 16).to_le_bytes());
                    buf.extend_from_slice(&offset.to_le_bytes());
                }
                buf.extend_from_slice(name.as_bytes());
            };
            head(&mut out, false);
            out.extend_from_slice(&packed);
            head(&mut dir, true);
        }
        let at = out.len() as u32;
        out.extend_from_slice(&dir);
        out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        out.extend_from_slice(&[0, 0, 0, 0]);
        out.extend_from_slice(&(files.len() as u16).to_le_bytes());
        out.extend_from_slice(&(files.len() as u16).to_le_bytes());
        out.extend_from_slice(&(dir.len() as u32).to_le_bytes());
        out.extend_from_slice(&at.to_le_bytes());
        out.extend_from_slice(&[0, 0]);
        out
    }

    const NEW_CARD: &[(&str, &[u8])] = &[
        ("SD-CARD/Slot/System/slot", b"new slot"),
        ("SD-CARD/Slot/System/launch.sh", b"new launch"),
        ("SD-CARD/Slot/Overlays/GB/Frame.png", b"new overlay"),
        ("SD-CARD/Slot/Config/defaults.txt", b"not ours"),
        ("SD-CARD/Slot/AUTOBOOT", b"not ours"),
    ];

    fn old_card(card: &Path) {
        for (rel, data) in [("Slot/System/slot", "old slot"), ("Slot/System/launch.sh", "old launch"),
                            ("Slot/Saves/GB/a.sav", "save"), ("Slot/Config/defaults.txt", "mine")] {
            fs::create_dir_all(card.join(rel).parent().unwrap()).unwrap();
            fs::write(card.join(rel), data).unwrap();
        }
    }

    fn read(card: &Path, rel: &str) -> String {
        fs::read_to_string(card.join(rel)).unwrap_or_default()
    }

    #[test]
    fn unpacking_takes_only_slots_own_files() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("card.zip"), card_zip(NEW_CARD)).unwrap();
        assert_eq!(stage(&dir.path().join("card.zip"), &dir.path().join("new")).unwrap(), 3);
        assert_eq!(read(&dir.path().join("new"), "Slot/System/slot"), "new slot");
        assert!(!dir.path().join("new/Slot/Config").exists() && !dir.path().join("new/Slot/AUTOBOOT").exists());
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(dir.path().join("new/Slot/System/slot")).unwrap().permissions().mode();
        assert_eq!(mode & 0o111, 0o111);
    }

    #[test]
    fn a_zip_reaching_outside_the_card_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let evil: &[(&str, &[u8])] = &[("SD-CARD/Slot/System/slot", b"x"), ("SD-CARD/Slot/System/../../../escape", b"x")];
        fs::write(dir.path().join("card.zip"), card_zip(evil)).unwrap();
        assert!(stage(&dir.path().join("card.zip"), &dir.path().join("new")).is_err());
        assert!(!dir.path().join("escape").exists());
    }

    /// A folder standing in for GitHub's releases: update.txt and the zip, signed.
    fn publish(dir: &Path, zip: &[u8], sign: bool) -> String {
        let sha: String = ring::digest::digest(&ring::digest::SHA256, zip).as_ref().iter().map(|b| format!("{b:02x}")).collect();
        let text = manifest_text("release", "v9.0", zip.len() as u64, &sha);
        let at = dir.join("releases");
        fs::create_dir_all(at.join("latest/download")).unwrap();
        fs::create_dir_all(at.join("download/v9.0")).unwrap();
        fs::write(at.join("latest/download/update.txt"), &text).unwrap();
        if sign {
            fs::write(at.join("latest/download/update.txt.sig"), sim_pair().sign(text.as_bytes()).as_ref()).unwrap();
        }
        fs::write(at.join("download/v9.0/slot-brick-v9.0-sd-card.zip"), zip).unwrap();
        format!("file://{}", at.display())
    }

    #[test]
    fn an_update_swaps_in_slots_files_and_keeps_the_old_ones_until_it_starts() {
        let dir = tempfile::tempdir().unwrap();
        let card = dir.path().join("card");
        old_card(&card);
        let base = publish(dir.path(), &card_zip(NEW_CARD), true);
        let m = check(&base, Channel::Release).unwrap();
        install(&base, &m, &card, &Progress::default()).unwrap();
        assert_eq!((read(&card, "Slot/System/slot"), read(&card, "Slot/System/launch.sh")), ("new slot".into(), "new launch".into()));
        assert_eq!(read(&card, "Slot/Overlays/GB/Frame.png"), "new overlay");
        assert_eq!((read(&card, "Slot/Saves/GB/a.sav"), read(&card, "Slot/Config/defaults.txt")), ("save".into(), "mine".into()));
        assert!(!card.join("Slot/AUTOBOOT").exists());
        assert_eq!(read(&card, ".slot-update/old/Slot/System/slot"), "old slot");
        assert!(card.join(".slot-update/pending").exists());
        settle(&card);
        assert!(!card.join(DIR).exists());
    }

    #[test]
    fn an_unsigned_or_damaged_update_is_never_installed() {
        let dir = tempfile::tempdir().unwrap();
        let card = dir.path().join("card");
        old_card(&card);
        let base = publish(dir.path(), &card_zip(NEW_CARD), false);
        assert!(check(&base, Channel::Release).unwrap_err().contains("not signed"));
        let base = publish(dir.path(), &card_zip(NEW_CARD), true);
        let m = check(&base, Channel::Release).unwrap();
        // The zip on the server is not the one the signed description names.
        let swapped: &[(&str, &[u8])] = &[("SD-CARD/Slot/System/slot", b"evil slot")];
        let mut other = card_zip(swapped);
        other.resize(m.size as usize, 0);
        fs::write(dir.path().join("releases/download/v9.0/slot-brick-v9.0-sd-card.zip"), other).unwrap();
        assert!(install(&base, &m, &card, &Progress::default()).unwrap_err().contains("checksum"));
        assert_eq!(read(&card, "Slot/System/slot"), "old slot");
        assert!(!card.join(DIR).exists());
    }

    #[test]
    fn a_tampered_description_fails_its_signature() {
        let dir = tempfile::tempdir().unwrap();
        let base = publish(dir.path(), &card_zip(NEW_CARD), true);
        let path = dir.path().join("releases/latest/download/update.txt");
        let text = fs::read_to_string(&path).unwrap().replace("v9.0", "v9.1");
        fs::write(&path, text).unwrap();
        assert!(check(&base, Channel::Release).unwrap_err().contains("signature"));
    }

    #[test]
    fn a_cancelled_download_leaves_the_card_as_it_was() {
        let dir = tempfile::tempdir().unwrap();
        let card = dir.path().join("card");
        old_card(&card);
        let base = publish(dir.path(), &card_zip(NEW_CARD), true);
        let m = check(&base, Channel::Release).unwrap();
        let progress = Progress::default();
        progress.cancel.store(true, Ordering::Relaxed);
        assert_eq!(install(&base, &m, &card, &progress).unwrap_err(), "Cancelled");
        assert_eq!(read(&card, "Slot/System/slot"), "old slot");
        assert!(!card.join(DIR).exists());
    }

    #[test]
    fn a_swap_cut_short_is_finished_at_startup() {
        let dir = tempfile::tempdir().unwrap();
        let card = dir.path().join("card");
        old_card(&card);
        let up = card.join(DIR);
        fs::write(dir.path().join("card.zip"), card_zip(NEW_CARD)).unwrap();
        stage(&dir.path().join("card.zip"), &up.join("new")).unwrap();
        fs::write(up.join(STAGED), "").unwrap();
        // The power went with launch.sh swapped and slot not yet.
        fs::create_dir_all(up.join("old/Slot/System")).unwrap();
        fs::rename(card.join("Slot/System/launch.sh"), up.join("old/Slot/System/launch.sh")).unwrap();
        fs::rename(up.join("new/Slot/System/launch.sh"), card.join("Slot/System/launch.sh")).unwrap();
        assert!(resume(&card));
        assert_eq!((read(&card, "Slot/System/slot"), read(&card, "Slot/System/launch.sh")), ("new slot".into(), "new launch".into()));
        assert_eq!(read(&card, ".slot-update/old/Slot/System/launch.sh"), "old launch");
        assert_eq!(read(&card, ".slot-update/old/Slot/System/slot"), "old slot");
        assert!(!resume(&card));
    }

    #[test]
    fn the_dev_channel_is_kept_and_release_forgets_it() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("Config")).unwrap();
        assert_eq!(saved_channel(dir.path()), Channel::Release);
        save_channel(dir.path(), Channel::Dev).unwrap();
        assert_eq!(saved_channel(dir.path()), Channel::Dev);
        save_channel(dir.path(), Channel::Release).unwrap();
        save_channel(dir.path(), Channel::Release).unwrap();
        assert_eq!(saved_channel(dir.path()), Channel::Release);
    }
}
