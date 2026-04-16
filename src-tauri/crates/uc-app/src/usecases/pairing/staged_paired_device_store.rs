use std::collections::HashMap;
use std::sync::Mutex;

use uc_core::network::PairedDevice;

/// Injectable store for staging paired devices during the pairing flow.
///
/// Replaces the former global `OnceLock`-based static. Each instance owns its
/// own `Mutex<HashMap>`, so separate instances do **not** share state.
pub struct StagedPairedDeviceStore {
    devices: Mutex<HashMap<String, PairedDevice>>,
}

impl StagedPairedDeviceStore {
    pub fn new() -> Self {
        Self {
            devices: Mutex::new(HashMap::new()),
        }
    }

    pub fn stage(&self, session_id: &str, device: PairedDevice) {
        if let Ok(mut staged) = self.devices.lock() {
            staged.insert(session_id.to_string(), device);
        }
    }

    pub fn take_by_peer_id(&self, peer_id: &str) -> Option<PairedDevice> {
        let mut staged = self.devices.lock().ok()?;
        let session_id = staged.iter().find_map(|(session_id, device)| {
            (device.peer_id.as_str() == peer_id).then(|| session_id.clone())
        })?;
        staged.remove(&session_id)
    }

    pub fn get_by_peer_id(&self, peer_id: &str) -> Option<PairedDevice> {
        let staged = self.devices.lock().ok()?;
        staged.iter().find_map(|(_session_id, device)| {
            (device.peer_id.as_str() == peer_id).then(|| device.clone())
        })
    }

    /// Clear all staged devices.
    ///
    /// Available for lifecycle shutdown cleanup (not test-only).
    pub fn clear(&self) {
        if let Ok(mut staged) = self.devices.lock() {
            staged.clear();
        }
    }
}
