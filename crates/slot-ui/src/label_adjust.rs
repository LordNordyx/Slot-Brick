//! Framing a picked label by hand. The cart stands in the middle of the screen with its label
//! slot clear, and the picture lies behind it, so what shows through the slot is the label as
//! the cart will wear it. The d-pad moves the picture, L and the d-pad stretch it, R evens its
//! shape back up and Y puts it back. Nothing is cut until A.
use crate::draw::{Draw, TexId, OUT_H, OUT_W};
use crate::plate::{centred_hints, hint_face, UndoFace, CAPTION_Y, HINT_H, LEGEND_GAP, LEGEND_Y};
use crate::slot_chrome::opening;

/// The slot is drawn as large as this, and the cart around it at whatever size that comes to:
/// near enough to see the label properly, with only the lip of plastic round it in view and
/// the rest of the cart running off the screen.
pub const SLOT_MAX_W: f32 = 520.0;
pub const SLOT_MAX_H: f32 = 252.0;
/// Where the top of the slot sits, leaving the lip above it in view.
const SLOT_TOP: f32 = 74.0;
/// The words and the keys sit on a dark band, since the cart runs on behind them.
const PLATE_TOP: f32 = 352.0;
const PLATE: [f32; 4] = [0.0, 0.0, 0.0, 0.74];
/// What is outside the slot is still shown, but darkened, so the slot reads at a glance.
const SHADE: [f32; 4] = [0.0, 0.0, 0.0, 0.22];
/// The cart is laid over the picture not quite solid, so what is outside the slot still shows
/// faintly through the plastic: enough to see what the next press will bring into the slot,
/// not enough to be mistaken for what will be worn.
const CART_ALPHA: f32 = 0.72;

/// A place on the screen: x, y, width, height.
pub type Rect = (f32, f32, f32, f32);

/// Where the cart stands and where its slot lands, with the slot drawn as large as it goes.
/// `face` is the cart's size, `slot` the label's place on it.
pub fn slot_stand(face: (u32, u32), slot: (u32, u32, u32, u32)) -> (Rect, Rect) {
    let (lw, lh) = (slot.2.max(1) as f32, slot.3.max(1) as f32);
    let k = (SLOT_MAX_W / lw).min(SLOT_MAX_H / lh);
    let (sw, sh) = ((lw * k).round(), (lh * k).round());
    let (sx, sy) = (((OUT_W as f32 - sw) / 2.0).round(), SLOT_TOP);
    let cart = (
        (sx - slot.0 as f32 * k).round(),
        (sy - slot.1 as f32 * k).round(),
        (face.0.max(1) as f32 * k).round(),
        (face.1.max(1) as f32 * k).round(),
    );
    (cart, (sx, sy, sw, sh))
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
        // The cart last: solid in the slot, where nothing covers the picture, and near enough
        // solid elsewhere that the rest of it only ghosts through.
        if let Some((tex, x, y, w, h)) = self.cart {
            out.push(Draw::Tex { x, y, w, h, tex, alpha: CART_ALPHA });
        }
        // The cart runs on behind the words, so they get a band of their own to sit on.
        out.push(Draw::Rect { x: 0.0, y: PLATE_TOP, w: OUT_W as f32, h: OUT_H as f32 - PLATE_TOP, colour: PLATE });
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

pub fn label_adjust_legend_faces() -> [UndoFace; 5] {
    [
        hint_face("B", "Back"),
        hint_face("+", "Move"),
        hint_face("L +", "Size"),
        hint_face("R", "Even"),
        hint_face("A", "Keep"),
    ]
}
