use crate::draw::{Draw, TexId, OUT_H, OUT_W};
use crate::plate::{arrows_hint_face, centred_hints, hint_face, UndoFace, HINT_H, LEGEND_GAP, LEGEND_Y};
use crate::power_menu::{MENU_H, MENU_INK, MENU_PAD, MENU_PX};
use crate::slot_chrome::{edge, opening};
use crate::text;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum QuickRow {
    FastForward,
    FastForwardSound,
    Screen,
    Rumble,
    DateTime,
    About,
    GbaShader,
    GbShader,
    GbaColourCorrection,
    Game,
    EjectSave,
    Turbo,
    Rewind,
    GbPalettes,
    Lights,
    LedMode,
    LedTop,
    LedButtons,
    LedTriggers,
    LedColor,
    LedEffect,
    Wifi,
    CartArt,
    Achievements,
    ScreenGb,
    ScreenGba,
    ScreenNes,
    ScreenSnes,
    GbColourCorrection,
    NesShader,
    SnesShader,
    GbOverlay,
    GbcOverlay,
    GbaOverlay,
    NesOverlay,
    SnesOverlay,
}

impl QuickRow {
    pub const ALL: [QuickRow; 36] = [
        QuickRow::FastForward,
        QuickRow::FastForwardSound,
        QuickRow::Screen,
        QuickRow::Rumble,
        QuickRow::DateTime,
        QuickRow::About,
        QuickRow::GbaShader,
        QuickRow::GbShader,
        QuickRow::GbaColourCorrection,
        QuickRow::Game,
        QuickRow::EjectSave,
        QuickRow::Turbo,
        QuickRow::Rewind,
        QuickRow::GbPalettes,
        QuickRow::Lights,
        QuickRow::LedMode,
        QuickRow::LedTop,
        QuickRow::LedButtons,
        QuickRow::LedTriggers,
        QuickRow::LedColor,
        QuickRow::LedEffect,
        QuickRow::Wifi,
        QuickRow::CartArt,
        QuickRow::Achievements,
        QuickRow::ScreenGb,
        QuickRow::ScreenGba,
        QuickRow::ScreenNes,
        QuickRow::ScreenSnes,
        QuickRow::GbColourCorrection,
        QuickRow::NesShader,
        QuickRow::SnesShader,
        QuickRow::GbOverlay,
        QuickRow::GbcOverlay,
        QuickRow::GbaOverlay,
        QuickRow::NesOverlay,
        QuickRow::SnesOverlay,
    ];

    #[cfg(not(feature = "brick"))]
    pub const MAIN: [QuickRow; 4] = [
        QuickRow::Screen,
        QuickRow::Game,
        QuickRow::DateTime,
        QuickRow::About,
    ];

    #[cfg(feature = "brick")]
    pub const MAIN: [QuickRow; 8] = [QuickRow::Screen,QuickRow::Game,QuickRow::Lights,QuickRow::Wifi,QuickRow::CartArt,QuickRow::Achievements,QuickRow::DateTime,QuickRow::About];
    pub const LIGHTS: [QuickRow; 6] = [QuickRow::LedMode,QuickRow::LedTop,QuickRow::LedButtons,QuickRow::LedTriggers,QuickRow::LedColor,QuickRow::LedEffect];
    pub fn led_setting(self) -> bool { Self::LIGHTS.contains(&self) }

    /// Screen: a page for each system, holding that system's own picture settings.
    pub const SCREEN: [QuickRow; 4] = [QuickRow::ScreenGb, QuickRow::ScreenGba, QuickRow::ScreenNes, QuickRow::ScreenSnes];
    pub const SCREEN_GB: [QuickRow; 5] =
        [QuickRow::GbShader, QuickRow::GbColourCorrection, QuickRow::GbPalettes, QuickRow::GbOverlay, QuickRow::GbcOverlay];
    pub const SCREEN_GBA: [QuickRow; 3] = [QuickRow::GbaShader, QuickRow::GbaColourCorrection, QuickRow::GbaOverlay];
    pub const SCREEN_NES: [QuickRow; 2] = [QuickRow::NesShader, QuickRow::NesOverlay];
    pub const SCREEN_SNES: [QuickRow; 2] = [QuickRow::SnesShader, QuickRow::SnesOverlay];

    /// The system an Overlay row picks the overlay for.
    pub fn overlay_of(self) -> Option<slot_store::Platform> {
        use slot_store::Platform;
        Some(match self {
            QuickRow::GbOverlay => Platform::Gb,
            QuickRow::GbcOverlay => Platform::Gbc,
            QuickRow::GbaOverlay => Platform::Gba,
            QuickRow::NesOverlay => Platform::Nes,
            QuickRow::SnesOverlay => Platform::Snes,
            _ => return None,
        })
    }

    pub const GAME: [QuickRow; 6] = [
        QuickRow::FastForward,
        QuickRow::FastForwardSound,
        QuickRow::Rewind,
        QuickRow::Turbo,
        QuickRow::Rumble,
        QuickRow::EjectSave,
    ];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn page(self) -> &'static [QuickRow] {
        match self {
            QuickRow::LedMode | QuickRow::LedTop | QuickRow::LedButtons | QuickRow::LedTriggers | QuickRow::LedColor | QuickRow::LedEffect => &QuickRow::LIGHTS,
            QuickRow::ScreenGb | QuickRow::ScreenGba | QuickRow::ScreenNes | QuickRow::ScreenSnes => &QuickRow::SCREEN,
            QuickRow::GbShader
            | QuickRow::GbColourCorrection
            | QuickRow::GbPalettes
            | QuickRow::GbOverlay
            | QuickRow::GbcOverlay => &QuickRow::SCREEN_GB,
            QuickRow::GbaShader | QuickRow::GbaColourCorrection | QuickRow::GbaOverlay => &QuickRow::SCREEN_GBA,
            QuickRow::NesShader | QuickRow::NesOverlay => &QuickRow::SCREEN_NES,
            QuickRow::SnesShader | QuickRow::SnesOverlay => &QuickRow::SCREEN_SNES,
            QuickRow::FastForward
            | QuickRow::FastForwardSound
            | QuickRow::Rewind
            | QuickRow::Turbo
            | QuickRow::Rumble
            | QuickRow::EjectSave => &QuickRow::GAME,
            _ => &QuickRow::MAIN,
        }
    }

    pub fn parent(self) -> Option<QuickRow> {
        match self.page()[0] {
            QuickRow::LedMode => Some(QuickRow::Lights),
            QuickRow::ScreenGb => Some(QuickRow::Screen),
            QuickRow::GbShader => Some(QuickRow::ScreenGb),
            QuickRow::GbaShader => Some(QuickRow::ScreenGba),
            QuickRow::NesShader => Some(QuickRow::ScreenNes),
            QuickRow::SnesShader => Some(QuickRow::ScreenSnes),
            QuickRow::FastForward => Some(QuickRow::Game),
            _ => None,
        }
    }

    pub fn child(self) -> Option<QuickRow> {
        match self {
            QuickRow::Lights => Some(QuickRow::LIGHTS[0]),
            QuickRow::Screen => Some(QuickRow::SCREEN[0]),
            QuickRow::ScreenGb => Some(QuickRow::SCREEN_GB[0]),
            QuickRow::ScreenGba => Some(QuickRow::SCREEN_GBA[0]),
            QuickRow::ScreenNes => Some(QuickRow::SCREEN_NES[0]),
            QuickRow::ScreenSnes => Some(QuickRow::SCREEN_SNES[0]),
            QuickRow::Game => Some(QuickRow::GAME[0]),
            _ => None,
        }
    }

    pub fn position(self) -> usize {
        self.page().iter().position(|r| *r == self).unwrap_or(0)
    }

    pub fn label(self) -> &'static str {
        match self {
            QuickRow::Wifi => "Wi-Fi",
            QuickRow::CartArt => "Cart Art",
            QuickRow::Achievements => "RetroAchievements",
            QuickRow::Lights => "LEDs",
            QuickRow::LedMode => "Lights",
            QuickRow::LedTop => "Top Bar",
            QuickRow::LedButtons => "Buttons",
            QuickRow::LedTriggers => "Triggers",
            QuickRow::LedColor => "Color",
            QuickRow::LedEffect => "Effect",
            QuickRow::FastForward => "Fast Forward",
            QuickRow::FastForwardSound => "Fast Forward Sound",
            QuickRow::Screen => "Screen",
            QuickRow::Rumble => "Rumble",
            QuickRow::DateTime => "Date & Time",
            QuickRow::About => "About",
            QuickRow::GbaShader | QuickRow::GbShader | QuickRow::NesShader | QuickRow::SnesShader => "Shader",
            QuickRow::GbColourCorrection | QuickRow::GbaColourCorrection => "Color Correction",
            QuickRow::GbOverlay => "GB Overlay",
            QuickRow::GbcOverlay => "GBC Overlay",
            QuickRow::GbaOverlay | QuickRow::NesOverlay | QuickRow::SnesOverlay => "Overlay",
            QuickRow::ScreenGb => "Game Boy / GBC",
            QuickRow::ScreenGba => "Game Boy Advance",
            QuickRow::ScreenNes => "NES",
            QuickRow::ScreenSnes => "SNES",
            QuickRow::Game => "Gameplay",
            QuickRow::EjectSave => "Auto Save on Eject",
            QuickRow::Turbo => "Turbo Buttons",
            QuickRow::Rewind => "Rewind",
            QuickRow::GbPalettes => "GB Palettes",
        }
    }

    pub fn opens(self) -> bool {
        matches!(
            self,
            QuickRow::DateTime | QuickRow::About | QuickRow::Screen | QuickRow::Game | QuickRow::Lights | QuickRow::Wifi | QuickRow::CartArt | QuickRow::Achievements
                | QuickRow::ScreenGb | QuickRow::ScreenGba | QuickRow::ScreenNes | QuickRow::ScreenSnes
        )
    }

    pub fn up(self) -> QuickRow {
        let page = self.page();
        page[(self.position() + page.len() - 1) % page.len()]
    }

    pub fn down(self) -> QuickRow {
        let page = self.page();
        page[(self.position() + 1) % page.len()]
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum QuickValue {
    Speed2,
    Speed3,
    Speed4,
    Speed6,
    On,
    Off,
    Lcd3x,
    Grid,
    Dot,
    Simpletex,
    Stock,
    Low,
    Medium,
    High,
    White,
    Blue,
    Green,
    Amber,
    Red,
    Purple,
    Steady,
    Breathe,
    Unavailable,
}

impl QuickValue {
    pub const ALL: [QuickValue; 23] = [
        QuickValue::Speed2,
        QuickValue::Speed3,
        QuickValue::Speed4,
        QuickValue::Speed6,
        QuickValue::On,
        QuickValue::Off,
        QuickValue::Lcd3x,
        QuickValue::Grid,
        QuickValue::Dot,
        QuickValue::Simpletex,
        QuickValue::Stock,
        QuickValue::Low,
        QuickValue::Medium,
        QuickValue::High,
        QuickValue::White,
        QuickValue::Blue,
        QuickValue::Green,
        QuickValue::Amber,
        QuickValue::Red,
        QuickValue::Purple,
        QuickValue::Steady,
        QuickValue::Breathe,
        QuickValue::Unavailable,
    ];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn text(self) -> &'static str {
        match self {
            QuickValue::Speed2 => "2×",
            QuickValue::Speed3 => "3×",
            QuickValue::Speed4 => "4×",
            QuickValue::Speed6 => "6×",
            QuickValue::On => "On",
            QuickValue::Off => "Off",
            QuickValue::Lcd3x => "LCD3x",
            QuickValue::Grid => "Grid",
            QuickValue::Dot => "Dot",
            QuickValue::Simpletex => "Simpletex",
            QuickValue::Stock => "Stock",
            QuickValue::Low => "Low",
            QuickValue::Medium => "Medium",
            QuickValue::High => "High",
            QuickValue::White => "White",
            QuickValue::Blue => "Blue",
            QuickValue::Green => "Green",
            QuickValue::Amber => "Amber",
            QuickValue::Red => "Red",
            QuickValue::Purple => "Purple",
            QuickValue::Steady => "Steady",
            QuickValue::Breathe => "Breathe",
            QuickValue::Unavailable => "Unavailable",

        }
    }

    pub fn speed(frames: u8) -> Option<QuickValue> {
        match frames {
            2 => Some(QuickValue::Speed2),
            3 => Some(QuickValue::Speed3),
            4 => Some(QuickValue::Speed4),
            6 => Some(QuickValue::Speed6),
            _ => None,
        }
    }

    pub fn flag(on: bool) -> QuickValue {
        if on {
            QuickValue::On
        } else {
            QuickValue::Off
        }
    }
}

pub const QUICK_PITCH: f32 = 52.0;
pub const QUICK_TOP: f32 = (OUT_H as f32 - quick_pitch(QuickRow::MAIN.len()) * QuickRow::MAIN.len() as f32) / 2.0;

/// Row spacing for a page of `rows`: QUICK_PITCH, or tighter for a page that would
/// otherwise run into the legend (the Brick's main menu).
pub const fn quick_pitch(rows: usize) -> f32 {
    let fit = ((QUICK_SPLIT - OUT_H as f32 / 2.0) * 2.0 / rows as f32) as u32 as f32;
    if fit < QUICK_PITCH { fit } else { QUICK_PITCH }
}
pub const QUICK_EDGE: f32 = 32.0;
const BAR_INSET: f32 = 4.0;
const TYPE_DROP: f32 = 4.0;
const CARET_GAP: f32 = 14.0;
const CARET_PX: f32 = 24.0;
pub const QUICK_SPLIT: f32 = LEGEND_Y - 2.0;
const DIM_INK: [u8; 3] = [0x9a, 0x9a, 0xa4];

pub fn quick_label_face(row: QuickRow) -> UndoFace {
    quick_text_face(row.label(), MENU_INK)
}

pub fn quick_value_face(text: &str, lit: bool) -> UndoFace {
    quick_text_face(text, if lit { MENU_INK } else { DIM_INK })
}

fn quick_text_face(label: &str, colour: [u8; 3]) -> UndoFace {
    let Some(font) = text::label_font() else {
        return UndoFace {
            rgba: Vec::new(),
            w: 0,
            h: 0,
        };
    };
    let layout = text::fit(font, label, OUT_W as f32, 1, MENU_PX, MENU_PX);
    let set = layout
        .lines
        .iter()
        .map(|l| text::line_width(font, l, layout.px, layout.tracking))
        .fold(0.0, f32::max);
    let w = set.ceil() as u32 + 2 * MENU_PAD;
    let mut rgba = vec![0u8; (w * MENU_H * 4) as usize];
    text::draw_centred(&mut rgba, w, MENU_H, &layout, colour);
    UndoFace { rgba, w, h: MENU_H }
}

pub fn quick_caret_face(right: bool) -> UndoFace {
    let glyph = if right { '\u{f0da}' } else { '\u{f0d9}' };
    let (Some(symbols), Some(label)) = (crate::icon::symbols_font(), text::label_font()) else {
        return UndoFace {
            rgba: Vec::new(),
            w: 0,
            h: 0,
        };
    };
    let (m, cov) = symbols.rasterize(glyph, CARET_PX);
    let (w, h) = (m.width as u32, MENU_H);
    let centre = match label.horizontal_line_metrics(MENU_PX) {
        Some(v) => {
            (h as f32 - v.new_line_size) / 2.0 + v.ascent
                - label.metrics('H', MENU_PX).height as f32 / 2.0
        }
        None => h as f32 / 2.0,
    };
    let top = (centre - m.height as f32 / 2.0).round() as i32;
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for gy in 0..m.height {
        let y = top + gy as i32;
        if y < 0 || y >= h as i32 {
            continue;
        }
        for gx in 0..m.width {
            let at = ((y as u32 * w + gx as u32) * 4) as usize;
            let a = cov[gy * m.width + gx];
            rgba[at..at + 4].copy_from_slice(&[MENU_INK[0], MENU_INK[1], MENU_INK[2], a]);
        }
    }
    UndoFace { rgba, w, h }
}

pub fn quick_legend_faces() -> [UndoFace; 3] {
    [
        hint_face("B", "Back"),
        arrows_hint_face("Change"),
        hint_face("A", "Open"),
    ]
}

pub struct QuickMenuFaces {
    pub labels: Vec<(TexId, u32, u32)>,
    pub values: Vec<[(TexId, u32, u32); 2]>,
    pub carets: [(TexId, u32, u32); 2],
    pub legend: [(TexId, u32); 3],
}

pub struct QuickMenu<'a> {
    pub row: QuickRow,
    pub values: [Option<QuickValue>; QuickRow::ALL.len()],
    pub clock: Option<[(TexId, u32, u32); 2]>,
    /// Rows whose value is free text rather than one of the QuickValues (an overlay's name):
    /// its faces, dim and lit.
    pub texts: [Option<[(TexId, u32, u32); 2]>; QuickRow::ALL.len()],
    pub faces: Option<&'a QuickMenuFaces>,
    /// Rows shown greyed out that do nothing, such as Cart Art and RetroAchievements with Wi-Fi off.
    pub disabled: [bool; QuickRow::ALL.len()],
}

impl QuickMenu<'_> {
    pub fn draw(&self, out: &mut Vec<Draw>) {
        out.push(Draw::Rect {
            x: 0.0,
            y: 0.0,
            w: OUT_W as f32,
            h: OUT_H as f32,
            colour: opening(),
        });
        out.push(Draw::Rect {
            x: 0.0,
            y: row_top(self.row) + BAR_INSET,
            w: OUT_W as f32,
            h: quick_pitch(self.row.page().len()) - 2.0 * BAR_INSET,
            colour: edge(),
        });
        let Some(faces) = self.faces else {
            return;
        };
        let (right, pad) = (OUT_W as f32 - QUICK_EDGE, MENU_PAD as f32);
        for &row in self.row.page() {
            let y = row_top(row) + TYPE_DROP;
            let lit = row == self.row;
            if let Some(&(tex, w, h)) = faces.labels.get(row.index()) {
                let alpha = if self.disabled[row.index()] { DISABLED_ALPHA } else { 1.0 };
                push_alpha(out, tex, QUICK_EDGE - pad, y, w, h, alpha);
            }
            let value = match row {
                QuickRow::DateTime => self.clock.map(|c| c[lit as usize]),
                _ if self.texts[row.index()].is_some() => self.texts[row.index()].map(|t| t[lit as usize]),
                _ => self.values[row.index()]
                    .and_then(|v| faces.values.get(v.index()))
                    .map(|v| v[lit as usize]),
            };
            let Some((tex, w, h)) = value else {
                continue;
            };
            if !lit || row.opens() {
                push(out, tex, right + pad - w as f32, y, w, h);
                continue;
            }
            let [(left_tex, lw, lh), (right_tex, rw, rh)] = faces.carets;
            let rx = right - rw as f32;
            push(out, right_tex, rx, y, rw, rh);
            let vx = rx - CARET_GAP + pad - w as f32;
            push(out, tex, vx, y, w, h);
            push(out, left_tex, vx + pad - CARET_GAP - lw as f32, y, lw, lh);
        }
        let [back, change, open] = faces.legend;
        let other = if self.row.opens() { open } else { change };
        let hints: &[(TexId, u32)] = if self.disabled[self.row.index()] { &[back] } else { &[back, other] };
        for (tex, w, x) in centred_hints(hints, LEGEND_GAP) {
            push(out, tex, x, LEGEND_Y, w, HINT_H);
        }
    }
}

fn row_top(row: QuickRow) -> f32 {
    let pitch = quick_pitch(row.page().len());
    let top = (OUT_H as f32 - pitch * row.page().len() as f32) / 2.0;
    top + pitch * row.position() as f32
}

const DISABLED_ALPHA: f32 = 0.35;

fn push(out: &mut Vec<Draw>, tex: TexId, x: f32, y: f32, w: u32, h: u32) {
    push_alpha(out, tex, x, y, w, h, 1.0);
}

fn push_alpha(out: &mut Vec<Draw>, tex: TexId, x: f32, y: f32, w: u32, h: u32, alpha: f32) {
    out.push(Draw::Tex {
        x: x.round(),
        y: y.round(),
        w: w as f32,
        h: h as f32,
        tex,
        alpha,
    });
}
