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
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
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
    platform: Platform,
    /// Built by brick/make-cart-labels.py, shipped on the card.
    index: &'static str,
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
    platform: Platform::Nes,
    index: "System/nes-cart-labels.txt",
    aspect: (1.08, 1.16),
    label: [0.405, 0.006, 0.865, 0.680],
    art: [0.405, 0.006, 0.865, 0.680],
    side: [0.02, 0.3, 0.065, 0.6],
    plain: [[0.42, 0.73, 0.85, 0.80], [0.885, 0.1, 0.93, 0.6]],
};

/// The US SNES cart: label across the top of the middle, ridged wings either side.
const SNES_PHOTO: CartPhoto = CartPhoto {
    platform: Platform::Snes,
    index: "System/snes-cart-labels.txt",
    aspect: (0.62, 0.67),
    label: [0.200, 0.008, 0.807, 0.432],
    art: [0.200, 0.008, 0.807, 0.432],
    side: [0.04, 0.35, 0.10, 0.65],
    plain: [[0.25, 0.47, 0.75, 0.55], [0.88, 0.2, 0.97, 0.8]],
};

/// The Famicom cart: a wide label over most of the front, plain plastic right of it and
/// below; the connector tab underneath. Makers placed the label a little differently, so
/// this takes its middle; the plastic is read below it, as the edges are shaded.
const FAMICOM_PHOTO: CartPhoto = CartPhoto {
    platform: Platform::Nes,
    index: "System/famicom-cart-labels.txt",
    aspect: (0.6, 0.7),
    label: [0.09, 0.125, 0.91, 0.74],
    art: [0.25, 0.25, 0.75, 0.65],
    side: [0.3, 0.9, 0.7, 0.95],
    plain: [[0.1, 0.9, 0.9, 0.97], [0.955, 0.2, 0.975, 0.7]],
};

/// The pictures a cart's label is cut from: a NES game on the Famicom cart takes a Famicom
/// cart's label, falling back to the NES one's (shown whole, as it is tall).
fn photo_specs(cart: &Cart) -> &'static [&'static CartPhoto] {
    match cart.platform {
        Platform::Nes if slot_ui::nes_famicom_of(cart) => &[&FAMICOM_PHOTO, &NES_PHOTO],
        Platform::Nes => &[&NES_PHOTO],
        Platform::Snes => &[&SNES_PHOTO],
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
        .filter(|c| c.label.is_none())
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
    Ok(save_label(src.root, &cart, &png, format!("label scan {path} ({how})")))
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
        match label_from_photo(&png, spec) {
            Ok(label) => return Ok(save_label(root, cart, &label, format!("{name} cart {file}"))),
            Err(e) => why.push(format!("{file}: {e}")),
        }
    }
    Ok(Outcome::Unusable(format!("{name}: {}", why.join("; "))))
}

/// Cuts the label out of a front picture of a cart, as a PNG. Only the standard cart (the
/// spec's shape, plain plastic where it should be) is used; others (Tengen, Famicom, PAL,
/// a box) put the label elsewhere and are refused.
fn label_from_photo(png: &[u8], spec: &CartPhoto) -> Result<Vec<u8>, &'static str> {
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
    if !(spec.aspect.0..=spec.aspect.1).contains(&(ch / cw)) || cw < 120.0 {
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
    if spec.plain.iter().any(|r| share(*r) < 0.7) {
        return Err("not the standard cart");
    }
    if share(spec.art) > 0.5 {
        return Err("no label on the cart");
    }
    // Rounded inward, so no sliver of plastic rides along the label's edge.
    let [fx0, fy0, fx1, fy1] = spec.label;
    let (lx, ly) = (x0 + (cw * fx0).ceil() as u32, y0 + (ch * fy0).ceil() as u32);
    let (lw, lh) = (x0 + (cw * fx1) as u32 - lx, y0 + (ch * fy1) as u32 - ly);
    let mut out = Vec::with_capacity((lw * lh * 4) as usize);
    for y in ly..ly + lh {
        let i = ((y * w + lx) * 4) as usize;
        out.extend_from_slice(&rgba[i..i + (lw * 4) as usize]);
    }
    let (out, lw, lh) = shrink(out, lw, lh, LABEL_MAX);
    let mut bytes = Vec::new();
    let mut e = png::Encoder::new(&mut bytes, lw, lh);
    e.set_color(png::ColorType::Rgba);
    e.set_depth(png::BitDepth::Eight);
    e.write_header()
        .and_then(|mut w| w.write_image_data(&out))
        .map_err(|_| "cannot encode the label")?;
    Ok(bytes)
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

fn decode_rgba(bytes: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
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

fn save_label(root: &Path, cart: &Cart, png: &[u8], from: String) -> Outcome {
    let label = root
        .join("Labels")
        .join(cart.platform.dir_name())
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
        let crc = crc32_file(rom)?;
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

const CRC_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            k += 1;
        }
        table[i] = c;
        i += 1;
    }
    table
};

/// The CRC32 No-Intro and the art set index ROMs by.
fn crc32(crc: u32, bytes: &[u8]) -> u32 {
    let mut c = !crc;
    for &b in bytes {
        c = CRC_TABLE[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8);
    }
    !c
}

fn crc32_file(path: &Path) -> std::io::Result<u32> {
    let mut file = std::fs::File::open(path)?;
    let mut buf = vec![0u8; 256 << 10];
    let mut crc = 0;
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            return Ok(crc);
        }
        crc = crc32(crc, &buf[..n]);
    }
}

fn is_png(bytes: &[u8]) -> bool {
    let mut dec = png::Decoder::new(bytes);
    dec.set_limits(png::Limits { bytes: 64 << 20 });
    dec.read_info().is_ok_and(|r| {
        let info = r.info();
        info.width > 0 && info.height > 0 && info.width <= 4096 && info.height <= 4096
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
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
    fn crc32_matches_the_standard_check_value() {
        assert_eq!(crc32(0, b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(crc32(0, b"1234"), b"56789"), 0xCBF4_3926, "streams in pieces");
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
        let crc = crc32(0, &[0u8; 0x200]);
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
        assert_eq!(Crcs::load(root).of(root, &rom_path).unwrap(), crc32(0, b"patched"));
    }

    #[test]
    fn a_web_page_is_not_saved_as_a_label() {
        assert!(!is_png(b"<html>not found</html>"));
        assert!(is_png(&png_bytes()));
    }
}
