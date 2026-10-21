use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};

use crate::gba::header_title_code;
use crate::platform::Platform;

/// Where a NES game's Famicom cart label is kept, inside Labels/NES.
pub const FAMICOM_LABELS: &str = "Famicom";

/// A game's labels as they are on the card now: (its own, its Famicom one).
pub fn labels_of(root: &Path, platform: Platform, stem: &str) -> (Option<PathBuf>, Option<PathBuf>) {
    let dir = root.join("Labels").join(platform.dir_name());
    let find = |dir: PathBuf| {
        listing(&dir).get(&format!("{stem}.png").to_lowercase()).map(|name| dir.join(name))
    };
    let famicom = (platform == Platform::Nes).then(|| find(dir.join(FAMICOM_LABELS))).flatten();
    (find(dir), famicom)
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Cart {
    pub platform: Platform,
    pub stem: String,
    pub rom: PathBuf,
    pub label: Option<PathBuf>,
    /// A NES game's label for the Famicom cart, kept apart from its NES one
    /// (Labels/NES/Famicom), as the two carts' labels are different shapes.
    pub famicom_label: Option<PathBuf>,
    pub title: String,
    pub code: String,
    pub shell: Option<crate::ShellChoice>,
}

impl Cart {
    pub fn read_header(&mut self) {
        match self.platform {
            Platform::Gba => {
                if let Some((title, code)) = header_title_code(&self.rom) {
                    self.title = title;
                    self.code = code;
                }
            }
            Platform::Gb | Platform::Gbc => {
                if let Some(title) = crate::gb::title(&self.rom) {
                    self.title = title;
                }
            }
            // The file name is the title: an iNES header names no game, a SNES header's
            // sits at one of several places a copier header can shift, and a disc's ID is
            // a catalogue number, not a name.
            Platform::Nes | Platform::Snes | Platform::Ps1 => {}
        }
    }
}

#[derive(Debug)]
pub enum StoreError {
    Io(std::io::Error),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::Io(e) => write!(f, "io: {e}"),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<std::io::Error> for StoreError {
    fn from(e: std::io::Error) -> Self {
        StoreError::Io(e)
    }
}

pub fn scan(root: &Path) -> Result<Vec<Cart>, StoreError> {
    let read = |file: &str| std::fs::read_to_string(root.join(file)).unwrap_or_default();
    let shells = crate::cart_shell::layered(
        &read(crate::CART_SHELL_FILE),
        &read(crate::LABELS_SHELL_FILE),
    );
    let mut carts = Vec::new();
    for platform in Platform::ALL {
        let dir = root.join("Games").join(platform.dir_name());
        let roms: Vec<PathBuf> = match std::fs::read_dir(&dir) {
            // A PlayStation game may be several files, and in a folder of its own.
            Ok(_) if platform == Platform::Ps1 => crate::ps1::games(&dir),
            Ok(d) => d
                .filter_map(|e| e.ok())
                .filter(|e| platform.accepts(&e.path()) && is_file(e))
                .map(|e| e.path())
                .collect(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => {
                eprintln!("slot: scan: {}: {e}", dir.display());
                continue;
            }
        };
        let labels_dir = root.join("Labels").join(platform.dir_name());
        let labels = listing(&labels_dir);
        let famicom_dir = labels_dir.join(FAMICOM_LABELS);
        let famicom = if platform == Platform::Nes { listing(&famicom_dir) } else { HashMap::new() };
        for rom in roms {
            if is_hidden(&rom) {
                continue;
            }
            let Some(stem) = rom.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let label = labels
                .get(&format!("{stem}.png").to_lowercase())
                .map(|name| labels_dir.join(name));
            let famicom_label = famicom
                .get(&format!("{stem}.png").to_lowercase())
                .map(|name| famicom_dir.join(name));
            carts.push(Cart {
                platform,
                stem: stem.to_string(),
                title: String::new(),
                code: String::new(),
                shell: shells.get(&crate::cart_shell::key(stem)).copied(),
                label,
                famicom_label,
                rom,
            });
        }
    }
    carts.sort_by(|a, b| {
        (a.platform as u8, sort_key(&a.stem)).cmp(&(b.platform as u8, sort_key(&b.stem)))
    });
    Ok(carts)
}

pub fn listing(dir: &Path) -> HashMap<String, String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return HashMap::new();
    };
    entries
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|name| !name.starts_with('.'))
        .map(|name| (name.to_lowercase(), name))
        .collect()
}

fn is_file(entry: &std::fs::DirEntry) -> bool {
    match entry.file_type() {
        Ok(t) if t.is_symlink() => entry.path().is_file(),
        Ok(t) => t.is_file(),
        Err(_) => false,
    }
}

pub fn sort_key(stem: &str) -> (u8, String) {
    (group_of(stem), stem.to_uppercase())
}

fn group_of(stem: &str) -> u8 {
    match stem.chars().find(|c| !c.is_whitespace()) {
        Some(c) if c.is_ascii_digit() => 0,
        Some(c) if c.is_alphabetic() => 1,
        _ => 2,
    }
}

pub fn initial(stem: &str) -> char {
    match group_of(stem) {
        1 => stem
            .chars()
            .find(|c| !c.is_whitespace())
            .and_then(|c| c.to_uppercase().next())
            .unwrap_or('#'),
        _ => '#',
    }
}

pub fn is_hidden(p: &Path) -> bool {
    p.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with('.'))
}
