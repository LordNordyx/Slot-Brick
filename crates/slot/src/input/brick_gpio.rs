//! The Brick's buttons read straight from the A133's GPIO pins, for Slot OS, which runs no
//! trimui_inputd and has no SDL. The pins and registers are those Mikhailzrick's open
//! trimui_inputd reads (github.com/Mikhailzrick/Trimui_Inputd, src/devices/brick): every
//! button is one pin, pressed when low. The power key is not among them; it is an input
//! device of its own, read by DeviceInput as on stock. A Slot OS image for the Brick Pro reads
//! its pins (another map) and its sticks (pro_sticks) instead.
use super::brick::StickPad;
use super::DeviceInput;
use slot_input::{Btn, InputSource, Millis, RawEvent};
use std::path::Path;

/// The pin controller's registers, physical; banks of 0x24 bytes, A at the start.
const PIO: libc::off_t = 0x0300_B000;
const BANK_STRIDE: usize = 0x24;
const CFG0: usize = 0x00;
const DATA: usize = 0x10;
const MAP_LEN: usize = 0x1000;

/// A pin is bank * 32 + its number in the bank: 109 is PD13. The Brick's, as Mikhailzrick's
/// trimui_inputd reads them, and as TrimUI's own for the Brick has them.
const BRICK_PINS: [(u32, Btn); 17] = [
    (113, Btn::Up),
    (110, Btn::Down),
    (111, Btn::Left),
    (112, Btn::Right),
    (108, Btn::A),
    (109, Btn::B),
    (114, Btn::X),
    (115, Btn::Y),
    (36, Btn::L1),
    (241, Btn::R1),
    (34, Btn::L2),
    (37, Btn::R2),
    (239, Btn::Select),
    (240, Btn::Start),
    (238, Btn::Menu),
    (116, Btn::VolDown),
    (117, Btn::VolUp),
];

/// The Brick Pro's, as TrimUI's trimui_inputd for it (firmware 1.1.1, tg4040) reads them: the
/// D-pad, A/B/X/Y, R1/R2 and the volume where the Brick has them; Select, Start, Menu and the
/// left shoulders on other pins; and its second Menu key (Home), Menu too as on stock. F1/F2
/// (239, 240) and the stick clicks (135, 136) are unused, as on stock.
const PRO_PINS: [(u32, Btn); 18] = [
    (113, Btn::Up),
    (110, Btn::Down),
    (111, Btn::Left),
    (112, Btn::Right),
    (108, Btn::A),
    (109, Btn::B),
    (114, Btn::X),
    (115, Btn::Y),
    (34, Btn::L1),
    (241, Btn::R1),
    (36, Btn::L2),
    (37, Btn::R2),
    (237, Btn::Select),
    (238, Btn::Start),
    (236, Btn::Menu),
    (242, Btn::Menu),
    (116, Btn::VolDown),
    (117, Btn::VolUp),
];

/// Every button the pins press, bit i for BUTTONS[i]: a held set by button rather than by pin,
/// as the Brick Pro has two pins for Menu.
const BUTTONS: [Btn; 17] = [
    Btn::Up,
    Btn::Down,
    Btn::Left,
    Btn::Right,
    Btn::A,
    Btn::B,
    Btn::X,
    Btn::Y,
    Btn::L1,
    Btn::R1,
    Btn::L2,
    Btn::R2,
    Btn::Select,
    Btn::Start,
    Btn::Menu,
    Btn::VolDown,
    Btn::VolUp,
];

/// How often the Brick Pro's sticks are read, as TrimUI's daemon reads them.
const STICK_MS: Millis = 16;

pub struct GpioInput {
    regs: *mut u8,
    power: DeviceInput,
    pins: &'static [(u32, Btn)],
    /// Which BUTTONS are held.
    held: u32,
    /// The Brick Pro's sticks, the left one as the D-pad (as on stock), and when last read.
    sticks: Option<super::pro_sticks::ProSticks>,
    stick: Option<StickPad>,
    stick_at: Millis,
    /// The D-pad as last reported: its pins and the stick together (hat bits).
    pad: u8,
    exit_since: Option<Millis>,
    exit: bool,
    trace: bool,
}

impl GpioInput {
    pub fn open(root: &Path) -> Result<Self, String> {
        let regs = unsafe {
            let fd = libc::open(c"/dev/mem".as_ptr(), libc::O_RDWR | libc::O_SYNC);
            if fd < 0 {
                return Err(format!(
                    "Brick GPIO: /dev/mem: {}",
                    std::io::Error::last_os_error()
                ));
            }
            let map = libc::mmap(
                std::ptr::null_mut(),
                MAP_LEN,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                PIO,
            );
            libc::close(fd);
            if map == libc::MAP_FAILED {
                return Err(format!(
                    "Brick GPIO: mmap: {}",
                    std::io::Error::last_os_error()
                ));
            }
            map as *mut u8
        };
        let pro = slot_power::model() == slot_power::Model::BrickPro;
        let sticks = pro.then(super::pro_sticks::ProSticks::open).flatten();
        let input = GpioInput {
            regs,
            power: DeviceInput::open(root),
            pins: if pro { &PRO_PINS } else { &BRICK_PINS },
            held: 0,
            stick: sticks.is_some().then(StickPad::default),
            sticks,
            stick_at: 0,
            pad: 0,
            exit_since: None,
            exit: false,
            trace: super::trace::enabled(),
        };
        // Every button pin an input (function 0), as trimui_inputd sets them.
        for &(pin, _) in input.pins {
            input.set_input(pin);
        }
        eprintln!("slot: {} buttons read from GPIO", if pro { "Brick Pro" } else { "Brick" });
        Ok(input)
    }

    pub fn exit_requested(&self) -> bool {
        self.exit
    }

    fn reg(&self, pin: u32, offset: usize) -> *mut u32 {
        let bank = (pin / 32) as usize;
        unsafe { self.regs.add(bank * BANK_STRIDE + offset) as *mut u32 }
    }

    fn set_input(&self, pin: u32) {
        let reg = self.reg(pin, CFG0 + ((pin % 32) / 8) as usize * 4);
        let shift = (pin % 8) * 4;
        unsafe { reg.write_volatile(reg.read_volatile() & !(0xf << shift)) };
    }

    fn pressed(&self, pin: u32) -> bool {
        let data = unsafe { self.reg(pin, DATA).read_volatile() };
        data & (1 << (pin % 32)) == 0
    }

    /// The Brick Pro's left stick, read now and then: its new position, for the D-pad.
    fn read_stick(&mut self, now: Millis) {
        let (Some(sticks), Some(stick)) = (&mut self.sticks, &mut self.stick) else {
            return;
        };
        if now.saturating_sub(self.stick_at) < STICK_MS {
            return;
        }
        self.stick_at = now;
        if let Some([(x, y), _]) = sticks.read() {
            stick.axis(0, x);
            stick.axis(1, y);
        }
    }
}

/// The presses and releases from `old` to `new`, bit i for BUTTONS[i].
fn changes(old: u32, new: u32) -> impl Iterator<Item = RawEvent> {
    BUTTONS.iter().enumerate().filter_map(move |(i, &btn)| {
        let bit = 1 << i;
        (old & bit != new & bit).then_some(if new & bit != 0 {
            RawEvent::Down(btn)
        } else {
            RawEvent::Up(btn)
        })
    })
}

fn bit_of(btn: Btn) -> u32 {
    BUTTONS.iter().position(|&b| b == btn).map_or(0, |i| 1 << i)
}

/// The buttons held, from which pins read pressed.
fn held_of(pins: &[(u32, Btn)], pressed: impl Fn(u32) -> bool) -> u32 {
    pins.iter()
        .filter(|&&(pin, _)| pressed(pin))
        .fold(0, |held, &(_, btn)| held | bit_of(btn))
}

const DPAD: [(u8, Btn); 4] = [(1, Btn::Up), (2, Btn::Right), (4, Btn::Down), (8, Btn::Left)];

/// The D-pad buttons in `held`, as hat bits (super::brick::hat_events).
fn hat_of(held: u32) -> u8 {
    DPAD.iter().filter(|&&(_, btn)| held & bit_of(btn) != 0).fold(0, |hat, &(bit, _)| hat | bit)
}

fn dpad_bits() -> u32 {
    DPAD.iter().fold(0, |bits, &(_, btn)| bits | bit_of(btn))
}

impl InputSource for GpioInput {
    fn stick(&self) -> Option<(i16, i16)> {
        self.stick.map(|s| s.position())
    }

    fn set_stick_to_game(&mut self, on: bool) {
        if let Some(stick) = &mut self.stick {
            stick.hand_to_game(on);
        }
    }

    fn poll(&mut self, now: Millis) -> Vec<RawEvent> {
        let mut out = self.power.poll(now);
        let held = held_of(self.pins, |pin| self.pressed(pin));
        if self.trace && held != self.held {
            eprintln!("brick gpio: held {held:#07x}");
        }
        self.read_stick(now);
        // The D-pad's pins and the left stick press the same four directions, as the stock
        // path merges SDL's hat with the stick: one press however many hold it.
        out.extend(changes(self.held & !dpad_bits(), held & !dpad_bits()));
        let pad = hat_of(held) | self.stick.map_or(0, |s| s.bits);
        out.extend(super::brick::hat_events(self.pad, pad));
        self.pad = pad;
        self.held = held;
        // Hold Menu + Start + Select for 1 second to save and return to the launcher.
        let chord = bit_of(Btn::Menu) | bit_of(Btn::Start) | bit_of(Btn::Select);
        if held & chord == chord {
            let since = *self.exit_since.get_or_insert(now);
            self.exit = now.saturating_sub(since) >= 1000;
        } else {
            self.exit_since = None;
        }
        out
    }
}

impl Drop for GpioInput {
    fn drop(&mut self) {
        unsafe { libc::munmap(self.regs as *mut libc::c_void, MAP_LEN) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_pin_is_one_button_and_every_button_is_known() {
        for pins in [&BRICK_PINS[..], &PRO_PINS[..]] {
            for (i, (pin, btn)) in pins.iter().enumerate() {
                assert!(pins[i + 1..].iter().all(|(p, _)| p != pin), "pin {pin} twice");
                assert!(BUTTONS.contains(btn), "{btn:?} is not in BUTTONS");
            }
        }
        // The Brick's buttons each on a pin of their own; the Pro's two Menu keys share one.
        for btn in BUTTONS {
            assert_eq!(BRICK_PINS.iter().filter(|(_, b)| *b == btn).count(), 1, "{btn:?} on the Brick");
            assert!(PRO_PINS.iter().any(|(_, b)| *b == btn), "{btn:?} on the Brick Pro");
        }
    }

    #[test]
    fn the_brick_pro_has_its_own_pins_for_select_start_menu_and_the_left_shoulders() {
        let pin = |pins: &[(u32, Btn)], btn: Btn| pins.iter().filter(|(_, b)| *b == btn).map(|(p, _)| *p).collect::<Vec<_>>();
        assert_eq!(pin(&PRO_PINS, Btn::Select), [237]);
        assert_eq!(pin(&PRO_PINS, Btn::Start), [238]);
        assert_eq!(pin(&PRO_PINS, Btn::Menu), [236, 242]);
        assert_eq!((pin(&PRO_PINS, Btn::L1), pin(&PRO_PINS, Btn::L2)), (vec![34], vec![36]));
        assert_eq!(pin(&BRICK_PINS, Btn::Start), [240], "the Brick's are as they were");
    }

    #[test]
    fn either_menu_key_is_menu_and_both_together_one_press() {
        let held = held_of(&PRO_PINS, |p| p == 236 || p == 242);
        assert_eq!(held, bit_of(Btn::Menu));
        assert_eq!(changes(0, held).collect::<Vec<_>>(), vec![RawEvent::Down(Btn::Menu)]);
    }

    #[test]
    fn a_change_of_buttons_is_presses_and_releases() {
        let (a, b) = (bit_of(Btn::A), bit_of(Btn::B));
        assert_eq!(changes(0, a).collect::<Vec<_>>(), vec![RawEvent::Down(Btn::A)]);
        assert_eq!(changes(a, b).collect::<Vec<_>>(), vec![RawEvent::Up(Btn::A), RawEvent::Down(Btn::B)]);
        assert_eq!(changes(b, b).count(), 0);
    }

    #[test]
    fn the_d_pad_pins_are_its_hat_bits() {
        assert_eq!(hat_of(bit_of(Btn::Up) | bit_of(Btn::Left) | bit_of(Btn::A)), 1 | 8);
        assert_eq!(dpad_bits() & bit_of(Btn::A), 0);
    }
}
