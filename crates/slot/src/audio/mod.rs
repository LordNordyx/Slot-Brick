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
pub use sfx::Sfx;
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
