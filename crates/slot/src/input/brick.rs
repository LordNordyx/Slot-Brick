//! TrimUI Brick input through the firmware's SDL joystick layer.
//! Button indices verified against NextUI workspace/tg5040/platform/platform.h.
use super::DeviceInput;
use libloading::Library;
use slot_input::{Btn, InputSource, Millis, RawEvent};
use std::ffi::{c_char, c_int, c_void, CStr};
use std::path::Path;

const JOYSTICK: u32 = 0x200;

pub fn button(index: u8) -> Option<Btn> {
    Some(match index {
        0 => Btn::B,
        1 => Btn::A,
        2 => Btn::Y,
        3 => Btn::X,
        4 => Btn::L1,
        5 => Btn::R1,
        6 => Btn::Select,
        7 => Btn::Start,
        8 => Btn::Menu,
        13 => Btn::VolDown,
        14 => Btn::VolUp,
        // The Brick Pro's second Menu key (NextUI's JOY_MENU_ALT). 9 and 10 are F1/F2 on the
        // Brick and the stick clicks on the Pro, 11 and 12 the Pro's F1/F2: none used.
        15 => Btn::Menu,
        _ => return None,
    })
}

pub fn hat_events(old: u8, new: u8) -> Vec<RawEvent> {
    let mut events = Vec::new();
    for (bit, btn) in [
        (1, Btn::Up),
        (2, Btn::Right),
        (4, Btn::Down),
        (8, Btn::Left),
    ] {
        if (old & bit) != (new & bit) {
            events.push(if new & bit != 0 {
                RawEvent::Down(btn)
            } else {
                RawEvent::Up(btn)
            });
        }
    }
    events
}

/// The Brick Pro's left stick, working as the D-pad: in menus and in games alike. Pushed past
/// half-way it presses that way, and lets go once back under a quarter, so it never flickers
/// at the edge. Diagonals press both. Its bits are the hat's: 1 up, 2 right, 4 down, 8 left.
#[derive(Default, Clone, Copy, Debug)]
pub struct StickPad {
    x: i16,
    y: i16,
    pub bits: u8,
    /// Handed to the game as a stick: it presses nothing.
    pub to_game: bool,
}

impl StickPad {
    const PRESS: i16 = 16000;
    const RELEASE: i16 = 8000;

    /// SDL axis 0 (left/right) or 1 (up/down), -32768..32767. The new bits, if this was one.
    pub fn axis(&mut self, axis: u8, value: i16) -> Option<u8> {
        match axis {
            0 => self.x = value,
            1 => self.y = value,
            _ => return None,
        }
        self.settle()
    }

    pub fn position(&self) -> (i16, i16) {
        (self.x, self.y)
    }

    /// Hands the stick to the game or takes it back. The D-pad presses it had in hand, or now
    /// makes, are the new bits.
    pub fn hand_to_game(&mut self, on: bool) -> Option<u8> {
        if self.to_game == on {
            return None;
        }
        self.to_game = on;
        self.settle()
    }

    fn settle(&mut self) -> Option<u8> {
        let held = |bit: u8, along: i16, pressed: bool| {
            let past = if self.bits & bit != 0 { Self::RELEASE } else { Self::PRESS };
            if pressed { along > past } else { along < -past }
        };
        let mut bits = 0;
        if self.to_game {
            self.bits = 0;
            return Some(0);
        }
        if held(1, self.y, false) { bits |= 1; }
        if held(2, self.x, true) { bits |= 2; }
        if held(4, self.y, true) { bits |= 4; }
        if held(8, self.x, false) { bits |= 8; }
        self.bits = bits;
        Some(bits)
    }
}

// TrimUI exposes rear L2/R2 as ABS_Z/ABS_RZ (SDL axes 2/5).
// Released is negative; require a positive excursion and use hysteresis.
fn trigger_event(held: &mut [bool; 2], axis: u8, value: i16) -> Option<RawEvent> {
    let (i, btn) = match axis { 2 => (0, Btn::L2), 5 => (1, Btn::R2), _ => return None };
    let down = if held[i] { value > 4000 } else { value > 16000 };
    if down == held[i] { return None; }
    held[i] = down;
    Some(if down { RawEvent::Down(btn) } else { RawEvent::Up(btn) })
}

// SDL_Event's ABI is 56 bytes and requires pointer alignment on aarch64.
#[repr(C, align(8))]
struct Event([u8; 56]);

pub struct BrickInput {
    poll: unsafe extern "C" fn(*mut Event) -> c_int,
    close: unsafe extern "C" fn(*mut c_void),
    quit: unsafe extern "C" fn(u32),
    joystick: *mut c_void,
    instance: i32,
    _lib: Library,
    power: DeviceInput,
    hat: u8,
    /// The Brick Pro's left stick; None on the Brick, which has none.
    stick: Option<StickPad>,
    /// D-pad presses let go of (or made) as the stick changes hands, for the next poll.
    pending: Vec<RawEvent>,
    held: u16,
    triggers: [bool; 2],
    exit_since: Option<Millis>,
    exit: bool,
    trace: bool,
}

impl BrickInput {
    pub fn open(root: &Path) -> Result<Self, String> {
        unsafe {
            let lib = Library::new("libSDL2-2.0.so.0")
                .or_else(|_| Library::new("libSDL2.so"))
                .map_err(|e| format!("Brick SDL input: {e}"))?;
            macro_rules! sym {
                ($name:literal, $ty:ty) => {
                    *lib.get::<$ty>(concat!($name, "\0").as_bytes())
                        .map_err(|e| e.to_string())?
                };
            }
            let init = sym!("SDL_InitSubSystem", unsafe extern "C" fn(u32) -> c_int);
            let quit = sym!("SDL_QuitSubSystem", unsafe extern "C" fn(u32));
            let error = sym!("SDL_GetError", unsafe extern "C" fn() -> *const c_char);
            let count = sym!("SDL_NumJoysticks", unsafe extern "C" fn() -> c_int);
            let open = sym!(
                "SDL_JoystickOpen",
                unsafe extern "C" fn(c_int) -> *mut c_void
            );
            let close = sym!("SDL_JoystickClose", unsafe extern "C" fn(*mut c_void));
            let instance_id = sym!(
                "SDL_JoystickInstanceID",
                unsafe extern "C" fn(*mut c_void) -> i32
            );
            let name = sym!(
                "SDL_JoystickName",
                unsafe extern "C" fn(*mut c_void) -> *const c_char
            );
            let poll = sym!("SDL_PollEvent", unsafe extern "C" fn(*mut Event) -> c_int);
            if init(JOYSTICK) != 0 {
                return Err(CStr::from_ptr(error()).to_string_lossy().into_owned());
            }
            // On a cold boot trimui_inputd is started just before Slot, and its virtual pad can
            // appear after SDL has looked: launch.sh no longer waits for it. Pump events for
            // hot-plug, and look again from scratch every second in case SDL doesn't see it.
            let started = std::time::Instant::now();
            let deadline = started + std::time::Duration::from_secs(8);
            let mut rescan = started + std::time::Duration::from_secs(1);
            while count() < 1 && std::time::Instant::now() < deadline {
                let mut event = std::mem::zeroed::<Event>();
                while poll(&mut event) != 0 {}
                std::thread::sleep(std::time::Duration::from_millis(20));
                if count() < 1 && std::time::Instant::now() >= rescan {
                    quit(JOYSTICK);
                    if init(JOYSTICK) != 0 {
                        return Err(CStr::from_ptr(error()).to_string_lossy().into_owned());
                    }
                    rescan += std::time::Duration::from_secs(1);
                }
            }
            if count() > 0 {
                eprintln!("slot: Brick joystick found after {} ms", started.elapsed().as_millis());
            }
            if count() < 1 {
                quit(JOYSTICK);
                return Err("No Brick joystick detected".into());
            }
            // The built-in pad is index zero with external USB/Bluetooth pads disconnected.
            let joystick = open(0);
            if joystick.is_null() {
                let msg = CStr::from_ptr(error()).to_string_lossy().into_owned();
                quit(JOYSTICK);
                return Err(msg);
            }
            let instance = instance_id(joystick);
            let label = name(joystick);
            if !label.is_null() {
                eprintln!(
                    "slot: Brick joystick {}",
                    CStr::from_ptr(label).to_string_lossy()
                );
            }
            Ok(Self {
                poll,
                close,
                quit,
                joystick,
                instance,
                _lib: lib,
                power: DeviceInput::open(root),
                hat: 0,
                stick: (slot_power::model() == slot_power::Model::BrickPro).then(StickPad::default),
                pending: Vec::new(),
                held: 0,
                triggers: [false; 2],
                exit_since: None,
                exit: false,
                trace: super::trace::enabled(),
            })
        }
    }
    pub fn exit_requested(&self) -> bool {
        self.exit
    }
}

impl InputSource for BrickInput {
    fn stick(&self) -> Option<(i16, i16)> {
        self.stick.map(|s| s.position())
    }

    fn set_stick_to_game(&mut self, on: bool) {
        if let Some(stick) = &mut self.stick {
            let before = self.hat | stick.bits;
            if let Some(bits) = stick.hand_to_game(on) {
                self.pending.extend(hat_events(before, self.hat | bits));
            }
        }
    }

    fn poll(&mut self, now: Millis) -> Vec<RawEvent> {
        let mut out = self.power.poll(now);
        out.append(&mut self.pending);
        let mut event = Event([0; 56]);
        while unsafe { (self.poll)(&mut event) } == 1 {
            let kind = u32::from_ne_bytes(event.0[0..4].try_into().unwrap());
            let which = i32::from_ne_bytes(event.0[8..12].try_into().unwrap());
            if which != self.instance {
                continue;
            }
            match kind {
                0x600 => {
                    let value = i16::from_ne_bytes(event.0[16..18].try_into().unwrap());
                    if let Some(raw) = trigger_event(&mut self.triggers, event.0[12], value) {
                        out.push(raw);
                    }
                    if let Some(stick) = &mut self.stick {
                        let before = self.hat | stick.bits;
                        if let Some(bits) = stick.axis(event.0[12], value) {
                            out.extend(hat_events(before, self.hat | bits));
                        }
                    }
                }
                0x603 | 0x604 => {
                    let index = event.0[12];
                    if self.trace {
                        eprintln!("brick input: button={index} down={}", kind == 0x603);
                    }
                    if index < 16 {
                        if kind == 0x603 {
                            self.held |= 1 << index;
                        } else {
                            self.held &= !(1 << index);
                        }
                    }
                    if let Some(btn) = button(index) {
                        out.push(if kind == 0x603 {
                            RawEvent::Down(btn)
                        } else {
                            RawEvent::Up(btn)
                        });
                    }
                }
                0x602 if event.0[12] == 0 => {
                    let new = event.0[13];
                    let stick = self.stick.map_or(0, |s| s.bits);
                    out.extend(hat_events(self.hat | stick, new | stick));
                    self.hat = new;
                }
                0x606 => {
                    // Built-in joystick removed: release held keys.
                    for index in 0..16 {
                        if self.held & (1 << index) != 0 {
                            if let Some(btn) = button(index) {
                                out.push(RawEvent::Up(btn));
                            }
                        }
                    }
                    out.extend(hat_events(self.hat | self.stick.map_or(0, |s| s.bits), 0));
                    if let Some(stick) = &mut self.stick {
                        *stick = StickPad::default();
                    }
                    for (i, btn) in [Btn::L2, Btn::R2].into_iter().enumerate() {
                        if self.triggers[i] { out.push(RawEvent::Up(btn)); }
                    }
                    self.triggers = [false; 2];
                    self.hat = 0;
                    self.held = 0;
                }
                _ => {}
            }
        }
        // Hold Menu + Start + Select for 1 second to save and return to the launcher.
        let chord = (1 << 6) | (1 << 7) | (1 << 8);
        if self.held & chord == chord {
            let since = *self.exit_since.get_or_insert(now);
            self.exit = now.saturating_sub(since) >= 1000;
        } else {
            self.exit_since = None;
        }
        out
    }
}

impl Drop for BrickInput {
    fn drop(&mut self) {
        unsafe {
            (self.close)(self.joystick);
            (self.quit)(JOYSTICK);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stick_handed_to_the_game_lets_go_of_the_d_pad_and_presses_nothing() {
        let mut s = StickPad::default();
        assert_eq!(s.axis(0, 30000), Some(2));
        assert_eq!(s.hand_to_game(true), Some(0), "the right press was not let go");
        assert_eq!(s.axis(1, -30000), Some(0));
        assert_eq!(s.position(), (30000, -30000));
        assert_eq!(s.hand_to_game(true), None);
        assert_eq!(s.hand_to_game(false), Some(1 | 2), "back as the D-pad, where it stands");
    }
    #[test]
    fn the_stick_presses_past_half_way_and_lets_go_under_a_quarter() {
        let mut s = StickPad::default();
        assert_eq!(s.axis(0, 12000), Some(0));
        assert_eq!(s.axis(0, 20000), Some(2));
        assert_eq!(s.axis(0, 12000), Some(2), "let go too early");
        assert_eq!(s.axis(0, 4000), Some(0));
        assert_eq!(s.axis(1, -30000), Some(1));
        assert_eq!(s.axis(0, -30000), Some(1 | 8), "a diagonal presses both");
        assert_eq!(s.axis(1, 30000), Some(4 | 8));
        assert_eq!(s.axis(3, 30000), None, "the right stick is not the D-pad");
        assert_eq!(button(15), Some(Btn::Menu));
    }
    #[test]
    fn brick_face_order() {
        assert_eq!(button(0), Some(Btn::B));
        assert_eq!(button(1), Some(Btn::A));
        assert_eq!(button(6), Some(Btn::Select));
        assert_eq!(button(9), None);
        assert_eq!(button(10), None);
        assert_eq!(button(13), Some(Btn::VolDown));
        assert_eq!(button(14), Some(Btn::VolUp));
        assert_eq!(button(12), None);
    }
    #[test]
    fn rear_triggers_press_hold_release() {
        let mut held = [false; 2];
        for (axis, btn) in [(2, Btn::L2), (5, Btn::R2)] {
            assert_eq!(trigger_event(&mut held, axis, -32768), None);
            assert_eq!(trigger_event(&mut held, axis, 32767), Some(RawEvent::Down(btn)));
            assert_eq!(trigger_event(&mut held, axis, 32767), None);
            assert_eq!(trigger_event(&mut held, axis, -32768), Some(RawEvent::Up(btn)));
        }
        assert_eq!(trigger_event(&mut held, 0, 32767), None);
    }
    #[test]
    fn diagonal_release_and_reverse() {
        assert_eq!(
            hat_events(3, 12),
            vec![
                RawEvent::Up(Btn::Up),
                RawEvent::Up(Btn::Right),
                RawEvent::Down(Btn::Down),
                RawEvent::Down(Btn::Left)
            ]
        );
        assert!(hat_events(3, 3).is_empty());
        assert_eq!(hat_events(8, 0), vec![RawEvent::Up(Btn::Left)]);
    }
}
