//! # RuntimeState
//!
//! Snapshot-only state for the daemon runtime. Tracks uptime and cached
//! worker health statuses. Does NOT own workers — `DaemonApp` owns workers
//! and periodically updates this snapshot.

use std::collections::HashMap;
use std::time::Instant;

use crate::worker::WorkerHealth;

#[derive(Debug, Clone, PartialEq)]
pub struct DaemonWorkerSnapshot {
    pub name: String,
    pub health: WorkerHealth,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonPairingSessionSnapshot {
    pub session_id: String,
    pub peer_id: Option<String>,
    pub device_name: Option<String>,
    pub state: String,
    pub updated_at_ms: i64,
}

/// Runtime state snapshot for the daemon.
///
/// This struct holds only pure data (start time + cached worker statuses).
/// It is fully `Send + Sync` without trait object concerns. RPC reads never
/// contend with worker lifecycle because this is a snapshot, not a live view.
pub struct RuntimeState {
    start_time: Instant,
    worker_statuses: Vec<DaemonWorkerSnapshot>,
    connected_peer_count: u32,
    pairing_sessions: HashMap<String, DaemonPairingSessionSnapshot>,
}

impl RuntimeState {
    /// Create a new RuntimeState with the given initial worker statuses.
    pub fn new(initial_statuses: Vec<DaemonWorkerSnapshot>) -> Self {
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

    /// Current cached worker statuses.
    pub fn worker_statuses(&self) -> &[DaemonWorkerSnapshot] {
        &self.worker_statuses
    }

    /// Replace the cached worker statuses with a fresh snapshot.
    pub fn update_worker_statuses(&mut self, statuses: Vec<DaemonWorkerSnapshot>) {
        self.worker_statuses = statuses;
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
            DaemonWorkerSnapshot {
                name: "clipboard-watcher".to_string(),
                health: WorkerHealth::Healthy,
            },
            DaemonWorkerSnapshot {
                name: "peer-discovery".to_string(),
                health: WorkerHealth::Stopped,
            },
        ]);
        assert_eq!(state.worker_statuses().len(), 2);
        assert_eq!(state.worker_statuses()[0].name, "clipboard-watcher");
        assert_eq!(state.worker_statuses()[1].health, WorkerHealth::Stopped);
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
        });

        let removed = state.remove_pairing_session("session-1");

        assert!(removed.is_some());
        assert!(state.pairing_session("session-1").is_none());
    }
}
