//! A label kept on the framing screen goes on its cart like a sticker: it lifts off the
//! framing screen with a corner curled up, flies over to the cart in the cart menu, is pressed
//! down, and the curled corner is smoothed flat last.
//!
//! The curl is a fold, square to the label's diagonal, with the label's paper back showing,
//! baked into the label's own picture a step at a time: at its most it reaches halfway, so
//! half the label is folded back over the other half. The GPU then only ever draws plain turned rectangles,
//! which is all the sprite pipeline has; at the size a label is on the Brick's screen a fold
//! shaded like a roll reads as a curl.
use crate::cart::CartFace;
use crate::draw::{Draw, TexId};
use crate::hud::Millis;
use crate::label_adjust::Rect;
use crate::slot_chrome::ease;

/// How many curls are baked, flat first: enough that peeling and smoothing it never steps.
pub const PEEL_STEPS: usize = 48;
/// A clear pixel round the label's picture, so its edges stay smooth once it is turned.
pub const PEEL_PAD: u32 = 1;
/// How far the drop shadow's soft edge reaches either side of the label's own.
pub const PEEL_SHADOW_PAD: u32 = 14;

/// Peeling up off the framing screen, the flight (which sets off while it is still peeling)
/// and smoothing it down, which starts the moment it touches the cart: it never hangs over the
/// slot first. The smoothing is the longest, as it rolls the fold back across half the label.
const LIFT_MS: Millis = 240;
const FLY_FROM_MS: Millis = 120;
const LAND_MS: Millis = 640;
pub const PEEL_MS: Millis = 1_100;
/// The framing screen is gone a little after the sticker is clear of it.
const BACKDROP_MS: Millis = LIFT_MS + 80;

/// Peeled up, it is a touch smaller than it was lying flat; high in the air, a touch larger.
const LIFT_SHRINK: f32 = 0.06;
const AIR_GROW: f32 = 0.06;
/// How far the flight bows off the straight line, as a share of the way it goes, at most.
const BOW: f32 = 0.3;
/// The tilt it peels up with, in radians: a few degrees.
const TILT: f32 = -0.06;
/// How far it banks into the bend of its flight, in radians, at most.
const BANK: f32 = 0.14;
/// How high it is held, as the distance its shadow falls from it.
const AIR: f32 = 18.0;
const SHADOW_ALPHA: f32 = 0.42;

/// How far the fold reaches at its most, as a share of the way to the label's middle.
const CURL: f32 = 1.0;
/// The flap stands up off the label rather than lying flat on it, so it is seen shortened:
/// this much of its length shows. Flat (1.0) it would hide all the art under it.
const STANDING: f32 = 0.5;
/// How wide the roll at the crease is at most, in pixels; a small fold is all roll.
const ROLL: f32 = 26.0;
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
/// slot on the cart. `bow`, -1 to 1, is which way and how far its flight bends off the
/// straight line (peel_bow), so it does not take the same path every time.
pub fn peel_pose(t: Millis, from: Rect, to: Rect, bow: f32) -> PeelPose {
    let span = |a: Millis, b: Millis| (t.saturating_sub(a) as f32 / (b - a) as f32).clamp(0.0, 1.0);
    let lift = ease(span(0, LIFT_MS));
    // Slow to leave, quick across, slowing right down into the slot: no stop above it.
    let u = span(FLY_FROM_MS, LAND_MS);
    let fly = u * u * u * (u * (6.0 * u - 15.0) + 10.0);
    // Smoothed from the crease out, slow to start and to finish, as a thumb would.
    let flat = span(LAND_MS, PEEL_MS);
    let flat = flat * flat * (3.0 - 2.0 * flat);

    // A curve from one slot's centre to the other's, bent to one side of the straight line.
    let centre = |(x, y, w, h): Rect| (x + w / 2.0, y + h / 2.0);
    let ((ax, ay), (bx, by)) = (centre(from), centre(to));
    let bend = bow.clamp(-1.0, 1.0) * BOW;
    let (cx, cy) = ((ax + bx) / 2.0 - (by - ay) * bend, (ay + by) / 2.0 + (bx - ax) * bend);
    let v = 1.0 - fly;
    let (px, py) = (v * v * ax + 2.0 * v * fly * cx + fly * fly * bx, v * v * ay + 2.0 * v * fly * cy + fly * fly * by);
    // Peeled up a touch smaller, larger high in the air, the slot's own size as it touches.
    let air = (std::f32::consts::PI * fly).sin();
    let k = 1.0 - LIFT_SHRINK * lift * (1.0 - fly);
    let grow = 1.0 + AIR_GROW * air;
    let (w, h) = ((from.2 * k + (to.2 - from.2) * fly) * grow, (from.3 * k + (to.3 - from.3) * fly) * grow);
    // Exactly there once there, so the hand-over to the cart's own label is to the pixel.
    let at = if u >= 1.0 { to } else { (px - w / 2.0, py - h / 2.0, w, h) };
    PeelPose {
        at,
        // The tilt it peels up with gives way to banking into the bend, and both straighten
        // out as it comes down.
        turn: if u >= 1.0 { 0.0 } else { TILT * lift * (1.0 - fly) - BANK * bow.clamp(-1.0, 1.0) * air },
        curl: lift * (1.0 - flat),
        lift: AIR * lift.max(air) * (1.0 - fly * 0.85) * (1.0 - flat),
        shadow: SHADOW_ALPHA * lift * (1.0 - flat),
        backdrop: 1.0 - ease(span(0, BACKDROP_MS)),
    }
}

/// A bend for a label's flight, -1 to 1, the same each time for the same game: never quite
/// straight, and not always to the same side.
pub fn peel_bow(stem: &str) -> f32 {
    let mut h: u32 = 0x811c_9dc5;
    for b in stem.bytes() {
        h = (h ^ b as u32).wrapping_mul(0x0100_0193);
    }
    // 0.4 to 1 either way: a gentle bend at the least.
    let size = 0.4 + 0.6 * ((h >> 8) % 1000) as f32 / 999.0;
    if h & 1 == 0 { size } else { -size }
}

/// Which of the baked curls to draw for `curl`.
pub fn peel_step(curl: f32) -> usize {
    (curl.clamp(0.0, 1.0) * (PEEL_STEPS - 1) as f32).round() as usize
}

/// The way the label folds: square to its diagonal, so a long label folds along its length
/// and a square one across the corner. A unit vector pointing at the bottom right corner.
fn fold_normal(w: f32, h: f32) -> (f32, f32) {
    let n = (w * w + h * h).sqrt().max(1.0);
    (w / n, h / n)
}

/// The clear edge the baked curls are given all round: enough that the folded half, which can
/// stand past the label's own edge, is never cut off. The same for every curl of a label.
pub fn peel_pad(w: u32, h: u32) -> u32 {
    let (wf, hf) = (w as f32, h as f32);
    let (nx, ny) = fold_normal(wf, hf);
    let c = (nx * wf + ny * hf) * (1.0 - CURL / 2.0);
    // The folded half is the label's corners past the fold, and where the fold crosses its
    // edges, mirrored: as far out as any of those lands is as far as it stands out.
    let mut points = vec![(0.0, 0.0), (wf, 0.0), (0.0, hf), (wf, hf)];
    if nx > 0.0 {
        points.extend([((c - ny * 0.0) / nx, 0.0), ((c - ny * hf) / nx, hf)]);
    }
    if ny > 0.0 {
        points.extend([(0.0, (c - nx * 0.0) / ny), (wf, (c - nx * wf) / ny)]);
    }
    let mut over: f32 = 0.0;
    for (x, y) in points {
        let e = c - (nx * x + ny * y);
        if e > 0.5 || x < -0.01 || y < -0.01 || x > wf + 0.01 || y > hf + 0.01 {
            continue;
        }
        let (mx, my) = (x + 2.0 * e * nx, y + 2.0 * e * ny);
        over = over.max(-mx).max(-my).max(mx - wf).max(my - hf);
    }
    // A pixel more for the fold's own smoothed edge, which can fall just past the label's.
    over.ceil() as u32 + 1 + PEEL_PAD
}

/// The label, `w` by `h` RGBA, peeled back by `curl` (0 flat, 1 most, halfway across): folded
/// back over itself along a line square to its diagonal, the back of the paper up and shaded
/// darkest at the crease as a roll would be, with the flap's shadow on the label beside it.
/// Comes with peel_pad clear all round.
pub fn peel_face(label: &[u8], w: u32, h: u32, curl: f32) -> CartFace {
    let pad = peel_pad(w, h);
    let (fw, fh) = (w + 2 * pad, h + 2 * pad);
    let mut rgba = vec![0u8; (fw * fh * 4) as usize];
    if label.len() < (w * h * 4) as usize {
        return CartFace { rgba, w: fw, h: fh };
    }
    let (wf, hf) = (w as f32, h as f32);
    let (nx, ny) = fold_normal(wf, hf);
    let deepest = nx * wf + ny * hf;
    let d = curl.clamp(0.0, 1.0) * CURL * deepest / 2.0;
    // The fold is the line n . p = c: past it the label has lifted away and folded back over.
    let c = deepest - d;
    let reach = (d * 0.25 + 3.0).min(24.0);
    let roll = d.clamp(1.0, ROLL);
    let inside = |x: f32, y: f32| {
        (x + 0.5).clamp(0.0, 1.0) * (wf - x + 0.5).clamp(0.0, 1.0) * (y + 0.5).clamp(0.0, 1.0) * (hf - y + 0.5).clamp(0.0, 1.0)
    };
    for oy in 0..fh {
        for ox in 0..fw {
            let dst = ((oy * fw + ox) * 4) as usize;
            let (x, y) = (ox as i64 - pad as i64, oy as i64 - pad as i64);
            let on_label = x >= 0 && y >= 0 && x < w as i64 && y < h as i64;
            let src = if on_label { ((y as u32 * w + x as u32) * 4) as usize } else { 0 };
            if d < 0.5 {
                if on_label {
                    rgba[dst..dst + 4].copy_from_slice(&label[src..src + 4]);
                }
                continue;
            }
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            // How far the pixel is on the label's side of the fold, smoothed over a pixel.
            let e = c - (nx * px + ny * py);
            let kept = (e + 0.5).clamp(0.0, 1.0);
            if kept <= 0.0 {
                continue;
            }
            // The flap is the peeled part turned back over the fold, standing up so it shows
            // shortened: this pixel is under it when the point that far up the flap is on the
            // label.
            let t = e + e / STANDING;
            let (mx, my) = (px + t * nx, py + t * ny);
            let flap = kept * inside(mx, my);
            let outside = (-mx).max(mx - wf).max(-my).max(my - hf).max(0.0);
            let shade = FLAP_SHADE * (1.0 - outside / reach).clamp(0.0, 1.0).powi(2) * (1.0 - flap);
            // From the crease out: dark in the roll, light where the paper lies flat.
            let along = (e / STANDING / roll).clamp(0.0, 1.0);
            let lum = 0.70 + 0.30 * (1.0 - (1.0 - along).powi(2));
            let front_a = if on_label { label[src + 3] as f32 / 255.0 * kept } else { 0.0 };
            let a = flap + front_a * (1.0 - flap);
            if a <= 0.0 {
                continue;
            }
            for ch in 0..3 {
                let front = if on_label { label[src + ch] as f32 * (1.0 - shade) } else { 0.0 };
                let v = (BACK[ch] * lum * flap + front * front_a * (1.0 - flap)) / a;
                rgba[dst + ch] = v.round().clamp(0.0, 255.0) as u8;
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
/// pixels in size, those still on their way to the GPU None, and `shadow` its drop shadow.
pub fn draw_peel(pose: &PeelPose, faces: &[Option<TexId>], shadow: TexId, label: (u32, u32), out: &mut Vec<Draw>) {
    let want = peel_step(pose.curl);
    // The curl wanted, or the nearest one there is while the rest are still going up.
    let nearest = (0..faces.len()).filter(|&i| faces[i].is_some()).min_by_key(|&i| i.abs_diff(want));
    let Some(tex) = nearest.and_then(|i| faces[i]) else {
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
    let pad = peel_pad(label.0, label.1) as f32;
    let (px, py) = (pad * kx, pad * ky);
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
        let start = peel_pose(0, FROM, TO, 0.7);
        assert_eq!((start.at, start.curl, start.shadow, start.backdrop), (FROM, 0.0, 0.0, 1.0));
        let end = peel_pose(PEEL_MS, FROM, TO, 0.7);
        assert_eq!((end.at, end.turn, end.curl, end.shadow, end.backdrop), (TO, 0.0, 0.0, 0.0, 0.0));
        assert_eq!(peel_pose(PEEL_MS + 500, FROM, TO, 0.7), end);
    }

    #[test]
    fn the_corner_is_curled_through_the_flight_and_smoothed_only_after_landing() {
        for t in [LIFT_MS, (LIFT_MS + LAND_MS) / 2, LAND_MS] {
            let p = peel_pose(t, FROM, TO, -0.5);
            assert!(p.curl > 0.99, "{t} ms: curl {}", p.curl);
        }
        let landed = peel_pose(LAND_MS, FROM, TO, -0.5);
        assert_eq!((landed.at, landed.turn), (TO, 0.0), "it lands flat and square before it is smoothed");
        let half = peel_pose((LAND_MS + PEEL_MS) / 2, FROM, TO, -0.5);
        assert!(half.curl > 0.0 && half.curl < 1.0);
    }

    #[test]
    fn it_comes_down_into_the_slot_without_stopping_over_it() {
        // Just before landing it is still on its way in, closing on the slot, not held over it.
        let centre = |(x, y, w, h): Rect| (x + w / 2.0, y + h / 2.0);
        let gap = |t| {
            let (a, b) = (centre(peel_pose(t, FROM, TO, 0.6).at), centre(TO));
            ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
        };
        let mut last = f32::MAX;
        for t in (FLY_FROM_MS..LAND_MS).step_by(40) {
            let g = gap(t);
            assert!(g <= last, "{t} ms: {g} is further than {last}");
            last = g;
        }
        let size = peel_pose(LAND_MS - 20, FROM, TO, 0.6).at;
        assert!((size.2 / TO.2 - 1.0).abs() < 0.01, "near enough its own size as it touches: {size:?}");
    }

    #[test]
    fn the_flight_bends_to_the_side_its_bow_says_and_differs_by_game() {
        let mid = (FLY_FROM_MS + LAND_MS) / 2;
        let (a, b) = (peel_pose(mid, FROM, TO, 1.0).at, peel_pose(mid, FROM, TO, -1.0).at);
        assert!((a.0 - b.0).abs() + (a.1 - b.1).abs() > 40.0, "{a:?} and {b:?} took the same path");
        let bows: Vec<f32> = ["Tetris", "Zelda", "Metroid", "Kirby", "Mother 3"].iter().map(|g| peel_bow(g)).collect();
        assert!(bows.iter().all(|b| (0.4..=1.0).contains(&b.abs())), "{bows:?}");
        assert!(bows.iter().any(|b| *b > 0.0) && bows.iter().any(|b| *b < 0.0), "always to one side: {bows:?}");
        assert_eq!(peel_bow("Tetris"), peel_bow("Tetris"));
    }

    #[test]
    fn a_flat_peel_is_the_label_untouched() {
        let label: Vec<u8> = (0..6 * 4 * 4).map(|i| (i * 7 % 256) as u8).collect();
        let face = peel_face(&label, 6, 4, 0.0);
        let pad = peel_pad(6, 4);
        assert_eq!((face.w, face.h), (6 + 2 * pad, 4 + 2 * pad));
        for y in 0..4 {
            let row = ((y + pad) * face.w + pad) as usize * 4;
            assert_eq!(&face.rgba[row..row + 24], &label[y as usize * 24..(y as usize + 1) * 24]);
        }
        assert!(face.rgba[..face.w as usize * 4].iter().all(|&v| v == 0), "the edge is clear");
    }

    #[test]
    fn peeled_halfway_the_far_half_is_gone_and_the_paper_back_lies_over_the_near_one() {
        let (w, h) = (100u32, 60u32);
        let label = [20u8, 40, 200, 255].repeat((w * h) as usize);
        let face = peel_face(&label, w, h, 1.0);
        let pad = peel_pad(w, h);
        let at = |x: u32, y: u32| {
            let i = (((y + pad) * face.w + x + pad) * 4) as usize;
            [face.rgba[i], face.rgba[i + 1], face.rgba[i + 2], face.rgba[i + 3]]
        };
        assert_eq!(at(w - 1, h - 1)[3], 0, "the far corner has lifted away");
        assert_eq!(at(w - 5, 3)[3], 0, "and so has the half it was on");
        let back = at(40, 30);
        assert!(back[3] == 255 && back[0] > 150 && back[0] > back[2] - 30, "paper back: {back:?}");
        assert_eq!(at(3, h - 3), [20, 40, 200, 255], "where the fold does not reach, the label as it was");
    }

    #[test]
    fn the_folded_half_is_never_cut_off_by_the_edge_of_the_face() {
        // A long label, a tall one and a square one: nothing is drawn on the outermost pixels
        // at any curl, so the fold always had room.
        for (w, h) in [(240u32, 52u32), (60, 150), (90, 90)] {
            let label = [200u8, 50, 50, 255].repeat((w * h) as usize);
            for i in 0..PEEL_STEPS {
                let face = peel_face(&label, w, h, i as f32 / (PEEL_STEPS - 1) as f32);
                let px = |x: u32, y: u32| face.rgba[((y * face.w + x) * 4 + 3) as usize];
                for x in 0..face.w {
                    assert_eq!((px(x, 0), px(x, face.h - 1)), (0, 0), "{w}x{h} step {i}");
                }
                for y in 0..face.h {
                    assert_eq!((px(0, y), px(face.w - 1, y)), (0, 0), "{w}x{h} step {i}");
                }
            }
        }
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
