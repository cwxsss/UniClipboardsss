//! # RuntimeState
//!
//! Snapshot-only state for the daemon runtime. Tracks uptime and cached
//! service health statuses. Does NOT own services — `DaemonApp` owns services
//! and periodically updates this snapshot.

use std::collections::HashMap;
use std::time::Instant;

use serde::Serialize;

use crate::service::ServiceHealth;

#[derive(Debug, Clone, PartialEq)]
pub struct DaemonServiceSnapshot {
    pub name: String,
    pub health: ServiceHealth,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DaemonPairingSessionSnapshot {
    pub session_id: String,
    pub peer_id: Option<String>,
    pub device_name: Option<String>,
    pub state: String,
    pub updated_at_ms: i64,
    #[serde(skip_serializing)]
    pub short_code: Option<String>,
    #[serde(skip_serializing)]
    pub peer_fingerprint: Option<String>,
}

/// Runtime state snapshot for the daemon.
///
/// This struct holds only pure data (start time + cached service statuses).
/// It is fully `Send + Sync` without trait object concerns. RPC reads never
/// contend with service lifecycle because this is a snapshot, not a live view.
pub struct RuntimeState {
    start_time: Instant,
    worker_statuses: Vec<DaemonServiceSnapshot>,
    connected_peer_count: u32,
    pairing_sessions: HashMap<String, DaemonPairingSessionSnapshot>,
}

impl RuntimeState {
    /// Create a new RuntimeState with the given initial service statuses.
    pub fn new(initial_statuses: Vec<DaemonServiceSnapshot>) -> Self {
        Self {
            start_time: Instant::now(),
            worker_statuses: initial_statuses,
            connected_peer_count: 0,
            pairing_sessions: HashMap::new(),
        }
    }

    /// Elapsed time since the daemon started, in seconds.
    pub fn uptime_seconds(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }

    /// Current cached service statuses.
    pub fn worker_statuses(&self) -> &[DaemonServiceSnapshot] {
        &self.worker_statuses
    }

    /// Replace the cached service statuses with a fresh snapshot.
    pub fn update_worker_statuses(&mut self, statuses: Vec<DaemonServiceSnapshot>) {
        self.worker_statuses = statuses;
    }

    /// Update the health of a single named service in the cached snapshot (Phase 67).
    ///
    /// Used to transition peer-discovery from `Stopped` → `Healthy` when the deferred
    /// `PeerDiscoveryWorker` starts after setup completes on an uninitialized device.
    /// No-op if the named service is not found.
    pub fn update_service_health(&mut self, name: &str, health: ServiceHealth) {
        if let Some(snapshot) = self.worker_statuses.iter_mut().find(|s| s.name == name) {
            snapshot.health = health;
        }
    }

    /// Current connected peer count tracked by the daemon runtime.
    pub fn connected_peer_count(&self) -> u32 {
        self.connected_peer_count
    }

    /// Replace the cached connected peer count with a fresh summary.
    pub fn update_connected_peer_count(&mut self, count: u32) {
        self.connected_peer_count = count;
    }

    /// Lookup a daemon-owned pairing session summary.
    pub fn pairing_session(&self, session_id: &str) -> Option<&DaemonPairingSessionSnapshot> {
        self.pairing_sessions.get(session_id)
    }

    /// Return all daemon-owned pairing session summaries.
    pub fn pairing_sessions(&self) -> Vec<DaemonPairingSessionSnapshot> {
        self.pairing_sessions.values().cloned().collect()
    }

    /// Replace a daemon-owned pairing session summary.
    pub fn upsert_pairing_session(&mut self, snapshot: DaemonPairingSessionSnapshot) {
        self.pairing_sessions
            .insert(snapshot.session_id.clone(), snapshot);
    }

    /// Remove a daemon-owned pairing session summary.
    pub fn remove_pairing_session(
        &mut self,
        session_id: &str,
    ) -> Option<DaemonPairingSessionSnapshot> {
        self.pairing_sessions.remove(session_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uptime_is_non_zero() {
        let state = RuntimeState::new(vec![]);
        // Sleep briefly to ensure elapsed time > 0
        std::thread::sleep(std::time::Duration::from_millis(10));
        assert!(
            state.start_time.elapsed().as_millis() > 0,
            "elapsed time should be > 0 after sleep"
        );
    }

    #[test]
    fn test_update_worker_statuses() {
        let mut state = RuntimeState::new(vec![]);
        assert_eq!(state.worker_statuses().len(), 0);

        state.update_worker_statuses(vec![
            DaemonServiceSnapshot {
                name: "clipboard-watcher".to_string(),
                health: ServiceHealth::Healthy,
            },
            DaemonServiceSnapshot {
                name: "peer-discovery".to_string(),
                health: ServiceHealth::Stopped,
            },
        ]);
        assert_eq!(state.worker_statuses().len(), 2);
        assert_eq!(state.worker_statuses()[0].name, "clipboard-watcher");
        assert_eq!(state.worker_statuses()[1].health, ServiceHealth::Stopped);
    }

    #[test]
    fn test_pairing_session_lookup_defaults_to_none() {
        let state = RuntimeState::new(vec![]);
        assert!(state.pairing_session("missing").is_none());
    }

    #[test]
    fn test_remove_pairing_session_removes_snapshot() {
        let mut state = RuntimeState::new(vec![]);
        state.upsert_pairing_session(DaemonPairingSessionSnapshot {
            session_id: "session-1".to_string(),
            peer_id: Some("peer-1".to_string()),
            device_name: Some("Desk".to_string()),
            state: "request".to_string(),
            updated_at_ms: 1,
            short_code: None,
            peer_fingerprint: None,
        });

        let removed = state.remove_pairing_session("session-1");

        assert!(removed.is_some());
        assert!(state.pairing_session("session-1").is_none());
    }

    #[test]
    fn pairing_session_snapshot_does_not_serialize_sensitive_fields() {
        let snapshot = DaemonPairingSessionSnapshot {
            session_id: "session-1".to_string(),
            peer_id: Some("peer-1".to_string()),
            device_name: Some("Desk".to_string()),
            state: "verification".to_string(),
            updated_at_ms: 123,
            short_code: Some("12345678".to_string()),
            peer_fingerprint: Some("fingerprint".to_string()),
        };

        let json = serde_json::to_string(&snapshot).expect("snapshot should serialize");

        assert!(json.contains("\"sessionId\":\"session-1\""));
        assert!(json.contains("\"state\":\"verification\""));
        assert!(!json.contains("\"code\""));
        assert!(!json.contains("fingerprint"));
        assert!(!json.contains("KeySlotFile"));
        assert!(!json.contains("challenge"));
    }
}
