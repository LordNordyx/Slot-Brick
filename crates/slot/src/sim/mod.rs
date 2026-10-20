//! The Brick simulator: the real Brick build of slot, drawn offscreen and driven from outside.
//!
//! The device loop runs as on the Brick; only the screen (an offscreen pbuffer), the buttons
//! (a queue) and the hardware files (a stand-in folder, see `slot_power::hw`) differ. Two
//! drivers share one control surface:
//! - `server`: a small HTTP API and a page showing the screen live, for people and browsers
//!   (Playwright) to look and press buttons;
//! - `script`: scenario files run start to finish, comparing screenshots against goldens,
//!   for the test pipeline.
//!
//! Configured from the environment:
//! - SLOT_SIM_HTTP: address to serve on, eg 127.0.0.1:8765
//! - SLOT_SIM_SCRIPT: a scenario file to run, then exit with its result
//! - SLOT_SIM_OUT: where scenario screenshots go (default target-sim/out)
//! - SLOT_SIM_GOLDEN: where the expected screenshots live (default brick/sim/golden)
//! - SLOT_SIM_UPDATE=1: write the screenshots as the new goldens instead of comparing
//! - SLOT_SIM_MODEL=pro: be a Brick Pro (its MainUI, its LED zones, its left stick)
use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant};

use slot_input::{Btn, RawEvent};

mod fixture;
mod image;
mod script;
mod server;

pub use fixture::populate;

/// The Brick panel the simulator draws.
pub const PANEL: (u32, u32) = (1024, 768);

/// What the device loop reports each pass, for drivers to wait on.
#[derive(Clone, Default)]
pub struct Status {
    pub frames: u64,
    pub phase: String,
    /// Since when the loop has had nothing new to draw; None while it is drawing.
    pub idle_since: Option<Instant>,
    pub sleeping: bool,
}

#[derive(Default)]
struct State {
    events: VecDeque<RawEvent>,
    /// The Brick Pro's left stick: SDL axis (0 across, 1 up and down) and where it is.
    axes: VecDeque<(u8, i16)>,
    status: Status,
    /// Screenshots asked for and taken: the loop takes one whenever `wanted > taken`.
    wanted: u64,
    taken: u64,
    frame: Arc<Vec<u8>>,
    /// Drivers waiting for the screen to hold still; while there are any, each frame drawn is
    /// read back and compared with the one before.
    watching: u32,
    last_hash: Option<u64>,
    quit: Option<i32>,
}

pub struct Shared {
    state: Mutex<State>,
    changed: Condvar,
}

impl Shared {
    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    // Driver side.

    pub fn push(&self, event: RawEvent) {
        self.lock().events.push_back(event);
    }

    /// The left stick to (x, y), -32767..32767 each, as the Brick Pro's SDL pad reports it.
    pub fn stick(&self, x: i16, y: i16) {
        let mut state = self.lock();
        state.axes.push_back((0, x));
        state.axes.push_back((1, y));
    }

    /// The left stick pushed to (x, y) for `ms`, then let go.
    pub fn flick(&self, x: i16, y: i16, ms: u64) {
        self.stick(x, y);
        std::thread::sleep(Duration::from_millis(ms));
        self.stick(0, 0);
        self.wait_frames(2, Duration::from_secs(2));
    }

    /// Down, held for `ms`, then up: long enough that the frontend sees one press.
    pub fn press(&self, btn: Btn, ms: u64) {
        self.push(RawEvent::Down(btn));
        std::thread::sleep(Duration::from_millis(ms));
        self.push(RawEvent::Up(btn));
        // Let the loop take in the release before the next step.
        self.wait_frames(2, Duration::from_secs(2));
    }

    pub fn status(&self) -> Status {
        self.lock().status.clone()
    }

    /// Waits for the loop to finish `n` more passes.
    pub fn wait_frames(&self, n: u64, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let mut state = self.lock();
        let target = state.status.frames + n;
        while state.status.frames < target {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() || state.quit.is_some() {
                return false;
            }
            state = self
                .changed
                .wait_timeout(state, left)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
        true
    }

    /// Waits until the screen has held still for `settle`: nothing animating, nothing new to
    /// draw. What a screenshot wants, so it shows the same picture every run.
    pub fn wait_idle(&self, settle: Duration, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let mut state = self.lock();
        state.watching += 1;
        loop {
            if let Some(since) = state.status.idle_since {
                if since.elapsed() >= settle {
                    state.watching -= 1;
                    return true;
                }
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() || state.quit.is_some() {
                state.watching -= 1;
                return false;
            }
            state = self
                .changed
                .wait_timeout(state, left.min(Duration::from_millis(50)))
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
    }

    /// The screen as RGBA, taken by the loop after this call.
    pub fn screenshot(&self, timeout: Duration) -> Option<Arc<Vec<u8>>> {
        let deadline = Instant::now() + timeout;
        let mut state = self.lock();
        state.wanted += 1;
        let ticket = state.wanted;
        while state.taken < ticket {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() || state.quit.is_some() {
                return None;
            }
            state = self
                .changed
                .wait_timeout(state, left)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
        Some(state.frame.clone())
    }

    pub fn quit(&self, code: i32) {
        self.lock().quit.get_or_insert(code);
        self.changed.notify_all();
    }

    // Loop side.

    pub fn take_events(&self) -> Vec<RawEvent> {
        self.lock().events.drain(..).collect()
    }

    pub fn take_axes(&self) -> Vec<(u8, i16)> {
        self.lock().axes.drain(..).collect()
    }

    /// Called by the device loop once a pass. `rendered`: it drew this pass. `grab` reads the
    /// screen, and is only called when a driver is waiting on it.
    pub fn tick(
        &self,
        phase: String,
        rendered: bool,
        sleeping: bool,
        grab: impl FnOnce() -> Vec<u8>,
    ) {
        let mut state = self.lock();
        state.status.frames += 1;
        state.status.phase = phase;
        state.status.sleeping = sleeping;
        let screenshot = state.wanted > state.taken;
        // Some screens redraw every frame without anything moving: still means the same
        // pixels, not nothing drawn. Only worth reading back while someone is waiting.
        let watched = rendered && state.watching > 0;
        let pixels = (screenshot || watched).then(grab);
        if !rendered {
            state.status.idle_since.get_or_insert_with(Instant::now);
        } else if let (true, Some(pixels)) = (watched, &pixels) {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            pixels.hash(&mut hasher);
            let hash = hasher.finish();
            if state.last_hash == Some(hash) {
                state.status.idle_since.get_or_insert_with(Instant::now);
            } else {
                state.status.idle_since = None;
            }
            state.last_hash = Some(hash);
        } else {
            state.status.idle_since = None;
            state.last_hash = None;
        }
        if let (true, Some(pixels)) = (screenshot, pixels) {
            state.frame = Arc::new(pixels);
            state.taken = state.wanted;
        }
        drop(state);
        self.changed.notify_all();
    }

    pub fn quit_requested(&self) -> Option<i32> {
        self.lock().quit
    }
}

/// Starts the drivers the environment asks for. With neither, the simulator just runs.
pub fn start() -> Arc<Shared> {
    let shared = Arc::new(Shared {
        state: Mutex::new(State::default()),
        changed: Condvar::new(),
    });
    if let Some(addr) = std::env::var_os("SLOT_SIM_HTTP") {
        server::spawn(&addr.to_string_lossy(), shared.clone());
    }
    if let Some(path) = std::env::var_os("SLOT_SIM_SCRIPT") {
        script::spawn(path.into(), shared.clone());
    }
    shared
}

/// The phase's name alone, eg "QuickMenu" from "QuickMenu { row: Screen }".
pub fn phase_name(debug: &str) -> &str {
    debug.split([' ', '{', '(']).next().unwrap_or(debug)
}

pub fn parse_btn(name: &str) -> Option<Btn> {
    Some(match name.to_ascii_lowercase().as_str() {
        "up" => Btn::Up,
        "down" => Btn::Down,
        "left" => Btn::Left,
        "right" => Btn::Right,
        "a" => Btn::A,
        "b" => Btn::B,
        "x" => Btn::X,
        "y" => Btn::Y,
        "l1" | "l" => Btn::L1,
        "r1" | "r" => Btn::R1,
        "l2" => Btn::L2,
        "r2" => Btn::R2,
        "start" => Btn::Start,
        "select" => Btn::Select,
        "menu" => Btn::Menu,
        "volup" | "vol+" => Btn::VolUp,
        "voldown" | "vol-" => Btn::VolDown,
        "power" => Btn::Power,
        _ => return None,
    })
}

/// A stick direction's (x, y), all the way over.
pub fn parse_stick(name: &str) -> Option<(i16, i16)> {
    Some(match name.to_ascii_lowercase().as_str() {
        "up" => (0, -32767),
        "down" => (0, 32767),
        "left" => (-32767, 0),
        "right" => (32767, 0),
        "centre" | "center" => (0, 0),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_names() {
        assert_eq!(phase_name("Shelf"), "Shelf");
        assert_eq!(phase_name("QuickMenu { row: Screen }"), "QuickMenu");
        assert_eq!(phase_name("Doze(3)"), "Doze");
    }

    #[test]
    fn button_names() {
        assert_eq!(parse_btn("A"), Some(Btn::A));
        assert_eq!(parse_btn("menu"), Some(Btn::Menu));
        assert_eq!(parse_btn("vol+"), Some(Btn::VolUp));
        assert_eq!(parse_btn("turbo"), None);
    }
}
