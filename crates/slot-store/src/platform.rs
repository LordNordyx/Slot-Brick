use std::path::Path;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum Platform {
    #[default]
    Gba,
    Gb,
    Gbc,
    Nes,
    Snes,
}

impl Platform {
    pub const ALL: [Platform; 5] =
        [Platform::Gba, Platform::Gb, Platform::Gbc, Platform::Nes, Platform::Snes];

    pub fn name(self) -> &'static str {
        match self {
            Platform::Gba => "Game Boy Advance",
            Platform::Gb => "Game Boy",
            Platform::Gbc => "Game Boy Color",
            Platform::Nes => "Nintendo Entertainment System",
            Platform::Snes => "Super Nintendo Entertainment System",
        }
    }

    /// Where the platform is in Platform::ALL, for arrays kept one per platform.
    pub fn index(self) -> usize {
        match self {
            Platform::Gba => 0,
            Platform::Gb => 1,
            Platform::Gbc => 2,
            Platform::Nes => 3,
            Platform::Snes => 4,
        }
    }

    pub fn dir_name(self) -> &'static str {
        match self {
            Platform::Gba => "GBA",
            Platform::Gb => "GB",
            Platform::Gbc => "GBC",
            Platform::Nes => "NES",
            Platform::Snes => "SNES",
        }
    }

    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            Platform::Gba => &["gba"],
            Platform::Gb | Platform::Gbc => &["gb", "gbc"],
            Platform::Nes => &["nes"],
            Platform::Snes => &["sfc", "smc"],
        }
    }

    pub fn picture(self) -> (u32, u32) {
        match self {
            Platform::Gba => (240, 160),
            Platform::Gb | Platform::Gbc => (160, 144),
            // With FCEUmm's and snes9x's 8-line overscan crop top and bottom.
            Platform::Nes | Platform::Snes => (256, 224),
        }
    }

    /// The frame canvas a core draws into, its picture centred: GBA-sized for the Game
    /// Boys, the full 256x240 (overscan included) for the NES and SNES. A SNES hi-res frame
    /// (512 wide, or 448 lines interlaced) is halved into it as it arrives.
    pub fn canvas(self) -> (u32, u32) {
        match self {
            Platform::Gba | Platform::Gb | Platform::Gbc => (240, 160),
            Platform::Nes | Platform::Snes => (256, 240),
        }
    }

    pub fn accepts(self, path: &Path) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|ext| {
                self.extensions()
                    .iter()
                    .any(|k| ext.eq_ignore_ascii_case(k))
            })
    }
}
