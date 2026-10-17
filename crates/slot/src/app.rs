use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use slot_gfx::{OUT_H, OUT_W};
use slot_input::{Action, Btn, MUTE_CHORD_MS};
use slot_power::{Battery, Charge, LedState, LidPolicy, Power};
use slot_retro::LinkChannel;
use slot_store::{
    format_stamp, read_slot_state, scan, write_slot_state, Cart, Core, GbPalette, Outline, Platform,
    Shader, ShellChoice, ShellFinish, SlotState, StateEntry, StateRing, Theme, BLUE_LIGHT_MAX,
    BRIGHTNESS_MAX, FF_SPEEDS, RING_MAX,
    VOLUME_MAX,
};
use slot_ui::{
    board_from, board_zoom, draw_backdrop, draw_empty_slot, draw_footer, draw_slot_name,
    draw_sticker, ease, grown, lid_at, lid_from, lift_of, on_board, shelf_cart_at, ClockPicker,
    Draw, FfState, GbShell, Hud, HudKind, Icon, LinkBadge, Millis, Placed, Polaroids, PowerChoice,
    QuickMenu, QuickMenuFaces, QuickRow, QuickValue, Refusal, Shelf, SlotChrome, TexId, Toast,
    CART_W, CHIP_H, CHIP_U, CHIP_V, CHIP_W, HINT_EDGE, HINT_H, HOP_LIFT,
    SHADOW_H, SHADOW_W, SOCKET_H, SOCKET_U, SOCKET_V, SOCKET_W, TURN_PAD,
};

use crate::audio::Sfx;
use crate::core_picker::{Chip, CorePicker, Outcome, Press};
use crate::link_kind::{link_carried, link_kind, serial_option, LinkKind};
use crate::link_radio::{radio_jobs, LinkRole, RadioJob, RadioJobs};
use crate::link_screen::LinkSprites;
use crate::link_start::{link_port, LinkFail, LinkProgress, LinkStarter, LinkStep};
use crate::persist::{self, Snapshot};
use crate::video_mode::{self, VideoMode};

pub const INSERT_S: f32 = 0.73;
const INSERT_HOLD_S: f32 = 0.28;

pub const SEATED_AT: f32 = INSERT_S - INSERT_HOLD_S;

pub const EJECT_S: f32 = SEATED_AT;

const EJECT_HOLD_S: f32 = 0.35;

const POWER_ON_S: f32 = 0.22;

const POWER_OFF_S: f32 = 0.16;

const VOLUME_STEP: u8 = 5;

const AUTOSAVE_MS: Millis = 60_000;
const CARD_CHECK_MS: Millis = 2_000;

const BATTERY_CRITICAL: u8 = 5;

const BATTERY_POLL_MS: Millis = 10_000;

const CHARGE_POLL_MS: Millis = 1_000;
const HEADPHONES_POLL_MS: Millis = 500;

#[cfg(feature = "brick")]
const BATTERY_LOW: u8 = 15;
#[cfg(not(feature = "brick"))]
const BATTERY_LOW: u8 = 20;

pub const UNDO_GRACE_MS: Millis = 30_000;

pub const LINK_LOST_MS: Millis = 2000;

const PLAY_HOLD_MS: Millis = 500;

const SHUTDOWN_SHOW_MS: Millis = 250;

const POWER_MENU_PITCH: f32 = 44.0;
const POWER_MENU_BAR_INSET: f32 = 4.0;
const CORE_PICKER_RECEDE: f32 = 0.26;

const SLOT_NAME_IN_MS: Millis = 200;
const SLOT_NAME_HOLD_MS: Millis = 1200;
const SLOT_NAME_OUT_MS: Millis = 800;
const SLOT_NAME_ALPHA: f32 = 0.4;
const CORE_PICKER_DIM: f32 = 0.614;
const CORE_LEGEND_Y: f32 = 386.0;
/// The core picker's legend keeps this far from the screen's edges.
const CORE_LEGEND_MARGIN: f32 = 16.0;
const LINK_TEXT_Y: f32 = 44.0;
const LINK_LEGEND_Y: f32 = 422.0;
const LINK_LEGEND_GAP: f32 = 40.0;
const LID_SHADOW_W: f32 = 168.0;
const LID_SHADOW_H: f32 = 18.0;
const LID_SHADOW_DROP: f32 = 29.0;
const LID_SHADOW_ALPHA: f32 = 0.8;
const FACES_WAIT_MS: Millis = 1500;

const ALERT_HOLD: f32 = 0.45;
const ALERT_GONE: f32 = 0.9;

const CLOCK_FLOOR: i64 = 1_577_836_800;

pub enum PendingUndo {
    Save {
        stamp: String,
        evicted: Option<(String, Vec<u8>, Vec<u8>)>,
    },
    Load {
        prior: Vec<u8>,
    },
}

struct LinkSession {
    client_id: u16,
    lost_at: Option<Millis>,
}

struct LinkStarting {
    starter: LinkStarter,
    client_id: u16,
}

struct Reload {
    stem: String,
    role: LinkRow,
    cancelled: bool,
    from: LinkKind,
    from_serial: &'static str,
    fallback: bool,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum GameMenu {
    Pick(LinkRow),
    Working {
        role: LinkRow,
        step: LinkStep,
        since: Millis,
    },
    Linked {
        role: LinkRow,
        worked: Millis,
        since: Millis,
        opened: bool,
    },
    Failed {
        role: LinkRow,
        fail: LinkFail,
        worked: Millis,
        since: Millis,
    },
    Unplug {
        role: LinkRow,
        since: Millis,
    },
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum LinkRow {
    Host,
    Join,
}

impl LinkRow {
    pub const ALL: [LinkRow; 2] = [LinkRow::Host, LinkRow::Join];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn text(self) -> &'static str {
        match self {
            LinkRow::Host => "Host",
            LinkRow::Join => "Join",
        }
    }

    pub fn role(self) -> LinkRole {
        match self {
            LinkRow::Host => LinkRole::Host,
            LinkRow::Join => LinkRole::Join,
        }
    }

    pub fn client_id(self) -> u16 {
        match self {
            LinkRow::Host => 0,
            LinkRow::Join => 1,
        }
    }

    pub fn other(self) -> LinkRow {
        match self {
            LinkRow::Host => LinkRow::Join,
            LinkRow::Join => LinkRow::Host,
        }
    }

    pub fn from_client_id(id: u16) -> LinkRow {
        if id == 0 {
            LinkRow::Host
        } else {
            LinkRow::Join
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum LinkLegend {
    Cancel,
    Mode,
    Swap,
    Link,
    Ok,
    Back,
    EndLink,
}

impl LinkLegend {
    pub const ALL: [LinkLegend; 7] = [
        LinkLegend::Cancel,
        LinkLegend::Mode,
        LinkLegend::Swap,
        LinkLegend::Link,
        LinkLegend::Ok,
        LinkLegend::Back,
        LinkLegend::EndLink,
    ];

    pub fn index(self) -> usize {
        self as usize
    }
}

pub const LINKED_HOLD_MS: Millis = 1000;

pub const UNPLUG_HOLD_MS: Millis = 420;

#[derive(Debug)]
pub enum Phase {
    SetClock {
        picker: ClockPicker,
        seed: i64,
        from_menu: bool,
    },
    Shelf,
    QuickMenu {
        row: QuickRow,
    },
    Inserting {
        cart: String,
        t: f32,
        core_ready: bool,
        resumed: bool,
        clean: bool,
    },
    Playing {
        cart: String,
    },
    Ejecting {
        cart: String,
        t: f32,
    },
    Polaroids {
        cart: String,
    },
    About,
    Doze {
        cart: Option<String>,
    },
}

pub struct App {
    #[cfg(feature = "brick")]
    pub(crate) wifi: Option<crate::brick_wifi::WifiMenu>,
    #[cfg(feature = "brick")]
    wifi_power: Option<Action>,
    /// wlan0 is associated, polled with the Fn switch; Cart Art and RetroAchievements need it.
    #[cfg(feature = "brick")]
    net_up: bool,
    phase: Phase,
    shelves: Vec<(Platform, Shelf)>,
    /// Faces of carts whose label changed, for CartFaces to recycle and rebuild.
    stale_faces: Vec<TexId>,
    /// Two-line notices waiting their turn across the top of the screen (RetroAchievements).
    /// Title, detail, badge image, when it was queued.
    banners: std::collections::VecDeque<(String, String, Option<std::path::PathBuf>, Millis)>,
    /// The one showing: id, title, detail, badge image, when it came up.
    banner: Option<(u64, String, String, Option<std::path::PathBuf>, Millis)>,
    banner_seq: u64,
    /// The rendered face, for the banner of that id.
    banner_face: Option<(u64, TexId)>,
    shelf_at: usize,
    play_held: Option<Millis>,
    refusal: Option<Refusal>,
    refused_from: Option<f32>,
    alert_face: Option<TexId>,
    shutdown_faces: Vec<(TexId, u32, u32)>,
    power_menu: Option<usize>,
    power_menu_faces: Vec<(TexId, u32, u32)>,
    core_picker: Option<CorePicker>,
    /// START on a cart: its plastic and shape.
    cart_menu: Option<CartMenu>,
    /// The cart's plastic while the core picker is open: Up/Down change it there.
    picker_look: Option<CartMenu>,
    cart_menu_faces: Option<slot_ui::CartMenuFaces>,
    /// Labels found for the cart open in the cart menu or the core picker, to choose from.
    label_pick: Option<LabelPick>,
    /// The label row's words as last drawn, [dim, lit]: rebuilt by the frontend when they change.
    /// The label grid's pictures and words, drawn and uploaded by the frontend.
    label_grid_faces: Option<LabelGridFaces>,
    /// The picture being framed by hand, and the words under it.
    label_adjust_faces: Option<LabelAdjustFaces>,
    core_board_face: Option<TexId>,
    core_lid_face: Option<TexId>,
    core_faces_stem: Option<String>,
    core_socket_faces: Vec<TexId>,
    core_chip_faces: Vec<TexId>,
    core_blank_chip_face: Option<TexId>,
    core_chip_shadow_face: Option<TexId>,
    core_legend_faces: Vec<(TexId, u32)>,
    /// Y, which searches for the cart's label: above the legend, off to one side.
    core_search_face: Option<(TexId, u32)>,
    game_menu: Option<GameMenu>,
    link_sprites: Option<LinkSprites>,
    link_hardware: LinkKind,
    last_role: LinkRow,
    link_choices: HashMap<String, LinkKind>,
    link_loaded: Option<&'static str>,
    link_reload: Option<(String, &'static str)>,
    reload: Option<Reload>,
    link_menu_faces: Vec<(TexId, u32, u32)>,
    link_linked_face: Option<(TexId, u32, u32)>,
    link_step_faces: Vec<(TexId, u32, u32)>,
    link_fail_faces: Vec<(TexId, u32, u32)>,
    link_legend_faces: Vec<(TexId, u32)>,
    starting: Option<LinkStarting>,
    link_transport: Option<(u16, Box<dyn LinkChannel>)>,
    restarting: bool,
    act_at: Millis,
    root: Option<PathBuf>,
    state: SlotState,
    vol_before: Vec<(u8, bool, Millis)>,
    snapshot: Option<Box<dyn Snapshot>>,
    core: Core,
    colour_pending: Option<bool>,
    palette_live: bool,
    palette_pending: Option<GbPalette>,
    link_player: Option<u8>,
    solo_serial: Option<&'static str>,
    platform: Platform,
    named_core: bool,
    video_mode: VideoMode,
    link: Option<LinkSession>,
    sfx: Option<Sfx>,
    polaroids: Option<Polaroids>,
    pending: Option<(PendingUndo, Millis)>,
    legend_faces: Vec<TexId>,
    undo_face: Option<TexId>,
    clock_faces: Option<(TexId, TexId)>,
    quick_menu_faces: Option<QuickMenuFaces>,
    quick_clock_faces: Option<[(TexId, u32, u32); 2]>,
    sticker_face: Option<TexId>,
    wallpaper: Option<TexId>,
    battery_percent: slot_ui::Printed,
    bolt: Option<TexId>,
    shelf_platform: slot_ui::Printed,
    name_pending: bool,
    slot_letter: Option<char>,
    shelf_named: Option<Millis>,
    shelf_clock: slot_ui::Printed,
    hud: Hud,
    screen: f32,
    lift: f32,
    game_ready: bool,
    clock: f64,
    power: Option<Power>,
    dozed_at: Millis,
    woke_on_press: bool,
    autosave_at: Millis,
    pending_save: Option<std::thread::JoinHandle<()>>,
    card_failing: Arc<AtomicBool>,
    card_read_only: bool,
    card_check_at: Millis,
    state_writer: crate::state_writer::StateWriter,
    battery_at: Millis,
    charge_at: Millis,
    headphones: bool,
    headphones_at: Millis,
    battery: Option<Battery>,
    last_led: Option<LedState>,
    #[cfg(feature = "brick")]
    brick_leds: Option<crate::brick_led::BrickLeds>,
    #[cfg(feature = "brick")]
    brick_led_failed: bool,
    #[cfg(feature = "brick")]
    brick_mute: Option<bool>,
    #[cfg(feature = "brick")]
    brick_switch_at: Millis,
    powering_off: bool,
    radio: Box<dyn RadioJobs>,
}

const FACE_AHEAD: i32 = 8;
const FACE_KEEP: i32 = 12;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct FaceWant {
    pub shelf: usize,
    pub index: usize,
    pub on_screen: bool,
}

/// How long a toast stays up, and how long it takes to slide in and out.
const BANNER_MS: Millis = 4500;
const BANNER_SLIDE: Millis = 300;
/// The longest a toast waits for its badge to download.
const BADGE_WAIT: Millis = 1500;

fn shelves_of(carts: Vec<Cart>) -> Vec<(Platform, Shelf)> {
    let mut rows: Vec<(Platform, Vec<Cart>)> =
        Platform::ALL.iter().map(|p| (*p, Vec::new())).collect();
    for cart in carts {
        if let Some((_, row)) = rows.iter_mut().find(|(p, _)| *p == cart.platform) {
            row.push(cart);
        }
    }
    rows.into_iter()
        .map(|(platform, carts)| (platform, Shelf::new(carts)))
        .collect()
}

impl App {
    pub fn new(carts: Vec<Cart>) -> Self {
        let shelves = shelves_of(carts);
        let shelf_at = shelves
            .iter()
            .position(|(_, s)| !s.carts.is_empty())
            .unwrap_or(0);
        let state_writer = crate::state_writer::StateWriter::spawn();
        let card_failing = state_writer.card_failing();
        App {
            radio: radio_jobs(),
            state_writer,
            card_failing,
            card_read_only: false,
            card_check_at: 0,
            phase: Phase::Shelf,
            shelves,
            stale_faces: Vec::new(),
            banners: std::collections::VecDeque::new(),
            banner: None,
            banner_seq: 0,
            banner_face: None,
            shelf_at,
            play_held: None,
            refusal: None,
            refused_from: None,
            alert_face: None,
            shutdown_faces: Vec::new(),
            #[cfg(feature = "brick")]
            wifi: None,
            #[cfg(feature = "brick")]
            wifi_power: None,
            #[cfg(feature = "brick")]
            net_up: false,
            power_menu: None,
            power_menu_faces: Vec::new(),
            core_picker: None,
            cart_menu: None,
            picker_look: None,
            cart_menu_faces: None,
            label_pick: None,
            label_grid_faces: None,
            label_adjust_faces: None,
            core_board_face: None,
            core_lid_face: None,
            core_faces_stem: None,
            core_socket_faces: Vec::new(),
            core_chip_faces: Vec::new(),
            core_blank_chip_face: None,
            core_chip_shadow_face: None,
            core_legend_faces: Vec::new(),
            core_search_face: None,
            game_menu: None,
            link_sprites: None,
            link_hardware: LinkKind::Cable,
            last_role: LinkRow::Host,
            link_choices: HashMap::new(),
            link_loaded: None,
            link_reload: None,
            reload: None,
            link_menu_faces: Vec::new(),
            link_linked_face: None,
            link_step_faces: Vec::new(),
            link_fail_faces: Vec::new(),
            link_legend_faces: Vec::new(),
            starting: None,
            link_transport: None,
            restarting: false,
            act_at: 0,
            root: None,
            state: SlotState::default(),
            vol_before: Vec::new(),
            snapshot: None,
            core: Core::default(),
            colour_pending: None,
            palette_live: false,
            palette_pending: None,
            link_player: None,
            solo_serial: None,
            platform: Platform::default(),
            named_core: false,
            video_mode: VideoMode::default(),
            link: None,
            sfx: None,
            polaroids: None,
            pending: None,
            legend_faces: Vec::new(),
            undo_face: None,
            clock_faces: None,
            quick_menu_faces: None,
            quick_clock_faces: None,
            sticker_face: None,
            wallpaper: None,
            battery_percent: slot_ui::Printed::default(),
            bolt: None,
            shelf_platform: slot_ui::Printed::default(),
            name_pending: false,
            slot_letter: None,
            shelf_named: None,
            shelf_clock: slot_ui::Printed::default(),
            hud: Hud::new(),
            screen: 0.0,
            lift: 1.0,
            game_ready: false,
            clock: 0.0,
            power: None,
            dozed_at: 0,
            woke_on_press: false,
            autosave_at: AUTOSAVE_MS,
            pending_save: None,
            battery_at: BATTERY_POLL_MS,
            charge_at: CHARGE_POLL_MS,
            headphones: false,
            headphones_at: 0,
            battery: None,
            last_led: None,
            #[cfg(feature = "brick")]
            brick_leds: None,
            #[cfg(feature = "brick")]
            brick_led_failed: false,
            #[cfg(feature = "brick")]
            brick_mute: None,
            #[cfg(feature = "brick")]
            brick_switch_at: 0,
            powering_off: false,
        }
    }

    pub fn boot(root: &Path) -> Self {
        crate::root::ensure(root);
        slot_ui::set_theme(Theme::read(root));
        let mut app = App::new(scan(root).unwrap_or_default());
        app.root = Some(root.to_path_buf());
        #[cfg(feature = "brick")]
        { app.brick_leds = Some(crate::brick_led::BrickLeds::open(root)); }
        app.state = read_slot_state(root);
        if app.state.clock_set {
            app.start();
        } else {
            app.phase = clock_screen(system_secs(), 0, false);
        }
        app
    }

    fn start(&mut self) {
        let seated = if self.single_cart() {
            Some((self.shelf_at, 0))
        } else {
            let stem = self.state.cart.clone();
            let platform = self.state.cart_platform;
            stem.and_then(|stem| self.seat_of(&stem, platform))
        };
        self.phase = Phase::Shelf;
        match seated {
            Some((at, i)) => {
                self.shelf_at = at;
                self.shelf_mut().select(i);
                self.insert(false);
                if let Phase::Inserting { resumed, t, .. } = &mut self.phase {
                    *resumed = true;
                    *t = INSERT_S;
                }
            }
            None => {
                self.state.cart = None;
                self.state.cart_platform = None;
                self.name_pending = true;
            }
        }
    }

    fn shelf(&self) -> &Shelf {
        &self.shelves[self.shelf_at].1
    }

    fn shelf_mut(&mut self) -> &mut Shelf {
        &mut self.shelves[self.shelf_at].1
    }

    fn switch_shelf(&mut self, by: i32) {
        let Some(to) = self.next_shelf(by) else {
            return;
        };
        self.shelf_mut().release_hold();
        self.play_held = None;
        self.shelf_at = to;
        self.shelf_platform = slot_ui::Printed::default();
        self.slot_letter = None;
        self.shelf_named = None;
        self.name_pending = true;
    }

    fn jump_letter(&mut self, dir: i32) {
        let before = self.shelf().index;
        match dir > 0 {
            true => self.shelf_mut().jump_next_letter(),
            false => self.shelf_mut().jump_prev_letter(),
        }
        if self.shelf().index == before {
            return;
        }
        let Some(letter) = self.selected_stem().map(slot_store::initial) else {
            return;
        };
        if self.slot_letter != Some(letter) {
            self.shelf_platform = slot_ui::Printed::default();
        }
        self.slot_letter = Some(letter);
        self.shelf_named = None;
        self.name_pending = true;
    }

    fn slot_name_alpha(&self) -> f32 {
        let Some(at) = self.shelf_named else {
            return 0.0;
        };
        let t = self.now().saturating_sub(at);
        let level = if t < SLOT_NAME_IN_MS {
            t as f32 / SLOT_NAME_IN_MS as f32
        } else if t < SLOT_NAME_IN_MS + SLOT_NAME_HOLD_MS {
            1.0
        } else {
            let out = t - SLOT_NAME_IN_MS - SLOT_NAME_HOLD_MS;
            1.0 - (out as f32 / SLOT_NAME_OUT_MS as f32).min(1.0)
        };
        SLOT_NAME_ALPHA * ease(level)
    }

    fn next_shelf(&self, by: i32) -> Option<usize> {
        let n = self.shelves.len() as i32;
        (1..n)
            .map(|step| (self.shelf_at as i32 + by * step).rem_euclid(n) as usize)
            .find(|at| !self.shelves[*at].1.carts.is_empty())
    }

    fn seat_of(&self, stem: &str, platform: Option<Platform>) -> Option<(usize, usize)> {
        self.shelves
            .iter()
            .enumerate()
            .filter(|(_, (p, _))| platform.is_none_or(|want| *p == want))
            .find_map(|(at, (_, shelf))| {
                shelf
                    .carts
                    .iter()
                    .position(|c| c.stem == stem)
                    .map(|i| (at, i))
            })
    }

    pub fn confirm_clock(&mut self) {
        let Phase::SetClock {
            picker,
            seed,
            from_menu,
        } = &self.phase
        else {
            return;
        };
        let (moved, offset, from_menu) = (picker.secs() - *seed, picker.offset_min(), *from_menu);
        let utc = self.utc_secs() + moved;
        if let Some(power) = &mut self.power {
            power.set_clock(utc);
        }
        self.state.utc_offset_min = offset as i16;
        self.state.clock_set = true;
        self.persist();
        if from_menu {
            self.phase = Phase::QuickMenu {
                row: QuickRow::DateTime,
            };
        } else {
            self.start();
        }
    }

    pub fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    pub fn wall_secs(&self) -> i64 {
        self.utc_secs() + i64::from(self.state.utc_offset_min) * 60
    }

    fn utc_secs(&self) -> i64 {
        self.power.as_ref().map_or_else(system_secs, |p| p.now())
    }

    pub fn picker(&self) -> Option<&ClockPicker> {
        match &self.phase {
            Phase::SetClock { picker, .. } => Some(picker),
            _ => None,
        }
    }

    pub fn quick_menu(&self) -> Option<QuickRow> {
        match self.phase {
            Phase::QuickMenu { row } => Some(row),
            _ => None,
        }
    }

    pub fn quick_value(&self, row: QuickRow) -> Option<QuickValue> {
        #[cfg(feature = "brick")]
        if row.led_setting() { return self.brick_leds.as_ref().and_then(|leds| leds.value(row)).or(Some(QuickValue::Unavailable)); }

        match row {
            QuickRow::FastForward => QuickValue::speed(self.state.ff_speed),
            QuickRow::FastForwardSound => Some(QuickValue::flag(self.state.ff_sound)),
            QuickRow::ColourCorrection => Some(QuickValue::flag(self.state.colour_correction)),
            QuickRow::GbPalettes => Some(QuickValue::flag(self.state.gb_palettes)),
            QuickRow::GbaShader => Some(shader_value(self.state.shader_gba)),
            QuickRow::GbShader => Some(shader_value(self.state.shader_gb)),
            QuickRow::Rumble => Some(QuickValue::flag(self.state.rumble)),
            QuickRow::EjectSave => Some(QuickValue::flag(self.state.eject_save)),
            QuickRow::Turbo => Some(QuickValue::flag(self.state.turbo)),
            QuickRow::Rewind => Some(QuickValue::flag(self.state.rewind)),
            QuickRow::Wifi | QuickRow::CartArt | QuickRow::Achievements | QuickRow::DateTime | QuickRow::About | QuickRow::Screen | QuickRow::Game | QuickRow::Lights | QuickRow::LedMode | QuickRow::LedTop | QuickRow::LedButtons | QuickRow::LedTriggers | QuickRow::LedColor | QuickRow::LedEffect => None,
        }
    }

    pub fn set_quick_menu_faces(&mut self, faces: QuickMenuFaces) {
        self.quick_menu_faces = Some(faces);
    }

    pub fn set_quick_clock_faces(&mut self, dim: (TexId, u32, u32), lit: (TexId, u32, u32)) {
        self.quick_clock_faces = Some([dim, lit]);
    }

    pub fn set_sticker_face(&mut self, face: TexId) {
        self.sticker_face = Some(face);
    }

    pub fn set_clock_faces(&mut self, line: TexId, hint: TexId) {
        self.clock_faces = Some((line, hint));
    }

    pub fn set_cart_shadow(&mut self, face: TexId) {
        for (_, shelf) in &mut self.shelves {
            shelf.set_shadow(face);
        }
    }

    pub fn set_nes_cart_shadows(&mut self, nes: TexId, famicom: TexId) {
        for (_, shelf) in &mut self.shelves {
            shelf.set_nes_shadow(slot_ui::NesShell::Nes, nes);
            shelf.set_nes_shadow(slot_ui::NesShell::Famicom, famicom);
        }
    }

    pub fn set_snes_cart_shadows(&mut self, us: TexId, pal: TexId) {
        for (_, shelf) in &mut self.shelves {
            shelf.set_snes_shadow(slot_ui::SnesShell::Us, us);
            shelf.set_snes_shadow(slot_ui::SnesShell::Pal, pal);
        }
    }

    pub fn set_gb_cart_shadows(&mut self, notched: TexId, rounded: TexId) {
        for (_, shelf) in &mut self.shelves {
            shelf.set_gb_shadow(GbShell::Notched, notched);
            shelf.set_gb_shadow(GbShell::Rounded, rounded);
        }
    }

    pub fn set_wallpaper(&mut self, face: TexId) {
        self.wallpaper = Some(face);
    }

    pub fn set_bolt_face(&mut self, bolt: TexId) {
        self.bolt = Some(bolt);
    }

    pub fn set_shelf_platform_face(&mut self, face: TexId, w: u32) {
        self.shelf_platform = slot_ui::Printed::new(face, w);
    }

    pub fn clear_shelf_platform(&mut self) {
        self.shelf_platform = slot_ui::Printed::default();
    }

    pub fn slot_text(&self) -> Option<String> {
        match self.slot_letter {
            Some(letter) => Some(letter.to_string()),
            None => self.shelf_platform_name().map(str::to_string),
        }
    }

    pub fn shelf_platform_name(&self) -> Option<&'static str> {
        self.next_shelf(1)?;
        Some(self.shelves[self.shelf_at].0.name())
    }

    pub fn set_battery_percent_face(&mut self, face: TexId, w: u32) {
        self.battery_percent = slot_ui::Printed::new(face, w);
    }

    pub fn set_shelf_clock_face(&mut self, face: TexId, w: u32) {
        self.shelf_clock = slot_ui::Printed::new(face, w);
    }

    pub fn phase(&self) -> &Phase {
        &self.phase
    }

    pub fn carts(&self) -> impl Iterator<Item = &Cart> {
        self.shelves
            .iter()
            .flat_map(|(_, shelf)| shelf.carts.iter())
    }

    pub fn seated_cart(&self) -> Option<&Cart> {
        let stem = match &self.phase {
            Phase::Inserting { cart, .. }
            | Phase::Playing { cart }
            | Phase::Ejecting { cart, .. }
            | Phase::Polaroids { cart } => cart,
            Phase::Doze { cart: Some(cart) } => cart,
            _ => return None,
        };
        self.shelf().carts.iter().find(|c| c.stem == *stem)
    }

    pub fn single_cart(&self) -> bool {
        self.carts().count() == 1
    }

    pub fn set_faces(&mut self, faces: Vec<TexId>) {
        let mut faces = faces.into_iter();
        for (_, shelf) in &mut self.shelves {
            let n = shelf.carts.len();
            shelf.set_faces(faces.by_ref().take(n).collect());
        }
    }

    pub fn face_wants(&self) -> Vec<FaceWant> {
        let here = self.shelf();
        let screen = here.on_screen();
        let mut wants: Vec<(usize, usize)> = screen.iter().map(|i| (self.shelf_at, *i)).collect();
        for at in self.neighbour_shelves() {
            wants.extend(self.shelves[at].1.on_screen().into_iter().map(|i| (at, i)));
        }
        wants.extend(
            here.around(FACE_AHEAD)
                .into_iter()
                .filter(|i| !screen.contains(i))
                .map(|i| (self.shelf_at, i)),
        );
        wants
            .into_iter()
            .filter(|(at, i)| self.shelves[*at].1.face(*i).is_none())
            .map(|(shelf, index)| FaceWant {
                shelf,
                index,
                on_screen: shelf == self.shelf_at && screen.contains(&index),
            })
            .collect()
    }

    pub fn face_cart(&self, shelf: usize, index: usize) -> Option<&Cart> {
        self.shelves.get(shelf)?.1.carts.get(index)
    }

    pub fn set_cart_face(&mut self, shelf: usize, index: usize, tex: TexId) -> Option<TexId> {
        self.shelves.get_mut(shelf)?.1.set_face(index, tex)
    }

    /// The Wi-Fi / Cart Art overlay is open.
    #[cfg(feature = "brick")]
    pub fn overlay_open(&self) -> bool {
        self.wifi.is_some()
    }

    fn quick_disabled(&self, row: QuickRow) -> bool {
        #[cfg(feature = "brick")]
        if matches!(row, QuickRow::CartArt | QuickRow::Achievements) {
            return !self.net_up;
        }
        let _ = row;
        false
    }

    /// A label fetched or deleted while running: the game's labels are read again (a NES
    /// game has one for each cart) and its face dropped so it is rebuilt.
    pub fn adopt_label(&mut self, platform: Platform, stem: &str, label: Option<std::path::PathBuf>) {
        let (own, famicom) = match &self.root {
            Some(root) => slot_store::labels_of(root, platform, stem),
            None => (label, None),
        };
        for (p, shelf) in &mut self.shelves {
            if *p != platform {
                continue;
            }
            if let Some(i) = shelf.carts.iter().position(|c| c.stem == stem) {
                shelf.carts[i].label = own.clone();
                shelf.carts[i].famicom_label = famicom.clone();
                self.stale_faces.extend(shelf.take_face(i));
            }
        }
    }

    pub fn shed_faces(&mut self) -> Vec<TexId> {
        let neighbours = self.neighbour_shelves();
        let mut shed = std::mem::take(&mut self.stale_faces);
        for (at, (_, shelf)) in self.shelves.iter_mut().enumerate() {
            let keep = if at == self.shelf_at {
                shelf.around(FACE_KEEP)
            } else if neighbours.contains(&at) {
                shelf.on_screen()
            } else {
                Vec::new()
            };
            for i in 0..shelf.carts.len() {
                if !keep.contains(&i) {
                    shed.extend(shelf.take_face(i));
                }
            }
        }
        shed
    }

    fn neighbour_shelves(&self) -> Vec<usize> {
        let mut out = Vec::new();
        for by in [1, -1] {
            if let Some(at) = self.next_shelf(by) {
                if !out.contains(&at) {
                    out.push(at);
                }
            }
        }
        out
    }

    pub fn set_snapshot(&mut self, snapshot: Box<dyn Snapshot>) {
        self.snapshot = Some(snapshot);
    }

    pub fn set_core(&mut self, core: Core) {
        self.core = core;
    }

    pub fn set_platform(&mut self, platform: Platform) {
        self.platform = platform;
    }

    pub fn set_named_core(&mut self, named: bool) {
        self.named_core = named;
    }

    pub fn set_video_mode(&mut self, mode: VideoMode) {
        self.video_mode = mode;
    }

    pub fn source_rect(&self) -> [f32; 4] {
        video_mode::source_rect(self.platform, self.video_mode)
    }

    /// L1/R1 switch the picture on consoles without shoulder buttons; on a GBA or SNES
    /// they are the game's own L and R.
    fn slot_owns_the_shoulders(&self) -> bool {
        matches!(self.phase, Phase::Playing { .. })
            && matches!(self.platform, Platform::Gb | Platform::Gbc | Platform::Nes)
    }

    pub fn taken_buttons(&self) -> &'static [Btn] {
        if self.slot_owns_the_shoulders() {
            &[Btn::L1, Btn::R1]
        } else {
            &[]
        }
    }

    pub fn takes_from_the_game(&self, action: Action) -> bool {
        match action {
            Action::GbaDown(btn) | Action::GbaUp(btn) => self.taken_buttons().contains(&btn),
            _ => false,
        }
    }

    fn set_picture(&mut self, mode: VideoMode) {
        if self.video_mode == mode {
            return;
        }
        self.video_mode = mode;
        let (Some(root), Phase::Playing { cart }) = (self.root.clone(), &self.phase) else {
            return;
        };
        let written = video_mode::write_video_mode(&root, cart, mode);
        if let Err(e) = &written {
            eprintln!("slot: video: could not write video_mode.ini: {e}");
        }
        self.card_failing.store(written.is_err(), Ordering::Relaxed);
    }

    /// Home+X on the Brick: GBA between its true 3:2 shape and 4:3 full screen.
    /// Remembered per game, like the L1/R1 choice for GB/GBC.
    fn toggle_gba_picture(&mut self) {
        if !matches!(self.phase, Phase::Playing { .. }) || self.game_menu_open() || self.power_menu.is_some() {
            return;
        }
        // Consoles whose L and R are the game's take this shortcut instead.
        if !matches!(self.platform, Platform::Gba | Platform::Snes) {
            self.refuse();
            return;
        }
        let (to, said) = match self.video_mode {
            VideoMode::Actual => (VideoMode::Stretch, Toast::PictureFull),
            VideoMode::Stretch => (VideoMode::Actual, Toast::PictureOriginal),
        };
        self.set_picture(to);
        self.hud.toast(said, self.now());
    }

    pub fn set_link_loaded(&mut self, serial: &'static str) {
        self.link_loaded = Some(serial);
    }

    pub fn link_mode(&self, stem: &str) -> (LinkKind, &'static str) {
        let Some((cart, auto)) = self.auto_link(stem) else {
            return (LinkKind::Cable, "auto");
        };
        let chosen = self.link_choices.get(stem).copied().unwrap_or(auto);
        (chosen, serial_option(chosen, auto, &cart.code, &cart.title))
    }

    pub fn link_active(&self) -> bool {
        self.link.is_some()
    }

    pub fn begin_link(&mut self, client_id: u16) {
        self.link = Some(LinkSession {
            client_id,
            lost_at: None,
        });
        self.sync_link_badge();
    }

    pub fn link_client_id(&self) -> Option<u16> {
        self.link.as_ref().map(|s| s.client_id)
    }

    pub fn may_rewind(&self) -> bool {
        self.state.rewind && !self.link_active()
    }

    pub fn turbo(&self) -> bool {
        self.state.turbo
    }

    pub fn records_rewind(&self) -> bool {
        self.state.rewind
    }

    /// The canvas the running core draws into.
    pub fn canvas(&self) -> (u32, u32) {
        self.platform.canvas()
    }

    pub fn picture_rect(&self) -> [f32; 4] {
        let (w, h) = self.platform.picture();
        let (sw, sh) = self.platform.canvas();
        let (sw, sh) = (sw as f32, sh as f32);
        let (x, y) = ((sw - w as f32) / 2.0, (sh - h as f32) / 2.0);
        [x / sw, y / sh, (x + w as f32) / sw, (y + h as f32) / sh]
    }

    pub fn may_load_state(&self) -> bool {
        !self.link_active()
    }

    pub fn may_fast_forward(&self) -> bool {
        !self.link_active()
    }

    pub fn end_link(&mut self) {
        self.link = None;
        self.link_player = None;
        self.solo_serial = None;
        self.sync_link_badge();
        self.radio.ask(RadioJob::Down);
        self.radio.ask(RadioJob::Cool);
    }

    pub fn peer_lost(&mut self) {
        if matches!(self.game_menu, Some(GameMenu::Linked { .. })) {
            self.game_menu = None;
        }
        let now = self.now();
        if let Some(session) = &mut self.link {
            session.lost_at.get_or_insert(now);
        }
        self.sync_link_badge();
    }

    pub fn bios_mismatch(&mut self) {
        if !self.link_active() {
            return;
        }
        let role = self
            .link_client_id()
            .map_or(self.last_role, LinkRow::from_client_id);
        self.end_link();
        self.hud.toast(Toast::BiosMismatch, self.now());
        self.unplug(role);
    }

    pub fn peer_ended(&mut self) {
        if !self.link_active() {
            return;
        }
        let role = self
            .link_client_id()
            .map_or(self.last_role, LinkRow::from_client_id);
        self.end_link();
        self.hud.toast(Toast::PeerEnded, self.now());
        self.unplug(role);
    }

    pub fn link_badge(&self) -> LinkBadge {
        self.hud.link()
    }

    pub fn set_link_badge_faces(&mut self, faces: Vec<TexId>) {
        self.hud.set_link_faces(faces);
    }

    fn sync_link_badge(&mut self) {
        let badge = match &self.link {
            None => LinkBadge::Off,
            Some(s) => match (s.client_id == 0, s.lost_at.is_some()) {
                (true, false) => LinkBadge::Hosting,
                (false, false) => LinkBadge::Joined,
                (true, true) => LinkBadge::HostingLost,
                (false, true) => LinkBadge::JoinedLost,
            },
        };
        self.hud.set_link(badge);
    }

    pub fn take_sfx(&mut self) -> Option<Sfx> {
        self.sfx.take()
    }

    pub fn set_power(&mut self, mut power: Power) {
        power.set_backlight(self.state.brightness);
        self.headphones = power.headphones();
        self.hud.set_headphones(self.headphones);
        let secs = power.now();
        if matches!(self.phase, Phase::SetClock { .. }) || secs < CLOCK_FLOOR {
            self.phase = clock_screen(secs, 0, false);
        }
        self.power = Some(power);
        self.battery_at = self.now();
        self.charge_at = self.now();
        self.last_led = None;
    }

    pub fn set_rumble(&mut self, strength: u16) {
        if let Some(power) = &mut self.power {
            power.set_rumble(strength);
        }
    }

    pub fn rumble_enabled(&self) -> bool {
        self.state.rumble
    }

    pub fn ff_speed(&self) -> u8 {
        self.state.ff_speed
    }

    pub fn ff_sound(&self) -> bool {
        self.state.ff_sound
    }

    pub fn screen_shader(&self) -> Shader {
        match self.platform {
            Platform::Gba => self.state.shader_gba,
            Platform::Gb | Platform::Gbc => self.state.shader_gb,
            Platform::Nes => self.state.shader_nes,
            Platform::Snes => self.state.shader_snes,
        }
    }

    pub fn colour_correction(&self) -> bool {
        self.state.colour_correction
    }

    pub fn powering_off(&self) -> bool {
        self.powering_off
    }

    pub fn ready_to_power_off(&self) -> bool {
        self.powering_off && self.now() >= self.act_at
    }

    pub fn ready_to_restart(&self) -> bool {
        self.restarting && self.now() >= self.act_at
    }

    pub fn shutting_down(&self) -> bool {
        self.powering_off || self.restarting
    }

    pub fn restarting(&self) -> bool {
        self.restarting
    }

    pub fn restore_brick_leds(&self) {
        #[cfg(feature = "brick")]
        if let Some(leds)=&self.brick_leds { if let Err(e)=leds.restore() { eprintln!("slot: LED restore: {e}"); } }
    }

    pub fn restart(&mut self) {
        self.settle_saves();
        self.restore_brick_leds();
        if let Some(power) = &mut self.power {
            power.restart();
        }
    }

    pub fn power_menu(&self) -> Option<usize> {
        self.power_menu
    }

    pub fn core_picker(&self) -> Option<Core> {
        self.core_picker.map(|p| p.seat())
    }

    pub fn core_picker_chip(&self) -> Option<Chip> {
        self.core_picker.map(|p| p.chip(self.now()))
    }

    pub fn game_menu_open(&self) -> bool {
        self.game_menu.is_some()
    }

    pub fn game_menu(&self) -> Option<GameMenu> {
        self.game_menu
    }

    pub fn take_link_transport(&mut self) -> Option<(u16, Box<dyn LinkChannel>)> {
        self.link_transport.take()
    }

    pub fn take_link_reload(&mut self) -> Option<(String, &'static str)> {
        self.link_reload.take()
    }

    /// A failed/cancelled link can leave the core in expensive link mode.
    /// Session performs the normal save/resume reload once the attempt settles.
    pub fn take_solo_reload(&mut self) -> Option<(String, &'static str)> {
        if self.link_active() || self.starting.is_some() || self.reload.is_some()
            || self.link_reload.is_some() || self.shutting_down()
            || matches!(self.game_menu, Some(GameMenu::Working { .. }))
            || !matches!(self.phase, Phase::Playing { .. }) {
            return None;
        }
        let serial = self.solo_serial?;
        if self.snapshot.as_ref().is_some_and(|s| !s.resume_trusted()) {return None;}
        let stem = self.seated()?.to_string();
        self.solo_serial = None;
        let was_link_player = self.link_player.take().is_some();
        self.radio.ask(RadioJob::Cool);
        if was_link_player || self.link_loaded != Some(serial) {
            Some((stem, serial))
        } else { None }
    }

    pub fn link_player(&self) -> Option<u8> {
        self.link_player
    }

    pub fn take_colour_correction(&mut self) -> Option<bool> {
        self.colour_pending.take()
    }

    pub fn gb_palette(&self) -> Option<GbPalette> {
        self.state.gb_palettes.then_some(self.state.gb_palette)
    }

    pub fn set_palette_live(&mut self, live: bool) {
        self.palette_live = live;
    }

    pub fn palette_live(&self) -> bool {
        self.palette_live
    }

    pub fn take_gb_palette(&mut self) -> Option<GbPalette> {
        self.palette_pending.take()
    }

    pub fn core(&self) -> Core {
        self.core
    }

    pub fn link_reload_done(&mut self) {
        let Some(reload) = self.reload.take() else {
            return;
        };
        if reload.fallback {
            self.link_choices.insert(reload.stem, reload.from);
            self.close_game_menu();
            return self.refuse();
        }
        if reload.cancelled {
            return self.close_game_menu();
        }
        let since = match self.game_menu {
            Some(GameMenu::Working { since, .. }) => since,
            _ => self.now(),
        };
        self.start_link_from(
            LinkStarter::spawn(reload.role.role(), link_port()),
            reload.role.client_id(),
            since,
        );
    }

    pub fn link_reload_failed(&mut self) {
        let Some(mut reload) = self.reload.take() else {
            return;
        };
        if !reload.fallback {
            self.link_reload = Some((reload.stem.clone(), reload.from_serial));
            reload.fallback = true;
            self.reload = Some(reload);
            return;
        }
        self.link_choices.insert(reload.stem, reload.from);
        self.refuse_seated();
    }

    pub fn selected_stem(&self) -> Option<&str> {
        self.shelf()
            .carts
            .get(self.shelf().index)
            .map(|c| c.stem.as_str())
    }

    pub fn battery(&self) -> Option<Battery> {
        self.battery
    }

    pub fn poweroff(&mut self) {
        self.settle_saves();
        if let Some(power) = &mut self.power {
            power.poweroff();
        }
    }

    pub fn apply(&mut self, action: Action) {
        #[cfg(feature = "brick")]
        if let Some(wifi)=&mut self.wifi {
            if matches!(action,Action::PowerTap|Action::PowerHold|Action::LidClose) && wifi.busy() {
                wifi.cancel();self.wifi_power=Some(action);return;
            }
        }
        if self.power_menu.is_some() {
            match action {
                Action::LidClose => return self.doze(),
                Action::LidOpen => return self.wake(),
                _ => return self.power_menu_input(action),
            }
        }
        match action {
            Action::LidClose => return self.doze(),
            Action::LidOpen => return self.wake(),
            Action::PowerPress => {
                if self.link_active() {
                    self.end_link();
                    return;
                }
                self.woke_on_press = matches!(self.phase, Phase::Doze { .. });
                if self.woke_on_press {
                    self.wake();
                }
                return self.flush_resume();
            }
            Action::PowerTap => return self.power_press(),
            Action::PowerHold => return self.open_power_menu(),
            Action::PowerOff => return,
            _ => {}
        }
        #[cfg(feature = "brick")]
        if let Some(wifi)=&mut self.wifi {
            if wifi.input(action) {
                // The label keyboard closed: with what was typed, the search for it begins.
                let typed = wifi.take_label_query();
                self.wifi=None;
                let stem = self.cart_menu.as_ref().or(self.picker_look.as_ref()).map(|m| m.stem.clone());
                if let (Some(query), Some(stem)) = (typed, stem) {self.search_labels(&stem, query);}
            }
            return;
        }
        if let Phase::SetClock {
            picker, from_menu, ..
        } = &mut self.phase
        {
            match action {
                Action::GbaDown(Btn::Left) | Action::ShelfLeft => picker.left(),
                Action::GbaDown(Btn::Right) | Action::ShelfRight => picker.right(),
                Action::GbaDown(Btn::Up) => picker.up(),
                Action::GbaDown(Btn::Down) => picker.down(),
                Action::GbaDown(Btn::A) | Action::Insert => self.confirm_clock(),
                Action::GbaDown(Btn::B) if *from_menu => {
                    self.phase = Phase::QuickMenu {
                        row: QuickRow::DateTime,
                    }
                }
                _ => {}
            }
            return;
        }
        if self.adjust(action) {
            return;
        }
        match action {
            Action::GbaUp(Btn::Left) => self.shelf_mut().release_left(),
            Action::GbaUp(Btn::Right) => self.shelf_mut().release_right(),
            _ => {}
        }
        if self.game_menu.is_some() {
            return self.game_menu_input(action);
        }
        let now = self.now();
        match self.phase {
            Phase::Shelf => match action {
                _ if self.label_adjust_open() => self.label_adjust_input(action),
                _ if self.label_grid_open() => self.label_grid_input(action),
                _ if self.cart_menu.is_some() => self.cart_menu_input(action),
                Action::GbaDown(Btn::Start) if self.core_picker.is_none() => {
                    if self.shelf().carts.get(self.shelf().index).is_some_and(|c| c.platform == Platform::Gba) {
                        self.open_core_picker();
                        if self.core_picker.is_some() {
                            let index = self.shelf().index;
                            self.picker_look = self.shelf().carts.get(index).map(CartMenu::open);
                        }
                    } else {
                        self.open_cart_menu();
                    }
                }
                _ if self.core_picker.is_some() => self.core_picker_input(action),
                Action::GbaDown(Btn::Up) => self.jump_letter(-1),
                Action::GbaDown(Btn::Down) => self.jump_letter(1),
                Action::ShelfLeft | Action::GbaDown(Btn::Left) => self.shelf_mut().hold_left(now),
                Action::ShelfRight | Action::GbaDown(Btn::Right) => {
                    self.shelf_mut().hold_right(now)
                }
                Action::QuickMenu => self.open_quick_menu(),
                Action::GbaDown(Btn::A) => self.play_held = Some(now),
                Action::GbaUp(Btn::A) => {
                    if self.play_held.take().is_some() {
                        self.insert(!self.state.eject_save);
                    }
                }
                Action::Insert => self.insert(!self.state.eject_save),
                Action::GbaDown(Btn::L1) => self.switch_shelf(-1),
                Action::GbaDown(Btn::R1) => self.switch_shelf(1),
                _ => {}
            },
            Phase::Inserting { .. } if action == Action::Eject => self.eject(),
            Phase::Playing { .. } => match action {
                Action::GbaDown(Btn::L1) if self.slot_owns_the_shoulders() => {
                    self.set_picture(VideoMode::Stretch)
                }
                Action::GbaDown(Btn::R1) if self.slot_owns_the_shoulders() => {
                    self.set_picture(VideoMode::Actual)
                }
                Action::Eject => self.eject(),
                Action::GameMenu => self.game_menu_shortcut(),
                Action::Polaroids => self.open_polaroids(),
                Action::SaveState => self.save_state(),
                Action::LoadState => self.load_newest(),
                Action::RewindStart if !self.may_rewind() => self.refuse(),
                Action::FfStart if !self.may_fast_forward() => self.refuse(),
                _ => {}
            },
            Phase::Polaroids { .. } => match action {
                Action::ShelfLeft | Action::GbaDown(Btn::Left) => self.flick(Polaroids::left),
                Action::ShelfRight | Action::GbaDown(Btn::Right) => self.flick(Polaroids::right),
                Action::GbaDown(Btn::A) => self.load_selected(),
                Action::GbaDown(Btn::B) | Action::Polaroids => self.close_polaroids(),
                Action::GbaDown(Btn::X) => self.undo(self.now()),
                Action::GbaDown(Btn::Y) => self.delete_selected(),
                _ => {}
            },
            Phase::About if action == Action::GbaDown(Btn::B) || action == Action::QuickMenu => {
                self.phase = Phase::QuickMenu {
                    row: QuickRow::About,
                }
            }
            Phase::QuickMenu { row } => self.quick_menu_input(row, action),
            _ => {}
        }
    }

    fn open_quick_menu(&mut self) {
        self.phase = Phase::QuickMenu {
            row: QuickRow::MAIN[0],
        };
    }

    fn quick_menu_input(&mut self, row: QuickRow, action: Action) {
        let row = match action {
            Action::GbaDown(Btn::Up) => row.up(),
            Action::GbaDown(Btn::Down) => row.down(),
            Action::GbaDown(Btn::Left) => return self.change_setting(row, false),
            Action::GbaDown(Btn::Right) => return self.change_setting(row, true),
            Action::GbaDown(Btn::A) => return self.open_quick_row(row),
            Action::GbaDown(Btn::B) | Action::QuickMenu => {
                self.phase = match row.parent() {
                    Some(row) => Phase::QuickMenu { row },
                    None => Phase::Shelf,
                };
                return;
            }
            _ => return,
        };
        self.phase = Phase::QuickMenu { row };
    }

    fn open_quick_row(&mut self, row: QuickRow) {
        match row {
            QuickRow::Wifi => {
                #[cfg(feature = "brick")]
                {self.wifi=Some(crate::brick_wifi::WifiMenu::new());}
            },
            QuickRow::CartArt => {
                #[cfg(feature = "brick")]
                if self.net_up {self.wifi=Some(crate::brick_wifi::WifiMenu::cart_art());}
            },
            QuickRow::Achievements => {
                #[cfg(feature = "brick")]
                if self.net_up {self.wifi=Some(crate::brick_wifi::WifiMenu::achievements());}
            },
            QuickRow::DateTime => {
                self.phase = clock_screen(self.utc_secs(), self.state.utc_offset_min, true);
            }
            QuickRow::About => self.phase = Phase::About,
            QuickRow::Screen | QuickRow::Game | QuickRow::Lights => {
                if let Some(row) = row.child() {
                    self.phase = Phase::QuickMenu { row };
                }
            }
            QuickRow::EjectSave
            | QuickRow::Turbo
            | QuickRow::Rewind
            | QuickRow::FastForward
            | QuickRow::FastForwardSound
            | QuickRow::ColourCorrection
            | QuickRow::GbaShader
            | QuickRow::GbShader
            | QuickRow::GbPalettes
            | QuickRow::Rumble | QuickRow::LedMode | QuickRow::LedTop | QuickRow::LedButtons | QuickRow::LedTriggers | QuickRow::LedColor | QuickRow::LedEffect => {}
        }
    }

    fn change_setting(&mut self, row: QuickRow, right: bool) {
        #[cfg(feature = "brick")]
        if row.led_setting() {
            if let Some(leds)=self.brick_leds.as_mut() {
                if let Err(e)=leds.change(row,right) { eprintln!("slot: LED setting: {e}"); self.refuse(); }
            }
            return;
        }

        let s = &mut self.state;
        match row {
            QuickRow::FastForward => {
                let to = ff_next(s.ff_speed, right);
                if to == s.ff_speed {
                    return;
                }
                s.ff_speed = to;
            }
            QuickRow::FastForwardSound => s.ff_sound = !s.ff_sound,
            QuickRow::ColourCorrection => {
                s.colour_correction = !s.colour_correction;
                self.colour_pending = Some(s.colour_correction);
            }
            QuickRow::GbaShader => {
                let to = s.shader_gba.step(&Shader::GBA, right);
                if to == s.shader_gba {
                    return;
                }
                s.shader_gba = to;
            }
            QuickRow::GbShader => {
                let to = s.shader_gb.step(&Shader::GB, right);
                if to == s.shader_gb {
                    return;
                }
                s.shader_gb = to;
            }
            QuickRow::Rumble => s.rumble = !s.rumble,
            QuickRow::EjectSave => s.eject_save = !s.eject_save,
            QuickRow::Turbo => s.turbo = !s.turbo,
            QuickRow::Rewind => s.rewind = !s.rewind,
            QuickRow::GbPalettes => s.gb_palettes = !s.gb_palettes,
            QuickRow::Wifi | QuickRow::CartArt | QuickRow::Achievements | QuickRow::DateTime | QuickRow::About | QuickRow::Screen | QuickRow::Game | QuickRow::Lights | QuickRow::LedMode | QuickRow::LedTop | QuickRow::LedButtons | QuickRow::LedTriggers | QuickRow::LedColor | QuickRow::LedEffect => return,
        }
        self.persist();
    }

    pub fn apply_at(&mut self, action: Action, now: Millis) {
        self.clock = now as f64;
        self.apply(action);
    }

    fn step_shader(&mut self, right: bool) {
        if !matches!(self.phase, Phase::Inserting { .. } | Phase::Playing { .. }) {
            return;
        }
        let (shader, choices) = match self.platform {
            Platform::Gba => (&mut self.state.shader_gba, &Shader::GBA[..]),
            Platform::Gb | Platform::Gbc => (&mut self.state.shader_gb, &Shader::GB[..]),
            Platform::Nes => (&mut self.state.shader_nes, &Shader::NES[..]),
            Platform::Snes => (&mut self.state.shader_snes, &Shader::NES[..]),
        };
        *shader = shader.step(choices, right);
        let said = match *shader {
            Shader::Off => Toast::ShaderOff,
            Shader::Lcd3x => Toast::ShaderLcd3x,
            Shader::Grid => Toast::ShaderGrid,
            Shader::Dot => Toast::ShaderDot,
            Shader::Simpletex => Toast::ShaderSimpletex,
        };
        self.persist();
        self.hud.toast(said, self.now());
    }

    fn adjust(&mut self, action: Action) -> bool {
        if action == Action::LedToggle {
            #[cfg(feature = "brick")]
            if matches!(self.phase, Phase::Playing { .. }) && !self.game_menu_open() && self.power_menu.is_none() {
                if let Some(leds) = &mut self.brick_leds {
                    if let Err(e) = leds.toggle() { eprintln!("slot: LED shortcut: {e}"); self.refuse(); }
                }
            }
            return true;
        }
        if action == Action::PictureToggle {
            self.toggle_gba_picture();
            return true;
        }
        if action == Action::MuteToggle {
            self.mute_toggle();
            return true;
        }
        if let Action::ShaderNext | Action::ShaderPrev = action {
            self.step_shader(action == Action::ShaderNext);
            return true;
        }
        if let Action::PaletteNext | Action::PalettePrev = action {
            if self.palette_live && matches!(self.phase, Phase::Playing { .. }) {
                let to = match action {
                    Action::PalettePrev => self.state.gb_palette.prev(),
                    _ => self.state.gb_palette.next(),
                };
                self.state.gb_palette = to;
                self.palette_pending = Some(to);
                self.persist();
                self.hud.toast(Toast::Palette(to), self.now());
            }
            return true;
        }
        if action == Action::ColourCorrectionToggle {
            // A NES or SNES drew on a TV: there is no LCD colour to correct.
            if matches!(self.platform, Platform::Nes | Platform::Snes) && matches!(self.phase, Phase::Playing { .. }) {
                self.refuse();
                return true;
            }
            self.change_setting(QuickRow::ColourCorrection, true);
            let said = match self.state.colour_correction {
                true => Toast::ColourOn,
                false => Toast::ColourOff,
            };
            self.hud.toast(said, self.now());
            return true;
        }
        let s = &self.state;
        let (kind, value) = match action {
            Action::BrightnessUp => (HudKind::Brightness, up(s.brightness, 1, BRIGHTNESS_MAX)),
            Action::BrightnessDown => (HudKind::Brightness, s.brightness.saturating_sub(1)),
            Action::BlueLightUp => (HudKind::BlueLight, up(s.blue_light, 1, BLUE_LIGHT_MAX)),
            Action::BlueLightDown => (HudKind::BlueLight, s.blue_light.saturating_sub(1)),
            Action::VolumeUp => (HudKind::Volume, up(self.level(), VOLUME_STEP, VOLUME_MAX)),
            Action::VolumeDown => (HudKind::Volume, self.level().saturating_sub(VOLUME_STEP)),
            _ => return false,
        };
        if kind == HudKind::Volume {
            self.remember_volume();
        }
        let level = match kind {
            HudKind::Brightness => &mut self.state.brightness,
            HudKind::BlueLight => &mut self.state.blue_light,
            HudKind::Volume if self.headphones => &mut self.state.volume_hp,
            HudKind::Volume => &mut self.state.volume,
            HudKind::Rewind => return false,
        };
        let moved = *level != value;
        *level = value;
        let unmuted = kind == HudKind::Volume && std::mem::take(self.muted_mut());
        let (shown, now) = (self.hud_value(kind, value), self.now());
        self.hud.show(kind, shown, self.muted(), now);
        if let (HudKind::Brightness, Some(power)) = (kind, &mut self.power) {
            power.set_backlight(value);
        }
        if moved || unmuted {
            self.persist_soon();
        }
        true
    }

    fn remember_volume(&mut self) {
        if self.vol_before.len() == 2 {
            self.vol_before.remove(0);
        }
        self.vol_before
            .push((self.level(), self.muted(), self.now()));
    }

    fn mute_toggle(&mut self) {
        let now = self.now();
        if let Some((volume, muted, _)) = self
            .vol_before
            .iter()
            .find(|(_, _, at)| now.saturating_sub(*at) <= MUTE_CHORD_MS)
            .copied()
        {
            *self.level_mut() = volume;
            *self.muted_mut() = muted;
        }
        self.vol_before.clear();
        let muted = !self.muted();
        *self.muted_mut() = muted;
        self.hud
            .show(HudKind::Volume, self.output_volume(), muted, now);
        self.persist_soon();
    }

    fn hud_value(&self, kind: HudKind, value: u8) -> u8 {
        match kind {
            HudKind::Volume => self.output_volume(),
            _ => value,
        }
    }

    pub fn show_rewind(&mut self, fill: u8) {
        let now = self.now();
        self.hud.show(HudKind::Rewind, fill, false, now);
    }

    pub fn hide_rewind(&mut self) {
        self.hud.release_rewind();
    }

    pub fn set_ff(&mut self, ff: FfState) {
        self.hud.set_ff(ff);
    }

    pub fn ff_badge(&self) -> Option<Icon> {
        self.hud.badge()
    }

    pub fn blue_light(&self) -> u8 {
        self.state.blue_light
    }

    pub fn brightness(&self) -> u8 {
        self.state.brightness
    }

    pub fn volume(&self) -> u8 {
        self.level()
    }

    fn level(&self) -> u8 {
        if self.headphones {
            self.state.volume_hp
        } else {
            self.state.volume
        }
    }

    fn level_mut(&mut self) -> &mut u8 {
        if self.headphones {
            &mut self.state.volume_hp
        } else {
            &mut self.state.volume
        }
    }

    #[cfg(feature = "brick")]
    pub fn set_brick_mute(&mut self, muted: bool) {
        if self.brick_mute == Some(muted) {return;}
        let initial = self.brick_mute.is_none();
        self.brick_mute = Some(muted);
        if !muted && !initial {
            self.state.muted = false;
            self.state.muted_hp = false;
        }
        if !initial {
            self.hud.show(HudKind::Volume,self.output_volume(),self.muted(),self.now());
        }
    }
    pub fn muted(&self) -> bool {
        #[cfg(feature = "brick")]
        if self.brick_mute == Some(true) {return true;}
        if self.headphones {
            self.state.muted_hp
        } else {
            self.state.muted
        }
    }

    fn muted_mut(&mut self) -> &mut bool {
        if self.headphones {
            &mut self.state.muted_hp
        } else {
            &mut self.state.muted
        }
    }

    pub fn output_volume(&self) -> u8 {
        if self.muted() {
            0
        } else {
            self.level()
        }
    }

    pub fn hud_icon(&self) -> Icon {
        self.hud.glyph()
    }

    pub fn now(&self) -> Millis {
        self.clock as Millis
    }

    fn flick(&mut self, step: fn(&mut Polaroids)) {
        if let Some(p) = &mut self.polaroids {
            step(p);
        }
    }

    pub fn update(&mut self, dt: f32) {
        self.clock += dt as f64 * 1000.0;
        if self.name_pending && self.shelf_platform.face.is_some() {
            self.name_pending = false;
            self.shelf_named = Some(self.now());
        }
        self.timers();
        self.poll_link();
        let now = self.now();
        let ready = self.core_faces_ready();
        if let Some(picker) = &mut self.core_picker {
            if picker.waiting() && (ready || picker.waited(now) >= FACES_WAIT_MS) {
                picker.start(now);
            }
        }
        if self.core_picker.is_some_and(|p| p.finished(now)) {
            self.core_picker = None;
        }
        self.poll_label_pick();
        self.tick_label_adjust();
        if !self.on_shelf() {
            self.shelf_mut().release_hold();
        }
        let mut touched = false;
        let at = self.shelf_at;
        let next = match &mut self.phase {
            Phase::Shelf => {
                let shelf = &mut self.shelves[at].1;
                shelf.tick(now);
                shelf.update(dt);
                None
            }
            Phase::Inserting {
                cart,
                t,
                core_ready,
                resumed,
                ..
            } => {
                let was = *t;
                *t += dt;
                let at = SEATED_AT - Sfx::Insert.lead();
                touched = !*resumed && was < at && *t >= at;
                (*t >= INSERT_S && *core_ready).then(|| Phase::Playing {
                    cart: std::mem::take(cart),
                })
            }
            Phase::Ejecting { t, .. } => {
                if self.screen <= 0.0 {
                    let was = *t;
                    *t += dt;
                    touched = was < 0.0 && *t >= 0.0;
                }
                (*t >= EJECT_S).then_some(Phase::Shelf)
            }
            _ => None,
        };
        if touched {
            self.sfx = Some(match self.phase {
                Phase::Ejecting { .. } => Sfx::Eject,
                _ => Sfx::Insert,
            });
        }
        if let Some(phase) = next {
            self.phase = phase;
            self.refused_from = None;
            let seated = match &self.phase {
                Phase::Playing { cart } => Some(cart.clone()),
                _ => None,
            };
            if seated.is_none() {
                self.link_loaded = None;
            }
            self.record_cart(seated);
        }
        self.step_screen(dt);
    }

    fn step_screen(&mut self, dt: f32) {
        let lit = matches!(self.phase, Phase::Playing { .. } | Phase::Polaroids { .. });
        let step = if lit {
            dt / POWER_ON_S
        } else {
            -dt / POWER_OFF_S
        };
        self.screen = (self.screen + step).clamp(0.0, 1.0);
        let docked = self.power_menu.is_none()
            && matches!(
                self.phase,
                Phase::Shelf | Phase::Inserting { .. } | Phase::Ejecting { .. }
            );
        let sliding = self.power_menu.is_none()
            && matches!(
                self.phase,
                Phase::Inserting { .. }
                    | Phase::Playing { .. }
                    | Phase::Polaroids { .. }
                    | Phase::Ejecting { .. }
            );
        let reach = if sliding { dt / POWER_ON_S } else { 1.0 };
        self.lift = match docked {
            true => (self.lift - reach).max(0.0),
            false => (self.lift + reach).min(1.0),
        };
    }

    pub fn frame_split(&self) -> Option<f32> {
        match (&self.phase, self.power_menu) {
            (Phase::QuickMenu { .. }, None) => Some(slot_ui::QUICK_SPLIT),
            _ => None,
        }
    }

    pub fn frame_lift(&self) -> f32 {
        self.lift
    }

    pub fn screen_power(&self) -> f32 {
        self.screen
    }

    pub fn set_game_ready(&mut self, ready: bool) {
        self.game_ready = ready;
    }

    pub fn game_visible(&self) -> bool {
        self.game_ready && self.screen > 0.0
    }

    pub fn tick_ms(&mut self, now: Millis) {
        self.clock = self.clock.max(now as f64);
        self.timers();
    }

    fn timers(&mut self) {
        let now = self.now();
        if self.banner.as_ref().is_some_and(|b| now.saturating_sub(b.4) >= BANNER_MS) {
            self.banner = None;
        }
        // A toast whose badge is still downloading waits for it, but not for long.
        let ready = |b: &(String, String, Option<std::path::PathBuf>, Millis)| {
            b.2.as_ref().is_none_or(|p| p.exists()) || now.saturating_sub(b.3) >= BADGE_WAIT
        };
        if self.banner.is_none() && self.banners.front().is_some_and(ready) {
            if let Some((title, detail, badge, _)) = self.banners.pop_front() {
                self.banner_seq += 1;
                self.banner = Some((self.banner_seq, title, detail, badge, now));
            }
        }
        #[cfg(feature = "brick")]
        {
            if let Some(wifi)=&mut self.wifi {wifi.poll();}
            for f in self.wifi.as_mut().map(|w|w.take_fetched()).unwrap_or_default() {self.adopt_label(f.platform,&f.stem,f.label);}
            if let Some(t)=self.wifi.as_mut().and_then(|w|w.take_test_unlock()) {
                self.achievement_notice(slot_cheevos::Notice::Unlocked{title:t.title,description:t.description,points:t.points,badge:String::new()},t.badge);
            }
            if self.wifi.as_ref().map(|w|!w.busy()).unwrap_or(true) {
                if let Some(action)=self.wifi_power.take(){self.apply(action);}
            }
        }
        #[cfg(feature = "brick")]
        if self.now() >= self.brick_switch_at {
            self.brick_switch_at = self.now() + 200;
            self.net_up = crate::brick_wifi::online();
            if let Ok(value) = std::fs::read_to_string(slot_power::hw("/sys/class/gpio/gpio243/value")) {
                match value.trim() {
                    "0" => self.set_brick_mute(false),
                    "1" => self.set_brick_mute(true),
                    _ => {}
                }
            }
        }
        self.play_hold();
        let offer = self.undo_label();
        if let Some(p) = &mut self.polaroids {
            p.set_undo(offer);
        }
        if self.doze_expired() {
            self.on_doze_timeout();
        }
        if self.now() >= self.autosave_at {
            self.autosave();
        }
        if self.now() >= self.card_check_at {
            self.card_check_at = self.now() + CARD_CHECK_MS;
            self.card_read_only = self.power.as_ref().is_some_and(|p| p.card_read_only());
        }
        let failing = self.card_failing.load(Ordering::Relaxed) || self.card_read_only;
        if failing != self.hud.alarmed() {
            eprintln!(
                "slot: card: alarm {} (read-only {})",
                if failing { "on" } else { "off" },
                self.card_read_only
            );
        }
        self.hud.set_alarm(failing, self.now());
        if self.now() >= self.battery_at {
            self.battery_at = self.now() + BATTERY_POLL_MS;
            self.battery = self.power.as_ref().and_then(|p| p.battery());
            if let Some(b) = self.battery {
                self.on_battery(b);
            }
        }
        if self.now() >= self.headphones_at {
            self.headphones_at = self.now() + HEADPHONES_POLL_MS;
            let on = self.power.as_ref().is_some_and(|p| p.headphones());
            if on != self.headphones {
                self.headphones = on;
                self.hud.set_headphones(on);
                let now = self.now();
                self.hud
                    .show(HudKind::Volume, self.output_volume(), self.muted(), now);
            }
        }
        if self.now() >= self.charge_at {
            self.charge_at = self.now() + CHARGE_POLL_MS;
            if let (Some(power), Some(b)) = (self.power.as_ref(), self.battery.as_mut()) {
                b.charge = power.charge();
            }
            let state = self.led_state();
            self.set_led(state);
        }
        if let Some(GameMenu::Linked {
            since,
            opened: false,
            ..
        }) = self.game_menu
        {
            if self.now().saturating_sub(since) >= LINKED_HOLD_MS {
                self.game_menu = None;
            }
        }
        if let Some(GameMenu::Unplug { since, .. }) = self.game_menu {
            if self.now().saturating_sub(since) >= UNPLUG_HOLD_MS {
                self.game_menu = None;
            }
        }
        if let Some(at) = self.link.as_ref().and_then(|s| s.lost_at) {
            if self.now().saturating_sub(at) >= LINK_LOST_MS {
                self.end_link();
            }
        }
    }

    pub fn led_state(&self) -> LedState {
        let Some(b) = self.battery else {
            return LedState::Running;
        };
        match b.charge {
            Charge::Charging => LedState::Charging,
            Charge::Full => LedState::Charged,
            #[cfg(feature = "brick")]
            _ if b.percent <= 10 => LedState::VeryLow,
            _ if b.percent <= BATTERY_LOW => LedState::Low,
            _ => LedState::Running,
        }
    }

    fn set_led(&mut self, state: LedState) {
        if self.shutting_down() && state != LedState::Off {
            return;
        }
        #[cfg(feature = "brick")]
        if self.last_led != Some(state) || self.brick_led_failed {
            if let Some(leds) = &mut self.brick_leds {
                match leds.set_power_state(state) {
                    Ok(()) => self.brick_led_failed = false,
                    Err(e) => {
                        if !self.brick_led_failed {
                            eprintln!("slot: battery LEDs: {e}");
                        }
                        self.brick_led_failed = true;
                    }
                }
            }
        }
        if self.last_led == Some(state) {
            return;
        }
        self.last_led = Some(state);
        if let Some(power) = self.power.as_mut() {
            power.set_led(state);
        }
    }

    fn record_cart(&mut self, cart: Option<String>) {
        let platform = self.seated_cart().map(|c| c.platform);
        if self.state.cart == cart && self.state.cart_platform == platform {
            return;
        }
        self.state.cart = cart;
        self.state.cart_platform = platform;
        self.persist();
    }

    fn persist(&self) {
        let Some(root) = &self.root else {
            return;
        };
        self.state_writer.flush();
        let written = write_slot_state(root, &self.state);
        if let Err(e) = &written {
            eprintln!("slot: slot.state: {e}");
        }
        self.card_failing.store(written.is_err(), Ordering::Relaxed);
    }

    /// Volume/brightness/mute: written in the background so held keys never stall a frame.
    fn persist_soon(&self) {
        if let Some(root) = &self.root {
            self.state_writer.save(root, &self.state);
        }
    }

    pub fn flush_state(&self) {
        self.state_writer.flush();
    }

    pub fn on_core_ready(&mut self) {
        let Phase::Inserting {
            cart, core_ready, ..
        } = &mut self.phase
        else {
            return;
        };
        *core_ready = true;
        let cart = cart.clone();
        self.retire_refused_resume(&cart);
    }

    fn retire_refused_resume(&mut self, stem: &str) {
        let Some(snapshot) = &self.snapshot else {
            return;
        };
        if snapshot.resume_trusted() {
            return;
        }
        if !self.named_core {
            return;
        }
        let Some(root) = &self.root else {
            return;
        };
        if let Some(h) = self.pending_save.take() {
            let _ = h.join();
        }
        let ring = StateRing::new(root, self.platform, self.core, stem);
        match ring.retire_resume(&format_stamp(self.wall_secs())) {
            Ok(Some(to)) => eprintln!(
                "slot: resume: {} refused this state, moved it to {}",
                self.core.as_str(),
                to.display()
            ),
            Ok(None) => {}
            Err(e) => eprintln!("slot: resume: could not move the refused state aside: {e}"),
        }
    }

    pub fn on_core_failed(&mut self) {
        let caught = self.seat();
        let Phase::Inserting { cart, .. } = &mut self.phase else {
            return;
        };
        let cart = std::mem::take(cart);
        self.refuse_out(cart, caught);
    }

    fn refuse_out(&mut self, cart: String, caught: f32) {
        let t = (1.0 - caught) * EJECT_S;
        self.phase = Phase::Ejecting { cart, t };
        self.refused_from = Some(t);
    }

    pub fn refuse(&mut self) {
        self.refusal = Some(Refusal::started(self.now()));
    }

    pub fn refusal_active(&self, now: Millis) -> bool {
        self.refusal.is_some_and(|r| r.active(now))
    }

    pub fn seat(&self) -> f32 {
        match &self.phase {
            Phase::Shelf => 0.0,
            Phase::Inserting { t, resumed, .. } => {
                if *resumed {
                    1.0
                } else {
                    (t / SEATED_AT).clamp(0.0, 1.0)
                }
            }
            Phase::Ejecting { t, .. } => 1.0 - (t / EJECT_S).clamp(0.0, 1.0),
            _ => 1.0,
        }
    }

    pub fn draw(&self, out: &mut Vec<Draw>) {
        self.draw_scene(out);
        self.draw_banner(out);
    }

    /// Shows a toast in the corner, after any already waiting.
    pub fn announce(&mut self, title: &str, detail: &str) {
        self.announce_with(title, detail, None);
    }

    /// A toast with an image on its left (a badge, cached at `badge` or on its way there).
    pub fn announce_with(&mut self, title: &str, detail: &str, badge: Option<std::path::PathBuf>) {
        self.banners.push_back((title.to_string(), detail.to_string(), badge, self.now()));
    }

    /// What RetroAchievements said, as a toast (or nothing worth interrupting play for).
    /// `badge`: where its image is cached, for unlocks and the game's own icon.
    pub fn achievement_notice(&mut self, notice: slot_cheevos::Notice, badge: Option<std::path::PathBuf>) {
        use slot_cheevos::Notice;
        match notice {
            Notice::LoggedIn(Ok(())) | Notice::LeaderboardFailed { .. } => {}
            Notice::LoggedIn(Err(e)) => self.announce("Not logged in", &e),
            // "Title: 3 of 40 achievements": the game on top, its count below.
            Notice::GameLoaded(Ok((summary, _))) => match summary.rsplit_once(": ") {
                Some((game, count)) if !game.is_empty() => self.announce_with(game, count, badge),
                _ => self.announce_with("Achievements", &summary, badge),
            },
            Notice::GameLoaded(Err(e)) => self.announce("No achievements", &e),
            Notice::Unlocked { title, description, points, .. } => {
                self.announce_with(&title, &format!("{points} pts - {description}"), badge)
            }
            Notice::GameCompleted { title, .. } => self.announce_with("Every achievement earned", &title, badge),
            Notice::LeaderboardStarted { title, .. } => self.announce("Leaderboard attempt", &title),
            Notice::LeaderboardSubmitted { title, score } => self.announce(&title, &score),
            Notice::ServerError { message, .. } => self.announce("Server error", &message),
            Notice::Disconnected => self.announce("Offline", "Unlocks will be sent later"),
            Notice::Reconnected => self.announce("Back online", "Unlocks sent"),
        }
    }

    /// The toast showing, if its face still has to be rendered: (id, title, detail, badge).
    pub fn banner_to_render(&self) -> Option<(u64, String, String, Option<std::path::PathBuf>)> {
        let (id, title, detail, badge, _) = self.banner.as_ref()?;
        (self.banner_face.map(|(f, _)| f) != Some(*id)).then(|| (*id, title.clone(), detail.clone(), badge.clone()))
    }

    pub fn set_banner_face(&mut self, id: u64, tex: TexId) {
        self.banner_face = Some((id, tex));
    }

    /// A banner is up or waiting, so the screen must keep being drawn.
    pub fn banner_active(&self) -> bool {
        self.banner.is_some() || !self.banners.is_empty()
    }

    fn draw_banner(&self, out: &mut Vec<Draw>) {
        let (Some((id, _, _, _, since)), Some((face_id, tex))) = (&self.banner, self.banner_face) else {
            return;
        };
        if *id != face_id {
            return;
        }
        // Slides in from past the right edge, rests in the corner, slides back out.
        let age = self.now().saturating_sub(*since);
        let shown = (age as f32 / BANNER_SLIDE as f32)
            .min(BANNER_MS.saturating_sub(age) as f32 / BANNER_SLIDE as f32)
            .clamp(0.0, 1.0);
        let eased = 1.0 - (1.0 - shown) * (1.0 - shown);
        let away = slot_gfx::OUT_W as f32 - slot_ui::BANNER_X;
        out.push(Draw::Tex {
            x: (slot_ui::BANNER_X + away * (1.0 - eased)).round(),
            y: slot_ui::BANNER_Y,
            w: slot_ui::BANNER_W as f32,
            h: slot_ui::BANNER_H as f32,
            tex,
            alpha: 1.0,
        });
    }

    fn draw_scene(&self, out: &mut Vec<Draw>) {
        if let Some(index) = self.power_menu {
            self.draw_power_menu(index, out);
            return;
        }
        if self.shutting_down() {
            out.push(Draw::Rect {
                x: 0.0,
                y: 0.0,
                w: OUT_W as f32,
                h: OUT_H as f32,
                colour: [0.0, 0.0, 0.0, 1.0],
            });
            let which = if self.restarting {
                PowerChoice::Restart
            } else {
                PowerChoice::PowerOff
            };
            if let Some((tex, w, h)) = self.shutdown_faces.get(which.index()).copied() {
                out.push(Draw::Tex {
                    x: ((OUT_W - w) / 2) as f32,
                    y: ((OUT_H - h) / 2) as f32,
                    w: w as f32,
                    h: h as f32,
                    tex,
                    alpha: 1.0,
                });
            }
            return;
        }
        #[cfg(feature = "brick")]
        if let Some(wifi)=&self.wifi {
            if let Some(tex)=wifi.face {out.push(Draw::Tex{x:0.0,y:0.0,w:640.0,h:480.0,tex,alpha:1.0});return;}
        }
        match &self.phase {
            Phase::SetClock {
                picker, from_menu, ..
            } => {
                let (line, hint) = match self.clock_faces {
                    Some((line, hint)) => (Some(line), Some(hint)),
                    None => (None, None),
                };
                let back = self
                    .quick_menu_faces
                    .as_ref()
                    .filter(|_| *from_menu)
                    .map(|f| f.legend[0]);
                picker.draw(line, hint, back, out);
                return;
            }
            Phase::QuickMenu { row } => QuickMenu {
                row: *row,
                values: QuickRow::ALL.map(|r| self.quick_value(r)),
                clock: self.quick_clock_faces,
                faces: self.quick_menu_faces.as_ref(),
                disabled: QuickRow::ALL.map(|r| self.quick_disabled(r)),
            }
            .draw(out),
            Phase::Shelf => {
                draw_backdrop(self.wallpaper, out);
                match (self.core_picker_shown(), self.selected_stem()) {
                    (Some(picker), Some(stem)) => {
                        let open = ease(picker.openness(self.now()));
                        let dim = 1.0 + (CORE_PICKER_DIM - 1.0) * open;
                        self.shelf()
                            .draw_row(Some(stem), 0.0, CORE_PICKER_RECEDE * open, dim, out);
                        draw_empty_slot(out);
                    }
                    _ => {
                        self.shelf().draw(self.shelf_shake(), out);
                        draw_slot_name(self.shelf_platform, self.slot_name_alpha(), out);
                    }
                }
                draw_footer(
                    self.battery,
                    self.battery_percent,
                    self.bolt,
                    self.shelf_clock,
                    out,
                );
            }
            Phase::About => {
                draw_backdrop(self.wallpaper, out);
                draw_sticker(self.sticker_face, out);
                return;
            }
            Phase::Inserting { cart, resumed, .. } => {
                if !resumed {
                    draw_backdrop(self.wallpaper, out);
                    self.shelf()
                        .draw_row(Some(cart), 0.0, self.seat(), 1.0, out);
                }
                self.chrome(cart, self.seat(), out);
            }
            Phase::Ejecting { cart, .. } => {
                draw_backdrop(self.wallpaper, out);
                self.shelf()
                    .draw_row(Some(cart), 0.0, self.seat(), 1.0, out);
                self.chrome(cart, self.seat(), out);
            }
            Phase::Playing { cart } if self.screen < 1.0 => self.chrome(cart, 0.0, out),
            Phase::Playing { .. } => self.push_game(out),
            Phase::Polaroids { .. } => {
                self.push_game(out);
                if let Some(p) = &self.polaroids {
                    p.draw(
                        self.battery,
                        self.battery_percent,
                        self.bolt,
                        self.shelf_clock,
                        out,
                    );
                }
            }
            Phase::Doze { .. } => {
                out.push(Draw::Rect {
                    x: 0.0,
                    y: 0.0,
                    w: OUT_W as f32,
                    h: OUT_H as f32,
                    colour: [0.0, 0.0, 0.0, 1.0],
                });
                return;
            }
        }
        if let Some(picker) = self.core_picker_shown() {
            self.draw_core_picker(&picker, out);
        }
        if let Some(menu) = &self.cart_menu {
            self.draw_cart_menu(menu, out);
        }
        if self.label_grid_open() {
            self.draw_label_grid(out);
        }
        if self.label_adjust_open() {
            self.draw_label_adjust(out);
        }
        if let Some(menu) = self.game_menu {
            self.draw_game_menu(menu, out);
        }
        self.hud.draw(self.now(), out);
    }

    pub fn screen_shake(&self) -> f32 {
        self.shake_at(self.now())
    }

    pub fn shake_at(&self, now: Millis) -> f32 {
        self.shake_when(matches!(self.phase, Phase::Playing { .. }), now)
    }

    pub fn shelf_shake(&self) -> f32 {
        self.shake_when(self.on_shelf() && self.core_picker.is_none(), self.now())
    }

    fn shake_when(&self, mine: bool, now: Millis) -> f32 {
        if !mine {
            return 0.0;
        }
        self.refusal.map_or(0.0, |r| r.offset(now))
    }

    fn push_game(&self, out: &mut Vec<Draw>) {
        if self.game_visible() {
            out.push(Draw::Game);
        }
    }

    fn chrome(&self, stem: &str, dim: f32, out: &mut Vec<Draw>) {
        let Some((cart, face)) = self.shelf().find(stem) else {
            return;
        };
        let alpha = self.alert_alpha();
        let (rest, scale) = self.shelf().selected_at();
        SlotChrome {
            cart,
            face,
            rest,
            scale,
            seat: self.seat(),
            alert: self.alert_face.filter(|_| alpha > 0.0).map(|t| (t, alpha)),
            dim,
            screen: self.screen,
            game: self.game_ready,
        }
        .draw(out);
    }

    pub fn alert_visible(&self) -> bool {
        self.alert_alpha() > 0.0
    }

    pub fn alert_alpha(&self) -> f32 {
        let (Some(from), Phase::Ejecting { t, .. }) = (self.refused_from, &self.phase) else {
            return 0.0;
        };
        let span = EJECT_S - from;
        if span <= 0.0 {
            return 0.0;
        }
        let u = ((t - from) / span).clamp(0.0, 1.0);
        ((ALERT_GONE - u) / (ALERT_GONE - ALERT_HOLD)).clamp(0.0, 1.0)
    }

    pub fn set_alert_face(&mut self, face: TexId) {
        self.alert_face = Some(face);
    }

    pub fn set_shutdown_faces(&mut self, faces: Vec<(TexId, u32, u32)>) {
        self.shutdown_faces = faces;
    }

    pub fn set_cart_menu_faces(&mut self, faces: slot_ui::CartMenuFaces) {
        self.cart_menu_faces = Some(faces);
    }

    /// The cart menu's open row and the cart's shell choice so far, while it is open.
    pub fn cart_menu(&self) -> Option<(slot_ui::CartRow, Option<ShellChoice>)> {
        self.cart_menu.as_ref().map(|m| (m.row, m.chosen()))
    }

    /// The cart's shell choice so far while the core picker is open.
    pub fn picker_shell(&self) -> Option<Option<ShellChoice>> {
        self.picker_look.as_ref().map(CartMenu::chosen)
    }

    pub fn set_power_menu_faces(&mut self, faces: Vec<(TexId, u32, u32)>) {
        self.power_menu_faces = faces;
    }

    pub fn set_core_board_faces(&mut self, board: TexId, lid: TexId) {
        self.core_board_face = Some(board);
        self.core_lid_face = Some(lid);
        self.core_faces_stem = self.selected_stem().map(str::to_string);
    }

    pub fn set_core_part_faces(
        &mut self,
        sockets: Vec<TexId>,
        chips: Vec<TexId>,
        blank: TexId,
        shadow: TexId,
    ) {
        self.core_socket_faces = sockets;
        self.core_chip_faces = chips;
        self.core_blank_chip_face = Some(blank);
        self.core_chip_shadow_face = Some(shadow);
    }

    pub fn set_core_search_face(&mut self, face: (TexId, u32)) {
        self.core_search_face = Some(face);
    }

    pub fn set_core_legend_faces(&mut self, faces: Vec<(TexId, u32)>) {
        self.core_legend_faces = faces;
    }

    pub fn set_link_menu_faces(&mut self, faces: Vec<(TexId, u32, u32)>) {
        self.link_menu_faces = faces;
    }

    pub fn set_link_linked_face(&mut self, face: (TexId, u32, u32)) {
        self.link_linked_face = Some(face);
    }

    pub fn set_link_step_faces(&mut self, faces: Vec<(TexId, u32, u32)>) {
        self.link_step_faces = faces;
    }

    pub fn set_link_fail_faces(&mut self, faces: Vec<(TexId, u32, u32)>) {
        self.link_fail_faces = faces;
    }

    pub fn set_link_legend_faces(&mut self, faces: Vec<(TexId, u32)>) {
        self.link_legend_faces = faces;
    }

    pub fn set_link_sprites(&mut self, sprites: LinkSprites) {
        self.link_sprites = Some(sprites);
    }

    pub fn link_sprites_ready(&self) -> bool {
        self.link_sprites.is_some()
    }

    fn draw_power_menu(&self, index: usize, out: &mut Vec<Draw>) {
        out.push(Draw::Rect {
            x: 0.0,
            y: 0.0,
            w: OUT_W as f32,
            h: OUT_H as f32,
            colour: slot_ui::opening(),
        });
        draw_menu_rows(
            &self.power_menu_faces,
            Some(index),
            centred_top(self.power_menu_faces.len()),
            out,
        );
    }

    fn core_picker_shown(&self) -> Option<CorePicker> {
        self.core_picker.filter(|p| !p.waiting())
    }

    fn draw_core_picker(&self, picker: &CorePicker, out: &mut Vec<Draw>) {
        let now = self.now();
        let progress = picker.openness(now);
        let lift = lift_of(progress);
        let (rest, scale) = self.shelf().selected_at();
        let shelf = shelf_cart_at(rest, scale);
        let board = board_from(shelf, progress);
        let zoom = board_zoom(board);
        let ready = self.core_faces_ready();

        if ready {
            if let Some(tex) = self.core_board_face {
                out.push(Draw::Tex {
                    x: board.x,
                    y: board.y,
                    w: board.w,
                    h: board.h,
                    tex,
                    alpha: 1.0,
                });
            }
            for (i, tex) in self.core_socket_faces.iter().copied().enumerate() {
                let (x, y) = on_board(board, SOCKET_U[i], SOCKET_V);
                out.push(Draw::Tex {
                    x: x.round(),
                    y: y.round(),
                    w: SOCKET_W as f32 * zoom,
                    h: SOCKET_H as f32 * zoom,
                    tex,
                    alpha: 1.0,
                });
            }

            let chip = picker.chip(now);
            let u = CHIP_U[0] + (CHIP_U[1] - CHIP_U[0]) * chip.across;
            if chip.lift > 0.0 {
                if let Some(tex) = self.core_chip_shadow_face {
                    let (cx, cy) = on_board(board, u + 19.0, CHIP_V + 29.4);
                    let (w, h) = (SHADOW_W as f32 * zoom, SHADOW_H as f32 * zoom);
                    out.push(Draw::Tex {
                        x: cx - w / 2.0,
                        y: cy - h / 2.0,
                        w,
                        h,
                        tex,
                        alpha: 0.6 * chip.lift * lift,
                    });
                }
            }
            let face = match chip.seated {
                Some(core) => self.core_chip_faces.get(core.index()).copied(),
                None => self.core_blank_chip_face,
            };
            if let Some(tex) = face {
                let (x, y) = on_board(board, u, CHIP_V - HOP_LIFT * chip.lift);
                let body = Placed {
                    x: x + chip.shake,
                    y,
                    w: CHIP_W as f32 * zoom,
                    h: CHIP_H as f32 * zoom,
                };
                let at = grown(body, TURN_PAD as f32 * zoom);
                out.push(Draw::Turned {
                    x: at.x.round(),
                    y: at.y.round(),
                    w: at.w,
                    h: at.h,
                    tex,
                    alpha: 1.0,
                    turn: chip.tip,
                });
            }
        }

        if let Some(tex) = self.core_chip_shadow_face {
            let (lid, _) = lid_from(shelf, progress);
            let k = lid.w / lid_at(1.0).0.w;
            let (w, h) = (LID_SHADOW_W * k, LID_SHADOW_H * k);
            out.push(Draw::Tex {
                x: lid.x + (lid.w - w) / 2.0,
                y: lid.y + lid.h + LID_SHADOW_DROP * k - h / 2.0,
                w,
                h,
                tex,
                alpha: LID_SHADOW_ALPHA * lift,
            });
        }

        if ready {
            if let Some(tex) = self.core_lid_face {
                let (lid, turn) = lid_from(shelf, progress);
                let at = grown(lid, TURN_PAD as f32 * lid.w / CART_W as f32);
                out.push(Draw::Turned {
                    x: at.x,
                    y: at.y,
                    w: at.w,
                    h: at.h,
                    tex,
                    alpha: 1.0,
                    turn,
                });
            }
        } else if let Some((_, Some(tex))) = self
            .selected_stem()
            .and_then(|stem| self.shelf().find(stem))
        {
            let (lid, turn) = lid_from(shelf, progress);
            out.push(Draw::Turned {
                x: lid.x,
                y: lid.y,
                w: lid.w,
                h: lid.h,
                tex,
                alpha: 1.0,
                turn,
            });
        }

        // Centred on the screen, the gaps narrowed until every hint fits: B, Core, Colour,
        // Finish, A. A face is drawn HINT_EDGE wider than its ink.
        let hints = &self.core_legend_faces;
        if !hints.is_empty() {
            let inked: f32 = hints.iter().map(|(_, w)| w.saturating_sub(HINT_EDGE) as f32).sum();
            let room = OUT_W as f32 - 2.0 * CORE_LEGEND_MARGIN;
            let gaps = (hints.len() - 1) as f32;
            let gap = ((room - inked) / gaps).clamp(8.0, slot_ui::LEGEND_GAP);
            let mut x = (OUT_W as f32 - inked - gap * gaps) / 2.0;
            let first = x;
            for &(tex, w) in hints {
                out.push(Draw::Tex {
                    x: x.round(),
                    y: CORE_LEGEND_Y,
                    w: w as f32,
                    h: HINT_H as f32,
                    tex,
                    alpha: lift,
                });
                x += w.saturating_sub(HINT_EDGE) as f32 + gap;
            }
            // Above the leftmost of them, standing apart from the core's own keys.
            if let Some((tex, w)) = self.core_search_face {
                out.push(Draw::Tex {
                    x: first.round(),
                    y: CORE_LEGEND_Y - HINT_H as f32 - 6.0,
                    w: w as f32,
                    h: HINT_H as f32,
                    tex,
                    alpha: lift * if self.online() { 1.0 } else { slot_ui::GREYED },
                });
            }
        }
    }

    fn draw_game_menu(&self, menu: GameMenu, out: &mut Vec<Draw>) {
        out.push(Draw::Rect {
            x: 0.0,
            y: 0.0,
            w: OUT_W as f32,
            h: OUT_H as f32,
            colour: slot_ui::opening(),
        });
        if let Some(sprites) = &self.link_sprites {
            crate::link_screen::draw_link_art(menu, self.link_hardware, self.now(), sprites, out);
        }
        let line = match menu {
            GameMenu::Pick(role) => self.link_menu_faces.get(role.index()).copied(),
            GameMenu::Working { step, .. } => self
                .link_step_faces
                .get(step.shown(self.radio.warmed()).index())
                .copied(),
            GameMenu::Linked { .. } => self.link_linked_face,
            GameMenu::Failed { fail, .. } => fail
                .shown()
                .and_then(|i| self.link_fail_faces.get(i).copied()),
            GameMenu::Unplug { .. } => None,
        };
        if let Some((tex, w, h)) = line {
            out.push(Draw::Tex {
                x: ((OUT_W as f32 - w as f32) / 2.0).round(),
                y: LINK_TEXT_Y,
                w: w as f32,
                h: h as f32,
                tex,
                alpha: 1.0,
            });
        }
        let switchable = self.seated().is_some_and(|stem| self.link_switchable(stem));
        let keys: &[LinkLegend] = match menu {
            GameMenu::Pick(_) if switchable => &[
                LinkLegend::Cancel,
                LinkLegend::Mode,
                LinkLegend::Swap,
                LinkLegend::Link,
            ],
            GameMenu::Pick(_) => &[LinkLegend::Cancel, LinkLegend::Swap, LinkLegend::Link],
            GameMenu::Working { .. } => &[LinkLegend::Cancel],
            GameMenu::Linked { opened: true, .. } => &[LinkLegend::Back, LinkLegend::EndLink],
            GameMenu::Linked { .. } => &[],
            GameMenu::Failed { .. } => &[LinkLegend::Ok],
            GameMenu::Unplug { .. } => &[],
        };
        let faces: Vec<(TexId, u32)> = keys
            .iter()
            .filter_map(|k| self.link_legend_faces.get(k.index()).copied())
            .collect();
        let seen = |w: u32| w.saturating_sub(HINT_EDGE) as f32;
        let total: f32 = faces.iter().map(|(_, w)| seen(*w)).sum::<f32>()
            + LINK_LEGEND_GAP * faces.len().saturating_sub(1) as f32;
        let mut x = ((OUT_W as f32 - total) / 2.0).round();
        for (tex, w) in faces {
            out.push(Draw::Tex {
                x,
                y: LINK_LEGEND_Y,
                w: w as f32,
                h: HINT_H as f32,
                tex,
                alpha: 1.0,
            });
            x += (seen(w) + LINK_LEGEND_GAP).round();
        }
    }

    fn auto_link(&self, stem: &str) -> Option<(&Cart, LinkKind)> {
        let cart = self.shelf().carts.iter().find(|c| c.stem == stem)?;
        let auto = link_kind(&cart.code, &cart.title, slot_store::header_clean(&cart.rom));
        Some((cart, auto))
    }

    fn on_shelf(&self) -> bool {
        matches!(self.phase, Phase::Shelf)
    }

    pub fn browsed_rom(&self) -> Option<&std::path::Path> {
        if !self.on_shelf() {
            return None;
        }
        let shelf = self.shelf();
        shelf.carts.get(shelf.index).map(|c| c.rom.as_path())
    }

    fn insert(&mut self, clean: bool) {
        if !self.on_shelf() {
            return;
        }
        // 1.6.0: the scan no longer opens every ROM; the header is read on insert.
        let at = self.shelf().index;
        let Some(cart) = self.shelf_mut().carts.get_mut(at).map(|c| {
            c.read_header();
            c.stem.clone()
        }) else {
            return;
        };
        self.play_held = None;
        self.refusal = None;
        self.refused_from = None;
        self.phase = Phase::Inserting {
            cart,
            t: 0.0,
            core_ready: false,
            resumed: false,
            clean,
        };
    }

    pub fn starting_clean(&self) -> bool {
        matches!(self.phase, Phase::Inserting { clean: true, .. })
    }

    fn play_hold(&mut self) {
        let Some(at) = self.play_held else {
            return;
        };
        if !self.on_shelf() {
            self.play_held = None;
            return;
        }
        if self.now().saturating_sub(at) >= PLAY_HOLD_MS {
            self.insert(true);
        }
    }

    fn eject(&mut self) {
        if self.single_cart() {
            return self.refuse();
        }
        let cart = match &mut self.phase {
            Phase::Playing { cart } | Phase::Inserting { cart, .. } => std::mem::take(cart),
            _ => return,
        };
        self.end_link();
        self.close_game_menu();
        self.flush_eject(&cart);
        self.pending = None;
        self.refusal = None;
        self.refused_from = None;
        self.phase = Phase::Ejecting {
            cart,
            t: -EJECT_HOLD_S,
        };
    }

    fn flush_eject(&mut self, stem: &str) {
        self.settle_saves();
        let (Some(root), Some(snapshot)) = (&self.root, &self.snapshot) else {
            return;
        };
        let Some(state) = snapshot.state() else {
            eprintln!("slot: eject: the core gave up no state");
            return;
        };
        let (state, sav) = trusted_write(snapshot.as_ref(), state, "eject");
        let state = state.filter(|_| self.state.eject_save);
        match persist::eject(
            root,
            self.platform,
            self.core,
            stem,
            state.as_deref(),
            sav.as_deref(),
        ) {
            Ok(()) => {
                self.state.cart = None;
                self.state.cart_platform = None;
                self.card_failing.store(false, Ordering::Relaxed);
            }
            Err(e) => {
                eprintln!("slot: eject: {e}");
                self.card_failing.store(true, Ordering::Relaxed);
            }
        }
    }

    fn doze(&mut self) {
        #[cfg(feature = "brick")]
        {self.wifi=None;}
        if self.link_active() {
            self.end_link();
        }
        self.close_game_menu();
        self.core_picker = None;
        // A cart left mid-change goes back to the plastic and label it had.
        self.end_label_pick(false);
        if let Some(look) = self.picker_look.take() {
            self.preview_shell(&look.stem, look.before);
        }
        if let Some(menu) = self.cart_menu.take() {
            self.preview_shell(&menu.stem, menu.before);
        }
        if matches!(self.phase, Phase::Doze { .. }) {
            return;
        }
        self.state_writer.flush();
        self.flush_resume();
        let cart = match &mut self.phase {
            Phase::Playing { cart } | Phase::Polaroids { cart } => Some(std::mem::take(cart)),
            _ => None,
        };
        self.polaroids = None;
        self.phase = Phase::Doze { cart };
        self.dozed_at = self.now();
        #[cfg(feature = "brick")]
        if let Some(leds) = &mut self.brick_leds {
            if let Err(e) = leds.sleep() { eprintln!("slot: LEDs sleep: {e}"); }
        }
        self.radio.ask(RadioJob::Cool);
        if let Some(power) = &mut self.power {
            power.on_close();
        }
    }

    /// Idle timeout: same as a power press, but only from calm screens and never mid-link.
    #[cfg(feature = "brick")]
    pub(crate) fn idle_doze(&mut self) {
        if self.link_active() {return}
        if matches!(self.phase, Phase::Shelf | Phase::QuickMenu { .. } | Phase::Playing { .. }) {
            self.doze();
        }
    }

    pub(crate) fn wake(&mut self) {
        let Phase::Doze { cart } = &mut self.phase else {
            return;
        };
        self.phase = match cart.take() {
            Some(cart) => Phase::Playing { cart },
            None => Phase::Shelf,
        };
        #[cfg(feature = "brick")]
        if let Some(leds) = &mut self.brick_leds {
            if let Err(e) = leds.wake() { eprintln!("slot: LEDs wake: {e}"); }
        }
        if let Some(power) = &mut self.power {
            power.on_open();
        }
    }

    pub fn on_doze_timeout(&mut self) {
        if !matches!(self.phase, Phase::Doze { .. }) {
            return;
        }
        self.begin_power_off();
    }

    fn power_press(&mut self) {
        if std::mem::take(&mut self.woke_on_press) {
            return;
        }
        match self.phase {
            Phase::Doze { .. } => self.wake(),
            _ => self.doze(),
        }
    }

    fn open_power_menu(&mut self) {
        if self.power_menu.is_some() {
            return;
        }
        if self.link_active() {
            return self.refuse();
        }
        self.flush_resume();
        self.power_menu = Some(0);
    }

    fn power_menu_input(&mut self, action: Action) {
        let Some(index) = self.power_menu else {
            return;
        };
        let last = PowerChoice::ALL.len() - 1;
        match action {
            Action::GbaDown(Btn::Up) => {
                self.power_menu = Some(if index == 0 { last } else { index - 1 })
            }
            Action::GbaDown(Btn::Down) => {
                self.power_menu = Some(if index >= last { 0 } else { index + 1 })
            }
            Action::GbaDown(Btn::B) => self.power_menu = None,
            Action::GbaDown(Btn::A) => {
                self.power_menu = None;
                self.close_game_menu();
                match PowerChoice::ALL[index] {
                    PowerChoice::Restart => {
                        self.restarting = true;
                        self.act_at = self.now() + SHUTDOWN_SHOW_MS;
                        self.set_led(LedState::Off);
                    }
                    PowerChoice::PowerOff => self.begin_power_off(),
                }
            }
            _ => {}
        }
    }

    fn open_cart_menu(&mut self) {
        if self.root.is_none() {
            return;
        }
        let index = self.shelf().index;
        let Some(cart) = self.shelf().carts.get(index).cloned() else {
            return;
        };
        self.cart_menu = Some(CartMenu::open(&cart));
        self.shelf_mut().release_hold();
        self.play_held = None;
    }

    fn cart_menu_input(&mut self, action: Action) {
        let Some(mut menu) = self.cart_menu.take() else {
            return;
        };
        let before = menu.chosen();
        match action {
            Action::GbaDown(Btn::Up) => menu.step_row(-1),
            Action::GbaDown(Btn::Down) => menu.step_row(1),
            Action::GbaDown(Btn::Y) => self.type_label_search(&menu.stem),
            Action::GbaDown(Btn::Left) | Action::ShelfLeft => menu.step_value(-1),
            Action::GbaDown(Btn::Right) | Action::ShelfRight => menu.step_value(1),
            Action::GbaDown(Btn::B) | Action::GbaDown(Btn::Start) => {
                self.end_label_pick(false);
                return self.preview_shell(&menu.stem, menu.before);
            }
            Action::GbaDown(Btn::A) => {
                if menu.row == slot_ui::CartRow::Reset {
                    menu.reset();
                    self.end_label_pick(false);
                    self.preview_shell(&menu.stem, None);
                }
                self.end_label_pick(true);
                return self.save_shell(&menu);
            }
            _ => {}
        }
        if menu.chosen() != before {
            // A label found for one shape is not one for another: the search starts over.
            if menu.chosen().map(|c| c.outline) != before.map(|c| c.outline) {
                self.end_label_pick(false);
            }
            self.preview_shell(&menu.stem, menu.chosen());
        }
        self.cart_menu = Some(menu);
    }

    /// Where the cart called `stem` is on the shelf now: found by name, not by a position
    /// remembered when a menu opened, which a rescan could have moved.
    fn shelf_index_of(&self, stem: &str) -> Option<usize> {
        self.shelf().carts.iter().position(|c| c.stem == stem)
    }

    /// The grid of labels a search found. The cursor walks it; A wears the one under it and
    /// goes back to the cart menu, B goes back leaving the cart as it was, Y searches again.
    fn label_grid_input(&mut self, action: Action) {
        let Some(pick) = &self.label_pick else {
            return;
        };
        let (found, stem) = (pick.choices.len(), pick.stem.clone());
        let by: isize = match action {
            Action::GbaDown(Btn::B) | Action::GbaDown(Btn::Start) => return self.end_label_pick(false),
            // Nothing found yet means nothing to frame: A then just goes back.
            Action::GbaDown(Btn::A) | Action::Insert => {
                return if found > 0 { self.open_label_adjust() } else { self.end_label_pick(false) }
            }
            Action::GbaDown(Btn::Y) => {
                self.close_label_grid();
                return self.type_label_search(&stem);
            }
            Action::GbaDown(Btn::Left) | Action::ShelfLeft => -1,
            Action::GbaDown(Btn::Right) | Action::ShelfRight => 1,
            Action::GbaDown(Btn::Up) => -(slot_ui::GRID_COLS as isize),
            Action::GbaDown(Btn::Down) => slot_ui::GRID_COLS as isize,
            _ => return,
        };
        let Some(pick) = &mut self.label_pick else {
            return;
        };
        // Never past the last one found: the rest are still on their way.
        let last = found.saturating_sub(1) as isize;
        pick.at = (pick.cursor() as isize + by).clamp(0, last.max(0)) as usize + 1;
        if found == 0 {
            pick.at = 0;
        }
        if let Some(search) = &pick.search {
            search.looking_at((pick.page() + 1) * slot_ui::PAGE);
        }
    }

    /// A on the grid: the label picked goes under a frame to be set by hand, starting as the
    /// piece it was cut from. Nothing is written until it is kept.
    fn open_label_adjust(&mut self) {
        let Some(pick) = &self.label_pick else {
            return;
        };
        let Some(choice) = pick.choices.get(pick.cursor()) else {
            return;
        };
        let (source, cut) = (choice.source.clone(), choice.frame);
        // The picture has to be readable to be framed; if it is not, keep the cut as it is.
        let Some(size) = std::fs::read(&source).ok().and_then(|png| crate::cart_art::png_size(&png)) else {
            return self.end_label_pick(true);
        };
        let Some(cart) = self.shelf_index_of(&pick.stem).and_then(|i| self.shelf().carts.get(i)) else {
            return;
        };
        let (face, slot) = slot_ui::cart_cutout_face(cart);
        let crop = (cut.x as f32, cut.y as f32, cut.w as f32, cut.h as f32);
        let mut adjust =
            Adjust { source, size, crop, original: crop, face: (face.w, face.h), slot, sizing: false, held: None };
        adjust.clamp();
        if let Some(pick) = &mut self.label_pick {
            pick.adjust = Some(adjust);
        }
    }

    /// Framing a picked label. The d-pad moves the frame; holding L, Up and Right grow it and
    /// Down and Left shrink it. R evens its shape back up, Y puts it back as it was, A keeps
    /// what is inside it and B goes back to the grid.
    fn label_adjust_input(&mut self, action: Action) {
        match action {
            Action::GbaDown(Btn::L1) => {
                if let Some(a) = self.adjust_mut() {
                    (a.sizing, a.held) = (true, None);
                }
                return;
            }
            Action::GbaUp(Btn::L1) => {
                if let Some(a) = self.adjust_mut() {
                    (a.sizing, a.held) = (false, None);
                }
                return;
            }
            Action::GbaUp(_) => {
                if let Some(a) = self.adjust_mut() {
                    a.held = None;
                }
                return;
            }
            Action::GbaDown(Btn::B) => {
                if let Some(pick) = &mut self.label_pick {
                    pick.adjust = None;
                }
                return;
            }
            Action::GbaDown(Btn::A) | Action::Insert => {
                let Some(a) = self.label_pick.as_ref().and_then(|p| p.adjust.as_ref()) else {
                    return;
                };
                let (source, frame) = (a.source.clone(), a.frame());
                return self.keep_framed_label(&source, frame);
            }
            Action::GbaDown(Btn::R1) => {
                if let Some(a) = self.adjust_mut() {
                    a.even_up();
                }
                return;
            }
            Action::GbaDown(Btn::Y) => {
                if let Some(a) = self.adjust_mut() {
                    a.crop = a.original;
                    a.clamp();
                }
                return;
            }
            _ => {}
        }
        let dir = match action {
            Action::GbaDown(b @ (Btn::Up | Btn::Down | Btn::Left | Btn::Right)) => b,
            _ => return,
        };
        let now = self.now();
        if let Some(a) = self.adjust_mut() {
            a.held = Some((dir, now, now));
        }
        self.step_label_frame(dir);
    }

    fn adjust_mut(&mut self) -> Option<&mut Adjust> {
        self.label_pick.as_mut()?.adjust.as_mut()
    }

    /// One step of the held direction: moving the frame, or sizing it while L is down.
    fn step_label_frame(&mut self, dir: Btn) {
        let Some(a) = self.adjust_mut() else {
            return;
        };
        if a.sizing {
            // The bottom left corner is pinned, so the edges that move are the right one and
            // the top one: each key moves the edge it points at. A bigger picture is less of
            // it inside the frame, so its piece shrinks as it grows.
            match dir {
                Btn::Up => a.resize(0.0, -STEP_PX),
                Btn::Down => a.resize(0.0, STEP_PX),
                Btn::Right => a.resize(-STEP_PX, 0.0),
                _ => a.resize(STEP_PX, 0.0),
            }
            return;
        }
        // Every key here moves the picture, not the frame: Up pushes the picture up, which is
        // to say the frame takes its piece from further down.
        match dir {
            Btn::Up => a.crop.1 += STEP_PX,
            Btn::Down => a.crop.1 -= STEP_PX,
            Btn::Left => a.crop.0 += STEP_PX,
            _ => a.crop.0 -= STEP_PX,
        }
        a.clamp();
    }

    /// A held direction keeps stepping, quickening the longer it is held, so a tap can be worth
    /// a pixel or two without it taking all day to cross the picture.
    fn tick_label_adjust(&mut self) {
        let now = self.now();
        let Some(a) = self.adjust_mut() else {
            return;
        };
        let Some((dir, since, last)) = a.held else {
            return;
        };
        let held = now.saturating_sub(since);
        if held < REPEAT_DELAY_MS {
            return;
        }
        let run = held - REPEAT_DELAY_MS;
        let every = REPEAT_MS - (REPEAT_MS - REPEAT_FAST_MS) * run.min(REPEAT_RAMP_MS) / REPEAT_RAMP_MS;
        if now.saturating_sub(last) < every {
            return;
        }
        a.held = Some((dir, since, now));
        self.step_label_frame(dir);
    }

    /// Cuts the frame out of the picture and wears it, then goes back to the cart menu.
    fn keep_framed_label(&mut self, source: &Path, frame: crate::cart_art::Frame) {
        let Some(pick) = self.label_pick.take() else {
            return;
        };
        let Some(root) = self.root.clone() else {
            return;
        };
        let Some(mut cart) = self.shelf_index_of(&pick.stem).and_then(|i| self.shelf().carts.get(i)).cloned() else {
            return;
        };
        (cart.label, cart.famicom_label) = pick.original.clone();
        match crate::cart_art::keep_framed(&root, &cart, source, frame) {
            Ok(kept) => eprintln!("slot: label: kept {} for {}", kept.display(), cart.stem),
            Err(e) => {
                eprintln!("slot: label: {e}");
                self.card_failing.store(true, Ordering::Relaxed);
            }
        }
        self.adopt_label(cart.platform, &cart.stem, None);
        crate::cart_art::forget_choices(&root);
    }

    /// The label being framed is in front of the grid.
    fn label_adjust_open(&self) -> bool {
        self.label_pick.as_ref().is_some_and(|p| p.adjust.is_some())
    }

    /// Puts the grid away without touching what was picked.
    fn close_label_grid(&mut self) {
        if let Some(pick) = &mut self.label_pick {
            pick.grid = false;
        }
    }

    /// The grid is up and should be drawn and fed input instead of the menu behind it.
    fn label_grid_open(&self) -> bool {
        self.label_pick.as_ref().is_some_and(|p| p.grid)
    }

    /// This page of the grid: each label's picture, where the cursor is in it, and the words
    /// under it. None when no grid is up.
    pub fn label_grid_page(&self) -> Option<(usize, Vec<PathBuf>, String)> {
        let pick = self.label_pick.as_ref().filter(|p| p.grid)?;
        let page = pick.page();
        let paths: Vec<PathBuf> =
            pick.choices.iter().skip(page * slot_ui::PAGE).take(slot_ui::PAGE).map(|c| c.path.clone()).collect();
        Some((page, paths, pick.grid_caption()))
    }

    /// Wi-Fi has an address: only the Brick has Wi-Fi to ask.
    fn online(&self) -> bool {
        #[cfg(feature = "brick")]
        return self.net_up;
        #[cfg(not(feature = "brick"))]
        false
    }

    /// Y on the label row (or in the core picker): the keyboard, starting from the game's name.
    fn type_label_search(&mut self, stem: &str) {
        let Some(cart) = self.shelf_index_of(stem).and_then(|i| self.shelf().carts.get(i)).cloned() else {
            return;
        };
        // Y is greyed out without Wi-Fi, so there is nothing to say here: it simply does not go.
        if !self.online() {
            return;
        }
        let pick = self.label_pick.get_or_insert_with(|| LabelPick::new(&cart));
        #[cfg(feature = "brick")]
        {
            let start = pick.query.clone().unwrap_or_else(|| slot_ui::label_text(&cart));
            self.wifi = Some(crate::brick_wifi::WifiMenu::label_query(&start));
        }
    }

    /// A search for the open cart's labels by what was typed: what an earlier one found is put
    /// aside, the cart showing its own label until the first arrives.
    // Searches are typed, on the Brick's keyboard only.
    #[cfg_attr(not(feature = "brick"), allow(dead_code))]
    fn search_labels(&mut self, stem: &str, query: String) {
        let Some(root) = self.root.clone() else {
            return;
        };
        let Some(cart) = self.shelf_index_of(stem).and_then(|i| self.shelf().carts.get(i)).cloned() else {
            return;
        };
        let net_up = self.online();
        let pick = self.label_pick.get_or_insert_with(|| LabelPick::new(&cart));
        pick.offline = !net_up;
        if !net_up {
            return;
        }
        pick.search = None;
        pick.choices.clear();
        pick.at = 0;
        pick.done = None;
        pick.jump = true;
        pick.query = Some(query.clone());
        pick.grid = true;
        let mut asked = cart;
        (asked.label, asked.famicom_label) = pick.original.clone();
        pick.search = Some(crate::cart_art::LabelSearch::start(root, asked, query));
        self.show_label_pick();
    }

    /// The cart wears the label picked (or its own, at 0) for the shape it is on now.
    fn show_label_pick(&mut self) {
        let Some(pick) = &self.label_pick else {
            return;
        };
        let (own, famicom) = pick.original.clone();
        let chosen = pick.at.checked_sub(1).and_then(|i| pick.choices.get(i)).map(|c| c.path.clone());
        let stem = pick.stem.clone();
        let Some(index) = self.shelf_index_of(&stem) else {
            return;
        };
        let shelf = self.shelf_mut();
        let Some(cart) = shelf.carts.get_mut(index) else {
            return;
        };
        let on_famicom = cart.platform == Platform::Nes && slot_ui::nes_famicom_of(cart);
        (cart.label, cart.famicom_label) = (own, famicom);
        if let Some(path) = chosen {
            if on_famicom {
                cart.famicom_label = Some(path);
            } else {
                cart.label = Some(path);
            }
        }
        let stale = shelf.take_face(index);
        self.stale_faces.extend(stale);
    }

    /// The menu closed: with `keep`, the label picked is written to the card as the cart's;
    /// otherwise the cart wears its own again. The search, if still running, stops.
    fn end_label_pick(&mut self, keep: bool) {
        let Some(mut pick) = self.label_pick.take() else {
            return;
        };
        pick.search = None;
        let chosen = pick.at.checked_sub(1).and_then(|i| pick.choices.get(i)).map(|c| c.path.clone());
        let (Some(root), Some(path), true) = (self.root.clone(), chosen, keep) else {
            pick.at = 0;
            self.label_pick = Some(pick);
            self.show_label_pick();
            self.label_pick = None;
            if let Some(root) = &self.root {
                crate::cart_art::forget_choices(root);
            }
            return;
        };
        let Some(mut cart) = self.shelf_index_of(&pick.stem).and_then(|i| self.shelf().carts.get(i)).cloned() else {
            return;
        };
        (cart.label, cart.famicom_label) = pick.original.clone();
        match crate::cart_art::keep_choice(&root, &cart, &path) {
            Ok(kept) => eprintln!("slot: label: kept {} for {}", kept.display(), cart.stem),
            Err(e) => {
                eprintln!("slot: label: {e}");
                self.card_failing.store(true, Ordering::Relaxed);
            }
        }
        // Read back from the card: the label kept, or the one it had if writing failed. The
        // labels not kept are not kept on the card either.
        self.adopt_label(cart.platform, &cart.stem, None);
        crate::cart_art::forget_choices(&root);
    }

    /// Labels arriving from the search: the first one is shown at once if the press that
    /// started it is still the last.
    fn poll_label_pick(&mut self) {
        let Some(pick) = &mut self.label_pick else {
            return;
        };
        let news = pick.search.as_ref().map(|s| s.poll()).unwrap_or_default();
        let mut show = false;
        for n in news {
            match n {
                crate::cart_art::PickNews::Found(choice) => {
                    pick.choices.push(choice);
                    if pick.jump {
                        pick.jump = false;
                        pick.at = 1;
                        show = !pick.grid;
                    }
                }
                crate::cart_art::PickNews::Done(result) => {
                    if let Err(e) = &result {
                        eprintln!("slot: label search: {e}");
                    }
                    pick.done = Some(result.is_ok());
                    pick.search = None;
                }
            }
        }
        // A page's worth is always worth having ready, whatever the search is told to run past.
        if let Some(pick) = &self.label_pick {
            if let Some(search) = &pick.search {
                search.looking_at((pick.page() + 1) * slot_ui::PAGE);
            }
        }
        if show {
            self.show_label_pick();
        }
    }

    /// The page the grid's pictures were made for, so they are made again only when it turns.
    pub fn label_grid_built(&self) -> Option<(usize, usize)> {
        self.label_grid_faces.as_ref().map(|g| g.built)
    }

    /// The words the grid was last given, which change far more often than its pictures.
    pub fn label_grid_caption_built(&self) -> Option<&str> {
        self.label_grid_faces.as_ref().map(|g| g.caption_built.as_str())
    }

    pub fn set_label_grid_faces(
        &mut self,
        built: (usize, usize),
        caption_built: String,
        title: (TexId, u32, u32),
        caption: (TexId, u32, u32),
        cells: Vec<Option<(TexId, u32, u32)>>,
        legend: [(TexId, u32); 3],
    ) {
        self.label_grid_faces = Some(LabelGridFaces { built, caption_built, title, caption, cells, legend });
    }

    /// Only the words again: the pictures on the page have not changed.
    pub fn set_label_grid_caption(&mut self, words: String, caption: (TexId, u32, u32)) {
        if let Some(grid) = &mut self.label_grid_faces {
            (grid.caption_built, grid.caption) = (words, caption);
        }
    }

    /// What was typed, for the grid's heading.
    pub fn label_grid_query(&self) -> Option<String> {
        self.label_pick.as_ref().filter(|p| p.grid).and_then(|p| p.query.clone())
    }

    /// The cart on the shelf wears `shell`; its face is rebuilt for the next frame.
    fn preview_shell(&mut self, stem: &str, shell: Option<ShellChoice>) {
        if let Some(index) = self.shelf_index_of(stem) {
            let stale = self.shelf_mut().set_shell(index, shell);
            self.stale_faces.extend(stale);
        }
    }

    /// Keeps the menu's choice: in Labels/cart_shell.ini, the layer that wins, with "auto"
    /// for the game's own plastic so no other layer brings an old choice back.
    fn save_shell(&mut self, menu: &CartMenu) {
        let chosen = menu.chosen();
        if chosen == menu.before {
            return;
        }
        let Some(root) = self.root.clone() else {
            return;
        };
        let value = chosen.map_or("auto".to_string(), |c| c.to_value());
        let written = slot_store::ini::write(&root, slot_store::LABELS_SHELL_FILE, &menu.stem, &value);
        if let Err(e) = &written {
            eprintln!("slot: cart shell: {e}");
        }
        self.card_failing.store(written.is_err(), Ordering::Relaxed);
    }

    fn draw_cart_menu(&self, menu: &CartMenu, out: &mut Vec<Draw>) {
        let index = self.shelf_index_of(&menu.stem);
        let cart = index.and_then(|i| self.shelf().carts.get(i));
        let face = index.and_then(|i| self.shelf().face(i)).zip(cart).map(|(tex, cart)| {
            let (w, h) = slot_ui::cart_box(cart.platform);
            (tex, w, h)
        });
        slot_ui::CartMenuView {
            rows: &menu.rows,
            row: menu.row,
            colour: menu.colour,
            finish: menu.finish,
            // The SNES shape names follow the Game Boy ones, the NES ones follow those.
            shape: menu.shape
                + match cart.map(|c| c.platform) {
                    Some(Platform::Snes) => slot_ui::SHAPES.len(),
                    Some(Platform::Nes) => slot_ui::SHAPES.len() + slot_ui::SNES_SHAPES.len(),
                    _ => 0,
                },
            cart: face,
            faces: self.cart_menu_faces.as_ref(),
            online: self.online(),
        }
        .draw(out);
    }

    fn draw_label_adjust(&self, out: &mut Vec<Draw>) {
        let Some((stand, frame, picture)) = self.label_adjust_layout() else {
            return;
        };
        let faces = self.label_adjust_faces.as_ref();
        slot_ui::LabelAdjustView {
            picture: faces.map(|f| (f.picture, picture.0, picture.1, picture.2, picture.3)),
            cart: faces.map(|f| (f.cart, stand.0, stand.1, stand.2, stand.3)),
            frame,
            caption: faces.map(|f| f.caption),
            legend: faces.map(|f| f.legend),
        }
        .draw(out);
    }

    /// Where the cart stands, where its slot is, and where the picture lies behind it.
    fn label_adjust_layout(&self) -> Option<(slot_ui::Rect, slot_ui::Rect, slot_ui::Rect)> {
        let adjust = self.label_pick.as_ref()?.adjust.as_ref()?;
        let (stand, frame) = slot_ui::slot_stand(adjust.face, adjust.slot);
        let (cx, cy, cw, ch) = stand;
        // The picture behind, stretched so the piece inside the slot fills it exactly.
        let (sx, sy) = (frame.2 / adjust.crop.2.max(1.0), frame.3 / adjust.crop.3.max(1.0));
        let picture = (
            frame.0 - adjust.crop.0 * sx,
            frame.1 - adjust.crop.1 * sy,
            adjust.size.0 as f32 * sx,
            adjust.size.1 as f32 * sy,
        );
        Some(((cx, cy, cw, ch), frame, picture))
    }

    /// The picture being framed and the words under it, for the frontend to draw.
    pub fn label_adjust_view(&self) -> Option<(PathBuf, String, Cart)> {
        let pick = self.label_pick.as_ref()?;
        let adjust = pick.adjust.as_ref()?;
        let choice = pick.choices.get(pick.cursor())?;
        let cart = self.shelf_index_of(&pick.stem).and_then(|i| self.shelf().carts.get(i))?.clone();
        // How large the picture is against the size it came in at, which is what moves as it
        // is stretched: the frame's own measurements run the other way and read as nonsense.
        let across = adjust.original.2 / adjust.crop.2.max(1.0) * 100.0;
        let down = adjust.original.3 / adjust.crop.3.max(1.0) * 100.0;
        let size = match (across - down).abs() < 1.0 {
            true => format!("{across:.0}%"),
            false => format!("{across:.0}% x {down:.0}%  stretched"),
        };
        Some((adjust.source.clone(), format!("{}   {size}", choice.caption), cart))
    }

    /// The hints already uploaded, so moving the frame does not draw them again.
    pub fn label_adjust_legend(&self) -> Option<([(TexId, u32); 5], TexId)> {
        self.label_adjust_faces.as_ref().map(|f| (f.legend, f.cart))
    }

    pub fn label_adjust_built(&self) -> Option<(&Path, &str)> {
        self.label_adjust_faces.as_ref().map(|f| (f.built.0.as_path(), f.built.1.as_str()))
    }

    pub fn set_label_adjust_faces(
        &mut self,
        built: (PathBuf, String),
        picture: TexId,
        cart: TexId,
        caption: (TexId, u32, u32),
        legend: [(TexId, u32); 5],
    ) {
        self.label_adjust_faces = Some(LabelAdjustFaces { built, picture, cart, caption, legend });
    }

    fn draw_label_grid(&self, out: &mut Vec<Draw>) {
        let cells = self.label_grid_faces.as_ref().map(|g| g.cells.as_slice()).unwrap_or(&[]);
        slot_ui::LabelGridView {
            title: self.label_grid_faces.as_ref().map(|g| g.title),
            caption: self.label_grid_faces.as_ref().map(|g| g.caption),
            cells,
            at: self.label_pick.as_ref().map_or(0, |p| p.cursor() % slot_ui::PAGE),
            legend: self.label_grid_faces.as_ref().map(|g| g.legend),
        }
        .draw(out);
    }

    fn open_core_picker(&mut self) {
        let Some(root) = self.root.clone() else {
            return;
        };
        let Some(cart) = self.shelf().carts.get(self.shelf().index) else {
            return;
        };
        if cart.platform != Platform::Gba {
            return;
        }
        let seat = slot_store::core_for(&root, &cart.stem);
        let now = self.now();
        let mut picker = CorePicker::open(seat, now);
        if self.core_faces_ready() {
            picker.start(now);
        }
        self.core_picker = Some(picker);
        self.shelf_mut().release_hold();
        self.play_held = None;
    }

    fn core_faces_ready(&self) -> bool {
        self.core_faces_stem
            .as_deref()
            .is_some_and(|stem| self.selected_stem() == Some(stem))
    }

    fn core_picker_input(&mut self, action: Action) {
        // Y: type what to search labels for.
        if let Action::GbaDown(Btn::Y) = action {
            let open = self.core_picker.is_some_and(|p| !p.closing());
            if let (true, Some(stem)) = (open, self.picker_look.as_ref().map(|l| l.stem.clone())) {
                self.type_label_search(&stem);
            }
            return;
        }
        // Up/Down: the cart's colour and finish, previewed on the lid.
        if let Action::GbaDown(dir @ (Btn::Up | Btn::Down)) = action {
            let open = self.core_picker.is_some_and(|p| !p.closing());
            if let (true, Some(mut look)) = (open, self.picker_look.take()) {
                look.row = if dir == Btn::Up { slot_ui::CartRow::Colour } else { slot_ui::CartRow::Finish };
                look.step_value(1);
                // The lid shows it (rebuilt off the UI thread); the shelf's own picture of the
                // cart, hidden under the lid, is redrawn once when the picker closes.
                if let Some(index) = self.shelf_index_of(&look.stem) {
                    if let Some(cart) = self.shelf_mut().carts.get_mut(index) {
                        cart.shell = look.chosen();
                    }
                }
                self.picker_look = Some(look);
            }
            return;
        }
        let press = match action {
            Action::GbaDown(Btn::Left) | Action::ShelfLeft => Press::Left,
            Action::GbaDown(Btn::Right) | Action::ShelfRight => Press::Right,
            Action::GbaDown(Btn::A) => Press::Keep,
            Action::GbaDown(Btn::B) => Press::Back,
            _ => return,
        };
        let now = self.now();
        let Some(picker) = &mut self.core_picker else {
            return;
        };
        let closing = picker.closing();
        let outcome = picker.press(press, now);
        if !closing && picker.closing() {
            if let Some(look) = self.picker_look.take() {
                let kept = if press == Press::Keep { look.chosen() } else { look.before };
                self.end_label_pick(press == Press::Keep);
                self.preview_shell(&look.stem, kept);
                if press == Press::Keep {
                    self.save_shell(&look);
                }
            }
        }
        if let Outcome::Write(core) = outcome {
            self.write_core(core);
        }
    }

    fn open_game_menu(&mut self) {
        if self.game_menu.is_some() {
            return;
        }
        if self.link_active() {
            return self.refuse();
        }
        if self.core == Core::Mgba {
            let wireless = self
                .seated()
                .is_some_and(|stem| self.link_mode(stem).0 == LinkKind::Wireless);
            if wireless {
                self.hud.toast(Toast::NeedsGpsp, self.now());
                return;
            }
            self.link_hardware = LinkKind::Cable;
            self.radio.ask(RadioJob::Warm);
            self.game_menu = Some(GameMenu::Pick(self.last_role));
            return;
        }
        if self.platform != Platform::Gba {
            self.hud.toast(Toast::NoLink, self.now());
            return;
        }
        let carried = self
            .seated()
            .and_then(|stem| self.auto_link(stem))
            .is_some_and(|(cart, _)| link_carried(&cart.code, &cart.title));
        if !carried {
            self.hud.toast(Toast::NoLink, self.now());
            return;
        }
        if self.core != Core::Gpsp {
            self.hud.toast(Toast::NeedsGpsp, self.now());
            return;
        }
        let hardware = self
            .seated()
            .map_or(LinkKind::Cable, |stem| self.link_mode(stem).0);
        self.link_hardware = hardware;
        self.radio.ask(RadioJob::Warm);
        self.game_menu = Some(GameMenu::Pick(self.last_role));
    }

    fn game_menu_shortcut(&mut self) {
        let Some(client_id) = self.link_client_id() else {
            return self.open_game_menu();
        };
        let now = self.now();
        self.game_menu = Some(GameMenu::Linked {
            role: LinkRow::from_client_id(client_id),
            worked: now,
            since: now,
            opened: true,
        });
    }

    fn end_link_from_menu(&mut self) {
        let role = self
            .link_client_id()
            .map_or(self.last_role, LinkRow::from_client_id);
        self.end_link();
        self.hud.toast(Toast::LinkEnded, self.now());
        self.unplug(role);
    }

    fn unplug(&mut self, role: LinkRow) {
        self.game_menu = Some(GameMenu::Unplug {
            role,
            since: self.now(),
        });
    }

    pub fn set_radio_jobs(&mut self, jobs: Box<dyn RadioJobs>) {
        self.radio = jobs;
    }

    fn game_menu_input(&mut self, action: Action) {
        let Some(menu) = self.game_menu else {
            return;
        };
        match menu {
            GameMenu::Pick(role) => match action {
                Action::GbaDown(Btn::Left) | Action::GbaDown(Btn::Right) => {
                    self.last_role = role.other();
                    self.game_menu = Some(GameMenu::Pick(role.other()));
                }
                Action::GbaDown(Btn::Select) => self.switch_hardware(),
                Action::GbaDown(Btn::A) => self.pick_link(role),
                Action::GbaDown(Btn::B) | Action::GameMenu => self.close_game_menu(),
                _ => {}
            },
            GameMenu::Working { .. } => {
                if action == Action::GbaDown(Btn::B) {
                    if let Some(starting) = &mut self.starting {
                        starting.starter.cancel();
                    }
                    if let Some(reload) = &mut self.reload {
                        reload.cancelled = true;
                    }
                }
            }
            GameMenu::Linked { opened: true, .. } => match action {
                Action::GbaDown(Btn::A) => self.end_link_from_menu(),
                Action::GbaDown(Btn::B) | Action::GameMenu => self.close_game_menu(),
                _ => {}
            },
            GameMenu::Linked { .. } => {}
            GameMenu::Failed { .. } => {
                if matches!(
                    action,
                    Action::GbaDown(Btn::A) | Action::GbaDown(Btn::B) | Action::GameMenu
                ) {
                    self.close_game_menu();
                }
            }
            GameMenu::Unplug { .. } => {}
        }
    }

    pub fn start_link(&mut self, starter: LinkStarter, client_id: u16) {
        self.start_link_from(starter, client_id, self.now());
    }

    fn start_link_from(&mut self, starter: LinkStarter, client_id: u16, since: Millis) {
        if let Some(mut old) = self.starting.replace(LinkStarting { starter, client_id }) {
            old.starter.cancel();
        }
        self.game_menu = Some(GameMenu::Working {
            role: LinkRow::from_client_id(client_id),
            step: LinkStep::Radio,
            since,
        });
    }

    fn switch_hardware(&mut self) {
        let Some(stem) = self.seated().map(str::to_string) else {
            return;
        };
        if !self.link_switchable(&stem) {
            return self.refuse();
        }
        let other = self.link_hardware.other();
        self.link_hardware = other;
        self.link_choices.insert(stem, other);
    }

    fn link_switchable(&self, stem: &str) -> bool {
        self.auto_link(stem).is_some_and(|(cart, auto)| {
            serial_option(self.link_hardware.other(), auto, &cart.code, &cart.title)
                != serial_option(self.link_hardware, auto, &cart.code, &cart.title)
        })
    }

    fn pick_link(&mut self, role: LinkRow) {
        let Some(stem) = self.seated().map(str::to_string) else {
            return;
        };
        let loaded = self.link_loaded.unwrap_or("auto");
        self.solo_serial.get_or_insert(loaded);
        if self.core == Core::Mgba {
            let player = role.client_id() as u8;
            if self.link_player == Some(player) {
                return self.start_link(
                    LinkStarter::spawn(role.role(), link_port()),
                    role.client_id(),
                );
            }
            if self.snapshot.as_ref().is_some_and(|s| !s.resume_trusted()) {
                return self.refuse();
            }
            self.link_player = Some(player);
            self.link_reload = Some((stem.clone(), loaded));
            self.reload = Some(Reload {
                stem,
                role,
                cancelled: false,
                from: self.link_hardware,
                from_serial: loaded,
                fallback: false,
            });
            self.game_menu = Some(GameMenu::Working {
                role,
                step: LinkStep::Radio,
                since: self.now(),
            });
            return;
        }
        let (_, serial) = self.link_mode(&stem);
        if serial == loaded {
            return self.start_link(
                LinkStarter::spawn(role.role(), link_port()),
                role.client_id(),
            );
        }
        if self.snapshot.as_ref().is_some_and(|s| !s.resume_trusted()) {
            return self.refuse();
        }
        self.link_reload = Some((stem.clone(), serial));
        self.reload = Some(Reload {
            stem,
            role,
            cancelled: false,
            from: self.link_hardware.other(),
            from_serial: loaded,
            fallback: false,
        });
        self.game_menu = Some(GameMenu::Working {
            role,
            step: LinkStep::Radio,
            since: self.now(),
        });
    }

    fn refuse_seated(&mut self) {
        self.close_game_menu();
        self.pending = None;
        let caught = self.seat();
        let cart = match &mut self.phase {
            Phase::Playing { cart } => Some(std::mem::take(cart)),
            Phase::Doze { cart } => {
                *cart = None;
                None
            }
            _ => None,
        };
        if let Some(cart) = cart {
            self.refuse_out(cart, caught);
        }
    }

    fn close_game_menu(&mut self) {
        self.game_menu = None;
        if let Some(mut starting) = self.starting.take() {
            starting.starter.cancel();
        }
        if !self.link_active() {
            self.radio.ask(RadioJob::Cool);
        }
        let uncollected =
            self.link_reload.is_some() && self.reload.as_ref().is_some_and(|r| !r.fallback);
        if uncollected {
            self.link_reload = None;
            self.reload = None;
        } else if let Some(reload) = &mut self.reload {
            reload.cancelled = true;
        }
    }

    fn working_role(&self, client_id: u16) -> (LinkRow, Millis) {
        match self.game_menu {
            Some(GameMenu::Working { role, since, .. }) => (role, since),
            _ => (LinkRow::from_client_id(client_id), self.now()),
        }
    }

    fn poll_link(&mut self) {
        if self.power_menu.is_some() {
            return;
        }
        let Some(mut starting) = self.starting.take() else {
            return;
        };
        match starting.starter.poll() {
            None => self.starting = Some(starting),
            Some(LinkProgress::At(step)) => {
                if let Some(GameMenu::Working { role, since, .. }) = self.game_menu {
                    self.game_menu = Some(GameMenu::Working { role, step, since });
                }
                self.starting = Some(starting);
            }
            Some(LinkProgress::Ready(link)) => {
                let (role, worked) = self.working_role(starting.client_id);
                self.game_menu = Some(GameMenu::Linked {
                    role,
                    worked,
                    since: self.now(),
                    opened: false,
                });
                self.begin_link(starting.client_id);
                self.link_transport = Some((starting.client_id, Box::new(link)));
            }
            Some(LinkProgress::Failed(LinkFail::Cancelled)) => self.game_menu = None,
            Some(LinkProgress::Failed(fail)) => {
                let (role, worked) = self.working_role(starting.client_id);
                self.game_menu = Some(GameMenu::Failed {
                    role,
                    fail,
                    worked,
                    since: self.now(),
                });
            }
        }
    }

    fn write_core(&self, core: Core) {
        let (Some(root), Some(cart)) = (
            self.root.clone(),
            self.shelf().carts.get(self.shelf().index),
        ) else {
            return;
        };
        if let Err(e) = slot_store::write_selected_core(&root, &cart.stem, core) {
            eprintln!("slot: core: could not write selected_core.ini: {e}");
        }
    }

    fn begin_power_off(&mut self) {
        if self.powering_off {
            return;
        }
        if self.link_active() {
            self.end_link();
        }
        self.close_game_menu();
        self.powering_off = true;
        self.act_at = self.now() + SHUTDOWN_SHOW_MS;
        self.set_led(LedState::Off);
    }

    pub fn on_battery(&mut self, b: Battery) {
        if b.percent > BATTERY_CRITICAL || self.powering_off {
            return;
        }
        if matches!(b.charge, Charge::Charging | Charge::Full) {
            return;
        }
        self.flush_resume();
        self.begin_power_off();
    }

    fn doze_expired(&self) -> bool {
        if self.link_active() {
            return false;
        }
        let (Phase::Doze { .. }, Some(power)) = (&self.phase, &self.power) else {
            return false;
        };
        self.now().saturating_sub(self.dozed_at) >= power.timeout().as_millis() as Millis
    }

    pub fn settle_saves(&mut self) {
        self.state_writer.flush();
        if let Some(h) = self.pending_save.take() {
            let _ = h.join();
        }
    }

    fn autosave(&mut self) {
        self.autosave_at = self.now() + AUTOSAVE_MS;
        self.settle_saves();
        let (Some(root), Some(snapshot), Some(cart)) = (&self.root, &self.snapshot, self.seated())
        else {
            return;
        };
        let Some(state) = snapshot.state() else {
            eprintln!("slot: autosave: the core gave up no state");
            return;
        };
        let (state, sav) = trusted_write(snapshot.as_ref(), state, "autosave");
        let (root, platform, core, cart) =
            (root.clone(), self.platform, self.core, cart.to_owned());
        let failing = self.card_failing.clone();
        let write = move || {
            let written = persist::flush(
                &root,
                platform,
                core,
                &cart,
                state.as_deref(),
                sav.as_deref(),
            );
            if let Err(e) = &written {
                eprintln!("slot: autosave: {e}");
            }
            failing.store(written.is_err(), Ordering::Relaxed);
        };
        match std::thread::Builder::new()
            .name("slot-autosave".into())
            .spawn(write)
        {
            Ok(h) => self.pending_save = Some(h),
            Err(e) => eprintln!("slot: autosave: no writer thread: {e}"),
        }
    }

    pub fn flush_resume(&mut self) {
        self.settle_saves();
        self.autosave_at = self.now() + AUTOSAVE_MS;
        let (Some(root), Some(snapshot), Some(cart)) = (&self.root, &self.snapshot, self.seated())
        else {
            return;
        };
        let Some(state) = snapshot.state() else {
            eprintln!("slot: flush: the core gave up no state");
            return;
        };
        let (state, sav) = trusted_write(snapshot.as_ref(), state, "flush");
        let written = persist::flush(
            root,
            self.platform,
            self.core,
            cart,
            state.as_deref(),
            sav.as_deref(),
        );
        if let Err(e) = &written {
            eprintln!("slot: flush: {e}");
        }
        self.card_failing.store(written.is_err(), Ordering::Relaxed);
    }

    fn ring(&self) -> Option<StateRing> {
        let (Some(root), Some(cart)) = (&self.root, self.seated()) else {
            return None;
        };
        Some(StateRing::new(root, self.platform, self.core, cart))
    }

    fn seated(&self) -> Option<&str> {
        match &self.phase {
            Phase::Playing { cart } | Phase::Polaroids { cart } => Some(cart),
            _ => None,
        }
    }

    fn entries(&self) -> Vec<StateEntry> {
        self.ring().and_then(|r| r.list().ok()).unwrap_or_default()
    }

    fn open_polaroids(&mut self) {
        if self.link_active() {
            return self.refuse();
        }
        let entries = self.entries();
        if entries.is_empty() {
            return self.refuse();
        }
        let Phase::Playing { cart } = &mut self.phase else {
            return;
        };
        let cart = std::mem::take(cart);
        let mut p = Polaroids::new(entries);
        p.set_undo(self.undo_label());
        self.polaroids = Some(p);
        self.phase = Phase::Polaroids { cart };
        self.push_hint_faces();
    }

    fn close_polaroids(&mut self) {
        let Phase::Polaroids { cart } = &mut self.phase else {
            return;
        };
        let cart = std::mem::take(cart);
        self.polaroids = None;
        self.phase = Phase::Playing { cart };
    }

    fn load_selected(&mut self) {
        let state = self
            .polaroids
            .as_ref()
            .and_then(|p| p.selected())
            .map(|e| e.state.clone());
        let refused = state.map(|state| self.load_file(&state)) == Some(false);
        if !refused {
            self.close_polaroids();
        }
    }

    fn delete_selected(&mut self) {
        let stamp = self
            .polaroids
            .as_ref()
            .and_then(|p| p.selected())
            .map(|e| e.stamp.clone());
        let (Some(stamp), Some(ring)) = (stamp, self.ring()) else {
            return;
        };
        if let Err(e) = ring.remove(&stamp) {
            eprintln!("slot: delete: {e}");
            return;
        }
        if self.undo_targets(&stamp) {
            self.pending = None;
        }
        let Some(p) = &mut self.polaroids else {
            return;
        };
        p.remove_selected();
        if p.is_empty() {
            self.close_polaroids();
        }
    }

    fn undo_targets(&self, stamp: &str) -> bool {
        match self.pending.as_ref() {
            Some((PendingUndo::Save { stamp: pending, .. }, _)) => pending == stamp,
            _ => false,
        }
    }

    fn load_newest(&mut self) {
        let Some(newest) = self.entries().first().map(|e| e.state.clone()) else {
            return self.refuse();
        };
        self.load_file(&newest);
    }

    fn load_file(&mut self, state: &Path) -> bool {
        if !self.may_load_state() {
            self.refuse();
            return false;
        }
        let Some(snapshot) = &self.snapshot else {
            return false;
        };
        let bytes = match std::fs::read(state) {
            Ok(bytes) => bytes,
            Err(e) => {
                eprintln!("slot: load: {e}");
                return false;
            }
        };
        let prior = snapshot.state();
        snapshot.load(bytes);
        self.hud.toast(Toast::StateLoaded, self.now());
        if let Some(prior) = prior {
            self.pending = Some((PendingUndo::Load { prior }, self.now()));
        }
        true
    }

    fn save_state(&mut self) {
        let (Some(ring), Some(snapshot)) = (self.ring(), &self.snapshot) else {
            return;
        };
        if !snapshot.resume_trusted() {
            eprintln!("slot: save: the core refused the resume it was given, not pushing a state");
            return self.refuse();
        }
        let Some(state) = snapshot.state() else {
            eprintln!("slot: save: the core gave up no state");
            return;
        };
        let thumb = snapshot.thumb().unwrap_or_default();
        let stamp = free_stamp(&ring, self.wall_secs());
        let evicted = doomed(&ring);
        if let Err(e) = ring.push(&state, &thumb, &stamp) {
            eprintln!("slot: save: {e}");
            return;
        }
        self.hud.toast(Toast::StateSaved, self.now());
        self.pending = Some((PendingUndo::Save { stamp, evicted }, self.now()));
    }

    pub fn undo_available(&self, now: Millis) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|(_, at)| now.saturating_sub(*at) <= UNDO_GRACE_MS)
    }

    pub fn undo_label(&self) -> Option<&'static str> {
        if !self.undo_available(self.now()) {
            return None;
        }
        match self.pending.as_ref()?.0 {
            PendingUndo::Save { .. } => Some("undo save"),
            PendingUndo::Load { .. } => Some("undo load"),
        }
    }

    pub fn set_legend_faces(&mut self, faces: Vec<TexId>) {
        self.legend_faces = faces;
        self.push_hint_faces();
    }

    pub fn set_undo_face(&mut self, face: Option<TexId>) {
        self.undo_face = face;
        self.push_hint_faces();
    }

    fn push_hint_faces(&mut self) {
        let mut faces = self.legend_faces.clone();
        faces.extend(self.undo_face);
        if let Some(p) = &mut self.polaroids {
            p.set_hint_faces(faces);
        }
    }

    pub fn set_icon_faces(&mut self, faces: Vec<TexId>) {
        self.hud.set_icons(faces);
    }

    pub fn set_toast_faces(&mut self, faces: Vec<TexId>) {
        self.hud.set_toasts(faces);
    }

    pub fn toast(&self) -> Option<Toast> {
        self.hud.said(self.now())
    }

    pub fn card_alarm(&self) -> bool {
        self.hud.alarmed()
    }

    pub fn undo(&mut self, now: Millis) {
        if !self.undo_available(now) {
            self.pending = None;
            return;
        }
        if matches!(&self.pending, Some((PendingUndo::Load { .. }, _))) && !self.may_load_state() {
            return self.refuse();
        }
        let Some((what, _)) = self.pending.take() else {
            return;
        };
        match what {
            PendingUndo::Save { stamp, evicted } => self.undo_save(&stamp, evicted),
            PendingUndo::Load { prior } => {
                if let Some(snapshot) = &self.snapshot {
                    snapshot.load(prior);
                }
            }
        }
        self.close_polaroids();
    }

    fn undo_save(&self, stamp: &str, evicted: Option<(String, Vec<u8>, Vec<u8>)>) {
        let Some(ring) = self.ring() else {
            return;
        };
        if let Err(e) = ring.remove(stamp) {
            eprintln!("slot: undo: {e}");
            return;
        }
        let Some((stamp, state, thumb)) = evicted else {
            return;
        };
        if let Err(e) = ring.push(&state, &thumb, &stamp) {
            eprintln!("slot: undo: {e}");
        }
    }

    pub fn polaroid_entries(&self) -> &[StateEntry] {
        match &self.polaroids {
            Some(p) => &p.entries,
            None => &[],
        }
    }

    pub fn set_polaroid_faces(&mut self, faces: Vec<TexId>) {
        if let Some(p) = &mut self.polaroids {
            p.set_faces(faces);
        }
    }

    pub fn polaroid_stamp(&self) -> Option<&str> {
        self.polaroids
            .as_ref()
            .and_then(|p| p.selected())
            .map(|e| e.stamp.as_str())
    }

    pub fn polaroid_title(&self, now: &str) -> String {
        self.polaroids
            .as_ref()
            .map_or_else(String::new, |p| p.title(now))
    }

    pub fn set_polaroid_title_face(&mut self, face: TexId) {
        if let Some(p) = &mut self.polaroids {
            p.set_title_face(Some(face));
        }
    }
}

fn trusted_write(
    snapshot: &dyn Snapshot,
    state: Vec<u8>,
    verb: &str,
) -> (Option<Vec<u8>>, Option<Vec<u8>>) {
    let state = if snapshot.resume_trusted() {
        Some(state)
    } else {
        eprintln!(
            "slot: {verb}: the core refused the resume it was given, not overwriting the saved one"
        );
        None
    };
    let sav = snapshot.save_ram();
    let sav = if snapshot.save_ram_trusted() {
        sav
    } else {
        if sav.is_some() {
            eprintln!(
                "slot: {verb}: the core refused the save ram it was given, not overwriting the saved one"
            );
        }
        None
    };
    (state, sav)
}

fn up(level: u8, step: u8, max: u8) -> u8 {
    level.saturating_add(step).min(max)
}

fn shader_value(shader: Shader) -> QuickValue {
    match shader {
        Shader::Off => QuickValue::Off,
        Shader::Lcd3x => QuickValue::Lcd3x,
        Shader::Grid => QuickValue::Grid,
        Shader::Dot => QuickValue::Dot,
        Shader::Simpletex => QuickValue::Simpletex,
    }
}

fn ff_next(from: u8, right: bool) -> u8 {
    let n = FF_SPEEDS.len();
    let at = FF_SPEEDS.iter().position(|&v| v == from).unwrap_or(0);
    FF_SPEEDS[if right {
        (at + 1) % n
    } else {
        (at + n - 1) % n
    }]
}

fn clock_screen(utc: i64, offset_min: i16, from_menu: bool) -> Phase {
    Phase::SetClock {
        picker: ClockPicker::local(utc, offset_min),
        seed: utc - utc.rem_euclid(60),
        from_menu,
    }
}

fn system_secs() -> i64 {
    #[cfg(feature = "sim")]
    if let Some(secs) = std::env::var("SLOT_SIM_TIME").ok().and_then(|v| v.parse().ok()) {
        return secs;
    }
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn doomed(ring: &StateRing) -> Option<(String, Vec<u8>, Vec<u8>)> {
    let entries = ring.list().ok()?;
    let oldest = entries.get(RING_MAX - 1)?;
    let (state, thumb) = ring.read(&oldest.stamp).ok()?;
    Some((oldest.stamp.clone(), state, thumb))
}

fn free_stamp(ring: &StateRing, now: i64) -> String {
    let taken: Vec<String> = ring
        .list()
        .map(|l| l.into_iter().map(|e| e.stamp).collect())
        .unwrap_or_default();
    let mut secs = now;
    let mut stamp = format_stamp(secs);
    while taken.contains(&stamp) {
        secs += 1;
        stamp = format_stamp(secs);
    }
    stamp
}

fn centred_top(rows: usize) -> f32 {
    (OUT_H as f32 - POWER_MENU_PITCH * rows as f32) / 2.0
}

fn draw_menu_rows(
    faces: &[(TexId, u32, u32)],
    index: Option<usize>,
    top: f32,
    out: &mut Vec<Draw>,
) {
    let row_w = faces.iter().map(|(_,w,_)|*w).max().unwrap_or(0) as f32;
    for (row, (tex, w, h)) in faces.iter().copied().enumerate() {
        let y = top + POWER_MENU_PITCH * row as f32;
        let x = ((OUT_W as f32 - w as f32) / 2.0).round();
        if index == Some(row) {
            out.push(Draw::Rect {
                x: ((OUT_W as f32-row_w)/2.0).round(),
                y: y + POWER_MENU_BAR_INSET,
                w: row_w,
                h: POWER_MENU_PITCH - 2.0 * POWER_MENU_BAR_INSET,
                colour: slot_ui::edge(),
            });
        }
        out.push(Draw::Tex {
            x,
            y: y + (POWER_MENU_PITCH - h as f32) / 2.0,
            w: w as f32,
            h: h as f32,
            tex,
            alpha: 1.0,
        });
    }
}

/// START on a cart: the rows it has and each one's value, as indices into the menu's faces.
#[derive(Clone)]
struct CartMenu {
    /// The cart it is for, by name.
    stem: String,
    rows: Vec<slot_ui::CartRow>,
    row: slot_ui::CartRow,
    colours: Vec<(&'static str, Option<slot_ui::Shell>)>,
    /// Which of them this cart is offered, as places in `colours`. A NES cart came in three
    /// plastics and no more; a Game Boy one came in all of them.
    offered: Vec<usize>,
    colour: usize,
    finish: usize,
    shape: usize,
    /// The game's own plastic: what Original and a cleared choice show.
    own: slot_ui::Shell,
    /// The cart's choice when the menu opened, put back by B.
    before: Option<ShellChoice>,
    /// A NES game's own look: whether it is a Famicom cart, and the plastic it has as one.
    famicom: Option<(bool, [u8; 3])>,
}

const OUTLINES: [Outline; 3] = [Outline::Auto, Outline::Notched, Outline::Rounded];

/// Labels found for one cart, to flip through: 0 is the label it had.
/// What the framing screen is drawn from: the whole picture, uploaded once.
struct LabelAdjustFaces {
    /// The picture and words these were made for.
    built: (PathBuf, String),
    picture: TexId,
    /// The cart with its label slot clear, laid over the picture.
    cart: TexId,
    caption: (TexId, u32, u32),
    legend: [(TexId, u32); 5],
}

/// What the label grid is drawn from, uploaded once a page and kept until the page turns.
struct LabelGridFaces {
    /// The page the pictures were made for: which page, and how many labels it holds.
    built: (usize, usize),
    /// The words, which change on their own as the cursor moves and labels land.
    caption_built: String,
    title: (TexId, u32, u32),
    caption: (TexId, u32, u32),
    cells: Vec<Option<(TexId, u32, u32)>>,
    legend: [(TexId, u32); 3],
}

struct LabelPick {
    stem: String,
    /// The cart's labels when the pick began (its own, its Famicom one), put back by B.
    original: (Option<PathBuf>, Option<PathBuf>),
    search: Option<crate::cart_art::LabelSearch>,
    choices: Vec<crate::cart_art::Choice>,
    at: usize,
    /// The search ended: whether it could be made at all.
    done: Option<bool>,
    /// Asked with Wi-Fi off.
    offline: bool,
    /// The press that started the search wants the first label found.
    jump: bool,
    /// What was typed to search for: the keyboard starts from it again.
    #[cfg_attr(not(feature = "brick"), allow(dead_code))]
    query: Option<String>,
    /// The grid of what was found is up, in front of the cart menu.
    grid: bool,
    /// The label picked is being framed by hand, in front of the grid.
    adjust: Option<Adjust>,
}

/// Framing a picked label. What is held is the piece of the picture the frame covers, in the
/// picture's own pixels, so a press moves it the same distance however far it is zoomed in.
struct Adjust {
    /// The whole picture being framed, and its size.
    source: PathBuf,
    size: (u32, u32),
    /// The piece of it inside the frame: left, top, width, height.
    crop: (f32, f32, f32, f32),
    /// The piece it started as, which Y goes back to and R takes its shape from.
    original: (f32, f32, f32, f32),
    /// The face of the cart the label is for, and where its label slot sits on it. Both are
    /// settled when the framing starts: the cart cannot change shape while it is going on.
    face: (u32, u32),
    slot: (u32, u32, u32, u32),
    /// L is down, so the d-pad stretches the picture rather than moving it.
    sizing: bool,
    /// The direction being held, when it began and when it last stepped, so holding a key
    /// keeps going. A tap alone is only a pixel or two, which would be unusable otherwise.
    held: Option<(Btn, Millis, Millis)>,
}

impl Adjust {
    /// Keeps the picture and the frame on each other, no closer in than a label can usefully
    /// be and no further out than MOST_OF_IT. The frame may be larger than the picture: a
    /// picture shrunk below the label's size leaves its edges empty, which is allowed.
    fn clamp(&mut self) {
        let (pw, ph) = (self.size.0 as f32, self.size.1 as f32);
        self.crop.2 = self.crop.2.clamp(MIN_FRAME_PX.min(pw), pw * MOST_OF_IT);
        self.crop.3 = self.crop.3.clamp(MIN_FRAME_PX.min(ph), ph * MOST_OF_IT);
        // Whichever of the two is larger holds the other, so the ends swap over as it passes.
        let span = |picture: f32, frame: f32| (0.0f32.min(picture - frame), 0.0f32.max(picture - frame));
        let (lo, hi) = span(pw, self.crop.2);
        self.crop.0 = self.crop.0.clamp(lo, hi);
        let (lo, hi) = span(ph, self.crop.3);
        self.crop.1 = self.crop.1.clamp(lo, hi);
    }

    /// Grows or shrinks away from the bottom left corner, which stays where it is: line the
    /// artwork up with the rim there and it keeps flush with it while the rest moves.
    fn resize(&mut self, dw: f32, dh: f32) {
        // Taking off the top whatever is added to the height leaves the bottom where it is.
        self.crop.1 -= dh;
        self.crop.2 += dw;
        self.crop.3 += dh;
        self.clamp();
    }

    /// R: back to the shape it started as, keeping the middle and the longer side.
    fn even_up(&mut self) {
        let shape = self.original.2 / self.original.3.max(1.0);
        let (w, h) = (self.crop.2, self.crop.3);
        let (want_w, want_h) = if w / h.max(1.0) > shape { (h * shape, h) } else { (w, w / shape) };
        self.resize(want_w - w, want_h - h);
    }

    /// What would be cut if it were kept now.
    fn frame(&self) -> crate::cart_art::Frame {
        crate::cart_art::Frame {
            x: self.crop.0.round() as i32,
            y: self.crop.1.round() as i32,
            w: (self.crop.2.round().max(1.0)) as u32,
            h: (self.crop.3.round().max(1.0)) as u32,
        }
    }
}

/// The frame never shrinks below this many of the picture's own pixels: past that it is mush.
const MIN_FRAME_PX: f32 = 48.0;
/// Nor does it open wider than this many times the picture, which leaves the picture a third
/// of the label at its smallest: enough for one that wants shrinking well past its own size.
const MOST_OF_IT: f32 = 3.0;
/// What one press is worth, in the picture's own pixels. Small enough to place a label exactly;
/// holding the key is what covers ground.
const STEP_PX: f32 = 2.0;
/// A held key waits this long, then steps this often, quickening to REPEAT_FAST_MS as it goes.
const REPEAT_DELAY_MS: Millis = 320;
const REPEAT_MS: Millis = 70;
const REPEAT_FAST_MS: Millis = 16;
/// How long a key must be held before it is stepping as fast as it will go.
const REPEAT_RAMP_MS: Millis = 1_200;

impl LabelPick {
    fn new(cart: &Cart) -> Self {
        LabelPick {
            stem: cart.stem.clone(),
            original: (cart.label.clone(), cart.famicom_label.clone()),
            search: None,
            choices: Vec::new(),
            at: 0,
            done: None,
            offline: false,
            jump: false,
            query: None,
            grid: false,
            adjust: None,
        }
    }

    /// The cell the cursor is on, counted from the first label found.
    fn cursor(&self) -> usize {
        self.at.saturating_sub(1)
    }

    /// The page of the grid it is on.
    fn page(&self) -> usize {
        self.cursor() / slot_ui::PAGE
    }

    /// The words under the grid: which label the cursor is on, of how many so far.
    fn grid_caption(&self) -> String {
        if self.offline {
            return "Connect to Wi-Fi first".into();
        }
        let found = self.choices.len();
        if found == 0 {
            return match (self.search.is_some(), self.done) {
                (true, _) => "Searching...".into(),
                (false, Some(false)) => "Can't search now".into(),
                _ => "None found".into(),
            };
        }
        // "of 20+" while more are still coming: the list has no end until the search ends.
        let more = if self.search.is_some() { "+" } else { "" };
        let choice = &self.choices[self.cursor().min(found - 1)];
        format!("{}   ({} of {found}{more})", choice.caption, self.cursor() + 1)
    }

}

fn finish_of(finish: ShellFinish) -> slot_ui::Finish {
    match finish {
        ShellFinish::Solid => slot_ui::Finish::Solid,
        ShellFinish::Clear => slot_ui::Finish::Translucent,
        ShellFinish::Glitter => slot_ui::Finish::Glitter,
        ShellFinish::Gold => slot_ui::Finish::Sheen,
    }
}

fn shell_finish(finish: slot_ui::Finish) -> ShellFinish {
    match finish {
        slot_ui::Finish::Solid => ShellFinish::Solid,
        slot_ui::Finish::Translucent => ShellFinish::Clear,
        slot_ui::Finish::Sheen => ShellFinish::Gold,
        slot_ui::Finish::Glitter => ShellFinish::Glitter,
    }
}

/// The plastics a cart is offered, as places in `colours`. A NES cart came in its system's
/// grey, the gold the two Zeldas were moulded in, and the black, blue and silver the
/// unlicensed publishers used; the Famicom cart came in its own eight, which are not the same
/// plastics at all. Every other console's carts are offered the lot. Whatever it wears is
/// always offered, so a colour set elsewhere can still be stepped away from.
fn offered_colours(
    nes: bool,
    famicom: bool,
    colours: &[(&'static str, Option<slot_ui::Shell>)],
) -> Vec<usize> {
    if !nes {
        return (0..colours.len()).collect();
    }
    const NES_PLASTICS: [&str; 5] =
        ["NES grey", "Zelda gold", "Tengen black", "Color Dreams blue", "Camerica silver"];
    let wanted = |name: &str| match famicom {
        true => slot_ui::FAMICOM_NAMES.contains(&name),
        // A colour set in Cart Studio belongs to no console's list, but has to stay reachable
        // or there would be no stepping away from it.
        false => NES_PLASTICS.contains(&name),
    } || name == "Custom";
    let mut out = vec![0];
    out.extend(colours.iter().enumerate().filter(|(_, (n, _))| wanted(n)).map(|(i, _)| i));
    out
}

impl CartMenu {
    fn open(cart: &Cart) -> Self {
        use slot_ui::CartRow;
        let mut plain = cart.clone();
        plain.shell = None;
        plain.read_header();
        let own = slot_ui::shell_for(&plain);
        let mut colours = slot_ui::colour_choices();
        let finish_at = |f: slot_ui::Finish| slot_ui::FINISHES.iter().position(|x| *x == f).unwrap_or(0);
        let famicom = (cart.platform == Platform::Nes)
            .then(|| (slot_ui::nes_famicom_of(&plain), slot_ui::famicom_shell(&cart.stem).colour));
        let (colour, finish, shape) = match cart.shell {
            None => (0, finish_at(own.finish), 0),
            Some(choice) => {
                let finish = finish_of(choice.finish);
                let colour = match colours.iter().position(|(_, s)| s.is_some_and(|s| s.colour == choice.colour)) {
                    // A Famicom cart in the plastic its name picks wears its own.
                    _ if famicom.is_some_and(|(_, own)| own == choice.colour) => 0,
                    Some(at) => at,
                    // A colour from elsewhere (Cart Studio): offered as "Custom", last.
                    None => {
                        colours.push(("Custom", Some(slot_ui::Shell { colour: choice.colour, finish })));
                        colours.len() - 1
                    }
                };
                (colour, finish_at(finish), OUTLINES.iter().position(|o| *o == choice.outline).unwrap_or(0))
            }
        };
        // NES and SNES carts came in their system's grey: only their shape changes, and a
        // colour an older Slot saved for one is dropped. A Famicom cart's colour is its own.
        let (colour, finish) = match cart.platform {
            Platform::Snes => (0, finish_at(own.finish)),
            Platform::Nes => (colour, finish_at(slot_ui::Finish::Solid)),
            _ => (colour, finish),
        };
        let rows = match cart.platform {
            // A NES cart's plastic can be chosen: the Zeldas came in gold, the rest in grey.
            Platform::Nes => vec![CartRow::Shape, CartRow::Colour, CartRow::Reset],
            Platform::Snes => vec![CartRow::Shape, CartRow::Reset],
            Platform::Gb | Platform::Gbc => vec![CartRow::Colour, CartRow::Finish, CartRow::Shape, CartRow::Reset],
            Platform::Gba => vec![CartRow::Colour, CartRow::Finish, CartRow::Reset],
        };
        let mut menu = CartMenu {
            stem: cart.stem.clone(),
            row: rows[0],
            rows,
            offered: Vec::new(),
            colours,
            colour,
            finish,
            shape,
            own,
            before: cart.shell,
            famicom,
        };
        menu.fit_colours();
        menu
    }

    /// A NES game on the Famicom cart: its shape as chosen, or its own.


    /// The shell these values make: None while they are all the game's own.
    /// On the Famicom shape a cart is offered the Famicom plastics, on the NES shape the NES
    /// ones. Stepping the shape moves it from one set to the other, so it keeps the plastic it
    /// is wearing if that is in both and falls back to its own if it is not.
    fn fit_colours(&mut self) {
        let famicom = self.famicom.is_some_and(|(own, _)| match OUTLINES[self.shape] {
            Outline::Auto => own,
            Outline::Notched => false,
            Outline::Rounded => true,
        });
        self.offered = offered_colours(self.famicom.is_some(), famicom, &self.colours);
        // A plastic the other shape came in is not one this shape came in: back to its own.
        if !self.offered.contains(&self.colour) {
            self.colour = 0;
        }
    }

    fn chosen(&self) -> Option<ShellChoice> {
        let picked = self.colours[self.colour].1;
        // A plastic brings its own finish; the finish row overrules it where there is one. The
        // gold carts have no row, and their shine is not among FINISHES to be picked anyway.
        let finish = match self.rows.contains(&slot_ui::CartRow::Finish) {
            true => slot_ui::FINISHES[self.finish],
            false => picked.map_or(self.own.finish, |s| s.finish),
        };
        let outline = OUTLINES[self.shape];
        if picked.is_none() && finish == self.own.finish && outline == Outline::Auto {
            return None;
        }
        // A Famicom cart's own colour is the one its name picks, whatever its shape now.
        let own = self.famicom.map_or(self.own.colour, |(_, colour)| colour);
        Some(ShellChoice {
            outline,
            colour: picked.map_or(own, |s| s.colour),
            finish: shell_finish(finish),
        })
    }

    fn step_row(&mut self, by: isize) {
        let n = self.rows.len() as isize;
        let at = self.rows.iter().position(|r| *r == self.row).unwrap_or(0) as isize;
        self.row = self.rows[(at + by).rem_euclid(n) as usize];
    }

    fn step_value(&mut self, by: isize) {
        let step = |v: usize, n: usize| (v as isize + by).rem_euclid(n as isize) as usize;
        match self.row {
            // The finish stays as it is: a colour only changes the colour.
            slot_ui::CartRow::Colour => {
                let at = self.offered.iter().position(|i| *i == self.colour).unwrap_or(0);
                self.colour = self.offered[step(at, self.offered.len())];
            }
            slot_ui::CartRow::Finish => self.finish = step(self.finish, slot_ui::FINISHES.len()),
            slot_ui::CartRow::Shape => {
                self.shape = step(self.shape, OUTLINES.len());
                self.fit_colours();
            }
            slot_ui::CartRow::Reset => {}
        }
    }

    fn reset(&mut self) {
        self.colour = 0;
        self.finish = slot_ui::FINISHES.iter().position(|f| *f == self.own.finish).unwrap_or(0);
        self.shape = 0;
    }
}
