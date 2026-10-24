//! The Brick's buttons read straight from the A133's GPIO pins, for Slot OS, which runs no
//! trimui_inputd and has no SDL. The pins and registers are those Mikhailzrick's open
//! trimui_inputd reads (github.com/Mikhailzrick/Trimui_Inputd, src/devices/brick): every
//! button is one pin, pressed when low. The power key is not among them; it is an input
//! device of its own, read by DeviceInput as on stock.
use super::DeviceInput;
use slot_input::{Btn, InputSource, Millis, RawEvent};
use std::path::Path;

/// The pin controller's registers, physical; banks of 0x24 bytes, A at the start.
const PIO: libc::off_t = 0x0300_B000;
const BANK_STRIDE: usize = 0x24;
const CFG0: usize = 0x00;
const DATA: usize = 0x10;
const MAP_LEN: usize = 0x1000;

/// A pin is bank * 32 + its number in the bank: 109 is PD13.
const PINS: [(u32, Btn); 17] = [
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

pub struct GpioInput {
    regs: *mut u8,
    power: DeviceInput,
    /// Which of PINS are held, bit i for PINS[i].
    held: u32,
    exit_since: Option<Millis>,
    exit: bool,
    trace: bool,
}

impl GpioInput {
    pub fn open(root: &Path) -> Result<Self, String> {
        let regs = unsafe {
            let fd = libc::open(c"/dev/mem".as_ptr(), libc::O_RDWR | libc::O_SYNC);
            if fd < 0 {
                return Err(format!("Brick GPIO: /dev/mem: {}", std::io::Error::last_os_error()));
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
                return Err(format!("Brick GPIO: mmap: {}", std::io::Error::last_os_error()));
            }
            map as *mut u8
        };
        let input = GpioInput {
            regs,
            power: DeviceInput::open(root),
            held: 0,
            exit_since: None,
            exit: false,
            trace: super::trace::enabled(),
        };
        // Every button pin an input (function 0), as trimui_inputd sets them.
        for (pin, _) in PINS {
            input.set_input(pin);
        }
        eprintln!("slot: Brick buttons read from GPIO");
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
}

/// The presses and releases from `old` to `new`, bit i for PINS[i].
fn changes(old: u32, new: u32) -> impl Iterator<Item = RawEvent> {
    PINS.iter().enumerate().filter_map(move |(i, &(_, btn))| {
        let bit = 1 << i;
        (old & bit != new & bit).then_some(if new & bit != 0 { RawEvent::Down(btn) } else { RawEvent::Up(btn) })
    })
}

fn bit_of(btn: Btn) -> u32 {
    PINS.iter().position(|&(_, b)| b == btn).map_or(0, |i| 1 << i)
}

impl InputSource for GpioInput {
    fn poll(&mut self, now: Millis) -> Vec<RawEvent> {
        let mut out = self.power.poll(now);
        let held = PINS
            .iter()
            .enumerate()
            .filter(|&(_, &(pin, _))| self.pressed(pin))
            .fold(0, |held, (i, _)| held | 1 << i);
        if self.trace && held != self.held {
            eprintln!("brick gpio: held {held:#07x}");
        }
        out.extend(changes(self.held, held));
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
    fn each_button_has_a_pin_of_its_own() {
        for (i, (pin, btn)) in PINS.iter().enumerate() {
            assert!(PINS[i + 1..].iter().all(|(p, b)| p != pin && b != btn), "{pin} {btn:?} twice");
        }
    }

    #[test]
    fn a_change_of_pins_is_presses_and_releases() {
        let (a, b) = (bit_of(Btn::A), bit_of(Btn::B));
        assert_eq!(changes(0, a).collect::<Vec<_>>(), vec![RawEvent::Down(Btn::A)]);
        assert_eq!(changes(a, b).collect::<Vec<_>>(), vec![RawEvent::Up(Btn::A), RawEvent::Down(Btn::B)]);
        assert_eq!(changes(b, b).count(), 0);
    }
}
