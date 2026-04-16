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
