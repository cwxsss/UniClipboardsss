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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uc_core::ids::PeerId;
    use uc_core::network::PairingState;

    fn make_device(peer_id: &str) -> PairedDevice {
        PairedDevice {
            peer_id: PeerId::from(peer_id),
            pairing_state: PairingState::Pending,
            identity_fingerprint: format!("fp-{}", peer_id),
            paired_at: Utc::now(),
            last_seen_at: None,
            device_name: format!("Device {}", peer_id),
            sync_settings: None,
        }
    }

    #[test]
    fn new_store_is_empty() {
        let store = StagedPairedDeviceStore::new();
        assert!(store.get_by_peer_id("any").is_none());
        assert!(store.take_by_peer_id("any").is_none());
    }

    #[test]
    fn stage_and_get_by_peer_id() {
        let store = StagedPairedDeviceStore::new();
        let device = make_device("peer-1");
        store.stage("session-1", device.clone());

        let retrieved = store.get_by_peer_id("peer-1");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().peer_id.as_str(), "peer-1");
    }

    #[test]
    fn take_by_peer_id_removes_device() {
        let store = StagedPairedDeviceStore::new();
        store.stage("session-1", make_device("peer-1"));

        let taken = store.take_by_peer_id("peer-1");
        assert!(taken.is_some());
        assert_eq!(taken.unwrap().peer_id.as_str(), "peer-1");

        // After take, device should be gone
        assert!(store.get_by_peer_id("peer-1").is_none());
        assert!(store.take_by_peer_id("peer-1").is_none());
    }

    #[test]
    fn clear_empties_the_store() {
        let store = StagedPairedDeviceStore::new();
        store.stage("s1", make_device("peer-1"));
        store.stage("s2", make_device("peer-2"));

        store.clear();

        assert!(store.get_by_peer_id("peer-1").is_none());
        assert!(store.get_by_peer_id("peer-2").is_none());
    }

    #[test]
    fn two_separate_instances_do_not_share_state() {
        let store_a = StagedPairedDeviceStore::new();
        let store_b = StagedPairedDeviceStore::new();

        store_a.stage("s1", make_device("peer-1"));

        // store_b should not see store_a's data
        assert!(store_b.get_by_peer_id("peer-1").is_none());

        // store_a still has it
        assert!(store_a.get_by_peer_id("peer-1").is_some());
    }
}
