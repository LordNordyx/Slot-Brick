//! An overlay laid over a game: a picture the size of the screen with a clear window, found by
//! looking for its fully clear pixels, that the game is fitted into.
use std::path::Path;

use slot_store::Platform;

/// The largest overlay file read, and the largest picture: a screen-sized PNG is far smaller.
const FILE_MAX: u64 = 16 << 20;
const SIDE_MAX: u32 = 4096;
/// A window smaller than this share of the picture is not one a game could be shown in.
const WINDOW_MIN: f32 = 0.05;

pub struct Overlay {
    pub rgba: Vec<u8>,
    pub w: u32,
    pub h: u32,
    /// The clear window, as shares of the picture: left, top, width, height.
    pub window: [f32; 4],
}

/// Reads the overlay at `path`; None when it is not a PNG Slot can use as one.
pub fn load(path: &Path) -> Option<Overlay> {
    if std::fs::metadata(path).ok()?.len() > FILE_MAX {
        return None;
    }
    let (rgba, w, h) = crate::cart_art::decode_rgba(&std::fs::read(path).ok()?)?;
    if w == 0 || h == 0 || w > SIDE_MAX || h > SIDE_MAX {
        return None;
    }
    let window = window(&rgba, w, h)?;
    Some(Overlay { rgba, w, h, window })
}

/// The box round the picture's fully clear pixels, as shares of its size.
pub fn window(rgba: &[u8], w: u32, h: u32) -> Option<[f32; 4]> {
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
    for y in 0..h {
        for x in 0..w {
            if rgba.get(((y * w + x) * 4 + 3) as usize) == Some(&0) {
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
            }
        }
    }
    if x0 > x1 {
        return None;
    }
    let (fw, fh) = (w as f32, h as f32);
    let found = [x0 as f32 / fw, y0 as f32 / fh, (x1 - x0 + 1) as f32 / fw, (y1 - y0 + 1) as f32 / fh];
    (found[2] * found[3] >= WINDOW_MIN).then_some(found)
}

/// The part of the frame canvas to show so `platform`'s picture fills `window` (shares of the
/// screen) as far as it can without changing shape, centred in it. Outside the picture the
/// screen shows black, under the overlay.
pub fn source_rect(platform: Platform, window: [f32; 4]) -> [f32; 4] {
    let (w, h) = platform.picture();
    let (cw, ch) = platform.canvas();
    let (sw, sh) = (slot_gfx::OUT_W as f32, slot_gfx::OUT_H as f32);
    let [wx, wy, ww, wh] = window;
    // Screen pixels to a picture pixel, and where the picture lands on the screen.
    let k = (ww * sw / w as f32).min(wh * sh / h as f32).max(f32::EPSILON);
    let (dx, dy) = (wx * sw + (ww * sw - w as f32 * k) / 2.0, wy * sh + (wh * sh - h as f32 * k) / 2.0);
    // The picture sits in the middle of the canvas.
    let (px, py) = ((cw - w) as f32 / 2.0, (ch - h) as f32 / 2.0);
    [(px - dx / k) / cw as f32, (py - dy / k) / ch as f32, sw / k / cw as f32, sh / k / ch as f32]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picture(w: u32, h: u32, clear: (u32, u32, u32, u32)) -> Vec<u8> {
        let mut rgba = vec![255u8; (w * h * 4) as usize];
        for y in clear.1..clear.1 + clear.3 {
            for x in clear.0..clear.0 + clear.2 {
                rgba[((y * w + x) * 4 + 3) as usize] = 0;
            }
        }
        rgba
    }

    #[test]
    fn the_window_is_the_box_round_the_clear_pixels() {
        let rgba = picture(100, 80, (10, 20, 50, 40));
        assert_eq!(window(&rgba, 100, 80), Some([0.1, 0.25, 0.5, 0.5]));
        assert_eq!(window(&picture(100, 80, (0, 0, 0, 0)), 100, 80), None, "no window");
        assert_eq!(window(&picture(100, 80, (5, 5, 2, 2)), 100, 80), None, "a speck is not a window");
    }

    #[test]
    fn the_picture_fills_the_window_keeping_its_shape_and_centred() {
        let (sw, sh) = (slot_gfx::OUT_W as f32, slot_gfx::OUT_H as f32);
        for (platform, window) in [
            (Platform::Gb, [0.125, 0.05, 0.75, 0.9]),
            (Platform::Gba, [0.0, 0.0, 1.0, 0.86]),
            (Platform::Snes, [0.2, 0.1, 0.6, 0.8]),
        ] {
            let [sx, sy, swid, shei] = source_rect(platform, window);
            let (w, h) = platform.picture();
            let (cw, ch) = platform.canvas();
            // Where the picture's corners land on the screen.
            let to_x = |cx: f32| (cx / cw as f32 - sx) / swid * sw;
            let to_y = |cy: f32| (cy / ch as f32 - sy) / shei * sh;
            let (px, py) = ((cw - w) as f32 / 2.0, (ch - h) as f32 / 2.0);
            let (l, t, r, b) = (to_x(px), to_y(py), to_x(px + w as f32), to_y(py + h as f32));
            let near = |a: f32, b: f32| (a - b).abs() < 0.01;
            assert!(near((r - l) / (b - t), w as f32 / h as f32), "{platform:?} changed shape");
            let [wx, wy, ww, wh] = window.map(|v| v);
            let (wl, wt, wr, wb) = (wx * sw, wy * sh, (wx + ww) * sw, (wy + wh) * sh);
            assert!(l >= wl - 0.01 && t >= wt - 0.01 && r <= wr + 0.01 && b <= wb + 0.01, "{platform:?} spills out");
            assert!(near(r - l, wr - wl) || near(b - t, wb - wt), "{platform:?} does not fill the window");
            assert!(near(l - wl, wr - r) && near(t - wt, wb - b), "{platform:?} is not centred");
        }
    }
}
