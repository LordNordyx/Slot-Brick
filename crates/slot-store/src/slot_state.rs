use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crate::atomic::atomic_write;
use crate::gb_palette::GbPalette;
use crate::platform::Platform;

pub const BRIGHTNESS_MAX: u8 = 9;
pub const BLUE_LIGHT_MAX: u8 = 9;
pub const VOLUME_MAX: u8 = 100;

/// HOME → System → System Sounds → Volume goes in steps of this many percent.
pub const SOUND_VOLUME_STEP: u8 = 10;

pub const UTC_OFFSET_MIN: i16 = -720;
pub const UTC_OFFSET_MAX: i16 = 840;

pub const FF_SPEEDS: [u8; 4] = [2, 3, 4, 6];

pub const FF_SPEED_DEFAULT: u8 = 6;

/// Minutes between autosaves in game; 0 is off.
pub const AUTOSAVE_MINS: [u8; 4] = [0, 1, 5, 10];
pub const AUTOSAVE_MIN_DEFAULT: u8 = 1;

/// The battery percent at which Slot saves and shuts down; 0 is off.
pub const BATTERY_SHUTDOWNS: [u8; 4] = [0, 3, 5, 10];
pub const BATTERY_SHUTDOWN_DEFAULT: u8 = 5;

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SlotState {
    pub cart: Option<String>,
    pub cart_platform: Option<Platform>,
    pub brightness: u8,
    pub blue_light: u8,
    pub volume: u8,
    pub volume_hp: u8,
    pub muted: bool,
    pub muted_hp: bool,
    pub clock_set: bool,
    pub utc_offset_min: i16,
    /// The clock is set from an internet time server whenever Wi-Fi is up.
    pub auto_clock: bool,
    /// HOME → Library: the order of each shelf, and whether only favorites are on them.
    pub library_sort: crate::LibrarySort,
    pub favorites_only: bool,
    /// HOME → Library → Systems: the systems kept off the shelves, in the order of Platform::ALL.
    pub hidden_systems: [bool; Platform::ALL.len()],
    /// Play time in hours and minutes rather than minutes alone.
    pub play_time_hours: bool,
    /// The selected game's name on a card under its cart.
    pub title_card: bool,
    pub rumble: bool,
    /// HOME → System → System Sounds: the HOME menu's pops, the favorite star's, the cart going
    /// in and out, a GBA cart's lid popping open, and a label going on.
    pub menu_sounds: bool,
    pub sound_star: bool,
    pub sound_cart: bool,
    pub sound_cart_open: bool,
    pub sound_label: bool,
    /// How loud all of them are, 0 to 100 in steps of SOUND_VOLUME_STEP, on top of the volume.
    pub sound_volume: u8,
    pub ff_speed: u8,
    pub ff_sound: bool,
    /// Colour correction, one for the Game Boys (GB and GBC) and one for the GBA.
    pub colour_correction_gb: bool,
    pub colour_correction_gba: bool,
    pub shader_gba: Shader,
    pub shader_gb: Shader,
    pub shader_nes: Shader,
    pub shader_snes: Shader,
    pub shader_ps1: Shader,
    /// The PlayStation's dithering: on as it was, off for smoother gradients.
    pub ps1_dithering: bool,
    pub eject_save: bool,
    pub autosave_min: u8,
    pub battery_shutdown: u8,
    pub turbo: bool,
    pub rewind: bool,
    pub gb_palettes: bool,
    pub gb_palette: GbPalette,
    /// The overlay chosen for each system, by file name in Overlays/<system>, in the order of
    /// Platform::ALL; None is off.
    pub overlays: [Option<String>; Platform::ALL.len()],
}

impl SlotState {
    pub fn shows(&self, platform: Platform) -> bool {
        !self.hidden_systems[platform.index()]
    }

    /// What HOME → System → Reset Settings leaves: the game in, the clock (and whether it
    /// sets itself), and the levels set with the buttons (brightness, blue light, volume, mute). Everything else goes back
    /// to how it came.
    pub fn reset(&self) -> SlotState {
        SlotState {
            cart: self.cart.clone(),
            cart_platform: self.cart_platform,
            brightness: self.brightness,
            blue_light: self.blue_light,
            volume: self.volume,
            volume_hp: self.volume_hp,
            muted: self.muted,
            muted_hp: self.muted_hp,
            clock_set: self.clock_set,
            utc_offset_min: self.utc_offset_min,
            auto_clock: self.auto_clock,
            ..SlotState::default()
        }
    }

    pub fn overlay(&self, platform: Platform) -> Option<&str> {
        self.overlays[platform.index()].as_deref()
    }

    pub fn set_overlay(&mut self, platform: Platform, name: Option<String>) {
        self.overlays[platform.index()] = name.filter(|n| crate::overlay_name_ok(n));
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Shader {
    Off,
    Lcd3x,
    Grid,
    Dot,
    Simpletex,
}

impl Shader {
    pub const GBA: [Shader; 4] = [Shader::Off, Shader::Lcd3x, Shader::Grid, Shader::Dot];
    pub const GB: [Shader; 3] = [Shader::Off, Shader::Grid, Shader::Simpletex];
    /// A NES or SNES drew on a TV: no LCD to imitate, at most a pixel grid.
    pub const NES: [Shader; 2] = [Shader::Off, Shader::Grid];

    pub fn name(self) -> &'static str {
        match self {
            Shader::Off => "off",
            Shader::Lcd3x => "lcd3x",
            Shader::Grid => "grid",
            Shader::Dot => "dot",
            Shader::Simpletex => "simpletex",
        }
    }

    pub fn parse(name: &str) -> Option<Shader> {
        [
            Shader::Off,
            Shader::Lcd3x,
            Shader::Grid,
            Shader::Dot,
            Shader::Simpletex,
        ]
        .into_iter()
        .find(|s| s.name() == name)
    }

    pub fn step(self, choices: &[Shader], right: bool) -> Shader {
        let n = choices.len();
        let at = choices.iter().position(|&s| s == self).unwrap_or(0);
        choices[if right {
            (at + 1) % n
        } else {
            (at + n - 1) % n
        }]
    }
}

impl Default for SlotState {
    fn default() -> Self {
        SlotState {
            cart: None,
            cart_platform: None,
            brightness: 5,
            blue_light: 0,
            volume: 60,
            volume_hp: 60,
            muted: false,
            muted_hp: false,
            clock_set: false,
            utc_offset_min: 0,
            auto_clock: true,
            library_sort: crate::LibrarySort::Az,
            favorites_only: false,
            hidden_systems: [false; Platform::ALL.len()],
            play_time_hours: false,
            title_card: true,
            rumble: true,
            menu_sounds: true,
            sound_star: true,
            sound_cart: true,
            sound_cart_open: true,
            sound_label: true,
            sound_volume: 100,
            ff_speed: FF_SPEED_DEFAULT,
            ff_sound: false,
            colour_correction_gb: false,
            colour_correction_gba: false,
            shader_gba: Shader::Grid,
            shader_gb: Shader::Simpletex,
            shader_nes: Shader::Off,
            shader_snes: Shader::Off,
            shader_ps1: Shader::Off,
            ps1_dithering: true,
            eject_save: true,
            autosave_min: AUTOSAVE_MIN_DEFAULT,
            battery_shutdown: BATTERY_SHUTDOWN_DEFAULT,
            turbo: true,
            rewind: true,
            gb_palettes: false,
            gb_palette: GbPalette::DEFAULT,
            overlays: Default::default(),
        }
    }
}

fn state_path(root: &Path) -> PathBuf {
    root.join("Config").join("slot.state")
}

pub fn read_slot_state(root: &Path) -> SlotState {
    std::fs::read(state_path(root))
        .ok()
        .and_then(|b| String::from_utf8(b).ok())
        .and_then(|s| parse(&s))
        .unwrap_or_default()
}

pub fn write_slot_state(root: &Path, s: &SlotState) -> std::io::Result<()> {
    let text = format!(
        "cart={}\ncart_platform={}\nbrightness={}\nblue_light={}\nvolume={}\nvolume_hp={}\nmuted={}\nmuted_hp={}\nclock_set={}\nutc_offset_min={}\nauto_clock={}\nlibrary_sort={}\nfavorites_only={}\nplay_time_hours={}\ntitle_card={}\nrumble={}\nff_speed={}\nff_sound={}\ncolour_correction_gb={}\ncolour_correction_gba={}\nshader_gba={}\nshader_gb={}\nshader_nes={}\nshader_snes={}\nshader_ps1={}\nps1_dithering={}\neject_save={}\nautosave_min={}\nbattery_shutdown={}\nturbo={}\nrewind={}\ngb_palettes={}\ngb_palette={}\n",
        s.cart.as_deref().unwrap_or(""),
        s.cart_platform.map_or(String::new(), platform_key),
        s.brightness,
        s.blue_light,
        s.volume,
        s.volume_hp,
        s.muted as u8,
        s.muted_hp as u8,
        s.clock_set as u8,
        s.utc_offset_min,
        s.auto_clock as u8,
        s.library_sort.name(),
        s.favorites_only as u8,
        s.play_time_hours as u8,
        s.title_card as u8,
        s.rumble as u8,
        s.ff_speed,
        s.ff_sound as u8,
        s.colour_correction_gb as u8,
        s.colour_correction_gba as u8,
        s.shader_gba.name(),
        s.shader_gb.name(),
        s.shader_nes.name(),
        s.shader_snes.name(),
        s.shader_ps1.name(),
        s.ps1_dithering as u8,
        s.eject_save as u8,
        s.autosave_min,
        s.battery_shutdown,
        s.turbo as u8,
        s.rewind as u8,
        s.gb_palettes as u8,
        s.gb_palette.core_name()
    );
    let mut text = text;
    for platform in Platform::ALL {
        let _ = writeln!(text, "overlay_{}={}", platform_key(platform), s.overlay(platform).unwrap_or(""));
    }
    for (key, on) in [
        ("menu_sounds", s.menu_sounds),
        ("sound_star", s.sound_star),
        ("sound_cart", s.sound_cart),
        ("sound_cart_open", s.sound_cart_open),
        ("sound_label", s.sound_label),
    ] {
        let _ = writeln!(text, "{key}={}", on as u8);
    }
    let _ = writeln!(text, "sound_volume={}", s.sound_volume);
    let hidden: Vec<String> = Platform::ALL.into_iter().filter(|p| s.hidden_systems[p.index()]).map(platform_key).collect();
    let _ = writeln!(text, "hidden_systems={}", hidden.join(","));
    atomic_write(&state_path(root), text.as_bytes())
}

fn parse(text: &str) -> Option<SlotState> {
    let mut cart = None;
    let mut cart_platform = None;
    let mut brightness = None;
    let mut blue_light = None;
    let mut volume = None;
    let mut volume_hp = None;
    let mut muted = None;
    let mut muted_hp = None;
    let mut clock_set = None;
    let mut utc_offset_min = None;
    let mut auto_clock = None;
    let mut library_sort = None;
    let mut favorites_only = None;
    let mut hidden_systems = [false; Platform::ALL.len()];
    let mut play_time_hours = None;
    let mut title_card = None;
    let mut rumble = None;
    let mut menu_sounds = None;
    let mut sound_star = None;
    let mut sound_cart = None;
    let mut sound_cart_open = None;
    let mut sound_label = None;
    let mut sound_volume = None;
    let mut ff_speed = None;
    let mut ff_sound = None;
    let mut colour_correction = None;
    let mut colour_correction_gb = None;
    let mut colour_correction_gba = None;
    let mut shader_gba = None;
    let mut shader_gb = None;
    let mut shader_nes = None;
    let mut shader_snes = None;
    let mut shader_ps1 = None;
    let mut ps1_dithering = None;
    let mut eject_save = None;
    let mut autosave_min = None;
    let mut battery_shutdown = None;
    let mut turbo = None;
    let mut rewind = None;
    let mut gb_palettes = None;
    let mut gb_palette = None;
    let mut overlays: [Option<String>; Platform::ALL.len()] = Default::default();
    for line in text.lines().filter(|l| !l.is_empty()) {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key {
            "cart" => cart = Some(value.to_string()),
            "cart_platform" => cart_platform = platform_value(value),
            "brightness" => brightness = Some(level(value, BRIGHTNESS_MAX)?),
            "blue_light" => blue_light = Some(level(value, BLUE_LIGHT_MAX)?),
            "volume" => volume = Some(level(value, VOLUME_MAX)?),
            "volume_hp" => volume_hp = Some(level(value, VOLUME_MAX)?),
            "muted" => muted = Some(level(value, 1)? == 1),
            "muted_hp" => muted_hp = Some(level(value, 1)? == 1),
            "clock_set" => clock_set = Some(level(value, 1)? == 1),
            "utc_offset_min" => utc_offset_min = Some(offset(value)?),
            "auto_clock" => auto_clock = flag(value),
            "library_sort" => library_sort = crate::LibrarySort::parse(value),
            "favorites_only" => favorites_only = flag(value),
            // hidden_systems=gb,nes: a system not known (or not written) is shown.
            "hidden_systems" => {
                hidden_systems = [false; Platform::ALL.len()];
                for platform in value.split(',').map(str::trim).filter_map(platform_value) {
                    hidden_systems[platform.index()] = true;
                }
            }
            "play_time_hours" => play_time_hours = flag(value),
            "title_card" => title_card = flag(value),
            "rumble" => rumble = flag(value),
            "menu_sounds" => menu_sounds = flag(value),
            "sound_star" => sound_star = flag(value),
            "sound_cart" => sound_cart = flag(value),
            "sound_cart_open" => sound_cart_open = flag(value),
            "sound_label" => sound_label = flag(value),
            "sound_volume" => sound_volume = level(value, 100).filter(|v| v % SOUND_VOLUME_STEP == 0),
            "ff_speed" => ff_speed = ff_speed_value(value),
            "ff_sound" => ff_sound = flag(value),
            // Before it was one setting for all three: it carries over to both.
            "colour_correction" => colour_correction = flag(value),
            "colour_correction_gb" => colour_correction_gb = flag(value),
            "colour_correction_gba" => colour_correction_gba = flag(value),
            "shader_gba" => shader_gba = Shader::parse(value).filter(|s| Shader::GBA.contains(s)),
            "eject_save" => eject_save = flag(value),
            "autosave_min" => autosave_min = choice(value, &AUTOSAVE_MINS),
            "battery_shutdown" => battery_shutdown = choice(value, &BATTERY_SHUTDOWNS),
            "turbo" => turbo = flag(value),
            "rewind" => rewind = flag(value),
            "gb_palettes" => gb_palettes = flag(value),
            "gb_palette" => gb_palette = GbPalette::parse(value),
            "shader_gb" => shader_gb = Shader::parse(value).filter(|s| Shader::GB.contains(s)),
            "shader_nes" => shader_nes = Shader::parse(value).filter(|s| Shader::NES.contains(s)),
            "shader_snes" => shader_snes = Shader::parse(value).filter(|s| Shader::NES.contains(s)),
            "shader_ps1" => shader_ps1 = Shader::parse(value).filter(|s| Shader::NES.contains(s)),
            "ps1_dithering" => ps1_dithering = flag(value),
            _ => {
                // overlay_gb=<file name>: one for each system, empty for off.
                let system = key.strip_prefix("overlay_").and_then(platform_value);
                if let Some(platform) = system {
                    overlays[platform.index()] = Some(value.to_string()).filter(|n| crate::overlay_name_ok(n));
                }
            }
        }
    }
    let cart = cart?;
    let fallback = SlotState::default();
    Some(SlotState {
        cart: (!cart.is_empty()).then_some(cart),
        cart_platform,
        brightness: brightness?,
        blue_light: blue_light?,
        volume: volume?,
        volume_hp: volume_hp.or(volume)?,
        muted: muted?,
        muted_hp: muted_hp.or(muted)?,
        clock_set: clock_set?,
        utc_offset_min: utc_offset_min?,
        auto_clock: auto_clock.unwrap_or(fallback.auto_clock),
        library_sort: library_sort.unwrap_or(fallback.library_sort),
        favorites_only: favorites_only.unwrap_or(fallback.favorites_only),
        hidden_systems,
        play_time_hours: play_time_hours.unwrap_or(fallback.play_time_hours),
        title_card: title_card.unwrap_or(fallback.title_card),
        rumble: rumble.unwrap_or(fallback.rumble),
        menu_sounds: menu_sounds.unwrap_or(fallback.menu_sounds),
        sound_star: sound_star.unwrap_or(fallback.sound_star),
        sound_cart: sound_cart.unwrap_or(fallback.sound_cart),
        sound_cart_open: sound_cart_open.unwrap_or(fallback.sound_cart_open),
        sound_label: sound_label.unwrap_or(fallback.sound_label),
        sound_volume: sound_volume.unwrap_or(fallback.sound_volume),
        ff_speed: ff_speed.unwrap_or(fallback.ff_speed),
        ff_sound: ff_sound.unwrap_or(fallback.ff_sound),
        colour_correction_gb: colour_correction_gb.or(colour_correction).unwrap_or(fallback.colour_correction_gb),
        colour_correction_gba: colour_correction_gba.or(colour_correction).unwrap_or(fallback.colour_correction_gba),
        shader_gba: shader_gba.unwrap_or(fallback.shader_gba),
        shader_gb: shader_gb.unwrap_or(fallback.shader_gb),
        shader_nes: shader_nes.unwrap_or(fallback.shader_nes),
        shader_snes: shader_snes.unwrap_or(fallback.shader_snes),
        shader_ps1: shader_ps1.unwrap_or(fallback.shader_ps1),
        ps1_dithering: ps1_dithering.unwrap_or(fallback.ps1_dithering),
        eject_save: eject_save.unwrap_or(fallback.eject_save),
        autosave_min: autosave_min.unwrap_or(fallback.autosave_min),
        battery_shutdown: battery_shutdown.unwrap_or(fallback.battery_shutdown),
        turbo: turbo.unwrap_or(fallback.turbo),
        rewind: rewind.unwrap_or(fallback.rewind),
        gb_palettes: gb_palettes.unwrap_or(fallback.gb_palettes),
        gb_palette: gb_palette.unwrap_or(fallback.gb_palette),
        overlays,
    })
}

fn platform_key(platform: Platform) -> String {
    platform.dir_name().to_ascii_lowercase()
}

fn platform_value(value: &str) -> Option<Platform> {
    Platform::ALL
        .into_iter()
        .find(|p| value.eq_ignore_ascii_case(p.dir_name()))
}

fn offset(value: &str) -> Option<i16> {
    value
        .parse()
        .ok()
        .filter(|n| (UTC_OFFSET_MIN..=UTC_OFFSET_MAX).contains(n))
}

fn ff_speed_value(value: &str) -> Option<u8> {
    choice(value, &FF_SPEEDS)
}

fn choice(value: &str, choices: &[u8]) -> Option<u8> {
    value.parse().ok().filter(|n| choices.contains(n))
}

fn level(value: &str, max: u8) -> Option<u8> {
    value.parse().ok().filter(|n| *n <= max)
}

fn flag(value: &str) -> Option<bool> {
    level(value, 1).map(|n| n == 1)
}
