#[cfg(feature = "brick")]
pub mod brick;
mod device;
#[cfg(feature = "brick")]
pub use brick::BrickInput;
pub mod evdev;
#[cfg(feature = "host")]
mod host;
#[cfg(feature = "host")]
mod keys;
mod mask;
#[cfg(feature = "sim")]
mod sim;
#[cfg(feature = "host")]
mod pad;
pub mod trace;

pub use device::DeviceInput;
#[cfg(feature = "host")]
pub use host::HostInput;
pub use mask::Pad;
#[cfg(feature = "sim")]
pub use sim::SimInput;
