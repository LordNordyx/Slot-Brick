//! The Brick Pro's sticks on Slot OS, where TrimUI's trimui_inputd does not run. As that daemon
//! reads them (the Brick Pro's firmware 1.1.1, tg4040): each stick is an analog-to-digital
//! converter on I2C bus 3, the left at 0x29 and the right at 0x28. Register 0xB0 gives four
//! bytes, X then Y, each a 12-bit reading, big-endian. TrimUI centres each axis on a calibration
//! file on its internal storage, which Slot OS does not mount; Slot starts from that daemon's own
//! defaults and widens them as the stick shows it reaches further.
use std::os::fd::{AsRawFd, OwnedFd};

const BUS: &std::ffi::CStr = c"/dev/i2c-3";
const LEFT: u16 = 0x29;
const RIGHT: u16 = 0x28;
const REGISTER: u8 = 0xb0;
const I2C_RDWR: libc::c_ulong = 0x0707;
const I2C_TIMEOUT: libc::c_ulong = 0x0702;
const I2C_RETRIES: libc::c_ulong = 0x0701;
const I2C_M_RD: u16 = 0x0001;

/// The reading at rest, and the least and most the daemon expects, when nothing says otherwise.
const CENTRE: i32 = 2048;
const MIN: i32 = 1450;
const MAX: i32 = 2750;
/// A first reading this far off centre is a stick held at startup, not its centre.
const CENTRE_SLACK: i32 = 300;
/// Inside this much of full tilt (as the daemon's 0.1), a stick reads as centred.
const DEAD_ZONE: i32 = 3276;
/// Read errors in a row before giving up on the bus, so a missing one costs nothing after.
const GIVE_UP: u32 = 50;

#[repr(C)]
struct I2cMsg {
    addr: u16,
    flags: u16,
    len: u16,
    buf: *mut u8,
}

#[repr(C)]
struct I2cRdwr {
    msgs: *mut I2cMsg,
    nmsgs: u32,
}

/// One axis's reading turned into the stick position Slot uses, -32767..32767.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Axis {
    centre: Option<i32>,
    min: i32,
    max: i32,
}

impl Default for Axis {
    fn default() -> Self {
        Axis { centre: None, min: MIN, max: MAX }
    }
}

impl Axis {
    /// `raw`, 0..4095, as a position. The first reading near the middle is taken for the centre;
    /// a reading past the range so far widens it. Inverted, as TrimUI's daemon reports them: a
    /// higher reading is left (or up).
    pub fn position(&mut self, raw: u16) -> i32 {
        let raw = i32::from(raw.min(4095));
        let centre = *self
            .centre
            .get_or_insert(if (raw - CENTRE).abs() <= CENTRE_SLACK { raw } else { CENTRE });
        self.min = self.min.min(raw).min(centre - 1);
        self.max = self.max.max(raw).max(centre + 1);
        let along = match raw >= centre {
            true => (raw - centre) * 32767 / (self.max - centre),
            false => (raw - centre) * 32767 / (centre - self.min),
        };
        -along.clamp(-32767, 32767)
    }
}

/// A stick's position with the dead zone round its centre taken out.
pub fn dead_zoned(x: i32, y: i32) -> (i16, i16) {
    let (x2, y2, r2) = (i64::from(x).pow(2), i64::from(y).pow(2), i64::from(DEAD_ZONE).pow(2));
    match x2 + y2 < r2 {
        true => (0, 0),
        false => (x as i16, y as i16),
    }
}

pub struct ProSticks {
    bus: OwnedFd,
    left: (Axis, Axis),
    right: (Axis, Axis),
    errors: u32,
}

impl ProSticks {
    /// The bus, if there is one: none on a Slot OS image built for the Brick.
    pub fn open() -> Option<ProSticks> {
        let fd = unsafe { libc::open(BUS.as_ptr(), libc::O_RDWR) };
        if fd < 0 {
            eprintln!("slot: Brick Pro sticks: {}: {}", BUS.to_string_lossy(), std::io::Error::last_os_error());
            return None;
        }
        // SAFETY: `fd` was just opened and is owned by nothing else.
        let bus = unsafe { <OwnedFd as std::os::fd::FromRawFd>::from_raw_fd(fd) };
        // As TrimUI's daemon: a 50 ms timeout, one retry.
        unsafe {
            libc::ioctl(bus.as_raw_fd(), I2C_TIMEOUT, 5 as libc::c_ulong);
            libc::ioctl(bus.as_raw_fd(), I2C_RETRIES, 1 as libc::c_ulong);
        }
        eprintln!("slot: Brick Pro sticks read from {}", BUS.to_string_lossy());
        Some(ProSticks { bus, left: Default::default(), right: Default::default(), errors: 0 })
    }

    /// Both sticks, -32767..32767 across then down: None when they could not be read (and,
    /// after GIVE_UP failures in a row, no longer asked).
    pub fn read(&mut self) -> Option<[(i16, i16); 2]> {
        if self.errors >= GIVE_UP {
            return None;
        }
        match (self.raw(LEFT), self.raw(RIGHT)) {
            (Ok(l), Ok(r)) => {
                self.errors = 0;
                let left = dead_zoned(self.left.0.position(l.0), self.left.1.position(l.1));
                let right = dead_zoned(self.right.0.position(r.0), self.right.1.position(r.1));
                Some([left, right])
            }
            (Err(e), _) | (_, Err(e)) => {
                self.errors += 1;
                if self.errors == 1 || self.errors == GIVE_UP {
                    eprintln!("slot: Brick Pro sticks: {e}{}", if self.errors == GIVE_UP { "; giving up" } else { "" });
                }
                None
            }
        }
    }

    /// One converter's X and Y, 0..4095.
    fn raw(&self, addr: u16) -> std::io::Result<(u16, u16)> {
        let mut register = [REGISTER];
        let mut data = [0u8; 4];
        let mut msgs = [
            I2cMsg { addr, flags: 0, len: 1, buf: register.as_mut_ptr() },
            I2cMsg { addr, flags: I2C_M_RD, len: data.len() as u16, buf: data.as_mut_ptr() },
        ];
        let mut rdwr = I2cRdwr { msgs: msgs.as_mut_ptr(), nmsgs: msgs.len() as u32 };
        // SAFETY: the messages and their buffers outlive the call, and their lengths are theirs.
        if unsafe { libc::ioctl(self.bus.as_raw_fd(), I2C_RDWR, &mut rdwr) } < 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok((u16::from_be_bytes([data[0], data[1]]), u16::from_be_bytes([data[2], data[3]])))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_axis_is_centred_on_its_first_reading_and_inverted_as_trimui_reports_it() {
        let mut a = Axis::default();
        assert_eq!(a.position(2000), 0, "the first reading is the centre");
        assert_eq!(a.position(2750), -32767, "TrimUI's top of the range is full tilt one way");
        assert_eq!(a.position(1450), 32767, "and its bottom the other");
        assert_eq!(a.position(2000 + 375), -16383, "half way");
    }

    #[test]
    fn a_stick_held_at_startup_is_not_taken_for_its_centre() {
        let mut a = Axis::default();
        assert_eq!(a.position(2700), -(2700 - CENTRE) * 32767 / (MAX - CENTRE));
        assert_eq!(a.position(2048), 0);
    }

    #[test]
    fn a_stick_that_reaches_further_widens_its_range() {
        let mut a = Axis::default();
        a.position(2048);
        assert_eq!(a.position(3000), -32767);
        assert_eq!(a.position(2750), -(2750 - 2048) * 32767 / (3000 - 2048), "full tilt is now further");
        assert_eq!(a.position(4095), -32767, "and never more than full tilt");
    }

    #[test]
    fn a_stick_near_its_centre_reads_centred() {
        assert_eq!(dead_zoned(2000, -2000), (0, 0));
        assert_eq!(dead_zoned(4000, 0), (4000, 0));
        assert_eq!(dead_zoned(-32767, 32767), (-32767, 32767));
    }
}
