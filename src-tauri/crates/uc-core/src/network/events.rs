use std::path::PathBuf;

use super::protocol::{ClipboardMessage, PairingMessage, PairingRequest, PairingResponse};
use crate::ports::transfer_progress::TransferProgress;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Network status for P2P connection
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NetworkStatus {
    Disconnected,
    Connecting,
    Connected,
    Error(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProtocolDirection {
    Inbound,
    Outbound,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProtocolDenyReason {
    NotTrusted,
    Blocked,
    RepoError,
    NotSupported,
}

/// A peer discovered via mDNS
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredPeer {
    pub peer_id: String,
    pub device_name: Option<String>,
    /// 6-digit device ID (from Identify agent_version)
    pub device_id: Option<String>,
    pub addresses: Vec<String>,
    pub discovered_at: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub is_paired: bool,
}

/// A peer we have an active connection with
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectedPeer {
    pub peer_id: String,
    pub device_name: String,
    pub connected_at: DateTime<Utc>,
}

/// Live per-peer runtime state driven by the recovery coordinator.
///
/// `PeerRuntimeState` is the user-facing three-state model defined in the
/// Connection Stability Recovery PRD
/// (`docs/p2p/2026-04-11-connection-stability-recovery-prd.md`). It is kept
/// distinct from [`crate::device::DeviceStatus`], which is a database-adjacent
/// DTO.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeerRuntimeState {
    Online,
    Recovering,
    Offline,
}

/// What triggered a recovery cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryTrigger {
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
pub enum RecoveryProof {
    /// A recovery probe's business-stream open call returned success.
    BusinessStreamOpen,
    /// A fresh libp2p `ConnectionEstablished` event arrived from the swarm.
    ConnectionEstablished,
}

/// Core network events (domain layer)
/// Infrastructure-specific events should extend this
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkEvent {
    // Discovery events
    PeerDiscovered(DiscoveredPeer),
    PeerLost(String), // peer_id
    /// A peer's device name was updated (via DeviceAnnounce message or Identify)
    PeerNameUpdated {
        peer_id: String,
        device_name: String,
    },

    // Connection events
    PeerConnected(ConnectedPeer),
    PeerDisconnected(String), // peer_id

    // Recovery events (Connection Stability Recovery wave 1).
    // See docs/p2p/2026-04-11-connection-stability-recovery-prd.md.
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

    // Readiness events (protocol-agnostic)
    /// A peer is now ready to receive broadcast messages
    PeerReady {
        peer_id: String,
    },
    /// A peer is no longer ready to receive broadcast messages
    PeerNotReady {
        peer_id: String,
    },

    // Pairing events
    PairingMessageReceived {
        peer_id: String,
        message: PairingMessage,
    },
    PairingRequestReceived {
        session_id: String,
        peer_id: String,
        request: PairingRequest,
    },
    PairingPinReady {
        session_id: String,
        pin: String,
        peer_device_name: String, // Responder's device name (for initiator to display)
        peer_device_id: String,   // Responder's 6-digit device ID
    },
    PairingResponseReceived {
        session_id: String,
        peer_id: String,
        response: PairingResponse,
    },
    PairingComplete {
        session_id: String,
        peer_id: String,
        /// Peer's 6-digit device ID (stable identifier from database)
        peer_device_id: String,
        /// Peer device name (the other device's name, not this device's name)
        peer_device_name: String,
    },
    PairingFailed {
        session_id: String,
        peer_id: String,
        error: String,
    },

    // Clipboard events
    ClipboardReceived(ClipboardMessage),
    ClipboardSent {
        id: String,
        peer_count: usize,
    },

    // Status events
    StatusChanged(NetworkStatus),
    ProtocolDenied {
        peer_id: String,
        protocol_id: String,
        pairing_state: crate::network::PairingState,
        direction: ProtocolDirection,
        reason: ProtocolDenyReason,
    },
    // File transfer lifecycle events
    FileTransferStarted {
        transfer_id: String,
        peer_id: String,
        filename: String,
        file_size: u64,
    },
    FileTransferCompleted {
        transfer_id: String,
        peer_id: String,
        filename: String,
        file_path: PathBuf,
        batch_id: Option<String>,
        batch_total: Option<u32>,
    },
    FileTransferFailed {
        transfer_id: String,
        peer_id: String,
        error: String,
    },
    FileTransferCancelled {
        transfer_id: String,
        peer_id: String,
        reason: String,
    },

    // Transfer progress events
    TransferProgress(TransferProgress),

    #[allow(dead_code)]
    Error(String),
}
