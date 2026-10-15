//! The label grid: Y on the cart menu asks what to search for, and what the search finds fills
//! this screen. Every label is shown as it was cut, the one under the cursor ringed. A wears it
//! and goes back to the cart menu; B goes back leaving the cart as it was.
use crate::draw::{Draw, TexId, OUT_H, OUT_W};
use crate::plate::{arrows_hint_face, centred_hints, hint_face, UndoFace, HINT_H, LEGEND_GAP};
use crate::power_menu::{MENU_H, MENU_INK};
use crate::slot_chrome::{edge, opening};
use crate::text;

/// The grid's shape, and so how many labels a page holds.
pub const GRID_COLS: usize = 4;
pub const GRID_ROWS: usize = 3;
pub const PAGE: usize = GRID_COLS * GRID_ROWS;

const TITLE_Y: f32 = 16.0;
const GRID_X: f32 = 24.0;
const GRID_Y: f32 = 54.0;
const GRID_W: f32 = OUT_W as f32 - GRID_X * 2.0;
const GRID_H: f32 = 300.0;
/// Room around a label inside its cell, so the ring never touches it.
const CELL_PAD: f32 = 8.0;
const CAPTION_Y: f32 = 374.0;
const LEGEND_Y: f32 = 427.0;
const TEXT_PX: f32 = 20.0;
const TEXT_MIN_PX: f32 = 13.0;

pub struct LabelGridView<'a> {
    pub title: Option<(TexId, u32, u32)>,
    pub caption: Option<(TexId, u32, u32)>,
    /// This page's labels in order, `None` where one has not arrived yet.
    pub cells: &'a [Option<(TexId, u32, u32)>],
    /// The cell the cursor is on, counted within the page.
    pub at: usize,
    /// Back, move, keep.
    pub legend: Option<[(TexId, u32); 3]>,
}

impl LabelGridView<'_> {
    pub fn draw(&self, out: &mut Vec<Draw>) {
        out.push(Draw::Rect { x: 0.0, y: 0.0, w: OUT_W as f32, h: OUT_H as f32, colour: opening() });
        if let Some((tex, w, h)) = self.title {
            push(out, tex, (OUT_W as f32 - w as f32) / 2.0, TITLE_Y, w, h);
        }
        let (cw, ch) = (GRID_W / GRID_COLS as f32, GRID_H / GRID_ROWS as f32);
        for i in 0..PAGE {
            let (x, y) = (GRID_X + cw * (i % GRID_COLS) as f32, GRID_Y + ch * (i / GRID_COLS) as f32);
            if i == self.at {
                out.push(Draw::Rect { x: x.round(), y: y.round(), w: cw.round(), h: ch.round(), colour: edge() });
            }
            let Some(&Some((tex, w, h))) = self.cells.get(i) else {
                continue;
            };
            // Whole, and as wide or tall as its cell leaves room for: a label is judged by its
            // shape as much as its picture.
            let k = ((cw - CELL_PAD * 2.0) / w as f32).min((ch - CELL_PAD * 2.0) / h as f32);
            let (dw, dh) = (w as f32 * k, h as f32 * k);
            out.push(Draw::Tex {
                x: (x + (cw - dw) / 2.0).round(),
                y: (y + (ch - dh) / 2.0).round(),
                w: dw,
                h: dh,
                tex,
                alpha: 1.0,
            });
        }
        if let Some((tex, w, h)) = self.caption {
            push(out, tex, (OUT_W as f32 - w as f32) / 2.0, CAPTION_Y, w, h);
        }
        if let Some(legend) = self.legend {
            for (tex, w, x) in centred_hints(&legend, LEGEND_GAP) {
                push(out, tex, x, LEGEND_Y, w, HINT_H);
            }
        }
    }
}

fn push(out: &mut Vec<Draw>, tex: TexId, x: f32, y: f32, w: u32, h: u32) {
    out.push(Draw::Tex { x: x.round(), y: y.round(), w: w as f32, h: h as f32, tex, alpha: 1.0 });
}

/// Which label is under the cursor, and how many there are to look through.
pub fn label_grid_caption_face(words: &str) -> UndoFace {
    text_face(words, MENU_INK, OUT_W as f32 - 48.0)
}

pub fn label_grid_legend_faces() -> [UndoFace; 3] {
    [hint_face("B", "Back"), arrows_hint_face("Move"), hint_face("A", "Keep")]
}

fn text_face(words: &str, colour: [u8; 3], max_w: f32) -> UndoFace {
    let Some(font) = text::label_font() else {
        return UndoFace { rgba: Vec::new(), w: 0, h: 0 };
    };
    let layout = text::fit(font, words, max_w, 1, TEXT_PX, TEXT_MIN_PX);
    let set = layout
        .lines
        .iter()
        .map(|l| text::line_width(font, l, layout.px, layout.tracking))
        .fold(0.0, f32::max);
    let w = (set.ceil() as u32).max(1);
    let mut rgba = vec![0u8; (w * MENU_H * 4) as usize];
    text::draw_centred(&mut rgba, w, MENU_H, &layout, colour);
    UndoFace { rgba, w, h: MENU_H }
}
