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
const ASSET_HZ: f32 = 48_000.0;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Sfx {
    Insert,
    Eject,
    /// Y on the shelf: a star goes on the cart, with one of the pops (0 to POP_TAKES - 1).
    Pop(usize),
    /// Y again: the star comes off, with a pop played lower and quieter.
    Unpop(usize),
}

/// A pop taken off is played this much lower (slower), and this loud.
const UNPOP_PITCH: f32 = 0.8;
const UNPOP_GAIN: f32 = 0.6;

impl Sfx {
    pub fn lead(self) -> f32 {
        match self {
            Sfx::Insert => 0.097,
            Sfx::Eject => 0.021,
            Sfx::Pop(_) | Sfx::Unpop(_) => 0.0,
        }
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

    /// The recording, how fast it is played (1 as recorded), and how loud.
    fn take(self) -> (&'static [u8], f32, f32) {
        match self {
            Sfx::Insert => (INSERT, 1.0, 1.0),
            Sfx::Eject => (EJECT, 1.0, 1.0),
            Sfx::Pop(n) => (POPS[n % POPS.len()], 1.0, 1.0),
            Sfx::Unpop(n) => (POPS[n % POPS.len()], UNPOP_PITCH, UNPOP_GAIN),
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
