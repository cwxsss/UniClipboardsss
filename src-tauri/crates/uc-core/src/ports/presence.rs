//! Presence port (Slice 2 Phase 1).
//!
//! Tracks whether each `SpaceMember` is currently reachable on the iroh
//! endpoint. Consumed by `MemberRosterFacade` for the roster view and by
//! `EnsureReachableAllUseCase` which fires after F1 `start_network` to
//! pre-connect every member.
//!
//! `ensure_reachable` is a single-target primitive; batching ("pre-connect
//! the whole roster") lives in the application layer so this port stays
//! minimal.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tokio::sync::broadcast;

use crate::ids::DeviceId;

/// Reachability snapshot for one member.
///
/// Intentionally three-valued: `Unknown` distinguishes "never probed" from
/// "probed and confirmed offline". No `Connecting` / `Degraded` — Slice 2
/// has no consumer that could act on those.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReachabilityState {
    Online,
    Offline,
    Unknown,
}

/// Notification delivered on state change.
#[derive(Debug, Clone)]
pub struct PresenceEvent {
    pub device_id: DeviceId,
    pub state: ReachabilityState,
    pub at: DateTime<Utc>,
}

#[derive(Debug, thiserror::Error)]
pub enum PresenceError {
    /// No stored [`PeerAddressRecord`](crate::ports::peer_address::PeerAddressRecord)
    /// for this device — cannot dial. Application layer treats this as
    /// "member is offline" rather than a fatal error.
    #[error("no known address for device {0:?}")]
    NoAddress(DeviceId),
    #[error("internal: {0}")]
    Internal(String),
}

#[async_trait]
pub trait PresencePort: Send + Sync {
    /// Actively probe / dial the target device.
    ///
    /// Returns the resulting state — typically `Online` on success, `Offline`
    /// on dial failure. A `NoAddress` error surfaces when the peer address
    /// repository has no record for this device.
    async fn ensure_reachable(&self, device: &DeviceId)
        -> Result<ReachabilityState, PresenceError>;

    /// Read the current cached state without dialing.
    ///
    /// Returns `Unknown` if the device has never been probed in the current
    /// process lifetime.
    async fn current_state(&self, device: &DeviceId) -> ReachabilityState;

    /// Multi-consumer subscription for state-change events.
    ///
    /// Each call returns a fresh receiver. Lagging receivers drop messages
    /// per `broadcast` contract — acceptable because the latest state can
    /// always be recovered via [`current_state`].
    fn subscribe(&self) -> broadcast::Receiver<PresenceEvent>;
}
