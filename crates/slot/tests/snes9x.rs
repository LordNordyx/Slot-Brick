mod common;

use slot_retro::RetroCore;
use slot_store::Core;

fn dylib() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../vendor")
        .join(slot::core::dylib_name(Core::Snes9x))
}

/// A 32 KB LoROM whose program turns the backdrop white, the screen on, and loops.
fn white_screen_rom() -> Vec<u8> {
    let mut rom = vec![0u8; 0x8000];
    let program = [
        0xa9, 0x00, 0x8d, 0x21, 0x21, // LDA #$00; STA $2121: colour 0
        0xa9, 0xff, 0x8d, 0x22, 0x21, // LDA #$FF; STA $2122
        0xa9, 0x7f, 0x8d, 0x22, 0x21, // LDA #$7F; STA $2122: white
        0xa9, 0x0f, 0x8d, 0x00, 0x21, // LDA #$0F; STA $2100: screen on, full brightness
        0x80, 0xfe, // BRA to itself
    ];
    rom[..program.len()].copy_from_slice(&program);
    rom[0x7fc0..0x7fc0 + 21].copy_from_slice(b"SLOT WHITE TEST      ");
    rom[0x7fd5] = 0x20; // LoROM
    rom[0x7fd7] = 0x05; // 32 KB
    rom[0x7fd9] = 0x01; // North America
    rom[0x7ffc..0x7ffe].copy_from_slice(&[0x00, 0x80]); // reset: $8000
    rom[0x7fdc..0x7fe0].copy_from_slice(&[0xff, 0xff, 0x00, 0x00]);
    let sum = rom.iter().map(|b| *b as u32).sum::<u32>() as u16;
    rom[0x7fdc..0x7fde].copy_from_slice(&(sum ^ 0xffff).to_le_bytes());
    rom[0x7fde..0x7fe0].copy_from_slice(&sum.to_le_bytes());
    rom
}

fn load(d: &std::path::Path) -> Option<Box<dyn RetroCore>> {
    let path = dylib();
    if !path.exists() {
        eprintln!("no snes9x dylib on this host, skipping");
        return None;
    }
    let rom = d.join("white.sfc");
    std::fs::write(&rom, white_screen_rom()).unwrap();
    let mut core = slot::core::open_core_for(d, Core::Snes9x, "auto", false, &[path]);
    core.load(&rom).expect("snes9x refused the cart");
    Some(core)
}

#[test]
fn a_snes_game_draws_its_picture_centred_on_its_canvas() {
    let _g = common::core_lock();
    let d = tempfile::tempdir().unwrap();
    let Some(mut core) = load(d.path()) else { return };
    for _ in 0..10 {
        core.run_frame(slot_retro::ButtonMask(0));
    }
    let (w, h) = core.canvas();
    assert_eq!((w, h), slot_store::Platform::Snes.canvas());
    let video = core.video_xrgb8888();
    assert_eq!(video.len(), (w * h * 4) as usize);
    let px = |x: u32, y: u32| {
        let i = ((y * w + x) * 4) as usize;
        [video[i], video[i + 1], video[i + 2]]
    };
    assert!(px(128, 120).iter().all(|c| *c > 0xc0), "the picture is not white: {:?}", px(128, 120));
    assert!(px(128, 9).iter().all(|c| *c > 0xc0), "the first line of the picture is missing");
    assert_eq!(px(128, 3), [0, 0, 0], "the cropped overscan is not black above the picture");
    assert_eq!(px(128, 236), [0, 0, 0], "the cropped overscan is not black below the picture");
    let state = core.serialize().expect("no save state");
    core.run_frame(slot_retro::ButtonMask(0));
    core.unserialize(&state).expect("its own save state was refused");
    assert!(core.av_info().sample_rate > 0.0);
}

#[test]
fn every_option_slot_sets_on_snes9x_is_one_it_declares() {
    let path = dylib();
    if !path.exists() {
        eprintln!("no snes9x dylib on this host, skipping");
        return;
    }
    let _g = common::core_lock();
    let mut core = slot_retro::LibretroCore::open(&path).expect("open snes9x");
    slot::core::apply_core_options(&mut core, Core::Snes9x, "auto", false, true, None);
    let declared = core.declared_options();
    assert!(!declared.is_empty(), "snes9x declared no options, so this would pass on anything");
    let set = core.options();
    assert!(set.iter().any(|(k, v)| k == "snes9x_mode7_hires" && v == "disabled"), "{set:?}");
    for (key, value) in set {
        let values = declared.get(&key).unwrap_or_else(|| panic!("snes9x declares no option {key:?}"));
        assert!(values.contains(&value), "snes9x declares {key:?} as {values:?}, not {value:?}");
    }
}

#[test]
fn the_emulator_thread_hands_the_screen_whole_snes_frames_with_x_and_y_untouched() {
    use slot::audio::{AudioSink, StubSink};
    use slot::emu::{CoreState, EmuHandle, Speed};
    let _g = common::core_lock();
    let d = tempfile::tempdir().unwrap();
    let Some(core) = load(d.path()) else { return };
    let mut sink = StubSink::new();
    sink.open(32_768).unwrap();
    let emu = EmuHandle::spawn(core, d.path().join("white.sfc"), sink.ring(), None, None);
    emu.set_native_xy(true);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while emu.state() != CoreState::Ready && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(emu.state(), CoreState::Ready, "snes9x never finished loading");
    emu.set_speed(Speed::Normal);
    while !emu.has_published() && std::time::Instant::now() < deadline {
        sink.device_drain();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let frame = emu.latest_frame().expect("no frame was published");
    let (w, h) = slot_store::Platform::Snes.canvas();
    assert_eq!(frame.len(), (w * h * 4) as usize, "a frame is not the SNES canvas");
}
