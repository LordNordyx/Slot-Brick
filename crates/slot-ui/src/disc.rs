//! A PlayStation game on the shelf is a disc: its own disc art when the player has some, or a
//! printed disc of Slot's own with the game's name on it. It goes into a disc slot, spinning.
use slot_store::Cart;

use crate::cart::{label_colour, label_text, CartFace};
use crate::text;

/// A disc's width and height on the shelf, at full size.
pub const DISC_PX: u32 = 300;
/// How far out each part of a disc reaches, as a share of its radius (a CD's 120 mm across:
/// the 15 mm hole, the clear hub to 33 mm, the mirror band to 46 mm, then the print).
const HOLE: f32 = 0.125;
const HUB: f32 = 0.29;
const MIRROR: f32 = 0.38;
const RIM: f32 = 0.985;

const SILVER: [u8; 3] = [0xc9, 0xca, 0xce];
const HUB_GREY: [u8; 3] = [0xe2, 0xe4, 0xe8];
const HUB_ALPHA: f32 = 0.55;
/// The title's box: across the print above the hub.
const TITLE_BOX: (f32, f32, f32, f32) = (0.18, 0.12, 0.64, 0.22);
const TITLE_LINES: usize = 2;
const TITLE_PX: f32 = 28.0;
const TITLE_MIN_PX: f32 = 12.0;
/// "DISC 2": under the hub.
const NUMBER_BOX: (f32, f32, f32, f32) = (0.3, 0.7, 0.4, 0.1);
const NUMBER_PX: f32 = 18.0;

/// The disc's face: `art` (DISC_PX square, RGBA) printed on it when given, else a disc in the
/// game's own colour with its name across the top, and which disc it is under the hub for a
/// game of several (`number`, from 1).
pub fn disc_face(cart: &Cart, art: Option<Vec<u8>>, number: Option<usize>) -> CartFace {
    let n = DISC_PX;
    let title = label_text(cart);
    let print = label_colour(&title);
    let mut rgba = vec![0u8; (n * n * 4) as usize];
    let r = n as f32 / 2.0;
    for y in 0..n {
        for x in 0..n {
            let o = ((y * n + x) * 4) as usize;
            // Four samples a pixel, so every edge between the rings is smooth.
            let (mut sum, mut cover) = ([0.0f32; 3], 0.0f32);
            for (sx, sy) in [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)] {
                let d = ((x as f32 + sx - r).powi(2) + (y as f32 + sy - r).powi(2)).sqrt() / r;
                if !(HOLE..=1.0).contains(&d) {
                    continue;
                }
                let (colour, alpha) = match &art {
                    Some(art) => ([art[o], art[o + 1], art[o + 2]], art[o + 3] as f32 / 255.0),
                    None if d < HUB => (HUB_GREY, HUB_ALPHA),
                    None if d < MIRROR => (shade(SILVER, 1.0 - 0.25 * (d - HUB) / (MIRROR - HUB)), 1.0),
                    None if d > RIM => (shade(print, 0.7), 1.0),
                    None => (print, 1.0),
                };
                for c in 0..3 {
                    sum[c] += colour[c] as f32 * alpha;
                }
                cover += alpha;
            }
            if cover > 0.0 {
                for c in 0..3 {
                    rgba[o + c] = (sum[c] / cover).round() as u8;
                }
                rgba[o + 3] = (cover / 4.0 * 255.0).round() as u8;
            }
        }
    }
    if art.is_none() {
        print_title(&mut rgba, n, &title, print, TITLE_BOX, TITLE_LINES, TITLE_PX);
        if let Some(number) = number {
            print_title(&mut rgba, n, &format!("DISC {number}"), print, NUMBER_BOX, 1, NUMBER_PX);
        }
    }
    CartFace { rgba, w: n, h: n }
}

/// The disc's outline, black: what a disc on the shelf's edge, drawn see-through, is drawn on.
pub fn disc_shadow() -> CartFace {
    let n = DISC_PX;
    let r = n as f32 / 2.0;
    let mut rgba = Vec::with_capacity((n * n * 4) as usize);
    for y in 0..n {
        for x in 0..n {
            let d = ((x as f32 + 0.5 - r).powi(2) + (y as f32 + 0.5 - r).powi(2)).sqrt();
            let cover = (r - d + 0.5).clamp(0.0, 1.0) * (d - HOLE * r + 0.5).clamp(0.0, 1.0);
            rgba.extend_from_slice(&[0, 0, 0, (cover * 255.0).round() as u8]);
        }
    }
    CartFace { rgba, w: n, h: n }
}

fn print_title(
    rgba: &mut [u8],
    n: u32,
    title: &str,
    print: [u8; 3],
    (bx, by, bw, bh): (f32, f32, f32, f32),
    lines: usize,
    px: f32,
) {
    let Some(font) = text::label_font() else {
        return;
    };
    let (x0, y0) = ((bx * n as f32) as u32, (by * n as f32) as u32);
    let (w, h) = ((bw * n as f32) as u32, (bh * n as f32) as u32);
    let layout = text::fit_box(font, title, w as f32, h as f32, lines, px, TITLE_MIN_PX);
    let mut block = vec![0u8; (w * h * 4) as usize];
    text::draw_centred(&mut block, w, h, &layout, ink(print));
    for y in 0..h {
        for x in 0..w {
            let s = ((y * w + x) * 4) as usize;
            let a = block[s + 3] as u32;
            if a == 0 {
                continue;
            }
            let d = (((y0 + y) * n + x0 + x) * 4) as usize;
            for c in 0..3 {
                rgba[d + c] = ((block[s + c] as u32 * a + rgba[d + c] as u32 * (255 - a)) / 255) as u8;
            }
        }
    }
}

fn shade(c: [u8; 3], k: f32) -> [u8; 3] {
    c.map(|v| (v as f32 * k).round().clamp(0.0, 255.0) as u8)
}

fn ink(bg: [u8; 3]) -> [u8; 3] {
    let luma = 0.2126 * bg[0] as f32 + 0.7152 * bg[1] as f32 + 0.0722 * bg[2] as f32;
    if luma > 140.0 {
        [0x1a, 0x18, 0x16]
    } else {
        [0xf4, 0xf1, 0xea]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn placeholder() -> Cart {
        Cart {
            platform: slot_store::Platform::Ps1,
            stem: "Disc Test".into(),
            rom: Default::default(),
            label: None,
            famicom_label: None,
            title: String::new(),
            code: String::new(),
            shell: None,
        }
    }

    fn alpha(face: &CartFace, x: u32, y: u32) -> u8 {
        face.rgba[((y * face.w + x) * 4 + 3) as usize]
    }

    #[test]
    fn a_disc_is_round_with_a_hole_in_the_middle() {
        let face = disc_face(&placeholder(), None, Some(2));
        let c = DISC_PX / 2;
        assert_eq!(alpha(&face, 0, 0), 0, "a corner is not clear");
        assert_eq!(alpha(&face, c, c), 0, "the hole is not clear");
        assert_eq!(alpha(&face, c, DISC_PX - 10), 255, "the print is not solid");
        assert!(alpha(&face, c, c + (0.2 * c as f32) as u32) < 255, "the hub is not see-through");
    }
}
