use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use fontdue::{Font, FontSettings};

use crate::CartFace;

/// Only the symbols Slot draws, cut from SymbolsNerdFontMono-Regular.ttf by
/// brick/subset-symbols.sh: the full font takes the font library about 0.6 s to read on a Brick.
const SYMBOLS_TTF: &[u8] = include_bytes!("../assets/SymbolsNerdFontMono-Slot.ttf");

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum Icon {
    Volume,
    VolumeZero,
    VolumeMuted,
    Brightness,
    BlueLight,
    FastForward,
    FastForwardLatched,
    Rewind,
    Alert,
    Charging,
    Headphones,
    HeadphonesMuted,
    /// Wi-Fi, as the shelf reports it: on the network, on but not reaching it, and off.
    Wifi,
    WifiNoRoute,
    WifiOff,
    /// A favorite, on its cart on the shelf.
    Star,
}

impl Icon {
    pub const ALL: [Icon; 16] = [
        Icon::Volume,
        Icon::VolumeZero,
        Icon::VolumeMuted,
        Icon::Brightness,
        Icon::BlueLight,
        Icon::FastForward,
        Icon::FastForwardLatched,
        Icon::Rewind,
        Icon::Alert,
        Icon::Charging,
        Icon::Headphones,
        Icon::HeadphonesMuted,
        Icon::Wifi,
        Icon::WifiNoRoute,
        Icon::WifiOff,
        Icon::Star,
    ];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn glyph(self) -> char {
        match self {
            Icon::Volume => '\u{f028}',
            Icon::VolumeZero => '\u{f026}',
            Icon::VolumeMuted => '\u{f075f}',
            Icon::Brightness => '\u{f185}',
            Icon::BlueLight => '\u{f186}',
            Icon::FastForward => '\u{f06d2}',
            Icon::FastForwardLatched => '\u{f0211}',
            Icon::Rewind => '\u{f04a}',
            Icon::Alert => '\u{f0026}',
            Icon::Charging => '\u{f0e7}',
            Icon::Headphones => '\u{f025}',
            Icon::HeadphonesMuted => '\u{f07ce}',
            Icon::Wifi => '\u{f05a9}',
            // One arc against the full fan above: on, but nothing coming back through it.
            Icon::WifiNoRoute => '\u{f091f}',
            Icon::WifiOff => '\u{f05aa}',
            Icon::Star => '\u{f005}',
        }
    }
}

const HALO: [u8; 3] = [0x08, 0x08, 0x0a];
pub const HALO_PX: u32 = 1;

pub fn haloed(cov: &[u8], cw: u32, ch: u32, colour: [u8; 3]) -> CartFace {
    haloed_by(cov, cw, ch, |_, _| colour)
}

/// As `haloed`, with each pixel's colour chosen by where it is in the glyph (x, y).
fn haloed_by(cov: &[u8], cw: u32, ch: u32, colour: impl Fn(usize, usize) -> [u8; 3]) -> CartFace {
    let pad = HALO_PX as usize;
    let (cw, ch) = (cw as usize, ch as usize);
    let (w, h) = (cw + 2 * pad, ch + 2 * pad);
    let at = |x: isize, y: isize| -> u8 {
        if x < 0 || y < 0 || x >= cw as isize || y >= ch as isize {
            0
        } else {
            cov[y as usize * cw + x as usize]
        }
    };

    let mut rgba = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        for x in 0..w {
            let (gx, gy) = (x as isize - pad as isize, y as isize - pad as isize);
            let ink = at(gx, gy);
            let mut halo = 0u8;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    halo = halo.max(at(gx + dx, gy + dy));
                }
            }
            if ink > 0 {
                let colour = colour(gx as usize, gy as usize);
                let a = ink as u32;
                let inv = 255 - a;
                let mix = |c: u8, s: u8| ((c as u32 * a + s as u32 * inv) / 255) as u8;
                rgba.extend_from_slice(&[
                    mix(colour[0], HALO[0]),
                    mix(colour[1], HALO[1]),
                    mix(colour[2], HALO[2]),
                    ink.max(halo),
                ]);
            } else {
                rgba.extend_from_slice(&[HALO[0], HALO[1], HALO[2], halo]);
            }
        }
    }
    CartFace {
        rgba,
        w: w as u32,
        h: h as u32,
    }
}

pub fn icon_face(icon: Icon, px: f32, colour: [u8; 3]) -> CartFace {
    let Some(r) = raster(icon, px) else {
        return CartFace {
            rgba: Vec::new(),
            w: 0,
            h: 0,
        };
    };
    haloed(&r.cov, r.w, r.h, colour)
}

/// A favorite's star, cut like a gem in three golds: each arm is split along its ridge, the
/// half facing the light (from the top left) lit, the half facing away shaded, and the rest
/// `base`. Outlined like the other icons.
pub fn star_face(px: f32, base: [u8; 3], light: [u8; 3], shade: [u8; 3]) -> CartFace {
    let Some(r) = raster(Icon::Star, px) else {
        return CartFace { rgba: Vec::new(), w: 0, h: 0 };
    };
    let (w, h) = (r.w as usize, r.h as usize);
    // Its centre, as the ink's balance point: a five-pointed star's is its middle.
    let (mut sx, mut sy, mut total) = (0.0f32, 0.0f32, 0.0f32);
    for y in 0..h {
        for x in 0..w {
            let a = r.cov[y * w + x] as f32;
            sx += a * x as f32;
            sy += a * y as f32;
            total += a;
        }
    }
    let (cx, cy) = match total > 0.0 {
        true => (sx / total, sy / total),
        false => (w as f32 / 2.0, h as f32 / 2.0),
    };
    let lit = {
        let (x, y) = (-1.0f32, -1.2f32);
        let n = (x * x + y * y).sqrt();
        (x / n, y / n)
    };
    haloed_by(&r.cov, r.w, r.h, |x, y| {
        let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
        if dx * dx + dy * dy < 1.0 {
            return base;
        }
        // The nearest arm (the first points straight up; y grows downwards), and which side
        // of its ridge this is.
        let step = std::f32::consts::TAU / 5.0;
        let up = -std::f32::consts::FRAC_PI_2;
        let angle = dy.atan2(dx);
        let arm = up + ((angle - up) / step).round() * step;
        let side = (angle - arm).sin().signum();
        // That half slopes away from the ridge, sideways and a little outwards.
        let (ax, ay) = (arm.cos(), arm.sin());
        let (nx, ny) = (0.5 * ax - side * ay, 0.5 * ay + side * ax);
        let n = (nx * nx + ny * ny).sqrt();
        match (nx * lit.0 + ny * lit.1) / n {
            d if d > 0.3 => light,
            d if d < -0.3 => shade,
            _ => base,
        }
    })
}

pub fn icon_box(px: f32) -> (u32, u32) {
    match raster(Icon::Volume, px) {
        Some(r) => (r.w + 2 * HALO_PX, r.h + 2 * HALO_PX),
        None => (0, 0),
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Badge {
    Link,
    LinkBroken,
}

impl Badge {
    pub fn glyph(self) -> char {
        match self {
            Badge::Link => '\u{f0339}',
            Badge::LinkBroken => '\u{f033a}',
        }
    }
}

pub fn badge_face(badge: Badge, px: f32, colour: [u8; 3]) -> CartFace {
    let Some(font) = symbols_font() else {
        return CartFace {
            rgba: Vec::new(),
            w: 0,
            h: 0,
        };
    };
    let f = frame(font, px);
    let (m, cov) = font.rasterize(badge.glyph(), px);
    let x0 = m.xmin - f.left;
    let y0 = f.top - (m.ymin + m.height as i32);
    let mut out = vec![0u8; (f.w * f.h) as usize];
    for gy in 0..m.height {
        for gx in 0..m.width {
            let (dx, dy) = (x0 + gx as i32, y0 + gy as i32);
            if dx < 0 || dy < 0 || dx >= f.w as i32 || dy >= f.h as i32 {
                continue;
            }
            out[(dy as u32 * f.w + dx as u32) as usize] = cov[gy * m.width + gx];
        }
    }
    haloed(&out, f.w, f.h, colour)
}

struct Raster {
    cov: Vec<u8>,
    w: u32,
    h: u32,
}

type Cache = Mutex<HashMap<(Icon, u32), Arc<Raster>>>;

fn raster(icon: Icon, px: f32) -> Option<Arc<Raster>> {
    static CACHE: OnceLock<Cache> = OnceLock::new();
    let cache = CACHE.get_or_init(Mutex::default);
    let key = (icon, px.to_bits());
    let mut cache = cache.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(r) = cache.get(&key) {
        return Some(r.clone());
    }
    let r = Arc::new(rasterise(icon, px)?);
    cache.insert(key, r.clone());
    Some(r)
}

fn rasterise(icon: Icon, px: f32) -> Option<Raster> {
    let font = symbols_font()?;
    let f = frame(font, px);
    let (m, cov) = font.rasterize(icon.glyph(), px);
    let x0 = m.xmin - f.left;
    let y0 = f.top - (m.ymin + m.height as i32);

    let mut out = vec![0u8; (f.w * f.h) as usize];
    for gy in 0..m.height {
        for gx in 0..m.width {
            let (dx, dy) = ((x0 + gx as i32) as u32, (y0 + gy as i32) as u32);
            out[(dy * f.w + dx) as usize] = cov[gy * m.width + gx];
        }
    }
    Some(Raster {
        cov: out,
        w: f.w,
        h: f.h,
    })
}

struct Frame {
    w: u32,
    h: u32,
    left: i32,
    top: i32,
}

fn frame(font: &Font, px: f32) -> Frame {
    let (mut left, mut right, mut top, mut bottom) = (i32::MAX, i32::MIN, i32::MIN, i32::MAX);
    for icon in Icon::ALL {
        let m = font.metrics(icon.glyph(), px);
        left = left.min(m.xmin);
        right = right.max((m.xmin + m.width as i32).max(m.advance_width.ceil() as i32));
        top = top.max(m.ymin + m.height as i32);
        bottom = bottom.min(m.ymin);
    }
    Frame {
        w: (right - left).max(1) as u32,
        h: (top - bottom).max(1) as u32,
        left,
        top,
    }
}

pub(crate) fn symbols_font() -> Option<&'static Font> {
    static FONT: OnceLock<Option<Font>> = OnceLock::new();
    FONT.get_or_init(|| Font::from_bytes(SYMBOLS_TTF, FontSettings::default()).ok())
        .as_ref()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A symbol missing from the cut-down font draws as nothing: rerun brick/subset-symbols.sh.
    #[test]
    fn every_symbol_slot_draws_is_in_the_font() {
        let font = symbols_font().expect("the symbols font loads");
        let src = concat!(
            include_str!("icon.rs"),
            include_str!("plate.rs"),
            include_str!("quick_menu.rs")
        );
        let mut used: Vec<char> = Icon::ALL.iter().map(|i| i.glyph()).collect();
        used.extend([Badge::Link.glyph(), Badge::LinkBroken.glyph()]);
        used.extend(src.split("\\u{").skip(1).filter_map(|rest| {
            let hex = rest.split('}').next()?;
            char::from_u32(u32::from_str_radix(hex, 16).ok()?)
                .filter(|c| matches!(*c as u32, 0xe000..=0xf8ff | 0xf0000..=0xfffff))
        }));
        for c in used {
            assert_ne!(font.lookup_glyph_index(c), 0, "U+{:X} is not in the symbols font", c as u32);
        }
    }
}
