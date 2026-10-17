//! A label kept on the framing screen goes on its cart like a sticker: it lifts off the
//! framing screen with a corner curled up, flies over to the cart in the cart menu, is pressed
//! down, and the curled corner is smoothed flat last.
//!
//! The curl is a fold across the corner with the label's paper back showing, baked into the
//! label's own picture a step at a time. The GPU then only ever draws plain turned rectangles,
//! which is all the sprite pipeline has; at the size a label is on the Brick's screen a fold
//! shaded like a roll reads as a curl.
use crate::cart::CartFace;
use crate::draw::{Draw, TexId};
use crate::hud::Millis;
use crate::label_adjust::Rect;
use crate::slot_chrome::ease;

/// How many curls are baked, flat first: enough that lifting and smoothing it never steps.
pub const PEEL_STEPS: usize = 10;
/// A clear pixel round the label's picture, so its edges stay smooth once it is turned.
pub const PEEL_PAD: u32 = 1;
/// How far the drop shadow's soft edge reaches either side of the label's own.
pub const PEEL_SHADOW_PAD: u32 = 14;

/// Lifting off the framing screen, the flight, the press onto the cart, smoothing the corner.
const LIFT_MS: Millis = 170;
const FLY_MS: Millis = 480;
const LAND_MS: Millis = 560;
pub const PEEL_MS: Millis = 720;
/// The framing screen is gone a little after the sticker is clear of it.
const BACKDROP_MS: Millis = LIFT_MS + 80;

/// Lifted, it is a touch smaller than it was lying flat; in the air over the cart, a touch
/// larger than it will be, so landing presses it down.
const LIFT_SHRINK: f32 = 0.06;
const AIR_GROW: f32 = 1.05;
/// How far the flight bows upwards, in pixels.
const ARC: f32 = 46.0;
/// The tilt it lifts with, in radians: a few degrees.
const TILT: f32 = -0.06;
/// How high it is held, as the distance its shadow falls from it.
const AIR: f32 = 18.0;
const SHADOW_ALPHA: f32 = 0.42;

/// How far into the label the fold reaches at its most, as a share of the shorter side.
const CURL: f32 = 0.42;
/// The paper on the back of a label.
const BACK: [f32; 3] = [238.0, 233.0, 222.0];
/// The shadow the curled flap casts on the label under it, at its darkest.
const FLAP_SHADE: f32 = 0.38;

/// Where the sticker is at one moment: its place on screen (the label itself, without the
/// clear edge), its tilt, how far the corner is curled (0 flat, 1 most), its shadow, and how
/// much of the framing screen still shows under it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PeelPose {
    pub at: Rect,
    pub turn: f32,
    pub curl: f32,
    pub lift: f32,
    pub shadow: f32,
    pub backdrop: f32,
}

/// The sticker `t` into its flight from `from`, the slot on the framing screen, to `to`, the
/// cart's slot in the cart menu.
pub fn peel_pose(t: Millis, from: Rect, to: Rect) -> PeelPose {
    let span = |a: Millis, b: Millis| ease((t.saturating_sub(a) as f32 / (b - a) as f32).clamp(0.0, 1.0));
    let (lift, fly, land, flat) = (span(0, LIFT_MS), span(LIFT_MS, FLY_MS), span(FLY_MS, LAND_MS), span(LAND_MS, PEEL_MS));
    let lifted = scaled(from, 1.0 - LIFT_SHRINK * lift);
    let mut at = mix(mix(lifted, scaled(to, AIR_GROW), fly), to, land);
    if fly < 1.0 {
        at.1 -= ARC * (std::f32::consts::PI * fly).sin();
    }
    PeelPose {
        at,
        // Tilted as it comes up, straightening some on the way and the rest as it lands.
        turn: TILT * lift * (1.0 - 0.4 * fly) * (1.0 - land),
        curl: lift * (1.0 - flat),
        lift: AIR * lift * (1.0 - 0.85 * land) * (1.0 - flat),
        shadow: SHADOW_ALPHA * lift * (1.0 - flat),
        backdrop: 1.0 - span(0, BACKDROP_MS),
    }
}

fn scaled((x, y, w, h): Rect, k: f32) -> Rect {
    let (nw, nh) = (w * k, h * k);
    (x + (w - nw) / 2.0, y + (h - nh) / 2.0, nw, nh)
}

fn mix(a: Rect, b: Rect, u: f32) -> Rect {
    // Exactly there once there, so the hand-over to the cart's own label is to the pixel.
    if u >= 1.0 {
        return b;
    }
    let m = |p: f32, q: f32| p + (q - p) * u;
    (m(a.0, b.0), m(a.1, b.1), m(a.2, b.2), m(a.3, b.3))
}

/// Which of the baked curls to draw for `curl`.
pub fn peel_step(curl: f32) -> usize {
    (curl.clamp(0.0, 1.0) * (PEEL_STEPS - 1) as f32).round() as usize
}

/// The label, `w` by `h` RGBA, with its bottom right corner curled up by `curl` (0 flat, 1
/// most): folded back over itself along a line across the corner, the back of the paper up and
/// shaded darkest at the crease as a roll would be, with the flap's shadow on the label beside
/// it. Comes with PEEL_PAD clear all round.
pub fn peel_face(label: &[u8], w: u32, h: u32, curl: f32) -> CartFace {
    let (fw, fh) = (w + 2 * PEEL_PAD, h + 2 * PEEL_PAD);
    let mut rgba = vec![0u8; (fw * fh * 4) as usize];
    if label.len() < (w * h * 4) as usize {
        return CartFace { rgba, w: fw, h: fh };
    }
    let (wf, hf) = (w as f32, h as f32);
    let d = curl.clamp(0.0, 1.0) * CURL * wf.min(hf);
    // The fold is the line x + y = k: past it the corner has lifted away and folded back over.
    let k = wf + hf - d;
    let reach = d * 0.3 + 3.0;
    for y in 0..h {
        for x in 0..w {
            let src = ((y * w + x) * 4) as usize;
            let dst = (((y + PEEL_PAD) * fw + x + PEEL_PAD) * 4) as usize;
            if d < 0.5 {
                rgba[dst..dst + 4].copy_from_slice(&label[src..src + 4]);
                continue;
            }
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let s = px + py;
            // How much of the pixel is on the label's side of the fold, smoothed over a pixel.
            let kept = ((k - s) / std::f32::consts::SQRT_2 + 0.5).clamp(0.0, 1.0);
            // The flap is the corner reflected across the fold: (x, y) is under it when its
            // reflection, (k - y, k - x), lands on the label.
            let flap = kept * (py - (k - wf) + 0.5).clamp(0.0, 1.0) * (px - (k - hf) + 0.5).clamp(0.0, 1.0);
            let outside = ((k - hf) - px).max((k - wf) - py).max(0.0);
            let shade = FLAP_SHADE * (1.0 - outside / reach).clamp(0.0, 1.0).powi(2) * (1.0 - flap);
            // Along the flap from the crease (0) to its tip (1): dark in the roll, light flat.
            let along = ((k - s) / d).clamp(0.0, 1.0);
            let lum = 0.72 + 0.28 * (1.0 - (1.0 - along).powi(2));
            let front_a = label[src + 3] as f32 / 255.0 * kept;
            let a = flap + front_a * (1.0 - flap);
            if a <= 0.0 {
                continue;
            }
            for c in 0..3 {
                let front = label[src + c] as f32 * (1.0 - shade);
                let v = (BACK[c] * lum * flap + front * front_a * (1.0 - flap)) / a;
                rgba[dst + c] = v.round().clamp(0.0, 255.0) as u8;
            }
            rgba[dst + 3] = (a * 255.0).round() as u8;
        }
    }
    CartFace { rgba, w: fw, h: fh }
}

/// The sticker's drop shadow: a dark rectangle the label's size, its edges soft for
/// PEEL_SHADOW_PAD either way.
pub fn peel_shadow_face(w: u32, h: u32) -> CartFace {
    let p = PEEL_SHADOW_PAD as f32;
    let (fw, fh) = (w + 2 * PEEL_SHADOW_PAD, h + 2 * PEEL_SHADOW_PAD);
    let edge = |i: u32, n: u32| {
        let from_edge = (i as f32 + 0.5).min(n as f32 - i as f32 - 0.5);
        let u = (from_edge / (2.0 * p)).clamp(0.0, 1.0);
        u * u * (3.0 - 2.0 * u)
    };
    let mut rgba = Vec::with_capacity((fw * fh * 4) as usize);
    for y in 0..fh {
        for x in 0..fw {
            let a = edge(x, fw) * edge(y, fh);
            rgba.extend_from_slice(&[0, 0, 0, (a * 255.0).round() as u8]);
        }
    }
    CartFace { rgba, w: fw, h: fh }
}

/// The sticker as it is at `pose`: `faces` are its baked curls, flat first, for a label `label`
/// pixels in size, and `shadow` its drop shadow.
pub fn draw_peel(pose: &PeelPose, faces: &[TexId], shadow: TexId, label: (u32, u32), out: &mut Vec<Draw>) {
    let Some(&tex) = faces.get(peel_step(pose.curl)).or(faces.first()) else {
        return;
    };
    let (x, y, w, h) = pose.at;
    let (kx, ky) = (w / label.0.max(1) as f32, h / label.1.max(1) as f32);
    if pose.shadow > 0.0 {
        let (px, py) = (PEEL_SHADOW_PAD as f32 * kx, PEEL_SHADOW_PAD as f32 * ky);
        out.push(Draw::Turned {
            x: x - px + pose.lift * 0.6,
            y: y - py + pose.lift,
            w: w + 2.0 * px,
            h: h + 2.0 * py,
            tex: shadow,
            alpha: pose.shadow,
            turn: pose.turn,
        });
    }
    let (px, py) = (PEEL_PAD as f32 * kx, PEEL_PAD as f32 * ky);
    out.push(Draw::Turned { x: x - px, y: y - py, w: w + 2.0 * px, h: h + 2.0 * py, tex, alpha: 1.0, turn: pose.turn });
}

/// Fades what has been drawn into `out` since `from` by `alpha`: the framing screen, going.
pub fn fade_from(out: &mut [Draw], from: usize, alpha: f32) {
    for d in out.iter_mut().skip(from) {
        match d {
            Draw::Rect { colour, .. } => colour[3] *= alpha,
            Draw::Tex { alpha: a, .. } | Draw::Turned { alpha: a, .. } => *a *= alpha,
            Draw::Game | Draw::Shot { .. } => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FROM: Rect = (250.0, 74.0, 520.0, 252.0);
    const TO: Rect = (60.0, 300.0, 180.0, 90.0);

    #[test]
    fn it_starts_where_it_was_framed_and_ends_flat_on_the_cart() {
        let start = peel_pose(0, FROM, TO);
        assert_eq!((start.at, start.curl, start.shadow, start.backdrop), (FROM, 0.0, 0.0, 1.0));
        let end = peel_pose(PEEL_MS, FROM, TO);
        assert_eq!((end.at, end.turn, end.curl, end.shadow, end.backdrop), (TO, 0.0, 0.0, 0.0, 0.0));
        assert_eq!(peel_pose(PEEL_MS + 500, FROM, TO), end);
    }

    #[test]
    fn the_corner_is_curled_through_the_flight_and_smoothed_only_after_landing() {
        for t in [LIFT_MS, (LIFT_MS + FLY_MS) / 2, FLY_MS, LAND_MS] {
            let p = peel_pose(t, FROM, TO);
            assert!(p.curl > 0.99, "{t} ms: curl {}", p.curl);
        }
        let landed = peel_pose(LAND_MS, FROM, TO);
        assert_eq!(landed.at, TO, "it lands before the corner goes down");
        let half = peel_pose((LAND_MS + PEEL_MS) / 2, FROM, TO);
        assert!(half.curl > 0.0 && half.curl < 1.0);
    }

    #[test]
    fn a_flat_peel_is_the_label_untouched() {
        let label: Vec<u8> = (0..6 * 4 * 4).map(|i| (i * 7 % 256) as u8).collect();
        let face = peel_face(&label, 6, 4, 0.0);
        assert_eq!((face.w, face.h), (6 + 2 * PEEL_PAD, 4 + 2 * PEEL_PAD));
        for y in 0..4 {
            let row = ((y + PEEL_PAD) * face.w + PEEL_PAD) as usize * 4;
            assert_eq!(&face.rgba[row..row + 24], &label[y as usize * 24..(y as usize + 1) * 24]);
        }
        assert!(face.rgba[..face.w as usize * 4].iter().all(|&v| v == 0), "the edge is clear");
    }

    #[test]
    fn curled_the_corner_is_gone_and_the_paper_back_shows_beside_it() {
        let (w, h) = (100u32, 60u32);
        let label = [20u8, 40, 200, 255].repeat((w * h) as usize);
        let face = peel_face(&label, w, h, 1.0);
        let at = |x: u32, y: u32| {
            let i = (((y + PEEL_PAD) * face.w + x + PEEL_PAD) * 4) as usize;
            [face.rgba[i], face.rgba[i + 1], face.rgba[i + 2], face.rgba[i + 3]]
        };
        assert_eq!(at(w - 1, h - 1)[3], 0, "the very corner has lifted away");
        let back = at(w - 18, h - 18);
        assert!(back[3] == 255 && back[0] > 150 && back[0] > back[2] - 30, "paper back: {back:?}");
        assert_eq!(at(5, 5), [20, 40, 200, 255], "far from the corner, the label as it was");
    }

    #[test]
    fn nothing_is_read_past_a_short_label() {
        let face = peel_face(&[255; 8], 10, 10, 1.0);
        assert!(face.rgba.iter().all(|&v| v == 0));
    }

    #[test]
    fn the_framing_screen_fades_and_nothing_before_it_does() {
        let mut out = vec![
            Draw::Rect { x: 0.0, y: 0.0, w: 1.0, h: 1.0, colour: [1.0; 4] },
            Draw::Rect { x: 0.0, y: 0.0, w: 1.0, h: 1.0, colour: [1.0; 4] },
        ];
        fade_from(&mut out, 1, 0.25);
        assert!(matches!(out[0], Draw::Rect { colour: [_, _, _, a], .. } if a == 1.0));
        assert!(matches!(out[1], Draw::Rect { colour: [_, _, _, a], .. } if a == 0.25));
    }
}
