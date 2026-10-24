//! Application-mode platform: no firmware updates, global LED or power services.
use crate::{Battery, Charge, LedState, Platform};
use std::fs::{self, OpenOptions};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct BrickPlatform {
    root: PathBuf,
}
extern "C" {
    fn ioctl(fd: i32, request: u64, ...) -> i32;
    fn clock_settime(clock: i32, time: *const ClockTime) -> i32;
}
#[repr(C)]
struct ClockTime { secs: std::os::raw::c_long, nanos: std::os::raw::c_long }
impl BrickPlatform {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
    fn read(&self, name: &str) -> Option<u32> {
        fs::read_to_string(crate::hw(&format!("/sys/class/power_supply/{name}")))
            .ok()?
            .trim()
            .parse()
            .ok()
    }
    fn finish(&self, status: i32) -> ! {
        // App::poweroff/restart has settled saves before this is called.
        let _ = std::process::Command::new("sync").status();
        std::process::exit(status)
    }
}
impl Platform for BrickPlatform {
    fn root(&self) -> &Path {
        &self.root
    }
    fn now(&self) -> i64 {
        // The simulator can stop the clock, so screenshots show the same time every run.
        if cfg!(feature = "sim") {
            if let Some(secs) = std::env::var("SLOT_SIM_TIME").ok().and_then(|v| v.parse().ok()) {
                return secs;
            }
        }
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }
    fn set_backlight(&mut self, step: u8) {
        // DISP_LCD_SET_BRIGHTNESS, from TrimUI/NextUI's tg5040 implementation.
        let level = if step == 0 {
            0
        } else {
            10 + u64::from(step.min(9)) * 245 / 9
        };
        // The simulator has no display driver; it shows the level it would have set instead.
        if cfg!(feature = "sim") {
            let _ = fs::write(crate::hw("/sim/backlight"), format!("{level}\n"));
            return;
        }
        match OpenOptions::new().read(true).write(true).open("/dev/disp") {
            Ok(dev) => {
                let args = [0u64, level, 0, 0];
                if unsafe { ioctl(dev.as_raw_fd(), 0x102, args.as_ptr()) } < 0 {
                    eprintln!(
                        "slot: Brick brightness: {}",
                        std::io::Error::last_os_error()
                    );
                }
            }
            Err(e) => eprintln!("slot: Brick brightness: /dev/disp: {e}"),
        }
    }
    fn battery(&self) -> Option<Battery> {
        Some(Battery {
            percent: self.read("axp2202-battery/capacity")?.min(100) as u8,
            charge: self.charge(),
        })
    }
    fn charge(&self) -> Charge {
        match self.read("axp2202-usb/online") {
            Some(1) if self.read("axp2202-battery/capacity") == Some(100) => Charge::Full,
            Some(1) => Charge::Charging,
            Some(0) => Charge::Discharging,
            _ => Charge::Unknown,
        }
    }
    fn poweroff(&mut self) -> ! {
        self.finish(64)
    }
    fn restart(&mut self) -> ! {
        // On Brick this menu choice means save and exit to the previous launcher.
        self.finish(0)
    }
    fn card_read_only(&self) -> bool {
        crate::read_only(&self.root)
    }
    fn set_led(&mut self, _: LedState) {} // Leave launcher/firmware LED policy alone.
    fn set_rumble(&mut self, _: u16) {} // Deferred until hardware validation.
    fn set_clock(&mut self, secs: i64) {
        let secs = match std::os::raw::c_long::try_from(secs) {
            Ok(value) => value,
            Err(_) => {eprintln!("slot: Brick clock: timestamp out of range"); return;}
        };
        // Never move the clock of the machine the simulator runs on.
        if cfg!(feature = "sim") {
            eprintln!("slot: sim: clock would be set to {secs}");
            return;
        }
        let time = ClockTime { secs, nanos: 0 };
        if unsafe { clock_settime(0, &time) } != 0 {
            eprintln!("slot: Brick clock: {}",std::io::Error::last_os_error()); return;
        }
        match std::process::Command::new("hwclock").args(["-w","-u"]).status() {
            Ok(s) if s.success() => {},
            other => eprintln!("slot: Brick RTC write: {other:?}"),
        }
    }
}
