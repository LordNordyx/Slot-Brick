//! Framing a picked label by hand. The cart stands in the middle of the screen with its label
//! slot clear, and the picture lies behind it, so what shows through the slot is the label as
//! the cart will wear it. The d-pad moves the picture, L and the d-pad stretch it, R evens its
//! shape back up and Y puts it back. Nothing is cut until A.
use crate::draw::{Draw, TexId, OUT_H, OUT_W};
use crate::plate::{centred_hints, hint_face, UndoFace, HINT_H, LEGEND_GAP};
use crate::power_menu::{MENU_H, MENU_INK};
use crate::slot_chrome::opening;
use crate::text;

/// The cart is drawn no larger than this, and the label slot in it is the frame.
pub const CART_MAX_W: f32 = 430.0;
pub const CART_MAX_H: f32 = 300.0;
const CART_CY: f32 = 185.0;
const CAPTION_Y: f32 = 374.0;
const LEGEND_Y: f32 = 427.0;
const TEXT_PX: f32 = 20.0;
const TEXT_MIN_PX: f32 = 13.0;
/// What is outside the slot is still shown, but darkened, so the slot reads at a glance.
const SHADE: [f32; 4] = [0.0, 0.0, 0.0, 0.62];

/// Where a cart `w` x `h` across stands on screen: as big as it goes without changing shape.
pub fn cart_stand(w: u32, h: u32) -> (f32, f32, f32, f32) {
    let (w, h) = (w.max(1) as f32, h.max(1) as f32);
    let k = (CART_MAX_W / w).min(CART_MAX_H / h);
    let (cw, ch) = (w * k, h * k);
    (((OUT_W as f32 - cw) / 2.0).round(), (CART_CY - ch / 2.0).round(), cw.round(), ch.round())
}

pub struct LabelAdjustView {
    /// The picture, where it sits on screen now: texture, x, y, width, height.
    pub picture: Option<(TexId, f32, f32, f32, f32)>,
    /// The cart, its label slot clear, laid over the picture: texture and where it stands.
    pub cart: Option<(TexId, f32, f32, f32, f32)>,
    /// The slot the picture is being lined up in: x, y, width, height.
    pub frame: (f32, f32, f32, f32),
    pub caption: Option<(TexId, u32, u32)>,
    /// Back, move, size, even up, keep.
    pub legend: Option<[(TexId, u32); 5]>,
}

impl LabelAdjustView {
    pub fn draw(&self, out: &mut Vec<Draw>) {
        out.push(Draw::Rect { x: 0.0, y: 0.0, w: OUT_W as f32, h: OUT_H as f32, colour: opening() });
        if let Some((tex, x, y, w, h)) = self.picture {
            out.push(Draw::Tex { x, y, w, h, tex, alpha: 1.0 });
        }
        // Everything but the slot, darkened, in four pieces around it. The cart covers most of
        // this; what is left is the picture spilling out past the cart's own edges.
        let (fx, fy, fw, fh) = self.frame;
        let (sw, sh) = (OUT_W as f32, OUT_H as f32);
        for (x, y, w, h) in
            [(0.0, 0.0, sw, fy), (0.0, fy + fh, sw, sh - fy - fh), (0.0, fy, fx, fh), (fx + fw, fy, sw - fx - fw, fh)]
        {
            if w > 0.0 && h > 0.0 {
                out.push(Draw::Rect { x, y, w, h, colour: SHADE });
            }
        }
        // The cart last, so the picture shows through its slot and nowhere else.
        if let Some((tex, x, y, w, h)) = self.cart {
            out.push(Draw::Tex { x, y, w, h, tex, alpha: 1.0 });
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

pub fn label_adjust_legend_faces() -> [UndoFace; 5] {
    [
        hint_face("B", "Back"),
        hint_face("+", "Move"),
        hint_face("L +", "Size"),
        hint_face("R", "Even"),
        hint_face("A", "Keep"),
    ]
}
