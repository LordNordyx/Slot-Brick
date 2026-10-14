use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::UNIX_EPOCH;

use slot_store::Cart;
use slot_ui::{fitted_label, label_size, nes_famicom_of, shown_label};

/// Versioned: a label is fitted differently now (a tall one on a wide label is shown whole),
/// so the older folder's are of no use.
const DIR: &str = "System/Cache/Labels2";
const OLD_DIRS: [&str; 1] = ["System/Cache/Labels"];

/// Removes older label caches in the background: they can be large, the card slow.
pub fn forget_old(root: &Path) {
    // Labels a search found and nobody kept, should Slot have stopped before clearing them.
    crate::cart_art::forget_choices(root);
    let old: Vec<PathBuf> = OLD_DIRS.iter().map(|d| root.join(d)).filter(|d| d.is_dir()).collect();
    if old.is_empty() {
        return;
    }
    let spawned = thread::Builder::new().name("slot-label-cache".into()).spawn(move || {
        for dir in old {
            if let Err(e) = std::fs::remove_dir_all(&dir) {
                eprintln!("slot: label cache: {}: {e}", dir.display());
            }
        }
    });
    if let Err(e) = spawned {
        eprintln!("slot: label cache: {e}");
    }
}
const HEAD: usize = 24;

pub fn label_art(root: &Path, cart: &Cart) -> Option<Vec<u8>> {
    let src = shown_label(cart)?;
    let (w, h) = label_size(cart);
    // A label being chosen in the cart menu is shown, not cached: it is likely gone in a moment.
    if src.starts_with(root.join(crate::cart_art::PICK_DIR)) {
        return fitted_label(cart, w, h);
    }
    let stamp = stamp(src)?;
    let cached = cache_path(root, cart);
    if let Some(art) = read(&cached, stamp, w, h) {
        return art;
    }
    let art = fitted_label(cart, w, h);
    if let Some(dir) = cached.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let (cw, ch) = if art.is_some() { (w, h) } else { (0, 0) };
    let mut bytes = Vec::with_capacity(HEAD + art.as_ref().map_or(0, Vec::len));
    bytes.extend_from_slice(&stamp.0.to_le_bytes());
    bytes.extend_from_slice(&stamp.1.to_le_bytes());
    bytes.extend_from_slice(&cw.to_le_bytes());
    bytes.extend_from_slice(&ch.to_le_bytes());
    bytes.extend_from_slice(art.as_deref().unwrap_or_default());
    if let Err(e) = std::fs::write(&cached, &bytes) {
        eprintln!("slot: label cache: {}: {e}", cached.display());
    }
    art
}

pub fn fresh(root: &Path, cart: &Cart) -> bool {
    let Some(src) = shown_label(cart) else {
        return true;
    };
    let Some(stamp) = stamp(src) else {
        return true;
    };
    let (w, h) = label_size(cart);
    std::fs::File::open(cache_path(root, cart))
        .and_then(|mut f| {
            let mut head = [0u8; HEAD];
            f.read_exact(&mut head)?;
            Ok((head, f.metadata()?.len()))
        })
        .is_ok_and(|(head, len)| {
            body_len(&head, stamp, w, h).is_some_and(|body| len == (HEAD + body) as u64)
        })
}

pub fn stale<'a>(root: &Path, carts: impl Iterator<Item = &'a Cart>) -> Vec<Cart> {
    let mut cached = HashMap::new();
    carts
        .filter(|c| shown_label(c).is_some())
        .filter(|c| {
            !cached
                .entry(c.platform)
                .or_insert_with(|| slot_store::listing(&root.join(DIR).join(c.platform.dir_name())))
                .contains_key(&cache_name(c).to_lowercase())
        })
        .cloned()
        .collect()
}

pub fn cache_path(root: &Path, cart: &Cart) -> PathBuf {
    root.join(DIR).join(cart.platform.dir_name()).join(cache_name(cart))
}

/// A NES game on the Famicom cart has its label cached apart: another picture, another size.
fn cache_name(cart: &Cart) -> String {
    if nes_famicom_of(cart) {
        format!("{}.famicom.rgba", cart.stem)
    } else {
        format!("{}.rgba", cart.stem)
    }
}

pub struct Rebuild {
    done: Arc<AtomicUsize>,
    total: usize,
}

impl Rebuild {
    pub fn start(root: &Path, carts: Vec<Cart>) -> Self {
        let total = carts.len();
        let done = Arc::new(AtomicUsize::new(0));
        let next = Arc::new(AtomicUsize::new(0));
        let carts = Arc::new(carts);
        let threads = thread::available_parallelism().map_or(1, |n| n.get());
        for n in 0..threads.min(total) {
            let (root, carts, next, done) = (
                root.to_path_buf(),
                carts.clone(),
                next.clone(),
                done.clone(),
            );
            let spawned = thread::Builder::new()
                .name(format!("slot-labels-{n}"))
                .spawn(move || loop {
                    let Some(cart) = carts.get(next.fetch_add(1, Ordering::Relaxed)) else {
                        return;
                    };
                    let _ = label_art(&root, cart);
                    done.fetch_add(1, Ordering::Release);
                });
            if let Err(e) = spawned {
                eprintln!("slot: label cache: rebuild thread failed to start: {e}");
            }
        }
        Rebuild { done, total }
    }

    pub fn done(&self) -> usize {
        self.done.load(Ordering::Acquire).min(self.total)
    }

    pub fn total(&self) -> usize {
        self.total
    }

    pub fn finished(&self) -> bool {
        self.done() == self.total
    }
}

fn stamp(src: &Path) -> Option<(u64, u64)> {
    let meta = std::fs::metadata(src).ok()?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs());
    Some((meta.len(), mtime))
}

fn read(path: &Path, stamp: (u64, u64), w: u32, h: u32) -> Option<Option<Vec<u8>>> {
    let bytes = std::fs::read(path).ok()?;
    let (head, rgba) = bytes.split_at_checked(HEAD)?;
    match body_len(head, stamp, w, h)? {
        0 if rgba.is_empty() => Some(None),
        n if rgba.len() == n => Some(Some(rgba.to_vec())),
        _ => None,
    }
}

fn body_len(head: &[u8], stamp: (u64, u64), w: u32, h: u32) -> Option<usize> {
    let word = |at: usize| u64::from_le_bytes(head[at..at + 8].try_into().unwrap_or_default());
    let half = |at: usize| u32::from_le_bytes(head[at..at + 4].try_into().unwrap_or_default());
    if (word(0), word(8)) != stamp {
        return None;
    }
    match (half(16), half(20)) {
        (0, 0) => Some(0),
        size if size == (w, h) => Some((w * h * 4) as usize),
        _ => None,
    }
}
