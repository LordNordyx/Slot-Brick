use std::sync::OnceLock;

use slot_gfx::{Draw, TexId, OUT_H, OUT_W};
use slot_store::Cart;
use slot_store::Theme;

use crate::cart::{cart_box, label_colour, label_text, seated_box, SEATED_W};
use crate::footer::Printed;
use crate::icon::icon_box;
use crate::plate::HINT_H;
use crate::shelf::foot_y;

pub const ALERT_PX: f32 = 44.0;

pub const MOUTH_H: f32 = 58.0;
const BAND_Y: f32 = OUT_H as f32 - MOUTH_H;

pub const MOUTH_W: f32 = SEATED_W as f32 + 14.0;
const SLIT_H: f32 = 9.0;
const MOUTH_X: f32 = (OUT_W as f32 - MOUTH_W) / 2.0;
const SLIT_Y: f32 = BAY_Y + 5.0;

pub const LIP_H: f32 = 2.0;

const BAY_W: f32 = MOUTH_W + 18.0;
const BAY_X: f32 = (OUT_W as f32 - BAY_W) / 2.0;
const BAY_Y: f32 = BAND_Y + LIP_H;
const SCOOP_W: f32 = MOUTH_W * 0.88;
const SCOOP_D: f32 = RECESS_H - (SCOOP_Y - BAY_Y);
const SCOOP_Y: f32 = SLIT_Y + SLIT_H;

const RECESS_H: f32 = 42.0;
/// The disc slot's slit: where the cart slot's opens, a little thinner.
const DISC_SLIT_Y: f32 = SLIT_Y;
const DISC_SLIT_H: f32 = 7.0;
const RIM_W: f32 = 2.0;
const CX: f32 = OUT_W as f32 / 2.0;
const SCOOP_FLAT: f32 = 4.0;

static THEME: OnceLock<Theme> = OnceLock::new();

pub fn set_theme(theme: Theme) {
    let _ = THEME.set(theme);
}

pub fn theme() -> &'static Theme {
    THEME.get_or_init(Theme::default)
}

fn rgb(c: [u8; 3]) -> [f32; 4] {
    [
        c[0] as f32 / 255.0,
        c[1] as f32 / 255.0,
        c[2] as f32 / 255.0,
        1.0,
    ]
}

pub fn housing() -> [f32; 4] {
    rgb(theme().housing)
}

pub fn opening() -> [f32; 4] {
    rgb(theme().opening)
}

pub fn edge() -> [f32; 4] {
    rgb(theme().edge)
}

pub fn recess() -> [f32; 4] {
    rgb(theme().recess)
}

const LIP_Y: f32 = BAND_Y;

const SEATED_Y: f32 = BAY_Y + 4.0;

fn catch_at(foot: f32, seated_foot: f32) -> f32 {
    (LIP_Y - foot) / (seated_foot - foot)
}

/// How many times a disc turns on its way in.
const DISC_TURNS: f32 = 3.0;

const CATCH_IN: f32 = 0.42;
const CATCH_OUT: f32 = 0.62;
const CREEP: f32 = 0.03;

pub struct SlotChrome<'a> {
    pub cart: &'a Cart,
    pub face: Option<TexId>,
    pub rest: f32,
    pub scale: f32,
    pub seat: f32,
    pub alert: Option<(TexId, f32)>,
    pub dim: f32,
    pub screen: f32,
    pub game: bool,
}

impl SlotChrome<'_> {
    pub fn draw(&self, out: &mut Vec<Draw>) {
        let seat = self.seat.clamp(0.0, 1.0);
        let dim = self.dim.clamp(0.0, 1.0);
        if dim > 0.0 {
            out.push(Draw::Rect {
                x: 0.0,
                y: 0.0,
                w: OUT_W as f32,
                h: OUT_H as f32,
                colour: [0.0, 0.0, 0.0, dim],
            });
        }

        let chrome = 1.0 - self.screen.clamp(0.0, 1.0);

        draw_slot_back(chrome, out);

        let (cw, ch) = cart_box(self.cart.platform);
        let (cw, ch) = (cw as f32, ch as f32);

        let scale = self.scale.clamp(0.0, 1.0);
        let (w0, h0) = (cw * scale, ch * scale);
        let (sw, sh) = seated_box(self.cart.platform);
        let (sw, sh) = (sw as f32, sh as f32);
        let (foot0, seated_foot) = (foot_y(ch), SEATED_Y + sh);
        let catch = catch_at(foot0, seated_foot);
        let travel = travel(seat, catch);
        let k = (travel / catch).min(1.0);
        let (cw, ch) = (w0 + (sw - w0) * k, h0 + (sh - h0) * k);
        let centre = self.rest + w0 / 2.0;
        let x = centre + (CX - centre) * k - cw / 2.0;
        let y = foot0 + (seated_foot - foot0) * travel - ch;
        let cart_alpha = if seat >= 1.0 { chrome } else { 1.0 };
        out.push(match self.face {
            // A disc spins up as it goes in, and down as it comes out.
            Some(tex) if self.cart.platform == slot_store::Platform::Ps1 => Draw::Turned {
                x,
                y,
                w: cw,
                h: ch,
                tex,
                alpha: cart_alpha,
                turn: DISC_TURNS * std::f32::consts::TAU * seat * seat,
            },
            Some(tex) => Draw::Tex {
                x,
                y,
                w: cw,
                h: ch,
                tex,
                alpha: cart_alpha,
            },
            None => {
                let c = label_colour(&label_text(self.cart));
                Draw::Rect {
                    x,
                    y,
                    w: cw,
                    h: ch,
                    colour: [
                        c[0] as f32 / 255.0,
                        c[1] as f32 / 255.0,
                        c[2] as f32 / 255.0,
                        cart_alpha,
                    ],
                }
            }
        });

        if let Some((tex, alpha)) = self.alert {
            let (w, h) = icon_box(ALERT_PX);
            let (w, h) = (w as f32, h as f32);
            out.push(Draw::Tex {
                x: x + (cw - w) / 2.0,
                y: y + (ch - h) / 2.0,
                w,
                h,
                tex,
                alpha,
            });
        }

        if chrome < 1.0 {
            front_bands(|x, y, w, h, _| {
                out.push(Draw::Rect {
                    x,
                    y,
                    w,
                    h,
                    colour: [0.0, 0.0, 0.0, 1.0],
                });
            });
        }

        if self.game && self.screen > 0.0 {
            out.push(Draw::Game);
        }

        draw_slot_front(chrome, out);
    }
}

fn band(x: f32, y: f32, w: f32, h: f32, c: [f32; 4], alpha: f32) -> Draw {
    Draw::Rect {
        x,
        y,
        w,
        h,
        colour: [c[0], c[1], c[2], c[3] * alpha],
    }
}

// Cache vector chrome at twice logical resolution; the cartridge is drawn
// between these layers to preserve insertion and ejection occlusion.
#[cfg(feature = "brick")]
static ART: OnceLock<(TexId, TexId)> = OnceLock::new();
/// The disc slot's two layers: a slot-loading drive's slit, where the cartridge slot's scoop was.
#[cfg(feature = "brick")]
static DISC_ART: OnceLock<(TexId, TexId)> = OnceLock::new();
/// How far the slot has turned into a disc slot: 0 a cartridge slot, 1 a disc slot.
static DISC_MIX: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

pub fn set_disc_slot(mix: f32) {
    DISC_MIX.store(mix.clamp(0.0, 1.0).to_bits(), std::sync::atomic::Ordering::Relaxed);
}

fn disc_mix() -> f32 {
    f32::from_bits(DISC_MIX.load(std::sync::atomic::Ordering::Relaxed))
}

#[cfg(feature = "brick")]
pub fn upload_slot_art(mut upload: impl FnMut(u32,u32,&[u8])->TexId) {
    if ART.get().is_some() {return;}
    if let (Some(back),Some(front))=(slot_art_face(false),slot_art_face(true)) {
        let _=ART.set((upload(back.w,back.h,&back.rgba),upload(front.w,front.h,&front.rgba)));
    }
    if let (Some(back),Some(front))=(disc_slot_face(false),disc_slot_face(true)) {
        let _=DISC_ART.set((upload(back.w,back.h,&back.rgba),upload(front.w,front.h,&front.rgba)));
    }
}

/// The disc slot: a flat face with a thin slit the width of the cart slot's mouth, a shadow
/// inside it, and a lip of light under it. Drawn over the cartridge slot as it fades in.
#[cfg(feature = "brick")]
fn disc_slot_face(front: bool) -> Option<crate::cart::CartFace> {
    let hex=|c: [u8;3]|format!("#{:02x}{:02x}{:02x}",c[0],c[1],c[2]);
    let t=theme();
    let (housing,opening,edge)=(hex(t.housing),hex(t.opening),hex(t.edge));
    let local=|y:f32|y-BAND_Y;
    let (top,h)=(local(DISC_SLIT_Y),DISC_SLIT_H);
    let r=h/2.0;
    let body=if front {
        format!("<path fill-rule='evenodd' fill='{housing}' d='M0,0 H{OUT_W} V{MOUTH_H} H0 Z M{},{top} H{} A{r},{r} 0 0 1 {},{} H{} A{r},{r} 0 0 1 {},{top} Z'/>\
            <rect x='{}' y='{}' width='{}' height='1' fill='{edge}'/>",
            MOUTH_X+r,MOUTH_X+MOUTH_W-r,MOUTH_X+MOUTH_W-r,top+h,MOUTH_X+r,MOUTH_X+r,
            MOUTH_X+r,top+h,MOUTH_W-2.0*r)
    } else {
        format!("<rect x='{MOUTH_X}' y='{top}' width='{MOUTH_W}' height='{h}' fill='{opening}'/><rect x='{MOUTH_X}' y='{top}' width='{MOUTH_W}' height='{}' fill='url(#shade)'/>",h/2.0)
    };
    raster(&body)
}
#[cfg(feature = "brick")]
pub fn slot_art_face(front: bool) -> Option<crate::cart::CartFace> {
    use std::fmt::Write;
    let hex=|c: [u8;3]|format!("#{:02x}{:02x}{:02x}",c[0],c[1],c[2]);
    let t=theme();
    let (housing,recess,opening,edge)=(hex(t.housing),hex(t.recess),hex(t.opening),hex(t.edge));
    let near=CX-SCOOP_W/2.0;let far=CX+SCOOP_W/2.0;let right=BAY_X+BAY_W;
    let local=|y:f32|y-BAND_Y;
    let curve=|offset:f32,reverse:bool| {
        let mut path=String::new();
        for i in 0..=256 {
            let u=if reverse {1.0-i as f32/256.0} else {i as f32/256.0};
            let x=near+SCOOP_W*u;
            let y=local(SCOOP_Y)+SCOOP_D*(1.0-(2.0*u-1.0).abs().powf(SCOOP_FLAT))+offset;
            let _=write!(path," L{x:.3},{y:.3}");
        }
        path
    };
    let mut body=String::new();
    if front {
        let _=write!(body,"<path fill='{housing}' d='M0,0 H{BAY_X} V{} H{near} {} H{right} V0 H{OUT_W} V{MOUTH_H} H0 Z'/>",local(SCOOP_Y)+RIM_W,curve(RIM_W,false));
        let _=write!(body,"<path fill='{edge}' d='M{near},{} {} {} Z'/>",local(SCOOP_Y),curve(0.0,false),curve(RIM_W,true));
        let _=write!(body,"<path fill='none' stroke='white' stroke-opacity='.08' stroke-width='.5' d='M{near},{} {}'/>",local(SCOOP_Y)+RIM_W,curve(RIM_W,false));
    } else {
        let _=write!(body,"<rect x='{BAY_X}' y='0' width='{BAY_W}' height='{LIP_H}' fill='{housing}'/><rect x='{MOUTH_X}' y='0' width='{MOUTH_W}' height='{LIP_H}' fill='{edge}'/><rect x='{BAY_X}' y='{}' width='{BAY_W}' height='{RECESS_H}' fill='{recess}'/>",local(BAY_Y));
        let _=write!(body,"<path fill='{opening}' d='M{MOUTH_X},{} H{} V{} H{far} {} H{MOUTH_X} Z'/>",local(SLIT_Y),MOUTH_X+MOUTH_W,local(SCOOP_Y),curve(0.0,true));
        let _=write!(body,"<rect x='{MOUTH_X}' y='{}' width='{MOUTH_W}' height='3' fill='url(#shade)'/>",local(SLIT_Y));
    }
    raster(&body)
}

/// Slot art, as an SVG body the width of the screen and the height of the slot's band, drawn at
/// twice the screen's resolution.
#[cfg(feature = "brick")]
fn raster(body: &str) -> Option<crate::cart::CartFace> {
    let svg=format!("<svg xmlns='http://www.w3.org/2000/svg' width='{OUT_W}' height='{MOUTH_H}'><defs><linearGradient id='shade' x2='0' y2='1'><stop stop-color='black' stop-opacity='.25'/><stop offset='1' stop-color='black' stop-opacity='0'/></linearGradient></defs>{body}</svg>");
    let tree=usvg::Tree::from_str(&svg,&usvg::Options::default()).ok()?;
    let (w,h)=(OUT_W*2,(MOUTH_H*2.0) as u32);
    let mut pixels=resvg::tiny_skia::Pixmap::new(w,h)?;
    resvg::render(&tree,resvg::tiny_skia::Transform::from_scale(2.0,2.0),&mut pixels.as_mut());
    let mut rgba=pixels.data().to_vec();
    // Convert premultiplied raster colours for the straight-alpha sprite shader.
    for p in rgba.chunks_exact_mut(4) {
        let a=u32::from(p[3]);
        if a>0 {for c in &mut p[..3] {*c=((u32::from(*c)*255+a/2)/a).min(255) as u8;}}
    }
    Some(crate::cart::CartFace{w,h,rgba})
}
#[cfg(feature = "brick")]
fn draw_art(front: bool, alpha: f32, out: &mut Vec<Draw>) -> bool {
    let Some(&(back,fore))=ART.get() else {return false;};
    if alpha>0.0 {out.push(Draw::Tex{x:0.0,y:BAND_Y,w:OUT_W as f32,h:MOUTH_H,tex:if front {fore}else{back},alpha});}
    // The disc slot over it, as far as it has come in: opaque where the cart slot is open, so
    // drawn on top it closes the scoop and leaves the slit.
    let mix=disc_mix();
    if let (true,Some(&(back,fore)))=(mix>0.0&&alpha>0.0,DISC_ART.get()) {
        out.push(Draw::Tex{x:0.0,y:BAND_Y,w:OUT_W as f32,h:MOUTH_H,tex:if front {fore}else{back},alpha:alpha*mix});
    }
    true
}

fn draw_slot_back(alpha: f32, out: &mut Vec<Draw>) {
    #[cfg(feature = "brick")]
    if draw_art(false,alpha,out) {return;}
    out.push(band(BAY_X, BAND_Y, BAY_W, LIP_H, housing(), alpha));
    out.push(band(MOUTH_X, BAND_Y, MOUTH_W, LIP_H, edge(), alpha));
    out.push(band(BAY_X, BAY_Y, BAY_W, RECESS_H, recess(), alpha));
    out.push(band(MOUTH_X, SLIT_Y, MOUTH_W, SLIT_H, opening(), alpha));
    for_each_scoop_span(|x, w, depth| {
        out.push(band(x, SCOOP_Y, w, depth, opening(), alpha));
    });
}

fn draw_slot_front(alpha: f32, out: &mut Vec<Draw>) {
    #[cfg(feature = "brick")]
    if draw_art(true,alpha,out) {return;}
    front_bands(|x, y, w, h, c| out.push(band(x, y, w, h, c, alpha)));
}

fn front_bands(mut band: impl FnMut(f32, f32, f32, f32, [f32; 4])) {
    let w = OUT_W as f32;
    let right = BAY_X + BAY_W;
    let floor = SCOOP_Y + SCOOP_D + RIM_W;
    band(0.0, BAND_Y, BAY_X, MOUTH_H, housing());
    band(right, BAND_Y, w - right, MOUTH_H, housing());

    let near = CX - SCOOP_W / 2.0;
    band(BAY_X, SCOOP_Y, near - BAY_X, floor - SCOOP_Y, housing());
    let far = CX + SCOOP_W / 2.0;
    band(far, SCOOP_Y, right - far, floor - SCOOP_Y, housing());

    for_each_scoop_span(|x, w, depth| {
        let top = SCOOP_Y + depth + RIM_W;
        band(x, top, w, floor - top, housing());
    });
    band(0.0, floor, w, OUT_H as f32 - floor, housing());
    for_each_scoop_span(|x, w, depth| {
        band(x, SCOOP_Y + depth, w, RIM_W, edge());
    });
}

fn for_each_scoop_span(mut span: impl FnMut(f32, f32, f32)) {
    let hw = SCOOP_W / 2.0;
    let depth = |x: f32| SCOOP_D * (1.0 - (x.abs() / hw).powf(SCOOP_FLAT)).max(0.0);
    let mut x = -hw;
    while x < hw {
        let d = depth(x + 0.5).round();
        let start = x;
        while x < hw && depth(x + 0.5).round() == d {
            x += 1.0;
        }
        span(CX + start, x - start, d);
    }
}

pub fn draw_slot_name(name: Printed, alpha: f32, out: &mut Vec<Draw>) {
    let (Some(tex), true) = (name.face, alpha > 0.0) else {
        return;
    };
    let (w, h) = (name.w as f32, HINT_H as f32);
    let hole = SCOOP_Y + SCOOP_D - SLIT_Y;
    out.push(Draw::Tex {
        x: CX - w / 2.0,
        y: SLIT_Y + (hole - h) / 2.0,
        w,
        h,
        tex,
        alpha,
    });
}

pub fn draw_empty_slot(out: &mut Vec<Draw>) {
    draw_slot_back(1.0, out);
    draw_slot_front(1.0, out);
}

fn travel(seat: f32, catch: f32) -> f32 {
    if seat < CATCH_IN {
        catch * ease(seat / CATCH_IN)
    } else if seat < CATCH_OUT {
        catch + CREEP * (seat - CATCH_IN) / (CATCH_OUT - CATCH_IN)
    } else {
        let caught = catch + CREEP;
        caught + (1.0 - caught) * ease((seat - CATCH_OUT) / (1.0 - CATCH_OUT))
    }
}

pub fn ease(u: f32) -> f32 {
    u * u * u * (u * (u * 6.0 - 15.0) + 10.0)
}
