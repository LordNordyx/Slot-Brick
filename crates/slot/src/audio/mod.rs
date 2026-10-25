mod alsa;
#[cfg(feature = "host")]
mod host;
mod ring;
mod sfx;
mod silence;
mod sink;
mod stub;
pub mod volume;

pub use alsa::AlsaSink;
#[cfg(feature = "host")]
pub use host::HostAudio;
pub use ring::{ring_capacity, Ring};
pub use sfx::{MenuSound, Sfx, SoundKind, LABEL_TAKES, POP_TAKES};
pub use silence::Silence;
pub use sink::{AudioError, AudioSink};
pub use stub::StubSink;

pub const GBA_HZ: u32 = 32_768;

/// The rate the output is opened at. On Slot OS the Brick's codec takes only 44.1/48 kHz:
/// opened at 32768 Hz, ALSA resampled everything a second time, crudely, and the buffer Slot
/// keeps level ran dry ("starved"), crackling. At 48 kHz Slot's own resampler is the only one.
pub fn output_hz() -> u32 {
    if slot_power::slot_os() {
        48_000
    } else {
        GBA_HZ
    }
}

/// The output, opened on a thread of its own (open_early) while the screen comes up, for
/// Session::boot to take: opening it costs about a fifth of a second at startup.
static EARLY: std::sync::Mutex<Option<std::thread::JoinHandle<Box<dyn AudioSink>>>> =
    std::sync::Mutex::new(None);

pub fn open_early() {
    match std::thread::Builder::new()
        .name("slot-audio-open".into())
        .spawn(opened)
    {
        Ok(handle) => *EARLY.lock().unwrap_or_else(|e| e.into_inner()) = Some(handle),
        Err(e) => eprintln!("slot: audio: {e}"),
    }
}

/// The output open_early opened, or opened now if it was not asked for.
pub fn take_opened() -> Box<dyn AudioSink> {
    let early = EARLY.lock().unwrap_or_else(|e| e.into_inner()).take();
    early.and_then(|h| h.join().ok()).unwrap_or_else(opened)
}

fn opened() -> Box<dyn AudioSink> {
    let mut sink = open_sink();
    if let Err(e) = sink.open(output_hz()) {
        eprintln!("slot: audio: {e}");
    }
    sink
}

#[cfg(feature = "host")]
pub fn open_sink() -> Box<dyn AudioSink> {
    if std::env::var_os("SLOT_SILENT").is_some() {
        return Box::new(StubSink::draining());
    }
    Box::new(HostAudio::new())
}

#[cfg(all(not(feature = "host"), not(feature = "sim")))]
pub fn open_sink() -> Box<dyn AudioSink> {
    Box::new(AlsaSink::new())
}

/// The simulator has no speaker: sound is played into nothing at the speed a speaker would.
#[cfg(feature = "sim")]
pub fn open_sink() -> Box<dyn AudioSink> {
    Box::new(StubSink::draining())
}
