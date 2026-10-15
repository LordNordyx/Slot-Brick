//! Scenario files: what to press and what the screen should then show, run top to bottom.
//!
//! One step a line; `#` starts a comment.
//! - `idle [settle_ms] [timeout_ms]`  wait until nothing has moved for settle_ms (400)
//! - `press BTN [ms]`                 press and release, held ms (80)
//! - `hold BTN ms`                    the same, for a long press
//! - `down BTN` / `up BTN`            for chords
//! - `wait ms`                        let time pass
//! - `phase NAME [timeout_ms]`        wait for the frontend to reach a phase, eg Shelf
//! - `snap NAME`                      wait for idle, screenshot to OUT/NAME.png, compare
//!   with GOLDEN/NAME.png (or replace it, with SLOT_SIM_UPDATE=1)
//! - `set KNOB VALUE`                 battery 0-100, charging 0/1, mute 0/1, online 0/1
//! - `led FILE VALUE`                 the LED driver file FILE must now hold VALUE
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use slot_input::RawEvent;

use super::{image, parse_btn, phase_name, Shared, PANEL};

const SETTLE: Duration = Duration::from_millis(400);
const TIMEOUT: Duration = Duration::from_secs(15);

pub fn spawn(path: PathBuf, shared: Arc<Shared>) {
    let started = std::thread::Builder::new()
        .name("slot-sim-script".into())
        .spawn(move || {
            let code = match run(&path, &shared) {
                Ok(()) => 0,
                Err(e) => {
                    eprintln!("sim: {}: {e}", path.display());
                    1
                }
            };
            shared.quit(code);
        });
    if let Err(e) = started {
        eprintln!("slot: sim script: {e}");
        std::process::exit(1);
    }
}

fn dir(var: &str, default: &str) -> PathBuf {
    std::env::var_os(var)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(default))
}

fn run(path: &Path, shared: &Shared) -> Result<(), String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let name = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let out = dir("SLOT_SIM_OUT", "target-sim/out").join(&name);
    let golden = dir("SLOT_SIM_GOLDEN", "brick/sim/golden").join(&name);
    let update = std::env::var_os("SLOT_SIM_UPDATE").is_some_and(|v| v == "1");
    std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    let mut mismatched = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        eprintln!("sim: {name}:{} {line}", n + 1);
        let words: Vec<&str> = line.split_whitespace().collect();
        let step = Step {
            shared,
            out: &out,
            golden: &golden,
            update,
        };
        match step.run(&words) {
            Ok(None) => {}
            Ok(Some(mismatch)) => mismatched.push(format!("line {}: {mismatch}", n + 1)),
            Err(e) => return Err(format!("line {}: {line}: {e}", n + 1)),
        }
    }
    if mismatched.is_empty() {
        eprintln!("sim: {name}: passed");
        Ok(())
    } else {
        Err(mismatched.join("; "))
    }
}

struct Step<'a> {
    shared: &'a Shared,
    out: &'a Path,
    golden: &'a Path,
    update: bool,
}

fn ms(word: Option<&&str>, default: Duration) -> Result<Duration, String> {
    match word {
        None => Ok(default),
        Some(w) => w
            .parse::<u64>()
            .map(Duration::from_millis)
            .map_err(|_| format!("{w:?} is not a number of milliseconds")),
    }
}

impl Step<'_> {
    /// Ok(Some) is a screenshot that differs: noted, and the scenario carries on, so one run
    /// shows every screen that changed.
    fn run(&self, words: &[&str]) -> Result<Option<String>, String> {
        let btn = |i: usize| {
            words
                .get(i)
                .and_then(|w| parse_btn(w))
                .ok_or_else(|| format!("needs a button, not {:?}", words.get(i)))
        };
        match words[0] {
            "idle" => {
                let settle = ms(words.get(1), SETTLE)?;
                if !self.shared.wait_idle(settle, ms(words.get(2), TIMEOUT)?) {
                    return Err("the screen never settled".into());
                }
            }
            "press" => self.shared.press(
                btn(1)?,
                ms(words.get(2), Duration::from_millis(80))?.as_millis() as u64,
            ),
            "hold" => self
                .shared
                .press(btn(1)?, ms(words.get(2), SETTLE)?.as_millis() as u64),
            "down" => self.shared.push(RawEvent::Down(btn(1)?)),
            "up" => self.shared.push(RawEvent::Up(btn(1)?)),
            "wait" => std::thread::sleep(ms(words.get(1), SETTLE)?),
            "phase" => {
                let want = words.get(1).ok_or("needs a phase name")?;
                let deadline = Instant::now() + ms(words.get(2), TIMEOUT)?;
                loop {
                    let status = self.shared.status();
                    if phase_name(&status.phase) == *want {
                        break;
                    }
                    if Instant::now() >= deadline {
                        return Err(format!("still {} after waiting for {want}", status.phase));
                    }
                    self.shared.wait_frames(1, Duration::from_millis(200));
                }
            }
            "set" => {
                let (knob, value) = (
                    words.get(1).ok_or("needs a knob")?,
                    words.get(2).ok_or("needs a value")?,
                );
                super::fixture::set(knob, value)?;
                // The frontend looks at the switches a few times a second.
                std::thread::sleep(Duration::from_millis(300));
            }
            "led" => {
                let (file, want) = (
                    words.get(1).ok_or("needs an LED file")?,
                    words.get(2).ok_or("needs a value")?,
                );
                if file.contains(['/', '.']) {
                    return Err("an LED file is a bare name, eg effect_lr".into());
                }
                let got = std::fs::read_to_string(slot_power::hw("/sys/class/led_anim").join(file))
                    .map_err(|e| format!("{file}: {e}"))?;
                if got.trim() != *want {
                    return Err(format!("{file} holds {:?}, not {want:?}", got.trim()));
                }
            }
            "snap" => return self.snap(words.get(1).ok_or("needs a name")?),
            other => return Err(format!("no step {other:?}")),
        }
        Ok(None)
    }

    fn snap(&self, name: &str) -> Result<Option<String>, String> {
        if name.contains(['/', '\\', '.']) {
            return Err("a screenshot name is a bare name, eg shelf".into());
        }
        if !self.shared.wait_idle(SETTLE, TIMEOUT) {
            return Err("the screen never settled".into());
        }
        let rgba = self
            .shared
            .screenshot(Duration::from_secs(5))
            .ok_or("no screenshot came back")?;
        let png = image::encode(PANEL.0, PANEL.1, &rgba);
        let shot = self.out.join(format!("{name}.png"));
        std::fs::write(&shot, &png).map_err(|e| format!("{}: {e}", shot.display()))?;
        let golden = self.golden.join(format!("{name}.png"));
        if self.update {
            std::fs::create_dir_all(self.golden).map_err(|e| e.to_string())?;
            std::fs::write(&golden, &png).map_err(|e| format!("{}: {e}", golden.display()))?;
            eprintln!("sim: wrote golden {}", golden.display());
            return Ok(None);
        }
        if !golden.exists() {
            return Ok(Some(format!(
                "{name}: no golden at {}; if {} looks right, run task sim:update",
                golden.display(),
                shot.display()
            )));
        }
        let (w, h, want) = image::decode(&golden)?;
        if (w, h) != PANEL {
            return Ok(Some(format!(
                "{name}: golden is {w}x{h}, the screen is {}x{}",
                PANEL.0, PANEL.1
            )));
        }
        let diff = image::compare(&rgba, &want);
        if diff.pixels == 0 {
            return Ok(None);
        }
        let marked = self.out.join(format!("{name}.diff.png"));
        let _ = std::fs::write(&marked, image::encode(PANEL.0, PANEL.1, &diff.marked));
        Ok(Some(format!(
            "{name}: {} of {} pixels differ from the golden; see {}",
            diff.pixels,
            diff.total,
            marked.display()
        )))
    }
}
