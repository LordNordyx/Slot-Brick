const INSERT: &[u8] = include_bytes!("../../assets/insert.pcm");
const EJECT: &[u8] = include_bytes!("../../assets/eject.pcm");
/// Five takes of a pop (licenses/pop-Pixabay.txt), for a star put on or taken off.
const POPS: [&[u8]; 5] = [
    include_bytes!("../../assets/pop1.pcm"),
    include_bytes!("../../assets/pop2.pcm"),
    include_bytes!("../../assets/pop3.pcm"),
    include_bytes!("../../assets/pop4.pcm"),
    include_bytes!("../../assets/pop5.pcm"),
];
pub const POP_TAKES: usize = POPS.len();
/// The HOME menu's two pops, a low one and a high one.
const MENU_LOW: &[u8] = include_bytes!("../../assets/menu-low.pcm");
const MENU_HIGH: &[u8] = include_bytes!("../../assets/menu-high.pcm");
/// A GBA cart's lid popping open for the core picker (licenses/cart-pop-Pixabay.txt).
const CART_POP: &[u8] = include_bytes!("../../assets/cart-pop.pcm");
/// Three takes of a label slapped down on a cart (licenses/label-Pixabay.txt), each with its
/// slap LABEL_LEAD seconds in.
const LABELS: [&[u8]; 3] = [
    include_bytes!("../../assets/label1.pcm"),
    include_bytes!("../../assets/label2.pcm"),
    include_bytes!("../../assets/label3.pcm"),
];
pub const LABEL_TAKES: usize = LABELS.len();
const ASSET_HZ: f32 = 48_000.0;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Sfx {
    Insert,
    Eject,
    /// Y on the shelf: a star goes on the cart, with one of the pops (0 to POP_TAKES - 1).
    Pop(usize),
    /// Y again: the star comes off, with a pop played lower and quieter.
    Unpop(usize),
    /// A step in the HOME menu.
    Menu(MenuSound),
    /// A GBA cart's lid pops open for the core picker, and shuts again (lower and softer).
    CartOpen,
    CartClose,
    /// A label lands on its cart, with one of the takes (0 to LABEL_TAKES - 1).
    Label(usize),
}

/// Which System → System Sounds switch a sound answers to.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum SoundKind {
    Menu,
    Star,
    Cart,
    Lid,
    Label,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum MenuSound {
    /// Up or Down: the bar moves.
    Move,
    /// Left or Right: a value changes.
    Change,
    /// A page, screen or the menu itself opens.
    Open,
    /// Back a page, or the menu closes.
    Back,
}

/// How loud the menu's pops and the cart's pop are played, against their clips.
const MENU_GAIN: f32 = 0.6;
const POP_GAIN: f32 = 0.6;
const CART_POP_GAIN: f32 = 0.6;
/// The cart's pop as it shuts is played this much lower (slower).
const CART_CLOSE_PITCH: f32 = 0.85;

/// A pop taken off is played this much lower (slower), and this loud.
const UNPOP_PITCH: f32 = 0.8;
const UNPOP_GAIN: f32 = 0.6;

impl Sfx {
    /// How far into the sound, as played, the moment that lands with the picture is: the
    /// cart's contacts, the lid's crack, the label's slap.
    pub fn lead(self) -> f32 {
        let recorded = match self {
            Sfx::Insert => 0.097,
            Sfx::Eject => 0.021,
            Sfx::Pop(_) | Sfx::Unpop(_) | Sfx::Menu(_) => 0.0,
            Sfx::CartOpen | Sfx::CartClose => 0.058,
            Sfx::Label(_) => 0.24,
        };
        // A sound played slower gets there later.
        recorded / self.take().1
    }

    pub fn tail(self) -> f32 {
        let (pcm, pitch, _) = self.take();
        pcm.len() as f32 / 2.0 / (ASSET_HZ * pitch) - self.lead()
    }

    pub fn render(self, sample_rate: u32) -> Vec<i16> {
        let (pcm, pitch, gain) = self.take();
        let mut out = Vec::with_capacity(pcm.len());
        for v in resampled(pcm, ASSET_HZ * pitch, sample_rate) {
            let s = (v * gain).clamp(-32768.0, 32767.0) as i16;
            out.push(s);
            out.push(s);
        }
        out
    }

    /// Which System Sounds switch turns it off.
    pub fn kind(self) -> SoundKind {
        match self {
            Sfx::Insert | Sfx::Eject => SoundKind::Cart,
            Sfx::Pop(_) | Sfx::Unpop(_) => SoundKind::Star,
            Sfx::Menu(_) => SoundKind::Menu,
            Sfx::CartOpen | Sfx::CartClose => SoundKind::Lid,
            Sfx::Label(_) => SoundKind::Label,
        }
    }

    /// The recording, how fast it is played (1 as recorded), and how loud.
    fn take(self) -> (&'static [u8], f32, f32) {
        match self {
            Sfx::Insert => (INSERT, 1.0, 1.0),
            Sfx::Eject => (EJECT, 1.0, 1.0),
            Sfx::Pop(n) => (POPS[n % POPS.len()], 1.0, POP_GAIN),
            Sfx::Unpop(n) => (POPS[n % POPS.len()], UNPOP_PITCH, POP_GAIN * UNPOP_GAIN),
            // The two pops, and each again higher or lower: four sounds of a kind, kept subtle.
            Sfx::Menu(MenuSound::Move) => (MENU_LOW, 1.0, MENU_GAIN),
            Sfx::Menu(MenuSound::Change) => (MENU_HIGH, 1.0, MENU_GAIN),
            Sfx::Menu(MenuSound::Open) => (MENU_HIGH, 1.15, MENU_GAIN),
            Sfx::Menu(MenuSound::Back) => (MENU_LOW, 0.85, MENU_GAIN * 0.85),
            Sfx::CartOpen => (CART_POP, 1.0, CART_POP_GAIN),
            Sfx::CartClose => (CART_POP, CART_CLOSE_PITCH, CART_POP_GAIN * 0.75),
            Sfx::Label(n) => (LABELS[n % LABELS.len()], 1.0, 1.0),
        }
    }
}

/// `pcm` (mono, little-endian) played as if recorded at `from_hz`, at `sample_rate`.
fn resampled(pcm: &[u8], from_hz: f32, sample_rate: u32) -> Vec<f32> {
    let src: Vec<f32> = pcm
        .chunks_exact(2)
        .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32)
        .collect();
    if sample_rate == from_hz as u32 || src.is_empty() {
        return src;
    }
    let ratio = sample_rate as f32 / from_hz;
    let n = (src.len() as f32 * ratio) as usize;
    (0..n)
        .map(|i| {
            let x = i as f32 / ratio;
            let a = x as usize;
            let f = x - a as f32;
            let b = (a + 1).min(src.len() - 1);
            src[a] * (1.0 - f) + src[b] * f
        })
        .collect()
}
