//! A zipped game, started: its core loads the ROM unpacked, and a damaged one is refused.
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use slot::audio::{AudioSink, StubSink};
use slot::emu::{CoreState, EmuHandle};
use slot_retro::{AvInfo, ButtonMask, CoreError, MockCore, RetroCore};

type Seen = Arc<Mutex<Option<(PathBuf, Vec<u8>)>>>;

/// The mock core, noting the file it was given and what was in it.
struct Loads {
    inner: MockCore,
    seen: Seen,
}

impl RetroCore for Loads {
    fn load(&mut self, rom: &Path) -> Result<(), CoreError> {
        *self.seen.lock().unwrap() = Some((rom.to_path_buf(), std::fs::read(rom)?));
        self.inner.load(rom)
    }
    fn run_frame(&mut self, input: ButtonMask) {
        self.inner.run_frame(input)
    }
    fn video_xrgb8888(&self) -> &[u8] {
        self.inner.video_xrgb8888()
    }
    fn take_audio(&mut self) -> Vec<i16> {
        self.inner.take_audio()
    }
    fn serialize(&mut self) -> Result<Vec<u8>, CoreError> {
        self.inner.serialize()
    }
    fn unserialize(&mut self, data: &[u8]) -> Result<(), CoreError> {
        self.inner.unserialize(data)
    }
    fn save_ram(&self) -> Option<Vec<u8>> {
        self.inner.save_ram()
    }
    fn load_save_ram(&mut self, data: &[u8]) -> Result<(), CoreError> {
        self.inner.load_save_ram(data)
    }
    fn av_info(&self) -> AvInfo {
        self.inner.av_info()
    }
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../slot-store/tests/fixtures")
        .join(name)
}

fn start(rom: PathBuf, unpack_into: PathBuf) -> (EmuHandle, Seen) {
    let mut sink = StubSink::new();
    sink.open(32_768).expect("the stub refused to open");
    let seen = Seen::default();
    let core = Box::new(Loads {
        inner: MockCore::new(),
        seen: seen.clone(),
    });
    let emu = EmuHandle::spawn_game(core, rom, unpack_into, sink.ring(), None, None, None);
    let deadline = Instant::now() + Duration::from_secs(5);
    while emu.state() == CoreState::Loading && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    (emu, seen)
}

#[test]
fn a_zipped_game_loads_its_rom_unpacked() {
    let d = tempfile::tempdir().unwrap();
    let zip = d.path().join("Pocket Zip.zip");
    std::fs::copy(fixture("rom.zip"), &zip).unwrap();
    let cache = d.path().join("Unpacked");
    let (emu, seen) = start(zip, cache.clone());
    assert_eq!(emu.state(), CoreState::Ready);
    let (path, bytes) = seen
        .lock()
        .unwrap()
        .clone()
        .expect("the core was never loaded");
    assert_eq!(path, cache.join("Pocket Zip.gb"));
    assert_eq!(bytes, std::fs::read(fixture("rom.gb")).unwrap());
}

#[test]
fn a_7zd_game_loads_its_rom_unpacked() {
    let d = tempfile::tempdir().unwrap();
    let sevenz = d.path().join("Pocket 7z.7z");
    std::fs::copy(fixture("rom-lzma2.7z"), &sevenz).unwrap();
    let cache = d.path().join("Unpacked");
    let (emu, seen) = start(sevenz, cache.clone());
    assert_eq!(emu.state(), CoreState::Ready);
    let (path, bytes) = seen.lock().unwrap().clone().unwrap();
    assert_eq!(path, cache.join("Pocket 7z.gb"));
    assert_eq!(bytes, std::fs::read(fixture("rom.gb")).unwrap());
}

#[test]
fn a_damaged_zip_fails_to_load_and_the_core_never_sees_it() {
    let d = tempfile::tempdir().unwrap();
    let mut bytes = std::fs::read(fixture("rom.zip")).unwrap();
    let dir = bytes
        .windows(4)
        .position(|w| w == 0x0201_4b50u32.to_le_bytes())
        .unwrap();
    bytes[dir + 16] ^= 0xff;
    let zip = d.path().join("Broken.zip");
    std::fs::write(&zip, bytes).unwrap();
    let (emu, seen) = start(zip, d.path().join("Unpacked"));
    assert_eq!(emu.state(), CoreState::Failed);
    assert!(seen.lock().unwrap().is_none());
}

#[test]
fn a_plain_rom_is_loaded_where_it_is() {
    let (emu, seen) = start(fixture("rom.gb"), PathBuf::from("/nonexistent/Unpacked"));
    assert_eq!(emu.state(), CoreState::Ready);
    assert_eq!(seen.lock().unwrap().clone().unwrap().0, fixture("rom.gb"));
}

#[test]
fn a_colour_game_inside_an_archive_is_no_dmg_only_game() {
    assert!(!slot::core::dmg_only(&fixture("rom.zip")));
    assert!(!slot::core::dmg_only(&fixture("rom-lzma2.7z")));
    let d = tempfile::tempdir().unwrap();
    let mut rom = std::fs::read(fixture("rom.gb")).unwrap();
    rom[0x143] = 0;
    let plain = d.path().join("dmg.gb");
    std::fs::write(&plain, &rom).unwrap();
    assert!(slot::core::dmg_only(&plain));
}
