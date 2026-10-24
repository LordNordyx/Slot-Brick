//! Application-mode platform: no firmware updates, global LED or power services.
use crate::{Battery, Charge, LedState, Platform};
use std::fs::{self, OpenOptions};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct BrickPlatform {
    root: PathBuf,
    /// Whether the rumble motor is on (None until first set; then whether it could be set).
    rumble: Option<bool>,
}
/// The rumble motor hangs on a plain GPIO: on or off, no strength (NextUI's tg5040 does the
/// same). On stock trimui_inputd exports it; under Slot OS nothing has, so Slot does.
const RUMBLE_GPIO: &str = "/sys/class/gpio/gpio227";
extern "C" {
    fn ioctl(fd: i32, request: u64, ...) -> i32;
    fn clock_settime(clock: i32, time: *const ClockTime) -> i32;
}
#[repr(C)]
struct ClockTime {
    secs: std::os::raw::c_long,
    nanos: std::os::raw::c_long,
}
impl BrickPlatform {
    pub fn new(root: PathBuf) -> Self {
        Self { root, rumble: None }
    }
    fn motor(&mut self, on: bool) {
        if self.rumble == Some(on) {
            return;
        }
        let gpio = crate::hw(RUMBLE_GPIO);
        match set_gpio(&gpio, &crate::hw("/sys/class/gpio/export"), on) {
            Ok(()) => self.rumble = Some(on),
            Err(e) => {
                if self.rumble.is_none() {
                    eprintln!("slot: Brick rumble: {}: {e}", gpio.display());
                }
                self.rumble = Some(!on); // Logged once; tried again on the next change.
            }
        }
    }
    fn read(&self, name: &str) -> Option<u32> {
        fs::read_to_string(crate::hw(&format!("/sys/class/power_supply/{name}")))
            .ok()?
            .trim()
            .parse()
            .ok()
    }
    fn finish(&mut self, status: i32) -> ! {
        // App::poweroff/restart has settled saves before this is called.
        if self.rumble == Some(true) {
            let _ = fs::write(crate::hw(RUMBLE_GPIO).join("value"), "0");
        }
        let _ = std::process::Command::new("sync").status();
        std::process::exit(status)
    }
}
/// Sets a GPIO output, exporting it first if no one has yet.
fn set_gpio(gpio: &Path, export: &Path, on: bool) -> std::io::Result<()> {
    if !gpio.exists() {
        let number = gpio
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_prefix("gpio"));
        let _ = fs::write(export, number.unwrap_or_default());
        let _ = fs::write(gpio.join("direction"), "out");
    }
    fs::write(gpio.join("value"), if on { "1" } else { "0" })
}
impl Drop for BrickPlatform {
    fn drop(&mut self) {
        if self.rumble == Some(true) {
            self.motor(false);
        }
    }
}
impl Platform for BrickPlatform {
    fn root(&self) -> &Path {
        &self.root
    }
    fn now(&self) -> i64 {
        // The simulator can stop the clock, so screenshots show the same time every run.
        if cfg!(feature = "sim") {
            if let Some(secs) = std::env::var("SLOT_SIM_TIME")
                .ok()
                .and_then(|v| v.parse().ok())
            {
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
    fn set_rumble(&mut self, strength: u16) {
        self.motor(strength > 0);
    }
    fn set_clock(&mut self, secs: i64) {
        let secs = match std::os::raw::c_long::try_from(secs) {
            Ok(value) => value,
            Err(_) => {
                eprintln!("slot: Brick clock: timestamp out of range");
                return;
            }
        };
        // Never move the clock of the machine the simulator runs on.
        if cfg!(feature = "sim") {
            eprintln!("slot: sim: clock would be set to {secs}");
            return;
        }
        let time = ClockTime { secs, nanos: 0 };
        if unsafe { clock_settime(0, &time) } != 0 {
            eprintln!("slot: Brick clock: {}", std::io::Error::last_os_error());
            return;
        }
        match std::process::Command::new("hwclock")
            .args(["-w", "-u"])
            .status()
        {
            Ok(s) if s.success() => {}
            other => eprintln!("slot: Brick RTC write: {other:?}"),
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn rumble_drives_the_motor_gpio_exporting_it_when_needed() {
        let dir = tempfile::tempdir().unwrap();
        let export = dir.path().join("export");
        let gpio = dir.path().join("gpio227");
        // Not exported: the number goes to export (the kernel then makes the folder).
        assert!(super::set_gpio(&gpio, &export, true).is_err());
        assert_eq!(std::fs::read_to_string(&export).unwrap(), "227");
        std::fs::create_dir(&gpio).unwrap();
        super::set_gpio(&gpio, &export, true).unwrap();
        assert_eq!(std::fs::read_to_string(gpio.join("value")).unwrap(), "1");
        super::set_gpio(&gpio, &export, false).unwrap();
        assert_eq!(std::fs::read_to_string(gpio.join("value")).unwrap(), "0");
    }
}
