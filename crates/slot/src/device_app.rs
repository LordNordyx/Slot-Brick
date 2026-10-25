use std::path::PathBuf;
use std::time::{Duration, Instant};

use slot::frontend::Frontend;
#[cfg(all(feature = "brick", not(feature = "sim")))]
use slot::input::BrickInput;
#[cfg(feature = "sim")]
use slot::input::SimInput as BrickInput;
#[cfg(not(feature = "brick"))]
use slot::input::DeviceInput;
use slot_gfx::{Compositor, FbdevSurface, Surface};
use slot::boot_times::mark as boot_time;
use slot_power::trace_first_frame;
#[cfg(feature = "brick")]
use slot_power::BrickPlatform;
#[cfg(not(feature = "brick"))]
use slot_power::DevicePlatform;

const CARD: &str = "/mnt/sdcard";

const MIN_FRAME: Duration = Duration::from_millis(12);

const STEP_TIMEOUT: Duration = Duration::from_millis(12);

const MARGIN: Duration = Duration::from_micros(1500);

struct Pacer {
    last_return: Option<Instant>,
    period: Duration,
    work: Duration,
    works: [Duration; WORK_WINDOW],
    next: usize,
}

const WORK_WINDOW: usize = 32;

impl Pacer {
    fn new() -> Self {
        Pacer {
            last_return: None,
            period: Duration::from_micros(16_760),
            work: Duration::from_millis(4),
            works: [Duration::from_millis(4); WORK_WINDOW],
            next: 0,
        }
    }

    fn wait(&self) {
        let Some(last) = self.last_return else {
            return;
        };
        let delay = self.period.saturating_sub(self.work + MARGIN);
        if let Some(left) = (last + delay).checked_duration_since(Instant::now()) {
            std::thread::sleep(left);
        }
    }

    fn swapped(&mut self, work: Duration, blocked: bool) -> bool {
        let now = Instant::now();
        let mut missed = false;
        if let Some(last) = self.last_return {
            let seen = now - last;
            missed = seen > self.period.mul_f64(1.5);
            if blocked && seen > Duration::from_millis(12) && seen < Duration::from_millis(22) {
                self.period = self.period.mul_f64(0.95) + seen.mul_f64(0.05);
            }
        }
        self.works[self.next] = work;
        self.next = (self.next + 1) % WORK_WINDOW;
        self.work = self.works.iter().copied().max().unwrap_or(work);
        self.last_return = Some(now);
        missed
    }
}

/// On Slot OS the GPU driver loads beside Slot's own start (init's SLOT_GPU_READY names the file
/// it makes once loaded): the screen waits for it, the sound and the games do not.
fn wait_for_gpu() {
    let Some(ready) = std::env::var_os("SLOT_GPU_READY").map(PathBuf::from) else {
        return;
    };
    let since = Instant::now();
    while !ready.exists() && since.elapsed() < Duration::from_secs(10) {
        std::thread::sleep(Duration::from_millis(5));
    }
    boot_time(&format!("GPU driver ready (waited {} ms)", since.elapsed().as_millis()));
}

/// Runs System/sleep.sh without going deaf: a Power press that lands while the
/// script is still preparing (Wi-Fi stop, sync) cancels the suspend instead of
/// being swallowed, which used to need a second press to wake. Once the script
/// has announced the kernel request (/tmp/slot-suspending) the press is the wake key.
#[cfg(all(feature = "brick", not(feature = "sim")))]
fn run_sleep_script(root: &std::path::Path, input: &mut BrickInput, frontend: &Frontend) -> (std::io::Result<std::process::ExitStatus>, bool) {
    use slot_input::{Btn, InputSource, RawEvent};
    const SUSPENDING: &str = "/tmp/slot-suspending";
    let _ = std::fs::remove_file(SUSPENDING);
    let mut child = match std::process::Command::new("/bin/sh")
        .arg(root.join("System/sleep.sh")).env("SLOT_ROOT", root).spawn() {
        Ok(c) => c,
        Err(e) => return (Err(e), false),
    };
    // Any Power press seen while the script runs means "wake up", even when it
    // came too late to cancel and only made the kernel abort the suspend.
    let mut pressed = false;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return (Ok(status), pressed),
            Ok(None) => {}
            Err(e) => return (Err(e), pressed),
        }
        if !pressed && input.poll(frontend.now()).into_iter().any(|e| matches!(e, RawEvent::Down(Btn::Power))) {
            pressed = true;
            if !std::path::Path::new(SUSPENDING).exists() {
                eprintln!("slot: sleep: Power pressed while preparing; cancelling the suspend");
                unsafe { libc::kill(child.id() as i32, libc::SIGTERM) };
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// The simulator's suspend: the screen goes dark and stays so until Power is pressed, as the
/// kernel's would; System/sleep.sh is never run, it would act on the real machine.
#[cfg(feature = "sim")]
fn sim_sleep(input: &mut BrickInput, frontend: &Frontend, sim: &slot::sim::Shared, surface: &FbdevSurface) -> (std::io::Result<std::process::ExitStatus>, bool) {
    use slot_input::{Btn, InputSource, RawEvent};
    use std::os::unix::process::ExitStatusExt;
    eprintln!("slot: sim: suspended; press Power to wake");
    loop {
        if let Some(code) = sim.quit_requested() {
            std::process::exit(code);
        }
        sim.tick(format!("{:?}", frontend.app().phase()), false, true, || surface.read_rgba());
        if input.poll(frontend.now()).into_iter().any(|e| matches!(e, RawEvent::Down(Btn::Power))) {
            return (Ok(std::process::ExitStatus::from_raw(0)), true);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

pub fn run() {
    boot_time("started");
    let root = std::env::var_os("SLOT_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(CARD));
    // An update that a power cut stopped halfway is finished before anything else, and one
    // stopped before its swap began is cleared away.
    #[cfg(feature = "brick")]
    if slot::update::resume(&slot::update::card_of(&root)) {
        std::process::exit(slot::update::EXIT_UPDATED);
    }
    #[cfg(feature = "brick")]
    slot::update::tidy(&slot::update::card_of(&root));
    #[cfg(feature = "sim")]
    if let Err(e) = slot::sim::populate() {
        eprintln!("slot: sim: stand-in hardware: {e}");
        std::process::exit(1);
    }
    // Knowing the model means reading all of the stock MainUI (several MB): on a
    // thread of its own, while the screen and sound open. The first to ask waits for it.
    #[cfg(feature = "brick")]
    let _ = std::thread::Builder::new().name("slot-model".into()).spawn(slot_power::model);
    // The font, before anything is drawn in it.
    eprintln!("slot: font: {}", slot::fonts::load_chosen(&root));
    // The sound output opens, and the app boots, beside the screen, for Session::boot to take.
    slot::audio::open_early();
    slot::app::App::boot_early(root.clone());
    #[cfg(feature = "sim")]
    let sim = slot::sim::start();
    wait_for_gpu();
    #[cfg(feature = "sim")]
    let surface = FbdevSurface::offscreen(slot::sim::PANEL);
    #[cfg(not(feature = "sim"))]
    let surface = FbdevSurface::new();
    let mut surface = match surface {
        Ok(s) => s,
        Err(e) => {
            eprintln!("slot: {e}");
            std::process::exit(1);
        }
    };
    let mut compositor = match Compositor::new(&surface) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("slot: {e}");
            // On Slot OS, what the GPU driver said about it, for the bug report.
            if slot_power::slot_os() {
                if let Ok(out) = std::process::Command::new("dmesg").output() {
                    let log = String::from_utf8_lossy(&out.stdout);
                    let tail: Vec<&str> = log.lines().rev().take(60).collect();
                    eprintln!("slot: kernel log, last lines:");
                    for line in tail.into_iter().rev() {
                        eprintln!("  {line}");
                    }
                }
            }
            std::process::exit(1);
        }
    };
    boot_time("screen ready");
    #[cfg(not(feature = "brick"))]
    let platform = DevicePlatform::new(root.clone());
    #[cfg(not(feature = "brick"))]
    {
        eprintln!("slot: {}", platform.report());
        platform.trace_boot();
    }
    #[cfg(feature = "brick")]
    let platform = BrickPlatform::new(root.clone());
    #[cfg(feature = "brick")]
    slot::brick_wifi::repair_at_boot();
    let mut frontend = Frontend::boot(Box::new(platform));
    boot_time("games and settings read");
    frontend.upload_shelf_faces(&mut compositor);
    frontend.upload_bezel(&mut compositor, surface.window_size());
    boot_time("faces drawn");
    #[cfg(not(feature = "brick"))]
    let mut input = DeviceInput::open(&root);
    #[cfg(feature = "sim")]
    let mut input = BrickInput::new(sim.clone());
    #[cfg(all(feature = "brick", not(feature = "sim")))]
    let mut input = match BrickInput::open(&root) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("slot: {e}");
            std::process::exit(1);
        }
    };
    boot_time("joystick ready");
    #[cfg(not(feature = "brick"))]
    let card = root.clone();
    #[cfg(not(feature = "brick"))]
    let _ = std::thread::Builder::new()
        .name("slot-bootlogo".into())
        .spawn(move || slot::bootlogo::refresh(&card));
    #[cfg(feature = "brick")]
    let mut cpu = slot::brick_power::CpuPower::open(&root);
    let mut drawn = false;
    frontend.drive_emulator();
    let mut pacer = Pacer::new();
    let (mut frames, mut missed, mut early) = (0u32, 0u32, 0u32);
    #[cfg(feature = "brick")]
    let mut sleep_since: Option<Instant> = None;
    #[cfg(feature = "brick")]
    let mut sleep_attempted = false;
    loop {
        pacer.wait();
        #[cfg(feature = "sim")]
        if let Some(code) = sim.quit_requested() {
            cpu.restore();
            std::process::exit(code);
        }
        let began = Instant::now();
        frontend.advance(&mut input);
        #[cfg(feature = "brick")]
        if input.exit_requested() {
            cpu.restore();
            frontend.exit_to_launcher();
            return;
        }
        if frontend.restarting() {
            #[cfg(feature = "brick")]
            cpu.restore();
            frontend.restart();
        }
        #[cfg(feature = "brick")]
        if frontend.updating() {
            cpu.restore();
            frontend.exit_for_update();
        }
        if frontend.powering_off() {
            #[cfg(feature = "brick")]
            cpu.restore();
            frontend.poweroff();
            return;
        }
        #[cfg(feature = "brick")]
        {
            frontend.idle_sleep();
            cpu.mode(frontend.power_mode(),frontend.audio_underruns());
            if matches!(frontend.app().phase(), slot::app::Phase::Doze { .. }) {
                let since = *sleep_since.get_or_insert_with(Instant::now);
                // Let the paused audio sink release ALSA and pending saves finish.
                if !sleep_attempted && since.elapsed() >= Duration::from_millis(3300) {
                    sleep_attempted = true;
                    if frontend.prepare_sleep() {
                        slot::brick_link::quiesce();
                        let suspended_before=slot::brick_power::suspended_seconds();
                        #[cfg(not(feature = "sim"))]
                        let (result, cancelled) = run_sleep_script(&root, &mut input, &frontend);
                        #[cfg(feature = "sim")]
                        let (result, cancelled) = sim_sleep(&mut input, &frontend, &sim, &surface);
                        let returned = Instant::now();
                        let suspended=slot::brick_power::suspended_seconds().zip(suspended_before).map(|(a,b)|(a-b).max(0.0));
                        let confirmed=suspended.is_some_and(|v|v>0.05);
                        cpu.mode(slot::brick_power::Mode::Boost,frontend.audio_underruns());
                        let woke = match &result {
                            Ok(status) if status.success() || confirmed || cancelled => {
                                frontend.resume_sleep(&mut input);
                                sleep_since = None;
                                sleep_attempted = false;
                                true
                            }
                            other => {eprintln!("slot: suspend unavailable ({other:?}); using standby"); false}
                        };
                        {
                            use std::io::Write;
                            if let Ok(mut log)=std::fs::OpenOptions::new().create(true).append(true).open(root.join("Logs/sleep.log")) {
                                let _=writeln!(log,"Slot verification: suspended_seconds={suspended:?}, confirmed={confirmed}, power_pressed_during_sleep_entry={cancelled}, helper={result:?}, screen_on={woke} {} ms after the helper returned",returned.elapsed().as_millis());
                            }
                        }
                    }
                }
                pacer = Pacer::new();
                #[cfg(feature = "sim")]
                sim.tick(format!("{:?}", frontend.app().phase()), false, true, || surface.read_rgba());
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }
            sleep_since = None;
            sleep_attempted = false;
        }
        #[cfg(feature = "brick")]
        if !frontend.needs_render() {
            pacer = Pacer::new();
            #[cfg(feature = "sim")]
            sim.tick(format!("{:?}", frontend.app().phase()), false, false, || surface.read_rgba());
            std::thread::sleep(Duration::from_millis(25));
            continue;
        }
        if !drawn {
            boot_time("  first input and update");
        }
        let frame_ready = frontend.step_emulator(pacer.period, STEP_TIMEOUT);
        #[cfg(not(feature = "brick"))]
        let _ = frame_ready;
        frontend.render(&mut compositor, surface.window_size());
        if !drawn {
            boot_time("  first render");
        }
        let swap = Instant::now();
        let work = swap - began;
        #[cfg(feature = "brick")]
        cpu.sample(frame_ready,work,pacer.period,frontend.audio_underruns());
        if let Err(e) = surface.swap() {
            eprintln!("slot: {e}");
            #[cfg(feature = "brick")]
            cpu.restore();
            std::process::exit(1);
        }
        let swap_took = swap.elapsed();
        #[cfg(feature = "sim")]
        sim.tick(format!("{:?}", frontend.app().phase()), true, false, || surface.read_rgba());
        slot::latency::swapped(swap_took.as_secs_f64() * 1000.0);
        let dropped = pacer.swapped(work, swap_took > Duration::from_millis(1));
        if slot::latency::tracing() {
            frames += 1;
            missed += u32::from(dropped);
            early += u32::from(swap_took > pacer.period / 2);
            if frames == 600 {
                eprintln!(
                    "slot: pace: 600 frames, {missed} missed a latch, {early} drawn early, work {:.1} ms, period {:.2} ms",
                    pacer.work.as_secs_f64() * 1e3,
                    pacer.period.as_secs_f64() * 1e3
                );
                (frames, missed, early) = (0, 0, 0);
            }
        }
        if !drawn {
            drawn = true;
            trace_first_frame();
            boot_time("first frame");
            // The menus are drawn now, behind the first frame: none opens before the next
            // frame's input.
            frontend.upload_menu_faces(&mut compositor);
            boot_time("menus drawn");
            #[cfg(feature = "brick")]
            slot::update::settle(&slot::update::card_of(&root));
            #[cfg(feature = "brick")]
            if let Some(path) = std::env::var_os("SLOT_READY_FILE") {
                if let Err(e) = std::fs::write(path, "first frame displayed\n") {
                    eprintln!("slot: ready marker: {e}");
                }
            }
        }
        if let Some(left) = MIN_FRAME.checked_sub(began.elapsed()) {
            std::thread::sleep(left);
        }
    }
}
