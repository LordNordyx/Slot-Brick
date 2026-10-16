//! Where the Brick's hardware files live. On the device that is where the kernel puts them;
//! in the simulator (feature `sim`) the same paths sit under SLOT_HW_ROOT, a plain folder the
//! simulator fills with stand-ins, so the Brick code runs unchanged and never touches the
//! real /sys of the machine it runs on.
use std::path::PathBuf;

#[cfg(not(feature = "sim"))]
pub fn hw(path: &str) -> PathBuf {
    PathBuf::from(path)
}

#[cfg(feature = "sim")]
pub fn hw(path: &str) -> PathBuf {
    sim_root().join(path.trim_start_matches('/'))
}

/// The simulator's stand-in for `/`. Unset means a path that never exists, so a sim build
/// started by hand still cannot reach real hardware.
#[cfg(feature = "sim")]
pub fn sim_root() -> PathBuf {
    std::env::var_os("SLOT_HW_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/nonexistent/slot-hw"))
}

#[cfg(all(test, feature = "sim"))]
mod tests {
    #[test]
    fn sim_paths_never_leave_the_stand_in_root() {
        std::env::set_var("SLOT_HW_ROOT", "/tmp/hw");
        assert_eq!(
            super::hw("/sys/class/led_anim"),
            std::path::Path::new("/tmp/hw/sys/class/led_anim")
        );
    }
}
