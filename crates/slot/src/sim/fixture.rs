//! The stand-in hardware: the sysfs files the Brick code reads and writes, as plain files under
//! SLOT_HW_ROOT, holding what a Brick fresh out of the box would show.
use std::fs;
use std::io;
use std::path::Path;

use slot_power::hw;

const LED: &str = "/sys/class/led_anim";
const BATTERY: &str = "/sys/class/power_supply/axp2202-battery";
const USB: &str = "/sys/class/power_supply/axp2202-usb";
const CPUFREQ: &str = "/sys/devices/system/cpu/cpu0/cpufreq";
const MUTE: &str = "/sys/class/gpio/gpio243/value";
const ROUTE: &str = "/proc/net/route";
const BACKLIGHT: &str = "/sim/backlight";
/// The firmware's menu program, which names the model (see `slot_power::model`).
const MAIN_UI: &str = "/usr/trimui/bin/MainUI";

/// SLOT_SIM_MODEL=pro: a Brick Pro.
fn pro() -> bool {
    std::env::var("SLOT_SIM_MODEL").is_ok_and(|m| m == "pro")
}

const ROUTE_HEADER: &str =
    "Iface\tDestination\tGateway \tFlags\tRefCnt\tUse\tMetric\tMask\t\tMTU\tWindow\tIRTT\n";

/// The LED driver's files: the Brick Pro's has a rear zone (its triggers) too.
fn led_files() -> Vec<(String, &'static str)> {
    let mut files = vec![
        ("max_scale".to_string(), "60"),
        ("max_scale_f1f2".to_string(), "60"),
        ("max_scale_lr".to_string(), "60"),
        ("effect_enable".to_string(), "0"),
    ];
    let zones: &[&str] = if pro() { &["m", "f1", "f2", "lr", "rear"] } else { &["m", "f1", "f2", "lr"] };
    if pro() {
        files.push(("max_scale_rear".to_string(), "60"));
    }
    for &zone in zones {
        files.push((format!("effect_rgb_hex_{zone}"), "FFFFFF"));
        files.push((format!("effect_cycles_{zone}"), "0"));
        files.push((format!("effect_duration_{zone}"), "1000"));
        files.push((format!("effect_{zone}"), "0"));
    }
    files
}

/// Fills in whatever stand-in file is missing; one already there keeps what it holds.
pub fn populate() -> io::Result<()> {
    let mut files: Vec<(std::path::PathBuf, String)> = Vec::new();
    for (name, value) in led_files() {
        files.push((hw(LED).join(name), format!("{value}\n")));
    }
    for (name, value) in [
        ("capacity", "80"),
        ("voltage_now", "3900000"),
        ("charge_counter", "2000000"),
    ] {
        files.push((hw(BATTERY).join(name), format!("{value}\n")));
    }
    files.push((hw(USB).join("online"), "0\n".into()));
    for (name, value) in [
        ("scaling_min_freq", "408000"),
        ("scaling_max_freq", "1800000"),
        ("scaling_cur_freq", "1008000"),
        ("scaling_governor", "ondemand"),
        (
            "scaling_available_governors",
            "ondemand userspace performance",
        ),
        (
            "scaling_available_frequencies",
            "408000 600000 816000 1008000 1200000 1416000 1608000 1800000",
        ),
    ] {
        files.push((hw(CPUFREQ).join(name), format!("{value}\n")));
    }
    files.push((hw(MUTE), "0\n".into()));
    files.push((hw(ROUTE), ROUTE_HEADER.into()));
    files.push((hw(BACKLIGHT), "\n".into()));
    // A stand-in, holding only the name MainUI carries for its model: written every start, so
    // a kept card follows MODEL.
    let main_ui = hw(MAIN_UI);
    let model = if pro() { "Trimui Brick Pro" } else { "Trimui Brick" };
    if let Some(dir) = main_ui.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(&main_ui, format!("\0{model}\0"))?;
    for (path, value) in files {
        if path.exists() {
            continue;
        }
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(&path, value)?;
    }
    Ok(())
}

/// Changes a stand-in, as unplugging the charger or flicking the mute switch would.
/// Only these named knobs: drivers never name a path.
pub fn set(knob: &str, value: &str) -> Result<(), String> {
    let number = |max: u32| -> Result<u32, String> {
        value
            .parse::<u32>()
            .ok()
            .filter(|v| *v <= max)
            .ok_or_else(|| format!("{knob} wants 0..={max}, not {value:?}"))
    };
    let (path, text) = match knob {
        "battery" => (hw(BATTERY).join("capacity"), number(100)?.to_string()),
        "charging" => (hw(USB).join("online"), number(1)?.to_string()),
        "mute" => (hw(MUTE), number(1)?.to_string()),
        "online" => {
            (
                hw(ROUTE),
                if number(1)? == 1 {
                    format!("{ROUTE_HEADER}wlan0\t00000000\t0101A8C0\t0003\t0\t0\t0\t00000000\t0\t0\t0\n")
                } else {
                    ROUTE_HEADER.to_string()
                },
            )
        }
        _ => {
            return Err(format!(
                "no knob {knob:?}; there are battery, charging, mute, online"
            ))
        }
    };
    fs::write(&path, format!("{}\n", text.trim_end())).map_err(|e| format!("{knob}: {e}"))
}

/// What the stand-ins hold now, as JSON object members: the LEDs as the driver was left,
/// the backlight level, and the knobs.
pub fn report() -> String {
    let read = |p: &Path| {
        fs::read_to_string(p)
            .map(|s| s.trim().to_string())
            .unwrap_or_default()
    };
    let leds: Vec<String> = led_files()
        .into_iter()
        .map(|(name, _)| {
            format!(
                "\"{name}\":\"{}\"",
                json_escape(&read(&hw(LED).join(&name)))
            )
        })
        .collect();
    format!(
        "\"leds\":{{{}}},\"backlight\":\"{}\",\"battery\":\"{}\",\"charging\":\"{}\",\"mute\":\"{}\",\"online\":{}",
        leds.join(","),
        json_escape(&read(&hw(BACKLIGHT))),
        json_escape(&read(&hw(BATTERY).join("capacity"))),
        json_escape(&read(&hw(USB).join("online"))),
        json_escape(&read(&hw(MUTE))),
        read(&hw(ROUTE)).contains("wlan0"),
    )
}

pub fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn escapes_json() {
        assert_eq!(super::json_escape("a\"b\\c\n"), "a\\\"b\\\\c\\u000a");
    }
}
