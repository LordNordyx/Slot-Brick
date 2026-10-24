//! The uptime at each step of startup, in the log the startup scripts keep (`SLOT_BOOT_TIMES`,
//! Slot/Logs/boot-times.log): where the time from power on to the shelf goes (issue #44).
use std::io::Write;

pub fn mark(step: &str) {
    let Some(path) = std::env::var_os("SLOT_BOOT_TIMES") else {
        return;
    };
    let Some(up) = std::fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|raw| slot_power::uptime_seconds(&raw))
    else {
        return;
    };
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{up} slot: {step}");
    }
}
