use crate::plate::UndoFace;
use crate::text;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum PowerChoice {
    Restart,
    PowerOff,
}

impl PowerChoice {
    pub const ALL: [PowerChoice; 2] = if cfg!(feature = "brick") {
        [PowerChoice::PowerOff, PowerChoice::Restart]
    } else { [PowerChoice::Restart, PowerChoice::PowerOff] };

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|choice| *choice == self).unwrap()
    }

    pub fn text(self) -> &'static str {
        match self {
            PowerChoice::Restart => if cfg!(feature = "brick") { "EXIT TO STOCK OS" } else { "Restart" },
            PowerChoice::PowerOff => if cfg!(feature = "brick") { "SHUT DOWN DEVICE" } else { "Power Off" },
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum PauseChoice {
    Resume,
    SaveState,
    LoadState,
    Quit,
}

impl PauseChoice {
    pub const ALL: [PauseChoice; 4] = [
        PauseChoice::Resume,
        PauseChoice::SaveState,
        PauseChoice::LoadState,
        PauseChoice::Quit,
    ];

    pub fn text(self) -> &'static str {
        match self {
            PauseChoice::Resume => "RESUME",
            PauseChoice::SaveState => "SAVE STATE",
            PauseChoice::LoadState => "LOAD STATE",
            PauseChoice::Quit => "QUIT GAME",
        }
    }
}

pub(crate) const MENU_PX: f32 = 30.0;
const MENU_MIN_PX: f32 = 18.0;
pub(crate) const MENU_H: u32 = 40;
pub const MENU_PAD: u32 = 18;
pub(crate) const MENU_INK: [u8; 3] = [0xf6, 0xf4, 0xef];

pub fn menu_face(label: &str) -> UndoFace {
    let Some(font) = text::label_font() else {
        return UndoFace {
            rgba: Vec::new(),
            w: 0,
            h: 0,
        };
    };
    let ink = text::line_width(font, label, MENU_PX, 0.0).ceil() as u32;
    let w = ink + 2 * MENU_PAD;
    let mut rgba = vec![0u8; (w * MENU_H * 4) as usize];
    let layout = text::fit(font, label, w as f32, 1, MENU_PX, MENU_MIN_PX);
    text::draw_centred(&mut rgba, w, MENU_H, &layout, MENU_INK);
    UndoFace { rgba, w, h: MENU_H }
}
