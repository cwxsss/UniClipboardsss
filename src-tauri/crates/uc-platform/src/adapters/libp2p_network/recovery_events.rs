//! Recovery-specific types emitted by the libp2p connection stability
//! coordinator.
//!
//! These types live next to the libp2p adapter because the three-state peer
//! model (`Online`/`Recovering`/`Offline`) and the supporting trigger/proof
//! enums are driven by libp2p-specific mechanics: mDNS record expiry,
//! `ConnectionEstablished` swarm events, business-stream open probes, and the
//! swarm session rebuild escalation. They are not a cross-transport business
//! contract and therefore do not belong in `uc-core`.
//!
//! See `docs/p2p/2026-04-11-connection-stability-recovery-prd.md`.

use serde::{Deserialize, Serialize};

/// Live per-peer runtime state driven by the recovery coordinator.
///
/// Part of the user-facing three-state model defined in the Connection
/// Stability Recovery PRD. Kept distinct from
/// `uc_core::device::DeviceStatus`, which is a database-adjacent DTO.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PeerRuntimeState {
    Online,
    Recovering,
    Offline,
}

/// What triggered a recovery cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RecoveryTrigger {
    /// mDNS record for a paired peer expired.
    MdnsExpired,
    /// Several consecutive dial failures to the same paired peer.
    DialFailureStreak,
    /// First outbound attempt after a sustained idle window.
    FirstAttemptAfterIdle,
    /// Local device has just resumed from sleep.
    WakeFromSleep,
    /// Local network interface or IP address changed.
    NetworkInterfaceChanged,
}

/// Transport-level proof that justified closing a recovery cycle as recovered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RecoveryProof {
    /// A recovery probe's business-stream open call returned success.
    BusinessStreamOpen,
    /// A fresh libp2p `ConnectionEstablished` event arrived from the swarm.
    ConnectionEstablished,
}

/// Events produced by the recovery coordinator for in-process observability
/// (tracing/logging). They never cross the libp2p adapter boundary.
///
/// Fields are consumed via the `Debug` impl by `tracing::debug!`; the
/// `dead_code` lint does not recognize Debug-only reads, so we silence it
/// at the enum level rather than annotating every field.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) enum RecoveryEvent {
    /// Per-peer runtime state changed. The coordinator is the only producer.
    PeerStateChanged {
        peer_id: String,
        state: PeerRuntimeState,
        /// Present while a recovery cycle is active.
        cycle_id: Option<String>,
    },
    /// A recovery cycle has begun. User-facing state may still be `Online`
    /// during the initial silent phase.
    PeerRecoveryStarted {
        peer_id: String,
        cycle_id: String,
        trigger: RecoveryTrigger,
    },
    /// A recovery cycle ended successfully with transport-level proof.
    PeerRecovered {
        peer_id: String,
        cycle_id: String,
        elapsed_ms: u64,
        proof: RecoveryProof,
    },
    /// A recovery cycle exhausted its escalation ladder without restoring the
    /// peer. The user-facing state transitions to `Offline` at this point.
    PeerRecoveryFailed {
        peer_id: String,
        cycle_id: String,
        elapsed_ms: u64,
        /// Last escalation level reached (1, 2, or 3).
        last_escalation: u8,
    },
}

impl RecoveryEvent {
    /// Short static label for logging/metric dimensions.
    pub(crate) fn label(&self) -> &'static str {
        match self {
            RecoveryEvent::PeerStateChanged { state, .. } => match state {
                PeerRuntimeState::Online => "PeerStateOnline",
                PeerRuntimeState::Recovering => "PeerStateRecovering",
                PeerRuntimeState::Offline => "PeerStateOffline",
            },
            RecoveryEvent::PeerRecoveryStarted { .. } => "PeerRecoveryStarted",
            RecoveryEvent::PeerRecovered { .. } => "PeerRecovered",
            RecoveryEvent::PeerRecoveryFailed { .. } => "PeerRecoveryFailed",
        }
    }
}
