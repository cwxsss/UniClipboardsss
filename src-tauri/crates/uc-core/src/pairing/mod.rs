mod paired_device;
mod role;

pub use paired_device::{resolve_sync_settings, PairedDevice, PairingState};
pub use role::PairingRole;
