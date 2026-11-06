//! The cart menu: START on a cart on the shelf (a GBA cart opens the core picker instead, where
//! Up/Down change its plastic). The cart stands large on the left, wearing the choice live; on
//! the right its plastic (colour, finish: Game Boy paks) and shape can be changed. NES and SNES
//! carts only change shape, as they came in their system's grey. A saves, B puts it all back.
use crate::draw::{Draw, TexId, OUT_H, OUT_W};
use crate::plate::{arrows_hint_face, centred_hints, hint_face, UndoFace, HINT_H, LEGEND_GAP, LEGEND_Y};
use crate::power_menu::{MENU_H, MENU_INK};
use crate::shell::{shell_presets, Finish, Shell};
use crate::slot_chrome::{edge, opening};
use crate::text;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum CartRow {
    Colour,
    Finish,
    Shape,
    Reset,
}

impl CartRow {
    pub const ALL: [CartRow; 4] = [CartRow::Colour, CartRow::Finish, CartRow::Shape, CartRow::Reset];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn label(self) -> &'static str {
        match self {
            CartRow::Colour => "COLOUR",
            CartRow::Finish => "FINISH",
            CartRow::Shape => "SHAPE",
            CartRow::Reset => "RESET TO ORIGINAL",
        }
    }

    /// A acts on this row instead of saving.
    pub fn acts(self) -> bool {
        self == CartRow::Reset
    }
}

/// The finishes a cart can be moulded in, in the order Left/Right steps through them.
pub const FINISHES: [Finish; 3] = [Finish::Solid, Finish::Translucent, Finish::Glitter];

pub fn finish_name(finish: Finish) -> &'static str {
    match finish {
        Finish::Solid => "Solid",
        Finish::Translucent => "See-through",
        Finish::Glitter => "Glitter",
        Finish::Sheen => "Gold",
    }
}

/// Shapes a Game Boy pak can take: the game's own, the original pak, the Color pak.
pub const SHAPES: [&str; 3] = ["Original", "Game Boy", "Game Boy Color"];

/// Shapes a SNES cart can take: the game's own, the US cart, the PAL / Super Famicom cart.
/// Their faces follow SHAPES' in CartMenuFaces::shapes.
pub const SNES_SHAPES: [&str; 3] = ["Original", "US", "Europe / Japan"];

/// Looks a NES cart can take: the game's own, the grey NES cart, a Famicom plastic, or
/// Tengen's own unlicensed black shell. Their faces follow SNES_SHAPES'.
pub const NES_SHAPES: [&str; 4] = ["Original", "NES", "Famicom", "Tengen"];

const fn solid(name: &'static str, colour: [u8; 3]) -> (&'static str, Shell) {
    (name, Shell { colour, finish: Finish::Solid })
}

const PLAIN: [(&str, Shell); 12] = [
    solid("Red", [0xc4, 0x2b, 0x2b]),
    solid("Orange", [0xe0, 0x78, 0x26]),
    solid("Yellow", [0xe8, 0xc2, 0x2e]),
    solid("Green", [0x3b, 0x8f, 0x4c]),
    solid("Teal", [0x1f, 0x9f, 0xa6]),
    solid("Blue", [0x2f, 0x5a, 0xb0]),
    solid("Purple", [0x6b, 0x4c, 0xa8]),
    solid("Pink", [0xe2, 0x8c, 0xa9]),
    solid("White", [0xe8, 0xe6, 0xe0]),
    solid("Grey", [0x9a, 0x99, 0x96]),
    solid("Black", [0x2b, 0x2a, 0x2e]),
    solid("Gold", [0xc9, 0xa2, 0x3a]),
];

/// The colours Left/Right steps through: the game's own first (None), then plain colours,
/// then the special-edition plastics, each with the finish it came in.
pub fn colour_choices() -> Vec<(&'static str, Option<Shell>)> {
    let mut out = vec![("Original", None)];
    out.extend(PLAIN.iter().map(|(n, s)| (*n, Some(*s))));
    out.extend(
        crate::shell::FAMICOM_NAMES.iter().zip(crate::shell::FAMICOM_SHELLS).map(|(n, s)| (*n, Some(s))),
    );
    for (name, shell) in shell_presets() {
        if !out.iter().any(|(_, s)| *s == Some(shell)) {
            out.push((name, Some(shell)));
        }
    }
    out
}

pub struct CartMenuFaces {
    pub labels: Vec<(TexId, u32, u32)>,
    /// [dim, lit] for each colour_choices() name (then "Custom"), FINISHES, and SHAPES then
    /// SNES_SHAPES then NES_SHAPES.
    pub colours: Vec<[(TexId, u32, u32); 2]>,
    pub finishes: Vec<[(TexId, u32, u32); 2]>,
    pub shapes: Vec<[(TexId, u32, u32); 2]>,
    pub carets: [(TexId, u32, u32); 2],
    /// Cancel, change, save, reset, search (the label row's Y).
    pub legend: [(TexId, u32); 5],
}

/// What the menu shows: the rows this cart has, the one selected, each row's value (an
/// index into its faces) and the cart's own face to stand on the left.
pub struct CartMenuView<'a> {
    pub rows: &'a [CartRow],
    pub row: CartRow,
    pub colour: usize,
    pub finish: usize,
    pub shape: usize,
    pub cart: Option<(TexId, u32, u32)>,
    /// While a label is on its way: the cart's empty label slot, in its plastic, under the cart.
    pub recess: Option<([f32; 4], crate::Rect)>,
    pub faces: Option<&'a CartMenuFaces>,
    /// Whether a label can be searched for at all: Y is greyed out without Wi-Fi.
    pub online: bool,
}

const PANEL_X: f32 = 300.0;
const PANEL_RIGHT: f32 = OUT_W as f32 - 20.0;
const PITCH: f32 = 50.0;
const VALUE_W: f32 = 170.0;
const TEXT_PX: f32 = 22.0;
const TEXT_MIN_PX: f32 = 14.0;
const CARET_GAP: f32 = 8.0;
const DIM_INK: [u8; 3] = [0x9a, 0x9a, 0xa4];
/// A key that cannot be pressed just now: there but plainly not for the taking.
pub const GREYED: f32 = 0.35;
/// Where the cart stands: left of the rows, above the legend.
const CART_BOX: [f32; 4] = [20.0, 40.0, PANEL_X - 40.0, LEGEND_Y - 60.0];

impl CartMenuView<'_> {
    pub fn draw(&self, out: &mut Vec<Draw>) {
        out.push(Draw::Rect { x: 0.0, y: 0.0, w: OUT_W as f32, h: OUT_H as f32, colour: opening() });
        if let Some((colour, (x, y, w, h))) = self.recess {
            out.push(Draw::Rect { x, y, w, h, colour });
        }
        if let Some((tex, w, h)) = self.cart {
            let (x, y, w, h) = cart_menu_cart_at(w, h);
            out.push(Draw::Tex { x, y, w, h, tex, alpha: 1.0 });
        }
        let top = rows_top(self.rows.len());
        if let Some(at) = self.rows.iter().position(|r| *r == self.row) {
            out.push(Draw::Rect {
                x: PANEL_X - 12.0,
                y: top + PITCH * at as f32 + 4.0,
                w: PANEL_RIGHT - PANEL_X + 24.0,
                h: PITCH - 8.0,
                colour: edge(),
            });
        }
        let Some(faces) = self.faces else {
            return;
        };
        for (at, &row) in self.rows.iter().enumerate() {
            let y = top + PITCH * at as f32 + (PITCH - MENU_H as f32) / 2.0;
            let lit = row == self.row;
            if let Some(&(tex, w, h)) = faces.labels.get(row.index()) {
                push(out, tex, PANEL_X, y, w, h);
            }
            let value = match row {
                CartRow::Colour => faces.colours.get(self.colour),
                CartRow::Finish => faces.finishes.get(self.finish),
                CartRow::Shape => faces.shapes.get(self.shape),
                CartRow::Reset => None,
            };
            let Some(&value) = value else {
                continue;
            };
            let (tex, w, h) = value[lit as usize];
            if !lit || row.acts() {
                push(out, tex, PANEL_RIGHT - w as f32, y, w, h);
                continue;
            }
            let [(left, lw, lh), (right, rw, rh)] = faces.carets;
            let rx = PANEL_RIGHT - rw as f32;
            push(out, right, rx, y, rw, rh);
            let vx = rx - CARET_GAP - w as f32;
            push(out, tex, vx, y, w, h);
            push(out, left, vx - CARET_GAP - lw as f32, y, lw, lh);
        }
        let [cancel, change, save, reset, search] = faces.legend;
        // Y is on every row, so it keeps its place in the line whether it can be used or not.
        let mut hints = [cancel; 4];
        let (shown, y_at) = match self.row {
            CartRow::Reset => {
                (hints[1], hints[2]) = (search, reset);
                (3, 1)
            }
            _ => {
                (hints[1], hints[2], hints[3]) = (change, search, save);
                (4, 2)
            }
        };
        for (i, (tex, w, x)) in centred_hints(&hints[..shown], LEGEND_GAP).into_iter().enumerate() {
            let greyed = i == y_at && !self.online;
            out.push(Draw::Tex {
                x: x.round(),
                y: LEGEND_Y,
                w: w as f32,
                h: HINT_H as f32,
                tex,
                alpha: if greyed { GREYED } else { 1.0 },
            });
        }
    }
}

/// Where a cart `w` by `h` stands in the menu: as large as fits left of the rows.
pub fn cart_menu_cart_at(w: u32, h: u32) -> crate::label_adjust::Rect {
    let [bx, by, bw, bh] = CART_BOX;
    let k = (bw / w.max(1) as f32).min(bh / h.max(1) as f32);
    let (cw, ch) = (w as f32 * k, h as f32 * k);
    ((bx + (bw - cw) / 2.0).round(), (by + (bh - ch) / 2.0).round(), cw, ch)
}

fn rows_top(rows: usize) -> f32 {
    (LEGEND_Y - PITCH * rows as f32) / 2.0
}

fn push(out: &mut Vec<Draw>, tex: TexId, x: f32, y: f32, w: u32, h: u32) {
    out.push(Draw::Tex { x: x.round(), y: y.round(), w: w as f32, h: h as f32, tex, alpha: 1.0 });
}

/// A row label: always lit.
pub fn cart_menu_label_face(row: CartRow) -> UndoFace {
    text_face(row.label(), MENU_INK, PANEL_RIGHT - PANEL_X)
}

/// A value: dim, or lit on the selected row; shrunk to fit beside its label.
pub fn cart_menu_value_face(value: &str, lit: bool) -> UndoFace {
    text_face(value, if lit { MENU_INK } else { DIM_INK }, VALUE_W)
}


pub fn cart_menu_legend_faces() -> [UndoFace; 5] {
    [
        hint_face("B", "Cancel"),
        arrows_hint_face("Change"),
        hint_face("A", "Save"),
        hint_face("A", "Reset"),
        hint_face("Y", "Label"),
    ]
}

fn text_face(label: &str, colour: [u8; 3], max_w: f32) -> UndoFace {
    let Some(font) = text::label_font() else {
        return UndoFace { rgba: Vec::new(), w: 0, h: 0 };
    };
    let layout = text::fit(font, label, max_w, 1, TEXT_PX, TEXT_MIN_PX);
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
