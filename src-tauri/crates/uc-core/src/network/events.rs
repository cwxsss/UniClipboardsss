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
        pairing_state: crate::pairing::PairingState,
        direction: ProtocolDirection,
        reason: ProtocolDenyReason,
    },
    // File transfer lifecycle events
    #[deprecated(
        since = "0.6.0",
        note = "use crate::file_transfer::FileTransferEvent::Started instead"
    )]
    FileTransferStarted {
        transfer_id: String,
        peer_id: String,
        filename: String,
        file_size: Option<u64>,
    },
    #[deprecated(
        since = "0.6.0",
        note = "use crate::file_transfer::FileTransferEvent::Completed instead"
    )]
    FileTransferCompleted {
        transfer_id: String,
        peer_id: String,
        filename: String,
        file_path: PathBuf,
        batch_id: Option<String>,
        batch_total: Option<u32>,
    },
    #[deprecated(
        since = "0.6.0",
        note = "use crate::file_transfer::FileTransferEvent::Failed instead"
    )]
    FileTransferFailed {
        transfer_id: String,
        peer_id: String,
        error: String,
    },
    #[deprecated(
        since = "0.6.0",
        note = "use crate::file_transfer::FileTransferEvent::Cancelled instead"
    )]
    FileTransferCancelled {
        transfer_id: String,
        peer_id: String,
        reason: String,
    },

    // Transfer progress events
    #[deprecated(
        since = "0.6.0",
        note = "use crate::file_transfer::FileTransferEvent::Progress instead"
    )]
    TransferProgress(TransferProgress),

    #[allow(dead_code)]
    Error(String),
}
