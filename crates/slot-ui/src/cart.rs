use slot_store::gb::Class;
use slot_store::{Cart, Outline, Platform};

use crate::art;
use crate::shell::{japanese, shell_for, snes_pal, Finish, Shell};
use crate::silhouette::{
    cart_depth, cart_mask, detail_mask, gb_cart_depth, gb_cart_mask, gb_detail_mask,
    nes_cart_depth, nes_cart_mask, nes_detail_mask, snes_cart_depth, snes_cart_mask,
    snes_detail_mask, Detail, GbShell, NesShell, SnesShell,
};
use crate::text;

pub const CART_W: u32 = 360;
pub const CART_H: u32 = (CART_W * SEATED_H + SEATED_W / 2) / SEATED_W;

pub const GB_CART_W: u32 = 312;

pub const GB_CART_H: u32 = (GB_CART_W * GB_SEATED_H + SEATED_W / 2) / SEATED_W;

/// About a Game Boy pak's size on the shelf, so the carousel keeps its rhythm; narrower,
/// as the NES cart is the taller shape and the row has room for ~340px.
pub const NES_CART_W: u32 = 276;

pub const NES_CART_H: u32 = (NES_CART_W * NES_SEATED_H + SEATED_W / 2) / SEATED_W;

/// A GBA cart's width: both are wide, low carts.
pub const SNES_CART_W: u32 = 360;

pub const SNES_CART_H: u32 = (SNES_CART_W * SNES_SEATED_H + SEATED_W / 2) / SEATED_W;

pub const fn label_panel(w: u32, h: u32) -> (u32, u32, u32, u32) {
    (
        (w * 135 + 500) / 1000,
        (h * 231 + 500) / 1000,
        (w * 875 + 500) / 1000,
        (h * 882 + 500) / 1000,
    )
}

pub const LABEL_X: u32 = label_panel(CART_W, CART_H).0;
pub const LABEL_Y: u32 = label_panel(CART_W, CART_H).1;
pub const LABEL_W: u32 = label_panel(CART_W, CART_H).2 - LABEL_X;
pub const LABEL_H: u32 = label_panel(CART_W, CART_H).3 - LABEL_Y;

pub const fn gb_label_panel(w: u32, h: u32) -> (u32, u32, u32, u32) {
    (
        (w * 147 + 500) / 1000,
        (h * 291 + 500) / 1000,
        (w * 854 + 500) / 1000,
        (h * 868 + 500) / 1000,
    )
}

pub const GB_LABEL_X: u32 = gb_label_panel(GB_CART_W, GB_CART_H).0;
pub const GB_LABEL_Y: u32 = gb_label_panel(GB_CART_W, GB_CART_H).1;
pub const GB_LABEL_W: u32 = gb_label_panel(GB_CART_W, GB_CART_H).2 - GB_LABEL_X;
pub const GB_LABEL_H: u32 = gb_label_panel(GB_CART_W, GB_CART_H).3 - GB_LABEL_Y;

/// The portrait recess right of the grip, open to the top edge (nes_cart_detail.svg:
/// 93.2..207.8 of 240, 1..193.8 of 288).
pub const fn nes_label_panel(w: u32, h: u32) -> (u32, u32, u32, u32) {
    (
        (w * 388 + 500) / 1000,
        (h * 4 + 500) / 1000,
        (w * 866 + 500) / 1000,
        (h * 673 + 500) / 1000,
    )
}

pub const NES_LABEL_X: u32 = nes_label_panel(NES_CART_W, NES_CART_H).0;
pub const NES_LABEL_Y: u32 = nes_label_panel(NES_CART_W, NES_CART_H).1;
pub const NES_LABEL_W: u32 = nes_label_panel(NES_CART_W, NES_CART_H).2 - NES_LABEL_X;
pub const NES_LABEL_H: u32 = nes_label_panel(NES_CART_W, NES_CART_H).3 - NES_LABEL_Y;

/// The landscape recess across the top of the raised middle, open to the top edge
/// (snes_cart_detail.svg: 47..191.3 of 240, 0.5..66.7 of 157).
pub const fn snes_label_panel(w: u32, h: u32) -> (u32, u32, u32, u32) {
    (
        (w * 196 + 500) / 1000,
        (h * 3 + 500) / 1000,
        (w * 797 + 500) / 1000,
        (h * 425 + 500) / 1000,
    )
}

pub const SNES_LABEL_X: u32 = snes_label_panel(SNES_CART_W, SNES_CART_H).0;
pub const SNES_LABEL_Y: u32 = snes_label_panel(SNES_CART_W, SNES_CART_H).1;
pub const SNES_LABEL_W: u32 = snes_label_panel(SNES_CART_W, SNES_CART_H).2 - SNES_LABEL_X;
pub const SNES_LABEL_H: u32 = snes_label_panel(SNES_CART_W, SNES_CART_H).3 - SNES_LABEL_Y;

/// The PAL / Super Famicom cart's wider, lower recess (snes_pal_cart_detail.svg's face:
/// 29.5..210.8 of 240, 10.5..72.5 of 157).
pub const fn snes_pal_label_panel(w: u32, h: u32) -> (u32, u32, u32, u32) {
    (
        (w * 123 + 500) / 1000,
        (h * 67 + 500) / 1000,
        (w * 878 + 500) / 1000,
        (h * 462 + 500) / 1000,
    )
}

const SNES_PAL_LABEL: (u32, u32, u32, u32) = {
    let (x0, y0, x1, y1) = snes_pal_label_panel(SNES_CART_W, SNES_CART_H);
    (x0, y0, x1 - x0, y1 - y0)
};

/// The Famicom cart's landscape label (famicom_cart.svg's face: 27.1..212.9 of 240,
/// 160.7..256.9 of 288), shaped as cart_art.rs cuts it from pictures of the cart.
pub const fn famicom_label_panel(w: u32, h: u32) -> (u32, u32, u32, u32) {
    (
        (w * 113 + 500) / 1000,
        (h * 558 + 500) / 1000,
        (w * 887 + 500) / 1000,
        (h * 892 + 500) / 1000,
    )
}

const FAMICOM_LABEL: (u32, u32, u32, u32) = {
    let (x0, y0, x1, y1) = famicom_label_panel(NES_CART_W, NES_CART_H);
    (x0, y0, x1 - x0, y1 - y0)
};

const PAD: u32 = 10;
const MAX_LINES: usize = 3;
const MAX_PX: f32 = LABEL_H as f32 / (MAX_LINES as f32 * 1.36);
const GB_MAX_PX: f32 = (GB_LABEL_H - 2 * PAD) as f32 / MAX_LINES as f32;
const NES_MAX_PX: f32 = (NES_LABEL_H - 2 * PAD) as f32 / MAX_LINES as f32;
const FAMICOM_MAX_PX: f32 = FAMICOM_LABEL.3 as f32 / (MAX_LINES as f32 * 1.36);
const SNES_MAX_PX: f32 = SNES_LABEL_H as f32 / (MAX_LINES as f32 * 1.36);
const SNES_PAL_MAX_PX: f32 = SNES_PAL_LABEL.3 as f32 / (MAX_LINES as f32 * 1.36);
const MIN_PX: f32 = 10.0;

const RIM: u32 = (4 * CART_W + SEATED_W / 2) / SEATED_W;
const GB_RIM: u32 = (7 * GB_CART_W + SEATED_W / 2) / SEATED_W;
const NES_RIM: u32 = (7 * NES_CART_W + SEATED_W / 2) / SEATED_W;
const SNES_RIM: u32 = (4 * SNES_CART_W + SEATED_W / 2) / SEATED_W;

pub struct CartFace {
    pub rgba: Vec<u8>,
    pub w: u32,
    pub h: u32,
}

struct Spec {
    w: u32,
    h: u32,
    label: (u32, u32, u32, u32),
    mask: &'static [u8],
    depth: &'static [u8],
    detail: &'static Detail,
    max_px: f32,
    max_h: f32,
    rim: u32,
    board: Board,
}

struct Board {
    x: (f32, f32),
    top: f32,
    pins: (f32, f32),
    contacts_from: f32,
    traces_from: f32,
}

enum Shape {
    Gba,
    Gb(GbShell),
    Nes(NesShell),
    Snes(SnesShell),
}

pub fn gb_shell_of(cart: &Cart) -> Option<GbShell> {
    match cart.platform {
        Platform::Gba | Platform::Nes | Platform::Snes => None,
        Platform::Gb | Platform::Gbc => Some(match cart.shell.map(|c| c.outline) {
            Some(Outline::Notched) => GbShell::Notched,
            Some(Outline::Rounded) => GbShell::Rounded,
            _ => match slot_store::gb::class(&cart.rom) {
                Class::ColourOnly => GbShell::Rounded,
                Class::Original | Class::DualMode => GbShell::Notched,
            },
        }),
    }
}

/// A SNES cart's shape: as chosen in the cart menu (notched is the US cart, rounded the
/// PAL / Super Famicom one), else where the ROM's header says it was sold, else the region in
/// its name. Reads the ROM.
pub fn snes_shell_of(cart: &Cart) -> Option<SnesShell> {
    (cart.platform == Platform::Snes).then(|| match cart.shell.map(|c| c.outline) {
        Some(Outline::Notched) => SnesShell::Us,
        Some(Outline::Rounded) => SnesShell::Pal,
        _ => match slot_store::region::snes_pal_or_japan(&cart.rom).unwrap_or_else(|| snes_pal(&cart.stem)) {
            true => SnesShell::Pal,
            false => SnesShell::Us,
        },
    })
}

/// The cart's label picture at w x h: filling the label, but on the Famicom cart a tall (NES)
/// label is shown whole, as filling would cut most of it away.
pub fn fitted_label(cart: &Cart, w: u32, h: u32) -> Option<Vec<u8>> {
    let path = shown_label(cart)?;
    if nes_famicom_of(cart) {
        art::label_fit(path, w, h)
    } else {
        art::cover(path, w, h)
    }
}

/// `fitted_label` for a label picture already in memory, `sw` by `sh` RGBA: as the cart will
/// show it once it is written.
pub fn fitted_label_rgba(cart: &Cart, src: &[u8], sw: u32, sh: u32, w: u32, h: u32) -> Option<Vec<u8>> {
    if nes_famicom_of(cart) {
        art::label_fit_rgba(src, sw, sh, w, h)
    } else {
        art::cover_rgba(src, sw, sh, w, h)
    }
}

/// The label a cart wears: a NES game keeps one for each of its carts, so swapping the
/// shape swaps the picture; while it has only one, both carts show it.
pub fn shown_label(cart: &Cart) -> Option<&std::path::Path> {
    let (own, famicom) = (cart.label.as_deref(), cart.famicom_label.as_deref());
    if nes_famicom_of(cart) {
        famicom.or(own)
    } else {
        own.or(famicom)
    }
}

/// A NES game on the Famicom cart: as chosen in the cart menu (notched is the NES cart,
/// rounded the Famicom), else a ROM named as a Japanese release or on a Famicom-only board.
pub fn nes_famicom_of(cart: &Cart) -> bool {
    cart.platform == Platform::Nes
        && match cart.shell.map(|c| c.outline) {
            Some(Outline::Notched) => false,
            Some(Outline::Rounded) => true,
            _ => japanese(&cart.stem) || slot_store::region::nes_famicom_board(&cart.rom),
        }
}

fn shape_of(cart: &Cart) -> Shape {
    match (cart.platform, gb_shell_of(cart)) {
        (Platform::Nes, _) if nes_famicom_of(cart) => Shape::Nes(NesShell::Famicom),
        (Platform::Nes, _) => Shape::Nes(NesShell::Nes),
        (Platform::Snes, _) => Shape::Snes(snes_shell_of(cart).unwrap_or(SnesShell::Us)),
        (_, None) => Shape::Gba,
        (_, Some(shell)) => Shape::Gb(shell),
    }
}

fn spec(shape: Shape) -> Spec {
    match shape {
        Shape::Gba => Spec {
            w: CART_W,
            h: CART_H,
            label: (LABEL_X, LABEL_Y, LABEL_W, LABEL_H),
            mask: cart_mask(),
            depth: cart_depth(),
            detail: detail_mask(),
            max_px: MAX_PX,
            max_h: f32::INFINITY,
            rim: RIM,
            board: Board {
                x: (0.135, 0.875),
                top: LABEL_Y as f32 / CART_H as f32,
                pins: (0.17, 0.84),
                contacts_from: 0.85,
                traces_from: 0.76,
            },
        },
        Shape::Gb(shell) => Spec {
            w: GB_CART_W,
            h: GB_CART_H,
            label: (GB_LABEL_X, GB_LABEL_Y, GB_LABEL_W, GB_LABEL_H),
            mask: gb_cart_mask(shell),
            depth: gb_cart_depth(shell),
            detail: gb_detail_mask(shell),
            max_px: GB_MAX_PX,
            max_h: (GB_LABEL_H - 2 * PAD) as f32,
            rim: GB_RIM,
            board: Board {
                x: (0.075, 0.925),
                top: 0.07,
                pins: (0.1, 0.9),
                contacts_from: 0.905,
                traces_from: 0.84,
            },
        },
        Shape::Nes(shell) => Spec {
            w: NES_CART_W,
            h: NES_CART_H,
            label: match shell {
                NesShell::Nes => (NES_LABEL_X, NES_LABEL_Y, NES_LABEL_W, NES_LABEL_H),
                NesShell::Famicom => FAMICOM_LABEL,
            },
            mask: nes_cart_mask(shell),
            depth: nes_cart_depth(shell),
            detail: nes_detail_mask(shell),
            max_px: match shell {
                NesShell::Nes => NES_MAX_PX,
                NesShell::Famicom => FAMICOM_MAX_PX,
            },
            max_h: match shell {
                NesShell::Nes => (NES_LABEL_H - 2 * PAD) as f32,
                NesShell::Famicom => f32::INFINITY,
            },
            rim: NES_RIM,
            board: Board {
                x: (0.04, 0.96),
                top: 0.04,
                pins: (0.06, 0.94),
                contacts_from: 0.93,
                traces_from: 0.86,
            },
        },
        Shape::Snes(shell) => Spec {
            w: SNES_CART_W,
            h: SNES_CART_H,
            label: match shell {
                SnesShell::Us => (SNES_LABEL_X, SNES_LABEL_Y, SNES_LABEL_W, SNES_LABEL_H),
                SnesShell::Pal => SNES_PAL_LABEL,
            },
            mask: snes_cart_mask(shell),
            depth: snes_cart_depth(shell),
            detail: snes_detail_mask(shell),
            max_px: match shell {
                SnesShell::Us => SNES_MAX_PX,
                SnesShell::Pal => SNES_PAL_MAX_PX,
            },
            max_h: f32::INFINITY,
            rim: SNES_RIM,
            board: Board {
                x: (0.17, 0.83),
                top: 0.03,
                pins: (0.2, 0.8),
                contacts_from: 0.9,
                traces_from: 0.8,
            },
        },
    }
}

pub const SEATED_W: u32 = 240;
const SEATED_H: u32 = 141;
const GB_SEATED_H: u32 = 259;
/// A NES cart's face, connector foot included, is about 5:6.
const NES_SEATED_H: u32 = 288;
/// A US SNES cart's face is about 3:2, wide and low like a GBA's.
const SNES_SEATED_H: u32 = 157;

pub fn seated_box(platform: Platform) -> (u32, u32) {
    match platform {
        Platform::Gba => (SEATED_W, SEATED_H),
        Platform::Gb | Platform::Gbc => (SEATED_W, GB_SEATED_H),
        Platform::Nes => (SEATED_W, NES_SEATED_H),
        Platform::Snes => (SEATED_W, SNES_SEATED_H),
    }
}

pub fn cart_box(platform: Platform) -> (u32, u32) {
    let s = spec(match platform {
        Platform::Gba => Shape::Gba,
        Platform::Gb | Platform::Gbc => Shape::Gb(GbShell::Notched),
        Platform::Nes => Shape::Nes(NesShell::Nes),
        Platform::Snes => Shape::Snes(SnesShell::Us),
    });
    (s.w, s.h)
}

pub fn cart_shadow() -> CartFace {
    shadow(CART_W, CART_H, cart_mask())
}

pub fn gb_cart_shadow(shell: GbShell) -> CartFace {
    shadow(GB_CART_W, GB_CART_H, gb_cart_mask(shell))
}

pub fn nes_cart_shadow(shell: NesShell) -> CartFace {
    shadow(NES_CART_W, NES_CART_H, nes_cart_mask(shell))
}

pub fn snes_cart_shadow(shell: SnesShell) -> CartFace {
    shadow(SNES_CART_W, SNES_CART_H, snes_cart_mask(shell))
}

fn shadow(w: u32, h: u32, mask: &[u8]) -> CartFace {
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for cover in mask {
        rgba.extend_from_slice(&[0, 0, 0, *cover]);
    }
    CartFace { rgba, w, h }
}

pub fn cart_face(cart: &Cart) -> CartFace {
    let (lw, lh) = label_size(cart);
    let art = fitted_label(cart, lw, lh);
    cart_face_with(cart, art)
}

pub fn label_size(cart: &Cart) -> (u32, u32) {
    let (_, _, lw, lh) = spec(shape_of(cart)).label;
    (lw, lh)
}

pub fn cart_face_with(cart: &Cart, art: Option<Vec<u8>>) -> CartFace {
    let s = spec(shape_of(cart));
    let shell = shell_for(cart);
    let mut face = shell_face(&s, &shell);
    let label = match art {
        Some(rgba) => rgba,
        None => generated_label(&s, &label_text(cart)),
    };
    mould_detail(&s, &mut face);
    recess_label(&s, &mut face, &shell);
    paste_label(&s, &mut face, &label);
    clip_to_silhouette(&s, &mut face);
    face
}

/// The cart with nothing in its label slot: the slot is clear through to whatever lies behind
/// it. Framing a label draws the picture behind this, so it is seen through the very window it
/// will be worn in. Gives the slot's place on the face as well, to line the picture up with.
pub fn cart_cutout_face(cart: &Cart) -> (CartFace, (u32, u32, u32, u32)) {
    let s = spec(shape_of(cart));
    let shell = shell_for(cart);
    let mut face = shell_face(&s, &shell);
    mould_detail(&s, &mut face);
    recess_label(&s, &mut face, &shell);
    clip_to_silhouette(&s, &mut face);
    let (lx, ly, lw, lh) = s.label;
    for y in ly..(ly + lh).min(face.h) {
        for x in lx..(lx + lw).min(face.w) {
            face.rgba[((y * face.w + x) * 4 + 3) as usize] = 0;
        }
    }
    (face, (lx, ly, lw, lh))
}

fn clip_to_silhouette(s: &Spec, face: &mut CartFace) {
    for (px, cover) in face.rgba.chunks_exact_mut(4).zip(s.mask) {
        px[3] = ((px[3] as u32 * *cover as u32 + 127) / 255) as u8;
    }
}

pub fn label_colour(title: &str) -> [u8; 3] {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in title.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hsv_to_rgb((h % 360) as f32, 0.52, 0.74)
}

pub fn label_text(cart: &Cart) -> String {
    clean_label(&cart.stem)
}

pub fn clean_label(stem: &str) -> String {
    let mut bare = String::with_capacity(stem.len());
    let mut depth = 0u32;
    for ch in stem.chars() {
        match ch {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            _ if depth == 0 => bare.push(ch),
            _ => {}
        }
    }

    let mut out = String::with_capacity(bare.len());
    for word in bare.split_whitespace().filter(|w| *w != "-") {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    if out.is_empty() {
        stem.to_string()
    } else {
        out
    }
}

pub fn label_tags(stem: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0u32;
    let mut cur = String::new();
    for ch in stem.chars() {
        match ch {
            '(' | '[' => {
                depth += 1;
                if depth == 1 {
                    cur.clear();
                    continue;
                }
            }
            ')' | ']' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    let t = cur.trim();
                    if !t.is_empty() {
                        out.push(t.to_string());
                    }
                    continue;
                }
            }
            _ => {}
        }
        if depth >= 1 {
            cur.push(ch);
        }
    }
    out
}

fn shell_face(s: &Spec, shell: &Shell) -> CartFace {
    let mut rgba = Vec::with_capacity((s.w * s.h * 4) as usize);
    let edge = rim_colour(through(shell.colour, shell.colour));
    for (i, depth) in s.depth.iter().enumerate() {
        let (x, y) = (i as u32 % s.w, i as u32 / s.w);
        let c = match shell.finish {
            Finish::Solid => shell.colour,
            Finish::Sheen => chrome(shell.colour, y, s.h),
            Finish::Translucent | Finish::Glitter => {
                let body = through(shell.colour, inside(s, x, y).unwrap_or(shell.colour));
                let c = lerp(edge, body, (*depth as u32).min(s.rim), s.rim);
                if shell.finish == Finish::Glitter && fleck(x, y) {
                    lerp(c, [0xff; 3], 100, 255)
                } else {
                    c
                }
            }
        };
        rgba.extend_from_slice(&[c[0], c[1], c[2], 255]);
    }
    CartFace {
        rgba,
        w: s.w,
        h: s.h,
    }
}

const SURFACE: u32 = 150;

const BOARD: [u8; 3] = [0x2c, 0x96, 0x52];
const TRACE: [u8; 3] = [0x5a, 0xb4, 0x74];
const GOLD: [u8; 3] = [0xe6, 0xb4, 0x46];

fn through(plastic: [u8; 3], behind: [u8; 3]) -> [u8; 3] {
    let filter =
        |a: [u8; 3], b: [u8; 3]| [0, 1, 2].map(|c| (a[c] as u32 * b[c] as u32 / 255) as u8);
    lerp(
        filter(plastic, behind),
        filter(plastic, plastic),
        SURFACE,
        255,
    )
}

fn inside(s: &Spec, x: u32, y: u32) -> Option<[u8; 3]> {
    let b = &s.board;
    let (fx, fy) = (x as f32 / s.w as f32, y as f32 / s.h as f32);
    if !(b.x.0..b.x.1).contains(&fx) || fy < b.top {
        return None;
    }
    let (c0, c1) = b.pins;
    let pitch = (c1 - c0) / 32.0;
    let on_contact = (c0..c1).contains(&fx) && ((fx - c0) % pitch) < pitch * 0.6;
    if on_contact && fy >= b.contacts_from {
        return Some(GOLD);
    }
    if on_contact && ((fx - c0) % pitch) < pitch * 0.2 && fy >= b.traces_from {
        Some(TRACE)
    } else {
        Some(BOARD)
    }
}

/// Polished metal: the light does not lie flat on it, it runs across in bands, bright where
/// the plastic turns to the light and dark where it turns away. That banding is what reads as
/// metal; a flat fill of the same colour only reads as yellow. The bands run into one another
/// rather than meeting at a line, or it reads as stripes instead.
fn chrome(base: [u8; 3], y: u32, h: u32) -> [u8; 3] {
    // Down the face: a lit top, a dark band under it, the broad shine across the middle, then
    // dark again towards the foot. Positive turns it to the light, negative away.
    const BANDS: [(f32, f32); 7] =
        [(0.0, 0.55), (0.18, -0.34), (0.34, 0.05), (0.50, 0.82), (0.63, 0.20), (0.80, -0.40), (1.0, 0.30)];
    let t = y as f32 / h.max(1) as f32;
    let turn = BANDS
        .windows(2)
        .find(|w| t < w[1].0 || w[1].0 >= 1.0)
        .map(|w| {
            let ((t0, v0), (t1, v1)) = (w[0], w[1]);
            let k = ((t - t0) / (t1 - t0).max(f32::EPSILON)).clamp(0.0, 1.0);
            // Eased, so the bands swell into one another the way a reflection does.
            let k = k * k * (3.0 - 2.0 * k);
            v0 + (v1 - v0) * k
        })
        .unwrap_or(0.0);
    let (to, by) = match turn >= 0.0 {
        true => (HIGHLIGHT, turn),
        false => (SHADOW, -turn),
    };
    lerp(base, to, (by * 255.0) as u32, 255)
}

/// Where the light comes back off polished metal, and where it does not.
const HIGHLIGHT: [u8; 3] = [0xff, 0xf6, 0xd8];
const SHADOW: [u8; 3] = [0x4a, 0x33, 0x09];

fn fleck(x: u32, y: u32) -> bool {
    let mut h = x.wrapping_mul(0x9e37_79b9) ^ y.wrapping_mul(0x85eb_ca6b);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^= h >> 12;
    h.is_multiple_of(100)
}

fn rim_colour(base: [u8; 3]) -> [u8; 3] {
    let mean = (base[0] as u16 + base[1] as u16 + base[2] as u16) / 3;
    base.map(|c| {
        let grey = (3 * c as u16 + mean) / 4;
        (grey + (255 - grey) * 2 / 5) as u8
    })
}

fn lerp(a: [u8; 3], b: [u8; 3], num: u32, den: u32) -> [u8; 3] {
    let mut out = [0u8; 3];
    for c in 0..3 {
        out[c] = ((a[c] as u32 * (den - num) + b[c] as u32 * num) / den) as u8;
    }
    out
}

const BEVEL: u32 = 3;

fn mould_detail(s: &Spec, face: &mut CartFace) {
    let mix = |px: &mut [u8], to: [u8; 3], a: u32| {
        for c in 0..3 {
            px[c] = ((to[c] as u32 * a + px[c] as u32 * (255 - a) + 127) / 255) as u8;
        }
    };
    let sides = s.detail.shadow.iter().zip(&s.detail.highlight);
    for (px, (shade, light)) in face.rgba.chunks_exact_mut(4).zip(sides) {
        let under = [px[0], px[1], px[2]];
        if *shade > 0 {
            mix(px, under.map(|c| (c as f32 * 0.62) as u8), *shade as u32);
        }
        if *light > 0 {
            mix(
                px,
                under.map(|c| c + ((255 - c) as f32 * 0.24) as u8),
                *light as u32,
            );
        }
    }
}

fn recess_label(s: &Spec, face: &mut CartFace, shell: &Shell) {
    let shade = |c: [u8; 3], f: f32| -> [u8; 3] {
        [
            (c[0] as f32 * f).clamp(0.0, 255.0) as u8,
            (c[1] as f32 * f).clamp(0.0, 255.0) as u8,
            (c[2] as f32 * f).clamp(0.0, 255.0) as u8,
        ]
    };
    let dark = shade(shell.colour, 0.55);
    let lit = shade(shell.colour, 1.45);

    let (lx, ly, lw, lh) = s.label;
    let (w, h) = (s.w, s.h);
    let bevel = (BEVEL * w + SEATED_W / 2) / SEATED_W;
    // A recess may run to the cart's edge (the NES label is open to the top).
    let (x0, y0) = (lx.saturating_sub(bevel), ly.saturating_sub(bevel));
    let (x1, y1) = (lx + lw + bevel, ly + lh + bevel);
    // Sheen is solid plastic: only the see-through finishes light the recess from behind.
    let clear = matches!(shell.finish, Finish::Translucent | Finish::Glitter);
    let mut put = |x: u32, y: u32, upper: bool| {
        if x >= w || y >= h {
            return;
        }
        let d = ((y * w + x) * 4) as usize;
        let c = if clear {
            let under = [face.rgba[d], face.rgba[d + 1], face.rgba[d + 2]];
            shade(under, if upper { 0.7 } else { 1.18 })
        } else if upper {
            dark
        } else {
            lit
        };
        face.rgba[d] = c[0];
        face.rgba[d + 1] = c[1];
        face.rgba[d + 2] = c[2];
    };
    for y in y0..y1 {
        for x in x0..x1 {
            let inside = (lx..lx + lw).contains(&x) && (ly..ly + lh).contains(&y);
            if inside {
                continue;
            }
            let from_top = y.saturating_sub(y0);
            let from_left = x.saturating_sub(x0);
            let from_bottom = y1.saturating_sub(y + 1);
            let from_right = x1.saturating_sub(x + 1);
            let upper = from_top.min(from_left);
            let lower = from_bottom.min(from_right);
            put(x, y, upper <= lower);
        }
    }
}

fn paste_label(s: &Spec, face: &mut CartFace, label: &[u8]) {
    let (lx, ly, lw, lh) = s.label;
    for y in 0..lh {
        for x in 0..lw {
            let src = ((y * lw + x) * 4) as usize;
            let a = label[src + 3] as u32;
            if a == 0 {
                continue;
            }
            let d = (((y + ly) * s.w + x + lx) * 4) as usize;
            for c in 0..3 {
                face.rgba[d + c] =
                    ((label[src + c] as u32 * a + face.rgba[d + c] as u32 * (255 - a) + 127) / 255)
                        as u8;
            }
        }
    }
}

fn generated_label(s: &Spec, title: &str) -> Vec<u8> {
    let (_, _, lw, lh) = s.label;
    let bg = label_colour(title);
    let mut rgba = Vec::with_capacity((lw * lh * 4) as usize);
    for _ in 0..lw * lh {
        rgba.extend_from_slice(&[bg[0], bg[1], bg[2], 255]);
    }

    if let Some(font) = text::label_font() {
        let layout = text::fit_box(
            font,
            title,
            (lw - 2 * PAD) as f32,
            s.max_h,
            MAX_LINES,
            s.max_px,
            MIN_PX,
        );
        text::draw_centred(&mut rgba, lw, lh, &layout, ink(bg));
    }
    rgba
}

fn ink(bg: [u8; 3]) -> [u8; 3] {
    let luma = 0.2126 * bg[0] as f32 + 0.7152 * bg[1] as f32 + 0.0722 * bg[2] as f32;
    if luma > 140.0 {
        [0x1a, 0x18, 0x16]
    } else {
        [0xf4, 0xf1, 0xea]
    }
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [u8; 3] {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    [
        ((r + m) * 255.0).round() as u8,
        ((g + m) * 255.0).round() as u8,
        ((b + m) * 255.0).round() as u8,
    ]
}
