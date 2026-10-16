//! Framing a picked label by hand: the frame stands still in the middle of the screen and the
//! picture moves under it, the way a profile picture is set. The d-pad moves it, L and R zoom,
//! A keeps what is inside the frame and B goes back to the grid. Nothing is cut until A.
use crate::draw::{Draw, TexId, OUT_H, OUT_W};
use crate::plate::{centred_hints, hint_face, UndoFace, HINT_H, LEGEND_GAP};
use crate::power_menu::{MENU_H, MENU_INK};
use crate::slot_chrome::opening;
use crate::text;

/// The frame is drawn no larger than this, keeping the label's own shape.
pub const FRAME_MAX_W: f32 = 380.0;
pub const FRAME_MAX_H: f32 = 270.0;
const FRAME_CY: f32 = 190.0;
const CAPTION_Y: f32 = 374.0;
const LEGEND_Y: f32 = 427.0;
const EDGE: f32 = 2.0;
const TEXT_PX: f32 = 20.0;
const TEXT_MIN_PX: f32 = 13.0;
/// What is outside the frame is still shown, but darkened, so the frame reads at a glance.
const SHADE: [f32; 4] = [0.0, 0.0, 0.0, 0.62];
const FRAME_INK: [f32; 4] = [1.0, 1.0, 1.0, 0.95];

/// The frame on screen for a label `w` x `h` across: as big as it can be without changing shape.
pub fn frame_box(w: u32, h: u32) -> (f32, f32, f32, f32) {
    let (w, h) = (w.max(1) as f32, h.max(1) as f32);
    let k = (FRAME_MAX_W / w).min(FRAME_MAX_H / h);
    let (fw, fh) = (w * k, h * k);
    (((OUT_W as f32 - fw) / 2.0).round(), (FRAME_CY - fh / 2.0).round(), fw.round(), fh.round())
}

pub struct LabelAdjustView {
    /// The picture, where it sits on screen now: texture, x, y, width, height.
    pub picture: Option<(TexId, f32, f32, f32, f32)>,
    /// The frame it is being moved under: x, y, width, height.
    pub frame: (f32, f32, f32, f32),
    pub caption: Option<(TexId, u32, u32)>,
    /// Back, move, zoom, keep.
    pub legend: Option<[(TexId, u32); 4]>,
}

impl LabelAdjustView {
    pub fn draw(&self, out: &mut Vec<Draw>) {
        out.push(Draw::Rect { x: 0.0, y: 0.0, w: OUT_W as f32, h: OUT_H as f32, colour: opening() });
        if let Some((tex, x, y, w, h)) = self.picture {
            out.push(Draw::Tex { x, y, w, h, tex, alpha: 1.0 });
        }
        let (fx, fy, fw, fh) = self.frame;
        // Everything outside the frame, in four pieces around it.
        let (sw, sh) = (OUT_W as f32, OUT_H as f32);
        for (x, y, w, h) in
            [(0.0, 0.0, sw, fy), (0.0, fy + fh, sw, sh - fy - fh), (0.0, fy, fx, fh), (fx + fw, fy, sw - fx - fw, fh)]
        {
            if w > 0.0 && h > 0.0 {
                out.push(Draw::Rect { x, y, w, h, colour: SHADE });
            }
        }
        for (x, y, w, h) in [
            (fx, fy, fw, EDGE),
            (fx, fy + fh - EDGE, fw, EDGE),
            (fx, fy, EDGE, fh),
            (fx + fw - EDGE, fy, EDGE, fh),
        ] {
            out.push(Draw::Rect { x, y, w, h, colour: FRAME_INK });
        }
        if let Some((tex, w, h)) = self.caption {
            let x = ((OUT_W as f32 - w as f32) / 2.0).round();
            out.push(Draw::Tex { x, y: CAPTION_Y, w: w as f32, h: h as f32, tex, alpha: 1.0 });
        }
        if let Some(legend) = self.legend {
            for (tex, w, x) in centred_hints(&legend, LEGEND_GAP) {
                out.push(Draw::Tex { x, y: LEGEND_Y, w: w as f32, h: HINT_H as f32, tex, alpha: 1.0 });
            }
        }
    }
}

pub fn label_adjust_caption_face(words: &str) -> UndoFace {
    let Some(font) = text::label_font() else {
        return UndoFace { rgba: Vec::new(), w: 0, h: 0 };
    };
    let layout = text::fit(font, words, OUT_W as f32 - 48.0, 1, TEXT_PX, TEXT_MIN_PX);
    let set = layout
        .lines
        .iter()
        .map(|l| text::line_width(font, l, layout.px, layout.tracking))
        .fold(0.0, f32::max);
    let w = (set.ceil() as u32).max(1);
    let mut rgba = vec![0u8; (w * MENU_H * 4) as usize];
    text::draw_centred(&mut rgba, w, MENU_H, &layout, MENU_INK);
    UndoFace { rgba, w, h: MENU_H }
}

pub fn label_adjust_legend_faces() -> [UndoFace; 4] {
    [hint_face("B", "Back"), hint_face("+", "Move"), hint_face("L / R", "Zoom"), hint_face("A", "Keep")]
}
