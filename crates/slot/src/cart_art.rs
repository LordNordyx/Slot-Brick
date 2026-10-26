//! Fetches real cartridge labels for games with no label, over HTTPS. A label the user
//! made is never replaced: only carts without one are looked up. Slot is built around
//! the cartridge, so no other kind of art is fetched.
//!
//! - GB, GBC, GBA: label scans from the slot art set behind Cart Studio (art.slot-cfw.fyi,
//!   scans from ScreenScraper), found by the ROM's CRC32.
//! - NES, SNES: the set has none, so the label is cut out of a LaunchBox Games Database
//!   "Cart - Front" picture (images.launchbox-app.com), found by the ROM's file name. Only
//!   pictures of the standard US cart are used, whose label always sits in the same place.
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use slot_store::{atomic_write, best_match, same_title_match, scan, Cart, Platform};

/// Origins: `https://host`, or `http://host:port` for the tests' local server.
const SLOT_ART: &str = "https://art.slot-cfw.fyi";
const PHOTOS: &str = "https://images.launchbox-app.com";
/// Pictures tried per game before giving up: the first may be a gold or odd cart.
const PHOTO_TRIES: usize = 3;
/// A cut label's longer side at most: the shelf draws it a few hundred pixels across, and
/// some pictures are 3000 pixels or more, which would cost megabytes on the card for nothing.
const LABEL_MAX: u32 = 512;

/// A console whose labels are cut from pictures of its cart: where the picture list is, the
/// standard cart's shape (height / width), where its label sits and where plain plastic
/// shows (side: beside the label, to learn the plastic's colour; plain: must be that
/// colour), all as fractions of the cart's outline.
struct CartPhoto {
    /// Built by brick/make-cart-labels.py, shipped on the card.
    index: &'static str,
    /// The folder inside Labels/<console> its labels go to: a Famicom cart's are kept apart.
    sub: &'static str,
    aspect: (f32, f32),
    label: [f32; 4],
    /// Where the label's art is surely printed: mostly not plastic coloured, or there is none.
    art: [f32; 4],
    /// Plain plastic, which gives the cart's colour.
    side: [f32; 4],
    plain: [[f32; 4]; 2],
}

/// The NES cart: label top right of a full-height grip.
const NES_PHOTO: CartPhoto = CartPhoto {
    index: "System/nes-cart-labels.txt",
    sub: "",
    aspect: (1.08, 1.16),
    label: [0.405, 0.006, 0.865, 0.680],
    art: [0.405, 0.006, 0.865, 0.680],
    side: [0.02, 0.3, 0.065, 0.6],
    plain: [[0.42, 0.73, 0.85, 0.80], [0.885, 0.1, 0.93, 0.6]],
};

/// The US SNES cart: label across the top of the middle, ridged wings either side.
const SNES_PHOTO: CartPhoto = CartPhoto {
    index: "System/snes-cart-labels.txt",
    sub: "",
    aspect: (0.62, 0.67),
    label: [0.200, 0.008, 0.807, 0.432],
    art: [0.200, 0.008, 0.807, 0.432],
    side: [0.04, 0.35, 0.10, 0.65],
    plain: [[0.25, 0.47, 0.75, 0.55], [0.88, 0.2, 0.97, 0.8]],
};

/// The Super Famicom and PAL SNES cart: rounder, the label wide across the top of the front,
/// plain plastic below it down to the window, and plain plastic either side of that.
const SNES_PAL_PHOTO: CartPhoto = CartPhoto {
    index: "System/snes-pal-cart-labels.txt",
    sub: "",
    aspect: (0.64, 0.78),
    label: [0.085, 0.085, 0.915, 0.42],
    art: [0.3, 0.15, 0.7, 0.38],
    side: [0.25, 0.52, 0.75, 0.58],
    plain: [[0.2, 0.75, 0.8, 0.85], [0.04, 0.55, 0.12, 0.85]],
};

/// The Famicom cart: a wide label over most of the front, plain plastic right of it and
/// below; the connector tab underneath. Makers placed the label a little differently, so
/// this takes its middle; the plastic is read below it, as the edges are shaded.
const FAMICOM_PHOTO: CartPhoto = CartPhoto {
    index: "System/famicom-cart-labels.txt",
    sub: slot_store::FAMICOM_LABELS,
    aspect: (0.6, 0.7),
    label: [0.09, 0.125, 0.91, 0.74],
    art: [0.25, 0.25, 0.75, 0.65],
    side: [0.3, 0.9, 0.7, 0.95],
    plain: [[0.1, 0.9, 0.9, 0.97], [0.955, 0.2, 0.975, 0.7]],
};

/// A PlayStation disc, printed side up: the picture is the disc, so it is kept whole, cut to
/// the disc's own round outline. Only `index` and `aspect` are read (see `disc_from_photo`).
const PS1_DISC_PHOTO: CartPhoto = CartPhoto {
    index: "System/ps1-disc-labels.txt",
    sub: "",
    aspect: (0.9, 1.1),
    label: [0.0, 0.0, 1.0, 1.0],
    art: [0.0, 0.0, 1.0, 1.0],
    side: [0.0, 0.0, 1.0, 1.0],
    plain: [[0.0, 0.0, 1.0, 1.0]; 2],
};

/// The pictures a cart's label is cut from: a NES game on the Famicom cart takes a Famicom
/// cart's label (kept apart from its NES one), else the NES one's if it has none yet (shown
/// whole, as it is tall).
fn photo_specs(cart: &Cart) -> &'static [&'static CartPhoto] {
    match cart.platform {
        Platform::Nes if slot_ui::nes_famicom_of(cart) && cart.label.is_none() => &[&FAMICOM_PHOTO, &NES_PHOTO],
        Platform::Nes if slot_ui::nes_famicom_of(cart) => &[&FAMICOM_PHOTO],
        Platform::Nes => &[&NES_PHOTO],
        // The cart's own shape first, the other's label (trimmed to fit) when it has none.
        Platform::Snes if slot_ui::snes_shell_of(cart) == Some(slot_ui::SnesShell::Pal) => &[&SNES_PAL_PHOTO, &SNES_PHOTO],
        Platform::Snes => &[&SNES_PHOTO, &SNES_PAL_PHOTO],
        Platform::Ps1 => &[&PS1_DISC_PHOTO],
        _ => &[],
    }
}

const LISTING_MAX: usize = 8 << 20;
const IMAGE_MAX: usize = 8 << 20;
/// The index changes rarely; a week-old copy saves a megabyte per run.
const LISTING_TTL: Duration = Duration::from_secs(7 * 24 * 3600);
const CACHE_DIR: &str = "System/Cache/Thumbs";
const LABEL_INDEX: &str = "slot-art-labels.txt";
/// ROM checksums, so a slow card is read once per ROM rather than once per run.
const CRC_CACHE: &str = "System/Cache/crc32.txt";
/// No-Intro's games (platform, CRC32, serial, name), shipped on the card: built by
/// brick/make-cart-art-games.py, so the multi-megabyte DATs are never downloaded here.
const GAMES: &str = "System/cart-art-games.txt";
const REPORT: &str = "Logs/cart-art.txt";

/// A label written to (or, with None, deleted from) the card, for the running library to pick up.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fetched {
    pub platform: Platform,
    pub stem: String,
    pub label: Option<PathBuf>,
}

#[derive(Debug)]
pub enum Progress {
    Step { done: usize, total: usize, name: String },
    Got(Fetched),
    Done(Result<Summary, String>),
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Summary {
    pub found: usize,
    pub missing: usize,
    pub failed: usize,
}

pub struct ArtJob {
    rx: mpsc::Receiver<Progress>,
    cancel: Arc<AtomicBool>,
}

impl ArtJob {
    pub fn start(root: PathBuf) -> Self {
        Self::start_at(root, Servers { labels: SLOT_ART.into(), photos: PHOTOS.into() })
    }

    /// Tests point the servers at a local one.
    fn start_at(root: PathBuf, servers: Servers) -> Self {
        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let token = cancel.clone();
        thread::spawn(move || {
            let done = run(&root, &servers, &token, &tx);
            let _ = tx.send(Progress::Done(done));
        });
        ArtJob { rx, cancel }
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    pub fn try_recv(&self) -> Result<Progress, mpsc::TryRecvError> {
        self.rx.try_recv()
    }
}

impl Drop for ArtJob {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// The label scans' origin and the cart pictures'.
struct Servers {
    labels: String,
    photos: String,
}

/// Everything fetched or computed once per run.
struct Sources<'a> {
    root: &'a Path,
    server: &'a str,
    photo_server: &'a str,
    labels: Option<HashMap<u32, String>>,
    crcs: Crcs,
    games: Games,
    /// Each picture list (by its file), read when its first cart needs it.
    photos: HashMap<&'static str, PhotoIndex>,
}

fn run(
    root: &Path,
    servers: &Servers,
    cancel: &AtomicBool,
    tx: &mpsc::Sender<Progress>,
) -> Result<Summary, String> {
    let carts: Vec<Cart> = scan(root)
        .map_err(|e| format!("Cannot read games: {e}"))?
        .into_iter()
        .filter(needs_label)
        .collect();
    let mut summary = Summary::default();
    let mut report = String::new();
    let mut src = Sources {
        root,
        server: &servers.labels,
        photo_server: &servers.photos,
        labels: None,
        crcs: Crcs::load(root),
        games: Games::load(root),
        photos: HashMap::new(),
    };
    let result = (|| {
        for (at, cart) in carts.iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                return Err("Cancelled".to_string());
            }
            let _ = tx.send(Progress::Step {
                done: at,
                total: carts.len(),
                name: cart.stem.clone(),
            });
            // No label scan (homebrew, most hacks): the shelf's file-name label stays.
            let outcome = match photo_specs(cart) {
                [] => fetch_label(cart, &mut src)?,
                specs => {
                    let mut outcome = Outcome::NoMatch;
                    for spec in specs {
                        outcome = fetch_photo_label(cart, spec, &mut src)?;
                        if matches!(outcome, Outcome::Saved(..)) {
                            break;
                        }
                    }
                    outcome
                }
            };
            let place = format!("{}/{}", cart.platform.dir_name(), cart.stem);
            match outcome {
                Outcome::Saved(from, label) => {
                    summary.found += 1;
                    report.push_str(&format!("found     {place} <- {from}\n"));
                    let _ = tx.send(Progress::Got(Fetched {
                        platform: cart.platform,
                        stem: cart.stem.clone(),
                        label: Some(label),
                    }));
                }
                Outcome::NoMatch => {
                    summary.missing += 1;
                    report.push_str(&format!("no match  {place}\n"));
                }
                Outcome::Unusable(why) => {
                    summary.missing += 1;
                    report.push_str(&format!("no label  {place}: {why}\n"));
                }
                Outcome::Failed(why) => {
                    summary.failed += 1;
                    report.push_str(&format!("failed    {place}: {why}\n"));
                }
            }
        }
        Ok(())
    })();
    // Checksums read before a cancel or a network failure are kept for next time.
    src.crcs.save(root);
    result?;
    eprintln!(
        "slot: cart art: {} found, {} no match, {} failed",
        summary.found, summary.missing, summary.failed
    );
    if let Err(e) = std::fs::write(root.join(REPORT), report) {
        eprintln!("slot: cart art: {REPORT}: {e}");
    }
    Ok(summary)
}

enum Outcome {
    /// Where the art came from, for the report, and the label written.
    Saved(String, PathBuf),
    NoMatch,
    /// The game was found, but none of its pictures gives a label: why, for the report.
    Unusable(String),
    Failed(String),
}

/// Platforms whose game lists may hold a cart: GB and GBC games are often
/// filed under the other one (Tetris DX is a GBC game many keep with GB games).
fn platforms_for(platform: Platform) -> &'static [Platform] {
    match platform {
        Platform::Gba => &[Platform::Gba],
        Platform::Gb => &[Platform::Gb, Platform::Gbc],
        Platform::Gbc => &[Platform::Gbc, Platform::Gb],
        Platform::Nes => &[Platform::Nes],
        Platform::Snes => &[Platform::Snes],
        Platform::Ps1 => &[Platform::Ps1],
    }
}

/// A real label scan. Found by the ROM's CRC32, or failing that (a trimmed or patched
/// ROM, a hack) by the GBA game code in its header, then by its file name.
fn fetch_label(cart: &Cart, src: &mut Sources) -> Result<Outcome, String> {
    let crc = match src.crcs.of(src.root, &cart.rom) {
        Ok(crc) => crc,
        Err(e) => return Ok(Outcome::Failed(format!("cannot read ROM: {e}"))),
    };
    if src.labels.is_none() {
        src.labels = Some(label_index(src.root, src.server)?);
    }
    let mut cart = cart.clone();
    cart.read_header();
    let labels = src.labels.as_ref().expect("just loaded");
    let Some((path, how)) = src
        .games
        .candidates(&cart, crc)
        .into_iter()
        .find_map(|(crc, how)| Some((labels.get(&crc)?.clone(), how)))
    else {
        return Ok(Outcome::NoMatch);
    };
    let path = &path;
    let png = match get(src.server, &format!("/{path}"), IMAGE_MAX) {
        Ok(Some(png)) if is_png(&png) => png,
        Ok(Some(_)) => return Ok(Outcome::Failed(format!("{path}: not a PNG"))),
        Ok(None) => return Ok(Outcome::NoMatch),
        Err(e) => return Ok(Outcome::Failed(e)),
    };
    Ok(save_label(src.root, &cart, "", &png, format!("label scan {path} ({how})")))
}

/// No label for the cart it is on now: a NES game on the Famicom cart wants a Famicom label
/// even when it has a NES one.
fn needs_label(cart: &Cart) -> bool {
    match cart.platform {
        Platform::Nes if slot_ui::nes_famicom_of(cart) => cart.famicom_label.is_none(),
        _ => cart.label.is_none(),
    }
}

/// Cart pictures by name, "Title (Region)" as the name matching scores them.
#[derive(Default)]
struct PhotoIndex {
    names: Vec<String>,
    files: HashMap<String, Vec<String>>,
}

impl PhotoIndex {
    fn load(root: &Path, spec: &CartPhoto) -> Self {
        let text = std::fs::read_to_string(root.join(spec.index)).unwrap_or_default();
        let mut index = PhotoIndex::default();
        for (name, file) in text.lines().filter(|l| !l.starts_with('#')).filter_map(|l| l.split_once('\t')) {
            if !index.files.contains_key(name) {
                index.names.push(name.to_string());
            }
            index.files.entry(name.to_string()).or_default().push(file.to_string());
        }
        index
    }
}

/// A file name on the picture host, nothing that could leave its root.
fn photo_file_ok(file: &str) -> bool {
    file.len() <= 64
        && file.ends_with(".png")
        && !file.starts_with('.')
        && file.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
}

/// A label cut from a picture of the cart, found by the ROM's name: the same title only, as
/// the lists hold many hacks whose names begin with a real game's. A ROM that says it is a
/// hack only matches a hack's picture, as for the other platforms. Err stops the run: the
/// picture host cannot be reached, so every other cart would wait on it too.
fn fetch_photo_label(cart: &Cart, spec: &CartPhoto, src: &mut Sources) -> Result<Outcome, String> {
    let root = src.root;
    let index = src.photos.entry(spec.index).or_insert_with(|| PhotoIndex::load(root, spec));
    let Some(name) = same_title_match(&cart.stem, &index.names) else {
        return Ok(Outcome::NoMatch);
    };
    let mut why = Vec::new();
    for file in index.files[name].iter().filter(|f| photo_file_ok(f)).take(PHOTO_TRIES) {
        // One picture failing (too large, refused, gone) leaves the game's others to try.
        let png = match get(src.photo_server, &format!("/{file}"), IMAGE_MAX) {
            Ok(Some(png)) => png,
            Ok(None) => {
                why.push(format!("{file}: gone"));
                continue;
            }
            Err(e) if crate::net::unreachable(&e) => return Err(format!("Can't reach the cart picture server: {e}")),
            Err(e) => {
                why.push(format!("{file}: {e}"));
                continue;
            }
        };
        let made = match spec.index == PS1_DISC_PHOTO.index {
            true => disc_from_photo(&png),
            false => label_from_photo(&png, spec),
        };
        match made {
            Ok(label) => return Ok(save_label(root, cart, spec.sub, &label, format!("{name} cart {file}"))),
            Err(e) => why.push(format!("{file}: {e}")),
        }
    }
    Ok(Outcome::Unusable(format!("{name}: {}", why.join("; "))))
}

/// Cuts the label out of a front picture of a cart, as a PNG. Only the standard cart (the
/// spec's shape, plain plastic where it should be) is used; others (Tengen, Famicom, PAL,
/// a box) put the label elsewhere and are refused.
/// How picky a cut is. The bulk run writes labels nobody has looked at, so it takes only a
/// clean one off a standard cart. A search is flipped through by eye, so it offers anything a
/// label-shaped piece can be cut from and leaves the judging to whoever is picking.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Cut {
    Clean,
    Loose,
}

/// A disc picture, cut to the disc: what is not see-through around it, which must be round
/// (as wide as it is tall) and large enough to print on a disc on the shelf.
fn disc_from_photo(png: &[u8]) -> Result<Vec<u8>, &'static str> {
    let (rgba, w, h) = decode_rgba(png).ok_or("not a readable PNG")?;
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    for y in 0..h {
        for x in 0..w {
            if rgba[((y * w + x) * 4 + 3) as usize] > 128 {
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1));
            }
        }
    }
    if x1 <= x0 || y1 <= y0 {
        return Err("an empty picture");
    }
    let (dw, dh) = (x1 - x0, y1 - y0);
    if dw < 120 || !(PS1_DISC_PHOTO.aspect.0..=PS1_DISC_PHOTO.aspect.1).contains(&(dh as f32 / dw as f32)) {
        return Err("not a disc");
    }
    cut_out(png, Frame { x: x0 as i32, y: y0 as i32, w: dw, h: dh })
}

fn label_from_photo(png: &[u8], spec: &CartPhoto) -> Result<Vec<u8>, &'static str> {
    label_cut(png, spec, Cut::Clean)
}

/// The label on a cart photo, cut out: where the bulk run gets one.
fn label_cut(png: &[u8], spec: &CartPhoto, cut: Cut) -> Result<Vec<u8>, &'static str> {
    cut_out(png, label_frame(png, spec, cut)?)
}

/// Where the label sits on a cart photo, and whether this picture has one worth taking.
fn label_frame(png: &[u8], spec: &CartPhoto, cut: Cut) -> Result<Frame, &'static str> {
    let (rgba, w, h) = decode_rgba(png).ok_or("not a readable PNG")?;
    let alpha = |x: u32, y: u32| rgba[((y * w + x) * 4 + 3) as usize];
    // The cart's outline: a cut-out picture is transparent around it.
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    for y in 0..h {
        for x in 0..w {
            if alpha(x, y) > 128 {
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1));
            }
        }
    }
    if x1 <= x0 || y1 <= y0 {
        return Err("an empty picture");
    }
    let (cw, ch) = ((x1 - x0) as f32, (y1 - y0) as f32);
    // Too small to cut anything legible from, whichever cut this is.
    if cw < 120.0 {
        return Err("not the cart's shape");
    }
    if cut == Cut::Clean && !(spec.aspect.0..=spec.aspect.1).contains(&(ch / cw)) {
        return Err("not the cart's shape");
    }
    let at = |fx: f32, fy: f32| {
        let x = (x0 + (cw * fx) as u32).min(x1 - 1);
        let y = (y0 + (ch * fy) as u32).min(y1 - 1);
        let i = ((y * w + x) * 4) as usize;
        [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
    };
    let grid = |[fx0, fy0, fx1, fy1]: [f32; 4], n: usize| {
        (0..n * n).map(move |i| {
            (fx0 + (fx1 - fx0) * (i % n) as f32 / (n - 1) as f32, fy0 + (fy1 - fy0) * (i / n) as f32 / (n - 1) as f32)
        })
    };
    // The plastic beside the label.
    let mut side: Vec<[u8; 4]> = grid(spec.side, 4).map(|(fx, fy)| at(fx, fy)).collect();
    side.sort_by_key(|p| p[0] as u32 + p[1] as u32 + p[2] as u32);
    let shell = side[side.len() / 2];
    let share = |region: [f32; 4]| {
        let same = grid(region, 12)
            .map(|(fx, fy)| at(fx, fy))
            .filter(|p| p[3] >= 128 && (0..3).all(|c| p[c].abs_diff(shell[c]) <= 40))
            .count();
        same as f32 / 144.0
    };
    if cut == Cut::Clean {
        if spec.plain.iter().any(|r| share(*r) < 0.7) {
            return Err("not the standard cart");
        }
        if share(spec.art) > 0.5 {
            return Err("no label on the cart");
        }
    }
    // Rounded inward, so no sliver of plastic rides along the label's edge.
    let [fx0, fy0, fx1, fy1] = spec.label;
    let (lx, ly) = (x0 + (cw * fx0).ceil() as u32, y0 + (ch * fy0).ceil() as u32);
    let (lw, lh) = (x0 + (cw * fx1) as u32 - lx, y0 + (ch * fy1) as u32 - ly);
    Ok(Frame { x: lx as i32, y: ly as i32, w: lw, h: lh })
}

/// Where on a picture a label sits. Kept beside a search's pictures so the frame can be moved
/// by hand later: nothing is cut until someone says so.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Frame {
    /// Where the frame's top left corner sits on the picture. Can be off it: a picture shrunk
    /// smaller than the label leaves the label's edges empty, which is allowed.
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

/// The piece of `png` inside `frame`. Where the frame reaches past the picture, which it does
/// once the picture has been shrunk smaller than the label, what is out there is left clear,
/// so the cart's recess shows through at the edges.
fn cut_rgba(png: &[u8], frame: Frame) -> Result<(Vec<u8>, u32, u32), &'static str> {
    let (rgba, w, h) = decode_rgba(png).ok_or("not a readable PNG")?;
    let (lw, lh) = (frame.w, frame.h);
    if lw == 0 || lh == 0 {
        return Err("an empty picture");
    }
    let mut out = vec![0u8; (lw as usize) * (lh as usize) * 4];
    for row in 0..lh {
        let sy = frame.y + row as i32;
        if sy < 0 || sy >= h as i32 {
            continue;
        }
        // Where this row of the frame overlaps the picture, in both of their own columns.
        let from = frame.x.max(0);
        let to = (frame.x + lw as i32).min(w as i32);
        if to <= from {
            continue;
        }
        let run = (to - from) as usize * 4;
        let src = ((sy as usize) * w as usize + from as usize) * 4;
        let dst = (row as usize * lw as usize + (from - frame.x) as usize) * 4;
        out[dst..dst + run].copy_from_slice(&rgba[src..src + run]);
    }
    Ok((out, lw, lh))
}

/// The piece of `png` inside `frame`, as a label of its own shape.
pub fn cut_out(png: &[u8], frame: Frame) -> Result<Vec<u8>, &'static str> {
    let (out, lw, lh) = cut_rgba(png, frame)?;
    let (out, lw, lh) = shrink(out, lw, lh, LABEL_MAX);
    encode(&out, lw, lh)
}

/// The piece of `png` inside `frame`, in the shape `into` wants. A frame can be sized a side
/// at a time, so it need not be the shape of the slot it is bound for; the cart fills its slot
/// by cropping, so a label handed over in another shape would be cropped rather than stretched
/// and would not be what was framed. Stretching it here is what makes the two agree.
pub fn cut_to(png: &[u8], frame: Frame, into: (u32, u32)) -> Result<Vec<u8>, &'static str> {
    let (out, nw, nh) = cut_label(png, frame, into)?;
    encode(&out, nw, nh)
}

/// The label `cut_to` makes, as RGBA and its size, before it is made a PNG.
pub fn cut_label(png: &[u8], frame: Frame, into: (u32, u32)) -> Result<(Vec<u8>, u32, u32), &'static str> {
    let (out, lw, lh) = cut_rgba(png, frame)?;
    let (iw, ih) = (into.0.max(1), into.1.max(1));
    let long = lw.max(lh).clamp(1, LABEL_MAX);
    // Rounded, not floored: a whole number of pixels can only come so close to a given shape,
    // and rounding keeps it to within a pixel rather than always falling short.
    let other = |want: u32, of: u32| ((long as u64 * want as u64 + of as u64 / 2) / of as u64).max(1) as u32;
    let (nw, nh) = match iw >= ih {
        true => (long, other(ih, iw)),
        false => (other(iw, ih), long),
    };
    Ok((resample(&out, lw, lh, nw, nh), nw, nh))
}

fn encode(rgba: &[u8], w: u32, h: u32) -> Result<Vec<u8>, &'static str> {
    let mut bytes = Vec::new();
    let mut e = png::Encoder::new(&mut bytes, w, h);
    e.set_color(png::ColorType::Rgba);
    e.set_depth(png::BitDepth::Eight);
    e.write_header()
        .and_then(|mut w| w.write_image_data(rgba))
        .map_err(|_| "cannot encode the label")?;
    Ok(bytes)
}

/// To any size, each way on its own: the average of what each new pixel covers when there is
/// more than one, and the nearest when the picture is being stretched rather than squeezed.
fn resample(rgba: &[u8], w: u32, h: u32, nw: u32, nh: u32) -> Vec<u8> {
    if (w, h) == (nw, nh) {
        return rgba.to_vec();
    }
    let mut out = Vec::with_capacity((nw * nh * 4) as usize);
    for y in 0..nh {
        let (sy0, sy1) = (y * h / nh, ((y + 1) * h / nh).max(y * h / nh + 1));
        for x in 0..nw {
            let (sx0, sx1) = (x * w / nw, ((x + 1) * w / nw).max(x * w / nw + 1));
            let mut sum = [0u64; 4];
            for sy in sy0..sy1.min(h) {
                for sx in sx0..sx1.min(w) {
                    let i = ((sy * w + sx) * 4) as usize;
                    for c in 0..4 {
                        sum[c] += rgba[i + c] as u64;
                    }
                }
            }
            let n = ((sy1.min(h) - sy0) * (sx1.min(w) - sx0)).max(1) as u64;
            out.extend(sum.map(|v| (v / n) as u8));
        }
    }
    out
}

/// A picture's size, without decoding it: the width and height in its IHDR.
pub fn png_size(png: &[u8]) -> Option<(u32, u32)> {
    let ihdr = png.get(12..16)?;
    if ihdr != b"IHDR" {
        return None;
    }
    let n = |at: usize| -> Option<u32> { Some(u32::from_be_bytes(png.get(at..at + 4)?.try_into().ok()?)) };
    match (n(16)?, n(20)?) {
        (0, _) | (_, 0) => None,
        size => Some(size),
    }
}

/// `rgba` (w x h) scaled down so its longer side is at most `max`, each pixel the average of
/// the ones it covers; a smaller picture is kept as it is.
fn shrink(rgba: Vec<u8>, w: u32, h: u32, max: u32) -> (Vec<u8>, u32, u32) {
    if w.max(h) <= max {
        return (rgba, w, h);
    }
    let scale = |v: u32| ((v as u64 * max as u64) / w.max(h) as u64).max(1) as u32;
    let (nw, nh) = (scale(w), scale(h));
    let mut out = Vec::with_capacity((nw * nh * 4) as usize);
    for y in 0..nh {
        let (sy0, sy1) = (y * h / nh, ((y + 1) * h / nh).max(y * h / nh + 1));
        for x in 0..nw {
            let (sx0, sx1) = (x * w / nw, ((x + 1) * w / nw).max(x * w / nw + 1));
            let mut sum = [0u64; 4];
            for sy in sy0..sy1 {
                for sx in sx0..sx1 {
                    let i = ((sy * w + sx) * 4) as usize;
                    for c in 0..4 {
                        sum[c] += rgba[i + c] as u64;
                    }
                }
            }
            let n = ((sy1 - sy0) * (sx1 - sx0)) as u64;
            out.extend(sum.map(|v| (v / n) as u8));
        }
    }
    (out, nw, nh)
}

pub(crate) fn decode_rgba(bytes: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    let mut dec = png::Decoder::new(bytes);
    dec.set_transformations(png::Transformations::normalize_to_color8());
    dec.set_limits(png::Limits { bytes: 64 << 20 });
    let mut reader = dec.read_info().ok()?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    let n = (info.width as usize).checked_mul(info.height as usize)?;
    let src = &buf[..info.buffer_size()];
    let rgba = match info.color_type {
        png::ColorType::Rgba => src.to_vec(),
        png::ColorType::Rgb => src.chunks_exact(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        png::ColorType::Grayscale => src.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::GrayscaleAlpha => src.chunks_exact(2).flat_map(|p| [p[0], p[0], p[0], p[1]]).collect(),
        png::ColorType::Indexed => return None,
    };
    (rgba.len() == n * 4).then_some((rgba, info.width, info.height))
}

struct Game {
    platform: Platform,
    crc: u32,
    serial: String,
    name: String,
}

/// The shipped No-Intro list, to turn a game code or a name into CRCs the label server knows.
#[derive(Default)]
struct Games {
    list: Vec<Game>,
    names: HashMap<Platform, Vec<String>>,
}

impl Games {
    fn load(root: &Path) -> Self {
        let text = std::fs::read_to_string(root.join(GAMES)).unwrap_or_default();
        let list: Vec<Game> = text
            .lines()
            .filter(|l| !l.starts_with('#'))
            .filter_map(|l| {
                let mut f = l.splitn(4, '\t');
                let platform = match f.next()? {
                    "GBA" => Platform::Gba,
                    "GB" => Platform::Gb,
                    "GBC" => Platform::Gbc,
                    _ => return None,
                };
                let crc = u32::from_str_radix(f.next()?, 16).ok()?;
                let serial = f.next()?.to_string();
                Some(Game { platform, crc, serial, name: f.next()?.to_string() })
            })
            .collect();
        let mut names: HashMap<Platform, Vec<String>> = HashMap::new();
        for g in &list {
            names.entry(g.platform).or_default().push(slot_store::libretro_name(&g.name));
        }
        Games { list, names }
    }

    /// CRCs worth looking up for `cart` (its header read), best first, each with how it was found.
    fn candidates(&self, cart: &Cart, crc: u32) -> Vec<(u32, String)> {
        let mut out = vec![(crc, "checksum".to_string())];
        let platforms = platforms_for(cart.platform);
        let code = cart.code.trim();
        // A trimmed or patched ROM keeps its game code, but so does a hack: the code only
        // counts when the file name names that game too, so a hack never wears its base
        // game's label and stays told apart by its own (file name) label.
        let named = |g: &Game| g.serial == code || g.serial.get(..3) == code.get(..3);
        let group: Vec<String> = self
            .list
            .iter()
            .filter(|g| g.platform == Platform::Gba && named(g))
            .map(|g| slot_store::libretro_name(&g.name))
            .collect();
        let code_ok = cart.platform == Platform::Gba
            && code.len() == 4
            && code.bytes().all(|b| b.is_ascii_alphanumeric())
            && best_match(&cart.stem, &group).is_some();
        if code_ok {
            // The exact release first, then the same game from other regions (BZ4E -> BZ4P).
            for same_region in [true, false] {
                for g in self.list.iter().filter(|g| g.platform == Platform::Gba) {
                    let hit = if same_region { g.serial == code } else { g.serial.get(..3) == code.get(..3) };
                    if hit {
                        out.push((g.crc, format!("game code {code}")));
                    }
                }
            }
        }
        for &platform in platforms {
            let Some(names) = self.names.get(&platform) else { continue };
            let Some(name) = best_match(&cart.stem, names) else { continue };
            let key = slot_store::title_key(name);
            let mut same: Vec<&Game> = self
                .list
                .iter()
                .filter(|g| g.platform == platform && slot_store::title_key(&g.name) == key)
                .collect();
            // The matched release first, then its other regions and revisions.
            same.sort_by_key(|g| slot_store::libretro_name(&g.name) != name);
            out.extend(same.into_iter().map(|g| (g.crc, format!("name {name}"))));
        }
        let mut seen = std::collections::HashSet::new();
        out.retain(|(crc, _)| seen.insert(*crc));
        out
    }
}

fn save_label(root: &Path, cart: &Cart, sub: &str, png: &[u8], from: String) -> Outcome {
    let label = root
        .join("Labels")
        .join(cart.platform.dir_name())
        .join(sub)
        .join(format!("{}.png", cart.stem));
    if label.exists() {
        return Outcome::Failed("a label appeared meanwhile; kept it".into());
    }
    if let Some(dir) = label.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    match atomic_write(&label, png) {
        Ok(()) => Outcome::Saved(from, label),
        Err(e) => Outcome::Failed(format!("cannot write label: {e}")),
    }
}

/// CRC32 -> path of its label scan, from the art set's index.json (cached on the card).
fn label_index(root: &Path, server: &str) -> Result<HashMap<u32, String>, String> {
    let cache = root.join(CACHE_DIR).join(LABEL_INDEX);
    let text = match read_fresh(&cache) {
        Some(lines) => lines.join("\n"),
        None => {
            let json = get(server, "/index.json", LISTING_MAX)
                .map_err(|e| format!("Can't reach the label server: {e}"))?
                .ok_or("The label server has no index")?;
            let pairs = parse_label_index(&String::from_utf8_lossy(&json));
            let text: Vec<String> = pairs.iter().map(|(crc, path)| format!("{crc:08X} {path}")).collect();
            if let Some(dir) = cache.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = atomic_write(&cache, text.join("\n").as_bytes());
            text.join("\n")
        }
    };
    Ok(text
        .lines()
        .filter_map(|l| {
            let (crc, path) = l.split_once(' ')?;
            Some((u32::from_str_radix(crc, 16).ok()?, label_path(path)?.to_string()))
        })
        .collect())
}

/// Every "support-texture" (label scan) path in index.json, keyed by the CRC in its name.
/// Only paths of the exact form `<GB|GBC|GBA>/support-texture/<CRC32>.png` are taken,
/// so the index can never send a request anywhere else.
fn parse_label_index(json: &str) -> Vec<(u32, String)> {
    let mut out = Vec::new();
    let mut rest = json;
    while let Some(at) = rest.find("\"support-texture\"") {
        rest = &rest[at + 17..];
        let Some(open) = rest.find('"') else { break };
        let after = &rest[open + 1..];
        let Some(close) = after.find('"') else { break };
        if let Some(path) = label_path(&after[..close]) {
            let crc = &path[path.len() - 12..path.len() - 4];
            if let Ok(crc) = u32::from_str_radix(crc, 16) {
                out.push((crc, path.to_string()));
            }
        }
        rest = &after[close..];
    }
    out
}

fn label_path(path: &str) -> Option<&str> {
    let (system, rest) = path.split_once('/')?;
    let file = rest.strip_prefix("support-texture/")?;
    let crc = file.strip_suffix(".png")?;
    let ok = ["GB", "GBC", "GBA"].contains(&system)
        && crc.len() == 8
        && crc.bytes().all(|b| b.is_ascii_hexdigit());
    ok.then_some(path)
}

/// ROM checksums by path, kept with each file's size and mtime.
struct Crcs {
    known: HashMap<String, (u64, u64, u32)>,
    changed: bool,
}

impl Crcs {
    fn load(root: &Path) -> Self {
        let text = std::fs::read_to_string(root.join(CRC_CACHE)).unwrap_or_default();
        let known = text
            .lines()
            .filter_map(|l| {
                let mut f = l.splitn(4, '\t');
                let crc = u32::from_str_radix(f.next()?, 16).ok()?;
                let size = f.next()?.parse().ok()?;
                let mtime = f.next()?.parse().ok()?;
                Some((f.next()?.to_string(), (size, mtime, crc)))
            })
            .collect();
        Crcs {
            known,
            changed: false,
        }
    }

    fn of(&mut self, root: &Path, rom: &Path) -> std::io::Result<u32> {
        let meta = std::fs::metadata(rom)?;
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs());
        let key = rom.strip_prefix(root).unwrap_or(rom).to_string_lossy().into_owned();
        if let Some(&(size, when, crc)) = self.known.get(&key) {
            if (size, when) == (meta.len(), mtime) {
                return Ok(crc);
            }
        }
        let crc = slot_store::archive::crc32(rom)?;
        self.known.insert(key, (meta.len(), mtime, crc));
        self.changed = true;
        Ok(crc)
    }

    fn save(&self, root: &Path) {
        if !self.changed {
            return;
        }
        let mut lines: Vec<String> = self
            .known
            .iter()
            .filter(|(k, _)| !k.contains(['\n', '\t']))
            .map(|(k, (size, mtime, crc))| format!("{crc:08X}\t{size}\t{mtime}\t{k}"))
            .collect();
        lines.sort();
        let path = root.join(CRC_CACHE);
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(e) = atomic_write(&path, lines.join("\n").as_bytes()) {
            eprintln!("slot: cart art: {CRC_CACHE}: {e}");
        }
    }
}

/// The widest or tallest picture read from the card, for labels and overlays alike.
pub(crate) const PICTURE_SIDE_MAX: u32 = 4096;

fn is_png(bytes: &[u8]) -> bool {
    let mut dec = png::Decoder::new(bytes);
    dec.set_limits(png::Limits { bytes: 64 << 20 });
    dec.read_info().is_ok_and(|r| {
        let info = r.info();
        info.width > 0 && info.height > 0 && info.width <= PICTURE_SIDE_MAX && info.height <= PICTURE_SIDE_MAX
    })
}

fn read_fresh(path: &Path) -> Option<Vec<String>> {
    let age = SystemTime::now()
        .duration_since(std::fs::metadata(path).ok()?.modified().ok()?)
        .unwrap_or_default();
    if age > LISTING_TTL {
        return None;
    }
    let text = std::fs::read_to_string(path).ok()?;
    let names: Vec<String> = text.lines().filter(|l| !l.is_empty()).map(String::from).collect();
    (!names.is_empty()).then_some(names)
}

/// GET over HTTP/1.0 (so no chunked bodies) from an origin. Ok(None) is a 404.
fn get(server: &str, path: &str, limit: usize) -> Result<Option<Vec<u8>>, String> {
    let mut path = path.to_string();
    for _ in 0..3 {
        match request(server, &path, limit)? {
            Reply::Body(body) => return Ok(Some(body)),
            Reply::NotFound => return Ok(None),
            Reply::Moved(to) => path = to,
        }
    }
    Err("too many redirects".into())
}

enum Reply {
    Body(Vec<u8>),
    NotFound,
    Moved(String),
}

fn request(origin: &str, path: &str, limit: usize) -> Result<Reply, String> {
    let (tls, server, _) = crate::net::split_url(origin).ok_or_else(|| format!("bad server {origin}"))?;
    let reply = crate::net::send(&crate::net::Request {
        tls,
        server: &server,
        method: "GET",
        path,
        form: None,
        limit,
    })?;
    reply_of(&reply, origin)
}

/// A redirect is only followed on the same origin, so never from HTTPS down to HTTP.
fn reply_of(reply: &crate::net::Response, origin: &str) -> Result<Reply, String> {
    match reply.status {
        200 => Ok(Reply::Body(reply.body.clone())),
        404 => Ok(Reply::NotFound),
        301 | 302 | 303 | 307 | 308 => {
            let to = reply.header("location").ok_or("redirect without a location")?;
            match to.strip_prefix(origin) {
                Some(rest) if rest.starts_with('/') => Ok(Reply::Moved(rest.to_string())),
                _ if to.starts_with('/') => Ok(Reply::Moved(to)),
                _ => Err(format!("server moved to {to}, which is not supported")),
            }
        }
        status => Err(format!("server said {status}")),
    }
}

/// Where the labels a search finds wait, as pictures, until one is kept.
pub const PICK_DIR: &str = "System/Cache/LabelPick";
/// Enough to choose from: more are mostly the same picture again.
/// How far past the page being looked at a search runs: one page more, and no further. The
/// list is as long as the console's game list, so the next page being ready the moment it is
/// turned to is worth fetching ahead for; the one after it is not, until it is asked for.
const LOOKAHEAD: usize = slot_ui::PAGE;
/// How often a search that is far enough ahead looks again at how far the picker has flipped.
const WAIT: Duration = Duration::from_millis(100);

/// A label found for the cart menu to offer: where it waits, and where it came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    /// The label as cut, which is what the grid shows.
    pub path: PathBuf,
    pub caption: String,
    /// The whole picture it was cut from, kept so the frame can be moved by hand.
    pub source: PathBuf,
    /// Where on `source` the cut was taken.
    pub frame: Frame,
}

#[derive(Debug, PartialEq, Eq)]
pub enum PickNews {
    Found(Choice),
    /// The search is over: Err when it could not be made at all (no network, no index).
    Done(Result<(), String>),
}

/// Every label the sources have for one cart, fetched one after another in the background,
/// so the cart menu can offer them to choose from. Dropping it stops the search.
pub struct LabelSearch {
    rx: mpsc::Receiver<PickNews>,
    cancel: Arc<AtomicBool>,
    /// Which label is being looked at: the search keeps LOOKAHEAD ahead of it and no further.
    looked_at: Arc<AtomicUsize>,
}

impl LabelSearch {
    /// Labels for `cart`, found by what was typed among every game of its console.
    pub fn start(root: PathBuf, cart: Cart, query: String) -> Self {
        // A sim scenario never goes to the internet: it is handed a made-up label instead.
        #[cfg(feature = "sim")]
        if std::env::var_os("SLOT_SIM_SCRIPT").is_some() {
            return Self::stand_in(root, cart);
        }
        Self::start_at(root, cart, query, Servers { labels: SLOT_ART.into(), photos: PHOTOS.into() })
    }

    /// One label, drawn here: a striped picture with a disc on it, framed in its middle.
    #[cfg(feature = "sim")]
    fn stand_in(root: PathBuf, cart: Cart) -> Self {
        let (tx, rx) = mpsc::channel();
        let offered = (|| {
            let (pw, ph) = (480u32, 360u32);
            let mut rgba = Vec::with_capacity((pw * ph * 4) as usize);
            for y in 0..ph {
                for x in 0..pw {
                    let (dx, dy) = (x as f32 - 240.0, y as f32 - 180.0);
                    let px = match () {
                        _ if dx * dx + dy * dy < 90.0 * 90.0 => [30, 30, 46],
                        _ if (x + y) / 40 % 2 == 0 => [226, 72, 58],
                        _ => [246, 190, 64],
                    };
                    rgba.extend_from_slice(&[px[0], px[1], px[2], 255]);
                }
            }
            let whole = encode(&rgba, pw, ph)?;
            let (lw, lh) = slot_ui::label_size(&cart);
            let w = 360u32;
            let h = (w * lh / lw.max(1)).clamp(1, ph - 20);
            let frame = Frame { x: ((pw - w) / 2) as i32, y: ((ph - h) / 2) as i32, w, h };
            let cut = cut_to(&whole, frame, (lw, lh))?;
            let dir = root.join(PICK_DIR).join("sim");
            std::fs::create_dir_all(&dir).map_err(|_| "cannot write to the card")?;
            let (path, source) = (dir.join("0.png"), dir.join("0-whole.png"));
            atomic_write(&path, &cut).map_err(|_| "cannot write to the card")?;
            atomic_write(&source, &whole).map_err(|_| "cannot write to the card")?;
            Ok::<_, &str>(Choice { path, caption: "Sim Label (Stand-in)".into(), source, frame })
        })();
        let done = offered.map(|choice| {
            let _ = tx.send(PickNews::Found(choice));
        });
        let _ = tx.send(PickNews::Done(done.map_err(str::to_string)));
        LabelSearch { rx, cancel: Arc::new(AtomicBool::new(false)), looked_at: Arc::new(AtomicUsize::new(0)) }
    }

    fn start_at(root: PathBuf, cart: Cart, query: String, servers: Servers) -> Self {
        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let token = cancel.clone();
        let looked_at = Arc::new(AtomicUsize::new(0));
        let place = looked_at.clone();
        let spawned = thread::Builder::new().name("slot-label-search".into()).spawn(move || {
            let done = search(&root, &cart, &query, &servers, &token, &place, &tx);
            let _ = tx.send(PickNews::Done(done));
        });
        if let Err(e) = spawned {
            eprintln!("slot: label search: {e}");
        }
        LabelSearch { rx, cancel, looked_at }
    }

    pub fn poll(&self) -> Vec<PickNews> {
        self.rx.try_iter().collect()
    }

    /// The label the picker has flipped to, which is how far ahead the search may run.
    pub fn looking_at(&self, at: usize) {
        self.looked_at.fetch_max(at, Ordering::Relaxed);
    }
}

impl Drop for LabelSearch {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

/// "USA, Europe" from "Pokemon - Emerald Version (USA, Europe)": the first tag in brackets.
fn region_of(name: &str) -> &str {
    name.split_once('(').and_then(|(_, r)| r.split_once(')')).map_or("", |(r, _)| r.trim())
}

fn caption(source: &str, name: &str) -> String {
    match region_of(name) {
        "" => source.to_string(),
        region => format!("{source} · {region}"),
    }
}

/// The words of a name, lower case, for a typed search to match: "Yoshi's Island" gives
/// yoshi, s, island.
fn words(name: &str) -> Vec<String> {
    name.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(str::to_lowercase).collect()
}

/// Every word typed begins a word of the name (its region tags left out): "yoshi island"
/// matches "Super Mario World 2: Yoshi's Island (USA)".
/// How many of the typed words begin no word of the name's title. 0 is a full match.
fn missed(query: &[String], name: &str) -> usize {
    let title = name.split(['(', '[']).next().unwrap_or(name);
    let have = words(title);
    query.iter().filter(|q| !have.iter().any(|w| w.starts_with(q.as_str()))).count()
}

/// The order a typed search's names come in: those that are just what was typed first (so a
/// full name finds its own game before the ones it begins), then the shortest.
fn query_order(query: &[String], name: &str) -> (usize, bool, usize) {
    let title = name.split(['(', '[']).next().unwrap_or(name);
    (missed(query, name), words(title) != query, name.len())
}

fn search(root: &Path, cart: &Cart, query: &str, servers: &Servers, cancel: &AtomicBool, looked_at: &AtomicUsize, tx: &mpsc::Sender<PickNews>) -> Result<(), String> {
    let typed = words(query);
    // Each search in a folder of its own, the last one's cleared away: a search stopped
    // mid-download can then never put its picture among the next one's.
    let _ = std::fs::remove_dir_all(root.join(PICK_DIR));
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let dir = root.join(PICK_DIR).join(stamp.to_string());
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot write to the card: {e}"))?;
    let mut found = 0;
    // A label goes to the card (the face builder reads it from there) and then to the menu.
    // The cut goes to the card for the grid to show, and the picture it came from beside it,
    // so the frame can still be moved after picking. The face builder reads both from there.
    let mut offer = |png: &[u8], whole: &[u8], frame: Frame, caption: String| -> bool {
        let path = dir.join(format!("{found}.png"));
        let source = dir.join(format!("{found}-whole.png"));
        if atomic_write(&path, png).is_err() || atomic_write(&source, whole).is_err() {
            return true;
        }
        found += 1;
        if tx.send(PickNews::Found(Choice { path, caption, source, frame })).is_err() {
            return false;
        }
        // Nothing is left out of the list, so wait here rather than fetch a picture that is a
        // hundred flips away. Flipping on raises `looked_at` and the search carries on.
        while found >= looked_at.load(Ordering::Relaxed) + LOOKAHEAD {
            if cancel.load(Ordering::Relaxed) {
                return false;
            }
            thread::sleep(WAIT);
        }
        true
    };
    // Everything for the cart's shape: on the Famicom cart, NES pictures too (shown whole).
    let specs: &[&CartPhoto] = match cart.platform {
        Platform::Nes if slot_ui::nes_famicom_of(cart) => &[&FAMICOM_PHOTO, &NES_PHOTO],
        _ => photo_specs(cart),
    };
    if specs.is_empty() {
        let games = Games::load(root);
        let platforms = platforms_for(cart.platform);
        // Every game of the console, the closest to what was typed first: nothing is left out.
        let mut hits: Vec<&Game> = games.list.iter().filter(|g| platforms.contains(&g.platform)).collect();
        hits.sort_by_key(|g| query_order(&typed, &g.name));
        let crcs: Vec<u32> = hits.into_iter().map(|g| g.crc).collect();
        let labels = label_index(root, &servers.labels)?;
        let mut seen = std::collections::HashSet::new();
        for crc in crcs {
            if cancel.load(Ordering::Relaxed) {
                return Ok(());
            }
            let Some(path) = labels.get(&crc).filter(|p| seen.insert(p.to_string())) else { continue };
            let name = games.list.iter().find(|g| g.crc == crc).map_or("", |g| g.name.as_str());
            // A search can land on other games: their names say which.
            let source = name.split(" (").next().unwrap_or(name);
            match get(&servers.labels, &format!("/{path}"), IMAGE_MAX) {
                Ok(Some(png)) if is_png(&png) => {
                    let Some((w, h)) = png_size(&png) else { continue };
                    let whole = Frame { x: 0, y: 0, w, h };
                    if !offer(&png, &png, whole, caption(source, name)) {
                        return Ok(());
                    }
                }
                Err(e) if crate::net::unreachable(&e) => return Err(format!("Can't reach the label server: {e}")),
                _ => {}
            }
        }
        return Ok(());
    }
    for spec in specs {
        let index = PhotoIndex::load(root, spec);
        let mut names: Vec<&String> = index.names.iter().collect();
        names.sort_by_key(|n| query_order(&typed, n));
        let mut seen = std::collections::HashSet::new();
        for name in names {
            for file in index.files[name].iter().filter(|f| photo_file_ok(f) && seen.insert(f.to_string())) {
                if cancel.load(Ordering::Relaxed) {
                    return Ok(());
                }
                let png = match get(&servers.photos, &format!("/{file}"), IMAGE_MAX) {
                    Ok(Some(png)) => png,
                    Err(e) if crate::net::unreachable(&e) => return Err(format!("Can't reach the cart picture server: {e}")),
                    _ => continue,
                };
                // Only a picture a clean label can be cut from is offered.
                if let Ok(frame) = label_frame(&png, spec, Cut::Loose) {
                    let Ok(label) = cut_out(&png, frame) else { continue };
                    let from = name.split(" (").next().unwrap_or(name);
                    if !offer(&label, &png, frame, caption(from, name)) {
                        return Ok(());
                    }
                }
            }
        }
    }
    Ok(())
}

/// A label as a picture to show in the grid: its pixels, no bigger than `max` either way.
pub fn label_picture(png: &[u8], max: u32) -> Option<(Vec<u8>, u32, u32)> {
    let (rgba, w, h) = decode_rgba(png)?;
    let (rgba, w, h) = if w.max(h) > max { shrink(rgba, w, h, max) } else { (rgba, w, h) };
    (w > 0 && h > 0).then_some((rgba, w, h))
}

/// The labels a search found and nobody kept, gone from the card. In the background: the
/// card is slow, and nothing waits on it.
pub fn forget_choices(root: &Path) {
    // Only the searches there now: one started a moment later keeps its folder.
    let Ok(entries) = std::fs::read_dir(root.join(PICK_DIR)) else {
        return;
    };
    let old: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    let spawned = thread::Builder::new().name("slot-label-pick".into()).spawn(move || {
        for dir in old {
            if let Err(e) = std::fs::remove_dir_all(&dir).or_else(|_| std::fs::remove_file(&dir)) {
                eprintln!("slot: label search: {}: {e}", dir.display());
            }
        }
    });
    if let Err(e) = spawned {
        eprintln!("slot: label search: {e}");
    }
}

/// The label kept from a search: written where the cart's label for its current shape goes,
/// in place of any it had.
/// Writes the piece of `source` inside `frame` as the cart's label: the cut happens here, once
/// whoever picked it has said where the frame goes.
pub fn keep_framed(root: &Path, cart: &Cart, source: &Path, frame: Frame) -> std::io::Result<PathBuf> {
    let png = std::fs::read(source)?;
    let cut = cut_label(&png, frame, slot_ui::label_size(cart))
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    keep_cut(root, cart, &cut)
}

/// Keeps a label already cut (`cut_label`) as the cart's own.
pub fn keep_cut(root: &Path, cart: &Cart, (rgba, w, h): &(Vec<u8>, u32, u32)) -> std::io::Result<PathBuf> {
    let cut = encode(rgba, *w, *h).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let target = label_target(root, cart);
    if let Some(dir) = target.parent() {
        std::fs::create_dir_all(dir)?;
    }
    atomic_write(&target, &cut)?;
    Ok(target)
}

pub fn keep_choice(root: &Path, cart: &Cart, choice: &Path) -> std::io::Result<PathBuf> {
    let target = label_target(root, cart);
    if let Some(dir) = target.parent() {
        std::fs::create_dir_all(dir)?;
    }
    atomic_write(&target, &std::fs::read(choice)?)?;
    Ok(target)
}

/// Where a cart's label goes: the one it already has, or a new file named after the game.
fn label_target(root: &Path, cart: &Cart) -> PathBuf {
    let famicom = cart.platform == Platform::Nes && slot_ui::nes_famicom_of(cart);
    let existing = if famicom { &cart.famicom_label } else { &cart.label };
    let dir = root.join("Labels").join(cart.platform.dir_name()).join(if famicom { slot_store::FAMICOM_LABELS } else { "" });
    // A label already there under another case keeps its name, so no second one appears.
    existing
        .as_ref()
        .filter(|p| !p.starts_with(root.join(PICK_DIR)))
        .cloned()
        .unwrap_or_else(|| dir.join(format!("{}.png", cart.stem)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    /// One-shot-per-connection HTTP server answering from a fixed table.
    fn serve(routes: Vec<(String, u16, Vec<u8>)>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        thread::spawn(move || {
            for conn in listener.incoming() {
                let Ok(mut conn) = conn else { return };
                let mut raw = Vec::new();
                let mut buf = [0u8; 1024];
                while !raw.windows(4).any(|w| w == b"\r\n\r\n") {
                    match conn.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => raw.extend_from_slice(&buf[..n]),
                    }
                }
                let req = String::from_utf8_lossy(&raw).to_string();
                let path = req.split_whitespace().nth(1).unwrap_or("").to_string();
                let (status, body) = routes
                    .iter()
                    .find(|(p, _, _)| *p == path)
                    .map(|(_, s, b)| (*s, b.clone()))
                    .unwrap_or((404, Vec::new()));
                let _ = write!(conn, "HTTP/1.1 {status} X\r\nContent-Length: {}\r\n\r\n", body.len());
                let _ = conn.write_all(&body);
            }
        });
        format!("http://{addr}")
    }

    fn png_bytes() -> Vec<u8> {
        let mut out = Vec::new();
        let mut e = png::Encoder::new(&mut out, 2, 2);
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        e.write_header().unwrap().write_image_data(&[200; 16]).unwrap();
        out
    }

    fn rom(root: &Path, rel: &str) {
        let path = root.join("Games").join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, vec![0u8; 0x200]).unwrap();
    }

    fn drain(job: &ArtJob) -> (Vec<Fetched>, Result<Summary, String>) {
        let mut got = Vec::new();
        loop {
            match job.rx.recv_timeout(Duration::from_secs(20)).expect("job finished") {
                Progress::Got(f) => got.push(f),
                Progress::Done(r) => return (got, r),
                Progress::Step { .. } => {}
            }
        }
    }

    #[test]
    fn an_unreachable_server_stops_with_a_message() {
        let d = tempfile::tempdir().unwrap();
        let err = label_index(d.path(), "http://127.0.0.1:1").unwrap_err();
        assert!(err.starts_with("Can't reach the label server"), "{err}");
    }

    #[test]
    fn redirects_on_the_same_host_are_followed_and_others_refused() {
        let reply = |raw: &[u8]| reply_of(&crate::net::parse(raw, 100).unwrap(), "https://h");
        let moved = b"HTTP/1.1 301 Moved\r\nLocation: https://h/new/\r\n\r\n";
        assert!(matches!(reply(moved), Ok(Reply::Moved(p)) if p == "/new/"));
        let down = b"HTTP/1.1 301 Moved\r\nLocation: http://h/new/\r\n\r\n";
        assert!(reply(down).is_err());
        let away = b"HTTP/1.1 301 Moved\r\nLocation: https://hx/new/\r\n\r\n";
        assert!(reply(away).is_err());
    }

    /// The real label server: `SLOT_LIVE_ART=1 ... live_label_server -- --ignored`, and
    /// SLOT_ART_PREVIEW=<dir> to render one GBA and one GB cart with a real label.
    #[test]
    #[ignore = "needs internet"]
    fn live_label_server() {
        if std::env::var_os("SLOT_LIVE_ART").is_none() {
            return;
        }
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        let index = label_index(root, SLOT_ART).unwrap();
        assert!(index.len() > 5000, "{} label scans", index.len());
        for system in ["GBA", "GB"] {
            let (crc, path) = index.iter().filter(|(_, p)| p.starts_with(&format!("{system}/"))).min().unwrap();
            let png = get(SLOT_ART, &format!("/{path}"), IMAGE_MAX).unwrap().unwrap();
            assert!(is_png(&png), "{path}");
            if std::env::var_os("SLOT_ART_PREVIEW").is_none() {
                continue;
            }
            rom(root, &format!("{system}/{crc:08X}.{}", system.to_lowercase()));
            std::fs::create_dir_all(root.join("Labels").join(system)).unwrap();
            std::fs::write(root.join(format!("Labels/{system}/{crc:08X}.png")), &png).unwrap();
        }
        if let Some(out) = std::env::var_os("SLOT_ART_PREVIEW") {
            for mut cart in scan(root).unwrap().into_iter().filter(|c| c.label.is_some()) {
                cart.read_header();
                let face = slot_ui::cart_face_with(&cart, crate::label_cache::label_art(root, &cart));
                let path = Path::new(&out).join(format!("label-{}.png", cart.platform.dir_name()));
                let mut e = png::Encoder::new(std::fs::File::create(path).unwrap(), face.w, face.h);
                e.set_color(png::ColorType::Rgba);
                e.set_depth(png::BitDepth::Eight);
                e.write_header().unwrap().write_image_data(&face.rgba).unwrap();
            }
        }
    }

    fn games(root: &Path, lines: &[&str]) -> Games {
        std::fs::create_dir_all(root.join("System")).unwrap();
        std::fs::write(root.join(GAMES), format!("# test\n{}\n", lines.join("\n"))).unwrap();
        Games::load(root)
    }

    fn gba_rom(root: &Path, name: &str, code: &str) -> Cart {
        let mut rom = vec![0u8; 0x200];
        rom[0xa0..0xa0 + 12].copy_from_slice(b"FINAL FANTAS");
        rom[0xac..0xb0].copy_from_slice(code.as_bytes());
        let path = root.join("Games/GBA").join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, rom).unwrap();
        let mut cart = scan(root).unwrap().into_iter().find(|c| c.rom == path).unwrap();
        cart.read_header();
        cart
    }

    #[test]
    fn a_trimmed_gba_rom_is_found_by_its_game_code_but_a_hack_is_not() {
        let d = tempfile::tempdir().unwrap();
        let g = games(d.path(), &[
            "GBA\t27F2BC67\tBZ4P\tFinal Fantasy IV Advance (Europe) (En,Fr,De,Es,It)",
            "GBA\tFEEEBDE4\tBZ4E\tFinal Fantasy IV Advance (USA)",
            "GBA\tDD88761C\tBPRE\tPokemon - FireRed Version (USA, Europe)",
        ]);
        let ff = gba_rom(d.path(), "Final Fantasy IV Advance (trimmed).gba", "BZ4E");
        let crcs: Vec<u32> = g.candidates(&ff, 0x1234).into_iter().map(|(c, _)| c).collect();
        assert_eq!(crcs, vec![0x1234, 0xFEEE_BDE4, 0x27F2_BC67], "own, same region, other regions");
        let hack = gba_rom(d.path(), "Pokemon Radical Red.gba", "BPRE");
        assert_eq!(g.candidates(&hack, 0x5678), vec![(0x5678, "checksum".to_string())], "a hack keeps its own label");
        let vague = gba_rom(d.path(), "ff4.gba", "BZ4E");
        assert_eq!(g.candidates(&vague, 9).len(), 1, "a code alone is not enough");
    }

    #[test]
    fn a_game_boy_rom_falls_back_to_its_name_and_other_releases() {
        let d = tempfile::tempdir().unwrap();
        let g = games(d.path(), &[
            "GB\t9F7FDD53\t-\tPokemon - Red Version (USA, Europe) (SGB Enhanced)",
            "GB\tAAAAAAAA\t-\tPokemon - Red Version (Germany) (SGB Enhanced)",
            "GBC\t46DF91AD\t-\tTetris DX (World) (SGB Enhanced) (GB Compatible)",
        ]);
        rom(d.path(), "GB/Pokemon Red (G).gb");
        rom(d.path(), "GB/Tetris DX.gb");
        let carts = scan(d.path()).unwrap();
        let red = carts.iter().find(|c| c.stem.starts_with("Pokemon")).unwrap();
        let crcs: Vec<u32> = g.candidates(red, 1).into_iter().map(|(c, _)| c).collect();
        assert_eq!(crcs, vec![1, 0xAAAA_AAAA, 0x9F7F_DD53], "the matched release, then its siblings");
        let tetris = carts.iter().find(|c| c.stem == "Tetris DX").unwrap();
        assert!(g.candidates(tetris, 1).iter().any(|(c, _)| *c == 0x46DF_91AD), "GBC list searched for a GB shelf");
    }

    #[test]
    fn a_label_is_found_for_a_rom_whose_checksum_is_unknown() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        games(root, &["GBA\tFEEEBDE4\tBZ4E\tFinal Fantasy IV Advance (USA)"]);
        gba_rom(root, "Final Fantasy IV Advance (trimmed).gba", "BZ4E");
        std::fs::create_dir_all(root.join("Logs")).unwrap();
        let path = "GBA/support-texture/FEEEBDE4.png";
        let server = serve(vec![
            ("/index.json".into(), 200, format!(r#"{{"FEEEBDE4": {{"support-texture": "{path}"}}}}"#).into_bytes()),
            (format!("/{path}"), 200, png_bytes()),
        ]);
        let (_, done) = drain(&ArtJob::start_at(root.to_path_buf(), servers(&server)));
        assert_eq!(done, Ok(Summary { found: 1, missing: 0, failed: 0 }));
        let report = std::fs::read_to_string(root.join(REPORT)).unwrap();
        assert!(report.contains("(game code BZ4E)"), "{report}");
    }

    fn search_all(search: &LabelSearch) -> (Vec<Choice>, Result<(), String>) {
        let mut found = Vec::new();
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        while std::time::Instant::now() < deadline {
            for news in search.poll() {
                match news {
                    PickNews::Found(c) => found.push(c),
                    PickNews::Done(done) => return (found, done),
                }
            }
            thread::sleep(Duration::from_millis(5));
        }
        panic!("the search never ended");
    }

    #[test]
    fn a_label_search_offers_every_region_of_the_game_and_keeps_the_one_picked() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        games(root, &[
            "GBA\t27F2BC67\tBZ4P\tFinal Fantasy IV Advance (Europe) (En,Fr,De,Es,It)",
            "GBA\tFEEEBDE4\tBZ4E\tFinal Fantasy IV Advance (USA)",
        ]);
        let cart = gba_rom(root, "Final Fantasy IV Advance (USA).gba", "BZ4E");
        let (us, eu) = ("GBA/support-texture/FEEEBDE4.png", "GBA/support-texture/27F2BC67.png");
        let server = serve(vec![
            (
                "/index.json".into(),
                200,
                format!(r#"{{"FEEEBDE4": {{"support-texture": "{us}"}}, "27F2BC67": {{"support-texture": "{eu}"}}}}"#).into_bytes(),
            ),
            (format!("/{us}"), 200, png_bytes()),
            (format!("/{eu}"), 200, png_bytes()),
        ]);
        let (found, done) = search_all(&LabelSearch::start_at(root.to_path_buf(), cart.clone(), "Final Fantasy IV Advance".into(), servers(&server)));
        assert_eq!(done, Ok(()));
        let captions: Vec<&str> = found.iter().map(|c| c.caption.as_str()).collect();
        assert_eq!(captions, ["Final Fantasy IV Advance · USA", "Final Fantasy IV Advance · Europe"]);
        assert!(found.iter().all(|c| c.path.starts_with(root.join(PICK_DIR))));
        let kept = keep_choice(root, &cart, &found[1].path).unwrap();
        assert_eq!(kept, root.join("Labels/GBA/Final Fantasy IV Advance (USA).png"));
        assert_eq!(std::fs::read(kept).unwrap(), png_bytes());
        forget_choices(root);
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let left = || std::fs::read_dir(root.join(PICK_DIR)).map_or(0, |d| d.count());
        while left() > 0 && std::time::Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(left(), 0, "the labels not kept stayed on the card");
        assert!(root.join("Labels/GBA/Final Fantasy IV Advance (USA).png").exists(), "the kept one went too");
    }

    #[test]
    fn a_label_search_offers_odd_carts_too_but_the_bulk_run_still_refuses_them() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        let rom = root.join("Games/NES/Mega Man (USA).nes");
        std::fs::create_dir_all(rom.parent().unwrap()).unwrap();
        std::fs::write(&rom, b"NES\x1a").unwrap();
        std::fs::create_dir_all(root.join("System")).unwrap();
        std::fs::write(
            root.join(NES_PHOTO.index),
            "# test\nMega Man (USA)\tus.png\nMega Man (Europe)\teu.png\nMega Man (Europe)\ttengen.png\nMega Man 2 (USA)\ttwo.png\n",
        )
        .unwrap();
        let photos = serve(vec![
            ("/us.png".into(), 200, cart_png(280, 312, GREY, RED, NES_PHOTO.label)),
            ("/eu.png".into(), 200, cart_png(280, 312, GREY, [0x20, 0x20, 0xc0], NES_PHOTO.label)),
            ("/tengen.png".into(), 200, cart_png(280, 318, [20, 20, 20], RED, [0.15, 0.3, 0.85, 0.9])),
            ("/two.png".into(), 200, cart_png(280, 312, GREY, RED, NES_PHOTO.label)),
        ]);
        let cart = scan(root).unwrap().into_iter().next().unwrap();
        let servers = Servers { labels: "http://127.0.0.1:1".into(), photos };
        let tengen = cart_png(280, 318, [20, 20, 20], RED, [0.15, 0.3, 0.85, 0.9]);
        let (found, done) = search_all(&LabelSearch::start_at(root.to_path_buf(), cart, "Mega Man".into(), servers));
        assert_eq!(done, Ok(()));
        let captions: Vec<&str> = found.iter().map(|c| c.caption.as_str()).collect();
        assert_eq!(
            captions,
            ["Mega Man · USA", "Mega Man · Europe", "Mega Man · Europe", "Mega Man 2 · USA"],
            "Tengen's odd cart was left out of a search"
        );
        // The bulk run writes labels unseen, so it still takes none off a cart like that.
        assert!(label_cut(&tengen, &NES_PHOTO, Cut::Loose).is_ok());
        assert_eq!(label_from_photo(&tengen, &NES_PHOTO), Err("not the standard cart"));
    }

    #[test]
    fn what_is_typed_orders_the_names_rather_than_leaving_any_out() {
        let q = |s: &str| words(s);
        assert_eq!(missed(&q("yoshi island"), "Super Mario World 2: Yoshi's Island (USA)"), 0);
        assert_eq!(missed(&q("Super Mario World 2 - Yoshi's Island"), "Super Mario World 2: Yoshi's Island"), 0);
        assert_eq!(missed(&q("FINAL fant ii"), "Final Fantasy II (USA)"), 0);
        assert_eq!(missed(&q("usa"), "Final Fantasy II (USA)"), 1, "a region tag matched");
        assert_eq!(missed(&q("ff"), "Final Fantasy II (USA)"), 1);
        let mut names = vec!["Final Fantasy III (Japan)", "Final Fantasy II (USA) (Rev 1)", "Final Fantasy II (USA)"];
        names.sort_by_key(|n| query_order(&q("final fantasy ii"), n));
        assert_eq!(names, ["Final Fantasy II (USA)", "Final Fantasy II (USA) (Rev 1)", "Final Fantasy III (Japan)"], "a full name found another game first");
        // A name with none of the words typed is still offered, last.
        let mut names = vec!["Zelda (USA)", "Final Fantasy II (USA)"];
        names.sort_by_key(|n| query_order(&q("final fantasy"), n));
        assert_eq!(names, ["Final Fantasy II (USA)", "Zelda (USA)"]);
        // Nothing typed leaves every name in, the shortest first.
        let mut names = vec!["Final Fantasy II (USA)", "Zelda (USA)"];
        names.sort_by_key(|n| query_order(&q("  "), n));
        assert_eq!(names, ["Zelda (USA)", "Final Fantasy II (USA)"]);
    }

    /// Collects what a search has offered so far, giving it a moment to stop on its own.
    fn search_so_far(search: &LabelSearch) -> (Vec<Choice>, bool) {
        let mut found = Vec::new();
        let mut done = false;
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while std::time::Instant::now() < deadline {
            for news in search.poll() {
                match news {
                    PickNews::Found(c) => found.push(c),
                    PickNews::Done(_) => done = true,
                }
            }
            thread::sleep(Duration::from_millis(5));
        }
        (found, done)
    }

    /// A picture in four coloured quarters, so where a cut landed can be read off its colour.
    fn quartered(w: u32, h: u32) -> Vec<u8> {
        let corner = |x: u32, y: u32| match (x < w / 2, y < h / 2) {
            (true, true) => [0xff, 0, 0, 0xff],
            (false, true) => [0, 0xff, 0, 0xff],
            (true, false) => [0, 0, 0xff, 0xff],
            (false, false) => [0xff, 0xff, 0, 0xff],
        };
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                rgba.extend_from_slice(&corner(x, y));
            }
        }
        let mut out = Vec::new();
        let mut e = png::Encoder::new(&mut out, w, h);
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        e.write_header().unwrap().write_image_data(&rgba).unwrap();
        out
    }

    #[test]
    fn a_frame_cuts_the_piece_it_covers_and_nothing_else() {
        let png = quartered(200, 100);
        assert_eq!(png_size(&png), Some((200, 100)));
        // The top right quarter is the green one, whole.
        let cut = cut_out(&png, Frame { x: 100, y: 0, w: 100, h: 50 }).unwrap();
        let (rgba, w, h) = decode_rgba(&cut).unwrap();
        assert_eq!((w, h), (100, 50));
        assert!(rgba.chunks_exact(4).all(|p| p == [0, 0xff, 0, 0xff]), "the cut took in another quarter");
        // And the bottom left is the blue one.
        let cut = cut_out(&png, Frame { x: 0, y: 50, w: 100, h: 50 }).unwrap();
        let (rgba, _, _) = decode_rgba(&cut).unwrap();
        assert!(rgba.chunks_exact(4).all(|p| p == [0, 0, 0xff, 0xff]));
    }

    #[test]
    fn a_frame_reaching_past_the_picture_leaves_what_is_out_there_clear() {
        let png = quartered(200, 100);
        // Past the right and bottom edges: the frame keeps its own size, and what hangs over
        // the edge is clear rather than clipped away.
        let cut = cut_out(&png, Frame { x: 150, y: 60, w: 400, h: 400 }).unwrap();
        let (over, w, h) = decode_rgba(&cut).unwrap();
        assert_eq!((w, h), (400, 400));
        let px = |rgba: &[u8], w: u32, x: u32, y: u32| -> [u8; 4] {
            let i = ((y * w + x) * 4) as usize;
            [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
        };
        assert_eq!(px(&over, w, 10, 10), [0xff, 0xff, 0, 0xff], "the overlap was not cut");
        assert_eq!(px(&over, w, 399, 399), [0, 0, 0, 0], "past the picture was not left clear");
        // Wholly beyond the picture: a label's worth of nothing, rather than an error.
        assert!(cut_out(&png, Frame { x: 900, y: 900, w: 10, h: 10 }).is_ok());
        // A frame larger than the picture keeps it whole, with the rest left clear.
        let cut = cut_out(&png, Frame { x: -100, y: -50, w: 400, h: 200 }).unwrap();
        let (rgba, w, h) = decode_rgba(&cut).unwrap();
        assert_eq!((w, h), (400, 200));
        assert_eq!(px(&rgba, w, 0, 0), [0, 0, 0, 0], "outside the picture was not left clear");
        assert_eq!(px(&rgba, w, 399, 199), [0, 0, 0, 0]);
        // The picture itself sits at (100, 50) of the frame: its top left is the red quarter.
        assert_eq!(px(&rgba, w, 101, 51), [0xff, 0, 0, 0xff]);
        assert_eq!(cut_out(&png, Frame { x: 0, y: 0, w: 0, h: 10 }), Err("an empty picture"));
        assert_eq!(png_size(b"<html>"), None);
    }

    #[test]
    fn a_label_is_cut_in_the_shape_its_slot_wants() {
        let png = quartered(200, 100);
        // A frame of one shape, a slot of another: the label comes out in the slot's shape, so
        // the cart stretches it the way it was framed instead of cropping it to fit.
        let frame = Frame { x: 0, y: 0, w: 200, h: 100 };
        // A whole number of pixels only comes so close to a given shape, so near enough is the
        // contract: within a pixel of the slot's own.
        let shape_of = |w: u32, h: u32| w as f32 / h as f32;
        let near = |got: f32, want: f32| (got / want - 1.0).abs() < 0.02;
        let (_, w, h) = decode_rgba(&cut_to(&png, frame, (216, 99)).unwrap()).unwrap();
        assert!(near(shape_of(w, h), shape_of(216, 99)), "{w} x {h} is not the slot's shape");
        // A tall slot gets a tall label from the very same frame.
        let (_, w, h) = decode_rgba(&cut_to(&png, frame, (99, 216)).unwrap()).unwrap();
        assert!(near(shape_of(w, h), shape_of(99, 216)), "{w} x {h} is not the slot's shape");
        // What it holds is still the picture: the top left stays the red quarter.
        let cut = cut_to(&png, frame, (216, 99)).unwrap();
        let (rgba, w, _) = decode_rgba(&cut).unwrap();
        let at = |x: u32, y: u32| rgba[(((y * w + x) * 4) as usize)..][..4].to_vec();
        assert_eq!(at(2, 2), vec![0xff, 0, 0, 0xff]);
        // And no label is written larger than a label needs to be.
        let big = Frame { x: 0, y: 0, w: 4000, h: 2000 };
        let (_, w, h) = decode_rgba(&cut_to(&png, big, (216, 99)).unwrap()).unwrap();
        assert!(w.max(h) <= LABEL_MAX, "{w} x {h}");
    }

    #[test]
    fn a_search_stays_a_few_labels_ahead_until_the_picker_flips_on() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        let rom = root.join("Games/SNES/Yoshi (hack).sfc");
        std::fs::create_dir_all(rom.parent().unwrap()).unwrap();
        std::fs::write(&rom, [0u8; 0x200]).unwrap();
        std::fs::create_dir_all(root.join("System")).unwrap();
        let many = LOOKAHEAD + 4;
        let mut index = String::from("# test\n");
        let mut routes = Vec::new();
        for i in 0..many {
            index.push_str(&format!("Game {i} (USA)\t{i}.png\n"));
            routes.push((format!("/{i}.png"), 200, cart_png(640, 410, SNES_GREY, RED, SNES_PHOTO.label)));
        }
        std::fs::write(root.join(SNES_PHOTO.index), index).unwrap();
        let servers = Servers { labels: "http://127.0.0.1:1".into(), photos: serve(routes) };
        let cart = scan(root).unwrap().into_iter().next().unwrap();
        let search = LabelSearch::start_at(root.to_path_buf(), cart, "game".into(), servers);

        let (found, done) = search_so_far(&search);
        assert_eq!(found.len(), LOOKAHEAD, "the search ran past the label being looked at");
        assert!(!done, "the search ended instead of waiting");

        // Flipping on lets the rest through.
        search.looking_at(many);
        let (rest, done) = search_all(&search);
        assert_eq!(done, Ok(()));
        assert_eq!(found.len() + rest.len(), many);
    }

    #[test]
    fn a_typed_search_finds_a_game_named_differently_and_says_which() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        let rom = root.join("Games/SNES/Yoshi (hack).sfc");
        std::fs::create_dir_all(rom.parent().unwrap()).unwrap();
        std::fs::write(&rom, [0u8; 0x200]).unwrap();
        std::fs::create_dir_all(root.join("System")).unwrap();
        std::fs::write(
            root.join(SNES_PHOTO.index),
            "# test\nSuper Mario World 2: Yoshi's Island (USA)\tisland.png\nYoshi's Safari (USA)\tsafari.png\nSuper Mario World (USA)\tworld.png\n",
        )
        .unwrap();
        let photos = serve(vec![
            ("/island.png".into(), 200, cart_png(640, 410, SNES_GREY, RED, SNES_PHOTO.label)),
            ("/safari.png".into(), 200, cart_png(640, 410, SNES_GREY, RED, SNES_PHOTO.label)),
            ("/world.png".into(), 200, cart_png(640, 410, SNES_GREY, RED, SNES_PHOTO.label)),
        ]);
        let cart = scan(root).unwrap().into_iter().next().unwrap();
        let servers = Servers { labels: "http://127.0.0.1:1".into(), photos };
        let (found, done) = search_all(&LabelSearch::start_at(root.to_path_buf(), cart, "yoshi island".into(), servers));
        assert_eq!(done, Ok(()));
        let captions: Vec<&str> = found.iter().map(|c| c.caption.as_str()).collect();
        // Nothing is left out: what was typed only decides the order.
        assert_eq!(
            captions,
            ["Super Mario World 2: Yoshi's Island · USA", "Yoshi's Safari · USA", "Super Mario World · USA"]
        );
    }

    #[test]
    fn a_super_famicom_picture_gives_a_wide_label_for_the_rounder_cart() {
        const PAL_GREY: [u8; 3] = [0xbf, 0xc1, 0xc1];
        let label = label_from_photo(&cart_png(600, 420, PAL_GREY, RED, [0.07, 0.06, 0.93, 0.45]), &SNES_PAL_PHOTO)
            .expect("the Super Famicom cart");
        let (rgba, w, h) = decode_rgba(&label).unwrap();
        assert!(w as f32 / h as f32 > 2.3, "{w}x{h} is not the wide PAL label");
        assert!(rgba.chunks_exact(4).all(|p| p == [0xc0, 0x20, 0x20, 255]), "plastic inside the label");
        assert_eq!(label_from_photo(&cart_png(280, 312, GREY, RED, NES_PHOTO.label), &SNES_PAL_PHOTO), Err("not the cart's shape"));
        let blank = cart_png(600, 420, PAL_GREY, PAL_GREY, [0.07, 0.06, 0.93, 0.45]);
        assert_eq!(label_from_photo(&blank, &SNES_PAL_PHOTO), Err("no label on the cart"));
    }

    #[test]
    fn a_disc_picture_is_kept_whole_cut_to_the_disc() {
        // A disc 200 across on a clear 300 by 260 picture: cut to the disc, 200 square.
        let (w, h) = (300u32, 260u32);
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                let (dx, dy) = (x as f32 + 0.5 - 150.0, y as f32 + 0.5 - 130.0);
                if dx * dx + dy * dy < 100.0 * 100.0 {
                    rgba[((y * w + x) * 4) as usize..][..4].copy_from_slice(&[200, 30, 30, 255]);
                }
            }
        }
        let disc = disc_from_photo(&encode(&rgba, w, h).unwrap()).unwrap();
        let (_, dw, dh) = decode_rgba(&disc).unwrap();
        assert_eq!((dw, dh), (200, 200));
        // A wide box is no disc.
        let wide = vec![255u8; (300 * 150 * 4) as usize];
        assert_eq!(disc_from_photo(&encode(&wide, 300, 150).unwrap()), Err("not a disc"));
    }

    #[test]
    fn a_snes_cart_looks_for_its_own_shape_first() {
        let cart = |stem: &str| Cart {
            platform: Platform::Snes, stem: stem.into(), rom: "missing.sfc".into(), label: None,
            famicom_label: None, title: String::new(), code: String::new(), shell: None,
        };
        assert_eq!(photo_specs(&cart("Mario (Japan)"))[0].index, SNES_PAL_PHOTO.index);
        assert_eq!(photo_specs(&cart("Mario (USA)"))[0].index, SNES_PHOTO.index);
        assert_eq!(photo_specs(&cart("Mario (USA)")).len(), 2, "no fallback to the other shape");
    }

    #[test]
    fn a_label_search_without_the_server_says_so() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        games(root, &["GBA\tFEEEBDE4\tBZ4E\tFinal Fantasy IV Advance (USA)"]);
        let cart = gba_rom(root, "Final Fantasy IV Advance (USA).gba", "BZ4E");
        let (found, done) = search_all(&LabelSearch::start_at(root.to_path_buf(), cart, "Final Fantasy".into(), servers("http://127.0.0.1:1")));
        assert!(found.is_empty());
        assert!(done.is_err(), "{done:?}");
    }

    #[test]
    fn the_shipped_game_list_loads() {
        let shipped = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../brick/card/Slot");
        let g = Games::load(&shipped);
        assert!(g.list.len() > 7000, "{} games", g.list.len());
        assert!(g.list.iter().any(|x| x.serial == "BPRE" && x.name.starts_with("Pokemon - FireRed")));
    }

    /// ROMs whose checksums the server cannot know, against the real server and the shipped
    /// game list: `SLOT_LIVE_ART=1 ... live_label_fallbacks -- --ignored --nocapture`.
    #[test]
    #[ignore = "needs internet"]
    fn live_label_fallbacks() {
        if std::env::var_os("SLOT_LIVE_ART").is_none() {
            return;
        }
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        std::fs::create_dir_all(root.join("System")).unwrap();
        let shipped = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../brick/card/Slot").join(GAMES);
        std::fs::copy(shipped, root.join(GAMES)).unwrap();
        std::fs::create_dir_all(root.join("Logs")).unwrap();
        gba_rom(root, "Final Fantasy IV Advance (trimmed).gba", "BZ4E");
        gba_rom(root, "Pokemon Radical Red.gba", "BPRE");
        gba_rom(root, "Pokemon Glazed.gba", "BPEE");
        rom(root, "GB/Pokemon - Red Version (U) [!].gb");
        rom(root, "GBA/My Homebrew.gba");
        let (_, done) = drain(&ArtJob::start(root.to_path_buf()));
        let report = std::fs::read_to_string(root.join(REPORT)).unwrap_or_default();
        eprintln!("{report}");
        assert_eq!(done, Ok(Summary { found: 2, missing: 3, failed: 0 }));
        assert!(report.contains("no match  GBA/Pokemon Radical Red"), "a hack keeps its own label");
        assert!(report.contains("no match  GBA/Pokemon Glazed"), "a hack keeps its own label");
    }

    #[test]
    fn the_label_index_only_yields_label_scan_paths() {
        let json = r#"{"000206C0": {"look": {"ground": "3067a3"}, "support-texture": "GBA/support-texture/000206C0.png", "wheel": "GBA/wheel/000206C0.png"},
            "00202DFA": {"wheel": "GB/wheel/00202DFA.png"},
            "DEADBEEF": {"support-texture": "../../etc/passwd"},
            "0006612D": {"support-texture":"GB/support-texture/0006612D.png"}}"#;
        assert_eq!(
            parse_label_index(json),
            vec![
                (0x0002_06C0, "GBA/support-texture/000206C0.png".to_string()),
                (0x0006_612D, "GB/support-texture/0006612D.png".to_string()),
            ]
        );
    }

    fn servers(labels: &str) -> Servers {
        Servers { labels: labels.into(), photos: "http://127.0.0.1:1".into() }
    }

    /// A cut-out picture of a cart: `shell` plastic, transparent around it, with a label of
    /// `label` colour at `at` (fractions of the cart, as NES_PHOTO.label).
    fn cart_png(w: u32, h: u32, shell: [u8; 3], label: [u8; 3], at: [f32; 4]) -> Vec<u8> {
        let pad = 10;
        let (pw, ph) = (w + 2 * pad, h + 2 * pad);
        let mut px = Vec::with_capacity((pw * ph * 4) as usize);
        for y in 0..ph {
            for x in 0..pw {
                let (cx, cy) = (x as f32 - pad as f32, y as f32 - pad as f32);
                let inside = cx >= 0.0 && cy >= 0.0 && cx < w as f32 && cy < h as f32;
                let (fx, fy) = (cx / w as f32, cy / h as f32);
                let on_label = fx >= at[0] && fx < at[2] && fy >= at[1] && fy < at[3];
                let c = if on_label { label } else { shell };
                px.extend_from_slice(&if inside { [c[0], c[1], c[2], 255] } else { [0, 0, 0, 0] });
            }
        }
        let mut out = Vec::new();
        let mut e = png::Encoder::new(&mut out, pw, ph);
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        e.write_header().unwrap().write_image_data(&px).unwrap();
        out
    }

    const GREY: [u8; 3] = [0x8e, 0x8d, 0x8a];
    const RED: [u8; 3] = [0xc0, 0x20, 0x20];

    #[test]
    fn a_nes_label_is_cut_from_the_standard_cart_and_odd_ones_are_refused() {
        let label = label_from_photo(&cart_png(280, 312, GREY, RED, NES_PHOTO.label), &NES_PHOTO).expect("the standard cart");
        let (rgba, w, h) = decode_rgba(&label).unwrap();
        assert!((w as f32 / 280.0 - 0.46).abs() < 0.01 && (h as f32 / 312.0 - 0.674).abs() < 0.01, "{w}x{h}");
        assert!(rgba.chunks_exact(4).all(|p| p == [0xc0, 0x20, 0x20, 255]), "shell inside the label");
        // Tengen's black cart: the label lower down, over where Nintendo's has plain plastic.
        let tengen = cart_png(280, 318, [20, 20, 20], RED, [0.15, 0.3, 0.85, 0.9]);
        assert_eq!(label_from_photo(&tengen, &NES_PHOTO), Err("not the standard cart"));
        let famicom = cart_png(400, 250, GREY, RED, [0.1, 0.1, 0.9, 0.7]);
        assert_eq!(label_from_photo(&famicom, &NES_PHOTO), Err("not the cart's shape"));
        assert_eq!(label_from_photo(&cart_png(280, 312, GREY, GREY, NES_PHOTO.label), &NES_PHOTO), Err("no label on the cart"));
        assert_eq!(label_from_photo(b"<html>", &NES_PHOTO), Err("not a readable PNG"));
    }

    #[test]
    fn a_famicom_picture_gives_a_wide_label_and_a_nes_one_is_refused() {
        const YELLOW: [u8; 3] = [0xe8, 0xb0, 0x10];
        let label = label_from_photo(&cart_png(400, 262, YELLOW, RED, [0.06, 0.1, 0.94, 0.76]), &FAMICOM_PHOTO)
            .expect("the Famicom cart");
        let (rgba, w, h) = decode_rgba(&label).unwrap();
        assert!(w > h * 2 - 20, "{w}x{h} is not a Famicom label");
        assert!(rgba.chunks_exact(4).all(|p| p == [0xc0, 0x20, 0x20, 255]), "plastic inside the label");
        assert_eq!(label_from_photo(&cart_png(280, 312, GREY, RED, NES_PHOTO.label), &FAMICOM_PHOTO), Err("not the cart's shape"));
        assert_eq!(
            label_from_photo(&cart_png(400, 262, YELLOW, YELLOW, [0.06, 0.1, 0.94, 0.76]), &FAMICOM_PHOTO),
            Err("no label on the cart")
        );
    }

    #[test]
    fn a_big_label_is_scaled_down_by_averaging_and_a_small_one_kept() {
        let px = |c: u8| [c, c, c, 255];
        // 4x4: left half black, right half white.
        let rgba: Vec<u8> = (0..16).flat_map(|i| px(if i % 4 < 2 { 0 } else { 255 })).collect();
        let (out, w, h) = shrink(rgba.clone(), 4, 4, 2);
        assert_eq!((w, h), (2, 2));
        assert_eq!(out, [px(0), px(255), px(0), px(255)].concat());
        let (odd, w, h) = shrink(rgba.clone(), 4, 4, 3);
        assert_eq!((w, h, odd.len()), (3, 3, 36), "a size that does not divide evenly");
        assert_eq!(shrink(rgba.clone(), 4, 4, 4), (rgba, 4, 4));
        let tall = cart_png(1400, 1560, GREY, RED, NES_PHOTO.label);
        let (_, w, h) = decode_rgba(&label_from_photo(&tall, &NES_PHOTO).unwrap()).unwrap();
        assert_eq!(h, LABEL_MAX);
        assert!((w as f32 / h as f32 - 644.0 / 1051.0).abs() < 0.01, "{w}x{h} lost the label's shape");
    }

    #[test]
    fn an_unreachable_picture_server_stops_the_run_and_a_bad_picture_does_not() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        let path = root.join("Games/NES/Mega Man (USA).nes");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"NES\x1a").unwrap();
        std::fs::create_dir_all(root.join("System")).unwrap();
        std::fs::create_dir_all(root.join("Logs")).unwrap();
        std::fs::write(root.join(NES_PHOTO.index), "Mega Man (USA)\trefused.png\nMega Man (USA)\tgood.png\n").unwrap();
        let down = Servers { labels: "http://127.0.0.1:1".into(), photos: "http://127.0.0.1:1".into() };
        let (_, done) = drain(&ArtJob::start_at(root.to_path_buf(), down));
        assert!(done.unwrap_err().starts_with("Can't reach the cart picture server"));
        let nes = serve(vec![
            ("/refused.png".into(), 403, b"no".to_vec()),
            ("/good.png".into(), 200, cart_png(280, 312, GREY, RED, NES_PHOTO.label)),
        ]);
        let (_, done) = drain(&ArtJob::start_at(root.to_path_buf(), Servers { labels: "http://127.0.0.1:1".into(), photos: nes }));
        assert_eq!(done, Ok(Summary { found: 1, missing: 0, failed: 0 }), "the refused picture ended the game");
    }

    #[test]
    fn a_game_whose_pictures_are_all_odd_says_why_in_the_report() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        let path = root.join("Games/NES/Hard Drivin' (USA).nes");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"NES\x1a").unwrap();
        std::fs::create_dir_all(root.join("System")).unwrap();
        std::fs::create_dir_all(root.join("Logs")).unwrap();
        std::fs::write(root.join(NES_PHOTO.index), "Hard Drivin' (USA)\ttengen.png\n").unwrap();
        let nes = serve(vec![("/tengen.png".into(), 200, cart_png(280, 318, [20, 20, 20], RED, [0.15, 0.3, 0.85, 0.9]))]);
        let (_, done) = drain(&ArtJob::start_at(root.to_path_buf(), Servers { labels: "http://127.0.0.1:1".into(), photos: nes }));
        assert_eq!(done, Ok(Summary { found: 0, missing: 1, failed: 0 }));
        let report = std::fs::read_to_string(root.join(REPORT)).unwrap();
        assert!(report.contains("no label  NES/Hard Drivin' (USA): Hard Drivin' (USA): tengen.png: not the standard cart"), "{report}");
    }

    const SNES_GREY: [u8; 3] = [0x95, 0x94, 0x9a];

    #[test]
    fn a_snes_label_is_cut_from_the_us_cart_and_a_pal_one_is_refused() {
        let label = label_from_photo(&cart_png(640, 410, SNES_GREY, RED, SNES_PHOTO.label), &SNES_PHOTO).expect("the US cart");
        let (rgba, w, h) = decode_rgba(&label).unwrap();
        assert!((w as f32 / 640.0 - 0.607).abs() < 0.01 && (h as f32 / 410.0 - 0.424).abs() < 0.01, "{w}x{h}");
        assert!(rgba.chunks_exact(4).all(|p| p == [0xc0, 0x20, 0x20, 255]), "plastic inside the label");
        // PAL and Super Famicom carts are taller for their width, their label elsewhere.
        let pal = cart_png(640, 560, SNES_GREY, RED, [0.15, 0.1, 0.85, 0.6]);
        assert_eq!(label_from_photo(&pal, &SNES_PHOTO), Err("not the cart's shape"));
        // A NES picture is no SNES cart, nor the other way round.
        assert!(label_from_photo(&cart_png(280, 312, GREY, RED, NES_PHOTO.label), &SNES_PHOTO).is_err());
        assert!(label_from_photo(&cart_png(640, 410, SNES_GREY, RED, SNES_PHOTO.label), &NES_PHOTO).is_err());
    }

    #[test]
    fn a_snes_cart_gets_its_label_from_the_snes_list_only() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        for rom in ["Games/SNES/Super Mario Kart (USA).sfc", "Games/NES/Super Mario Kart (USA).nes"] {
            let path = root.join(rom);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, b"x").unwrap();
        }
        std::fs::create_dir_all(root.join("System")).unwrap();
        std::fs::create_dir_all(root.join("Logs")).unwrap();
        std::fs::write(root.join(SNES_PHOTO.index), "Super Mario Kart (USA)\tkart.png\n").unwrap();
        let photos = serve(vec![("/kart.png".into(), 200, cart_png(640, 410, SNES_GREY, RED, SNES_PHOTO.label))]);
        let (got, done) = drain(&ArtJob::start_at(root.to_path_buf(), Servers { labels: "http://127.0.0.1:1".into(), photos }));
        assert_eq!(done, Ok(Summary { found: 1, missing: 1, failed: 0 }), "the NES cart has no list, the SNES one is found");
        assert_eq!(got[0].platform, Platform::Snes);
        assert!(root.join("Labels/SNES/Super Mario Kart (USA).png").exists());
    }

    #[test]
    fn only_plain_file_names_are_asked_of_the_picture_host() {
        assert!(photo_file_ok("r2_9a3090ee-82d1-4664-8e2c-6bfda50d44f4.png"));
        for bad in ["../x.png", "a/b.png", "a b.png", "x.jpg", ".png", "x.png?y=1", &"a".repeat(70)] {
            assert!(!photo_file_ok(bad), "{bad}");
        }
    }

    #[test]
    fn a_nes_cart_gets_the_label_off_the_first_standard_picture_of_its_game() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        for rom in ["Games/NES/Mega Man (USA).nes", "Games/NES/My Homebrew.nes", "Games/NES/Mega Man (Hack).nes"] {
            let path = root.join(rom);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, b"NES\x1a").unwrap();
        }
        std::fs::create_dir_all(root.join("System")).unwrap();
        std::fs::create_dir_all(root.join("Logs")).unwrap();
        std::fs::write(
            root.join(NES_PHOTO.index),
            "# test\nMega Man (USA)\ttengen.png\nMega Man (USA)\tnintendo.png\nMega Man (Japan)\t../evil.png\n",
        )
        .unwrap();
        let tengen = cart_png(280, 318, [20, 20, 20], RED, [0.15, 0.3, 0.85, 0.9]);
        let nes = serve(vec![
            ("/tengen.png".into(), 200, tengen),
            ("/nintendo.png".into(), 200, cart_png(280, 312, GREY, RED, NES_PHOTO.label)),
        ]);
        let servers = Servers { labels: "http://127.0.0.1:1".into(), photos: nes };
        let (got, done) = drain(&ArtJob::start_at(root.to_path_buf(), servers));
        assert_eq!(done, Ok(Summary { found: 1, missing: 2, failed: 0 }), "the label server was never needed");
        assert_eq!(got[0].stem, "Mega Man (USA)");
        let (rgba, _, _) = decode_rgba(&std::fs::read(root.join("Labels/NES/Mega Man (USA).png")).unwrap()).unwrap();
        assert!(rgba.chunks_exact(4).all(|p| p == [0xc0, 0x20, 0x20, 255]), "not the label");
        let report = std::fs::read_to_string(root.join(REPORT)).unwrap();
        assert!(report.contains("no match  NES/Mega Man (Hack)"), "a hack wore the game's label: {report}");
        assert!(!root.join(CRC_CACHE).exists(), "a NES ROM was checksummed for nothing");
    }

    #[test]
    fn a_famicom_cart_gets_its_own_label_beside_the_nes_one() {
        const YELLOW: [u8; 3] = [0xe8, 0xb0, 0x10];
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        let rom = root.join("Games/NES/Rockman (Japan).nes");
        std::fs::create_dir_all(rom.parent().unwrap()).unwrap();
        std::fs::write(&rom, b"NES\x1a").unwrap();
        std::fs::create_dir_all(root.join("Labels/NES")).unwrap();
        std::fs::write(root.join("Labels/NES/Rockman (Japan).png"), b"its NES label").unwrap();
        std::fs::create_dir_all(root.join("System")).unwrap();
        std::fs::create_dir_all(root.join("Logs")).unwrap();
        std::fs::write(root.join(FAMICOM_PHOTO.index), "# test\nRockman (Japan)\tfc.png\n").unwrap();
        std::fs::write(root.join(NES_PHOTO.index), "# test\nRockman (Japan)\tnes.png\n").unwrap();
        let photos = serve(vec![("/fc.png".into(), 200, cart_png(400, 262, YELLOW, RED, [0.06, 0.1, 0.94, 0.76]))]);
        let servers = Servers { labels: "http://127.0.0.1:1".into(), photos };
        let (_, done) = drain(&ArtJob::start_at(root.to_path_buf(), servers));
        assert_eq!(done, Ok(Summary { found: 1, missing: 0, failed: 0 }));
        assert!(root.join("Labels/NES/Famicom/Rockman (Japan).png").exists(), "no Famicom label");
        assert_eq!(std::fs::read(root.join("Labels/NES/Rockman (Japan).png")).unwrap(), b"its NES label", "the NES label was touched");
        let cart = scan(root).unwrap().into_iter().next().unwrap();
        assert!(cart.label.is_some() && cart.famicom_label.is_some(), "{cart:?}");
        assert!(!needs_label(&cart), "a second run would fetch it again");
    }

    /// As live_nes_labels, for SNES carts: `SLOT_LIVE_ART=1 ... live_snes_labels -- --ignored`.
    #[test]
    #[ignore = "needs internet"]
    fn live_snes_labels() {
        if std::env::var_os("SLOT_LIVE_ART").is_none() {
            return;
        }
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        std::fs::create_dir_all(root.join("System")).unwrap();
        std::fs::create_dir_all(root.join("Logs")).unwrap();
        let shipped = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../brick/card/Slot").join(SNES_PHOTO.index);
        std::fs::copy(shipped, root.join(SNES_PHOTO.index)).unwrap();
        for g in [
            "Super Mario Kart (USA)",
            "Super Mario World (USA)",
            "Super Metroid (Japan, USA) (En,Ja)",
            "Donkey Kong Country (USA) (Rev 2)",
            "Chrono Trigger (USA)",
            "F-Zero (USA)",
            "Final Fantasy II (USA)",
            "Not A Real Game",
        ] {
            let path = root.join("Games/SNES").join(format!("{g}.sfc"));
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, b"x").unwrap();
        }
        let (_, done) = drain(&ArtJob::start(root.to_path_buf()));
        eprintln!("{}", std::fs::read_to_string(root.join(REPORT)).unwrap_or_default());
        let done = done.unwrap();
        assert!(done.found >= 2 && done.failed == 0, "{done:?}");
        if let Some(out) = std::env::var_os("SLOT_ART_PREVIEW") {
            for mut cart in scan(root).unwrap().into_iter().filter(|c| c.label.is_some()) {
                cart.read_header();
                let face = slot_ui::cart_face_with(&cart, crate::label_cache::label_art(root, &cart));
                let path = Path::new(&out).join(format!("snes-{}.png", cart.stem));
                let mut e = png::Encoder::new(std::fs::File::create(path).unwrap(), face.w, face.h);
                e.set_color(png::ColorType::Rgba);
                e.set_depth(png::BitDepth::Eight);
                e.write_header().unwrap().write_image_data(&face.rgba).unwrap();
            }
        }
    }

    /// The real picture host and the shipped index: `SLOT_LIVE_ART=1 ... live_nes_labels --
    /// --ignored --nocapture`, and SLOT_ART_PREVIEW=<dir> to render the carts that got one.
    #[test]
    #[ignore = "needs internet"]
    fn live_nes_labels() {
        if std::env::var_os("SLOT_LIVE_ART").is_none() {
            return;
        }
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        std::fs::create_dir_all(root.join("System")).unwrap();
        std::fs::create_dir_all(root.join("Logs")).unwrap();
        let shipped = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../brick/card/Slot").join(NES_PHOTO.index);
        std::fs::copy(shipped, root.join(NES_PHOTO.index)).unwrap();
        let games = [
            "Super Mario Bros. (World)",
            "Legend of Zelda, The (USA)",
            "Mega Man 2 (USA)",
            "Castlevania (USA)",
            "Metroid (USA)",
            "Kirby's Adventure (USA)",
            "Tetris (USA)",
            "Punch-Out!! (USA)",
            "Not A Real Game",
        ];
        for g in games {
            let path = root.join("Games/NES").join(format!("{g}.nes"));
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, b"NES\x1a").unwrap();
        }
        let (got, done) = drain(&ArtJob::start(root.to_path_buf()));
        eprintln!("{}", std::fs::read_to_string(root.join(REPORT)).unwrap_or_default());
        let done = done.unwrap();
        assert!(done.found >= 6 && done.failed == 0, "{done:?}");
        assert!(!got.iter().any(|f| f.stem == "Not A Real Game"));
        if let Some(out) = std::env::var_os("SLOT_ART_PREVIEW") {
            for mut cart in scan(root).unwrap().into_iter().filter(|c| c.label.is_some()) {
                cart.read_header();
                let face = slot_ui::cart_face_with(&cart, crate::label_cache::label_art(root, &cart));
                let path = Path::new(&out).join(format!("nes-{}.png", cart.stem));
                let mut e = png::Encoder::new(std::fs::File::create(path).unwrap(), face.w, face.h);
                e.set_color(png::ColorType::Rgba);
                e.set_depth(png::BitDepth::Eight);
                e.write_header().unwrap().write_image_data(&face.rgba).unwrap();
            }
        }
    }

    #[test]
    fn real_labels_are_found_by_checksum_whatever_the_rom_is_called() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        rom(root, "GBA/my weird name.gba");
        rom(root, "GB/homebrew.gb");
        std::fs::write(root.join("Games/GB/homebrew.gb"), b"something else").unwrap();
        std::fs::create_dir_all(root.join("Logs")).unwrap();
        let crc = 0xB2AA_7578u32; // 0x200 zero bytes, as rom() writes
        let path = format!("GBA/support-texture/{crc:08X}.png");
        let index = format!(r#"{{"{crc:08X}": {{"support-texture": "{path}"}}}}"#);
        let server = serve(vec![
            ("/index.json".into(), 200, index.into_bytes()),
            (format!("/{path}"), 200, png_bytes()),
        ]);
        let (got, done) = drain(&ArtJob::start_at(root.to_path_buf(), servers(&server)));
        assert_eq!(done, Ok(Summary { found: 1, missing: 1, failed: 0 }));
        assert_eq!(got[0].stem, "my weird name");
        assert!(is_png(&std::fs::read(root.join("Labels/GBA/my weird name.png")).unwrap()));
        let cached = std::fs::read_to_string(root.join(CRC_CACHE)).unwrap();
        assert!(cached.contains(&format!("{crc:08X}	512	")), "{cached}");
        assert!(cached.contains("Games/GBA/my weird name.gba"), "{cached}");
        // Second run: the index comes from the card and nothing is left to fetch.
        let (_, again) = drain(&ArtJob::start_at(root.to_path_buf(), servers(&server)));
        assert_eq!(again, Ok(Summary { found: 0, missing: 1, failed: 0 }));
    }

    #[test]
    fn a_cached_checksum_is_used_until_the_rom_changes() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        rom(root, "GBA/a.gba");
        let rom_path = root.join("Games/GBA/a.gba");
        let mut crcs = Crcs::load(root);
        let real = crcs.of(root, &rom_path).unwrap();
        crcs.save(root);
        let mut text = std::fs::read_to_string(root.join(CRC_CACHE)).unwrap();
        text = text.replacen(&format!("{real:08X}"), "12345678", 1);
        std::fs::write(root.join(CRC_CACHE), text).unwrap();
        assert_eq!(Crcs::load(root).of(root, &rom_path).unwrap(), 0x1234_5678, "trusted while size and mtime match");
        std::fs::write(&rom_path, b"patched").unwrap();
        assert_eq!(Crcs::load(root).of(root, &rom_path).unwrap(), 0x83FC_6D73, "the CRC32 of \"patched\"");
    }

    #[test]
    fn a_zipped_roms_checksum_is_its_roms() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../slot-store/tests/fixtures");
        let zip = root.join("Games/GB/Zipped.zip");
        std::fs::create_dir_all(zip.parent().unwrap()).unwrap();
        std::fs::copy(fixtures.join("rom.zip"), &zip).unwrap();
        let plain = slot_store::archive::crc32(&fixtures.join("rom.gb")).unwrap();
        assert_eq!(Crcs::load(root).of(root, &zip).unwrap(), plain);
    }

    #[test]
    fn a_web_page_is_not_saved_as_a_label() {
        assert!(!is_png(b"<html>not found</html>"));
        assert!(is_png(&png_bytes()));
    }
}
