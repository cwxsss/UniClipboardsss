//! Host event emitter port — abstract event delivery for background tasks.
//!
//! This module defines the [`HostEventEmitterPort`] trait and the [`HostEvent`]
//! type system that background tasks use to deliver events to the host environment.
//!
//! The port is intentionally free of Tauri, serde, and any infrastructure
//! dependency. Adapters (e.g., `TauriEventEmitter`) own the serialization
//! contract and event name mapping.
//!
//! # Design
//!
//! - [`HostEvent`] is a pure semantic model — no serde annotations.
//! - [`HostEventEmitterPort`] is synchronous (fire-and-forget semantics).
//! - Emit failures are best-effort: callers log the error and continue.
//! - [`TransferProgress`] is reused from [`crate::ports::transfer_progress`].

use crate::ports::realtime::RealtimeFrontendEvent;
use crate::ports::transfer_progress::TransferProgress;
use crate::setup::SetupState;

// ---------------------------------------------------------------------------
// ClipboardOriginKind
// ---------------------------------------------------------------------------

/// Indicates whether clipboard content originated locally or from a remote peer.
#[derive(Debug, Clone)]
pub enum ClipboardOriginKind {
    /// Captured from the local clipboard watcher.
    Local,
    /// Received from a remote peer via sync.
    Remote,
}

// ---------------------------------------------------------------------------
// ClipboardHostEvent
// ---------------------------------------------------------------------------

/// Semantic events emitted by the clipboard subsystem.
#[derive(Debug, Clone)]
pub enum ClipboardHostEvent {
    /// New clipboard content was captured or received.
    ///
    /// `preview` is always present — a brief text summary of the content.
    NewContent {
        entry_id: String,
        preview: String,
        origin: ClipboardOriginKind,
    },
    /// An inbound clipboard message from a remote peer could not be processed.
    InboundError {
        message_id: String,
        origin_device_id: String,
        error: String,
    },
    /// The inbound clipboard subscription recovered after repeated failures.
    InboundSubscribeRecovered { recovered_after_attempts: u32 },
    /// The inbound clipboard subscription failed for the first time.
    /// Maps to Tauri event "inbound-clipboard-subscribe-error".
    InboundSubscribeError { attempt: u32, error: String },
    /// The inbound clipboard subscription is retrying after a failure.
    /// Maps to Tauri event "inbound-clipboard-subscribe-retry".
    InboundSubscribeRetry {
        attempt: u32,
        retry_in_ms: u64,
        error: String,
    },
    /// Daemon WS bridge reconnected after degraded state — consumers should refetch stale data.
    /// Maps to Tauri event "daemon://ws-reconnected".
    DaemonReconnected,
}

// ---------------------------------------------------------------------------
// PeerDiscoveryHostEvent
// ---------------------------------------------------------------------------

/// Semantic events emitted by the peer discovery subsystem.
#[derive(Debug, Clone)]
pub enum PeerDiscoveryHostEvent {
    /// A new peer was discovered on the network.
    Discovered {
        peer_id: String,
        device_name: Option<String>,
        addresses: Vec<String>,
    },
    /// A previously discovered peer is no longer reachable.
    Lost {
        peer_id: String,
        device_name: Option<String>,
        addresses: Vec<String>,
    },
}

// ---------------------------------------------------------------------------
// PeerConnectionHostEvent
// ---------------------------------------------------------------------------

/// Semantic events emitted by the peer connection subsystem.
///
/// Note: Both `PeerReady` and `PeerConnected` network events map to
/// [`Connected`]; both `PeerNotReady` and `PeerDisconnected` map to
/// [`Disconnected`]. This collapses the lower-level network states into
/// the binary connected/disconnected view that the frontend consumes.
#[derive(Debug, Clone)]
pub enum PeerConnectionHostEvent {
    /// A peer connection became active (covers both PeerReady and PeerConnected).
    Connected {
        peer_id: String,
        device_name: Option<String>,
    },
    /// A peer connection became inactive (covers both PeerNotReady and PeerDisconnected).
    Disconnected {
        peer_id: String,
        device_name: Option<String>,
    },
    /// A peer's displayed device name was updated.
    NameUpdated {
        peer_id: String,
        device_name: String,
    },
}

// ---------------------------------------------------------------------------
// TransferHostEvent
// ---------------------------------------------------------------------------

/// Semantic events emitted by the file transfer subsystem.
#[derive(Debug, Clone)]
pub enum TransferHostEvent {
    /// Progress update for an active transfer.
    Progress(TransferProgress),
    /// A file transfer completed successfully.
    Completed {
        transfer_id: String,
        filename: String,
        peer_id: String,
        file_size: u64,
        auto_pulled: bool,
        file_path: String,
    },
    /// The status of a transfer entry changed.
    StatusChanged {
        transfer_id: String,
        entry_id: String,
        status: String,
        reason: Option<String>,
    },
}

// ---------------------------------------------------------------------------
// PairingVerificationKind
// ---------------------------------------------------------------------------

/// The kind of pairing verification event — serde-free, maps to lowercase strings in adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PairingVerificationKind {
    Request,
    Verification,
    Verifying,
    Complete,
    Failed,
}

// ---------------------------------------------------------------------------
// PairingHostEvent
// ---------------------------------------------------------------------------

/// Semantic events emitted by the pairing subsystem.
#[derive(Debug, Clone)]
pub enum PairingHostEvent {
    /// A pairing verification step occurred (7 emit sites in wiring.rs).
    /// Maps to Tauri event "p2p-pairing-verification".
    Verification {
        session_id: String,
        kind: PairingVerificationKind,
        peer_id: Option<String>,
        device_name: Option<String>,
        code: Option<String>,
        local_fingerprint: Option<String>,
        peer_fingerprint: Option<String>,
        error: Option<String>,
    },
    /// The pairing events subscription failed.
    /// Maps to Tauri event "pairing-events-subscribe-failure".
    SubscribeFailure {
        attempt: u32,
        retry_in_ms: u64,
        error: String,
    },
    /// The pairing events subscription recovered after failures.
    /// Maps to Tauri event "pairing-events-subscribe-recovered".
    SubscribeRecovered { recovered_after_attempts: u32 },
}

// ---------------------------------------------------------------------------
// SetupHostEvent
// ---------------------------------------------------------------------------

/// Semantic events emitted by the setup subsystem.
#[derive(Debug, Clone)]
pub enum SetupHostEvent {
    /// The setup wizard state changed.
    /// Maps to Tauri event "setup-state-changed".
    ///
    /// IMPORTANT: `state` carries the full `SetupState` enum (not a String) to
    /// preserve data-carrying variants (JoinSpaceConfirmPeer, ProcessingCreateSpace, etc.).
    StateChanged {
        state: SetupState,
        session_id: Option<String>,
    },
}

// ---------------------------------------------------------------------------
// SpaceAccessHostEvent
// ---------------------------------------------------------------------------

/// Semantic events emitted by the space access subsystem.
#[derive(Debug, Clone)]
pub enum SpaceAccessHostEvent {
    /// A space access attempt completed (WebDAV / local path).
    /// Maps to Tauri event "space-access-completed".
    ///
    /// IMPORTANT: `peer_id` is `String` (non-optional), matching the existing
    /// wire contract and `SpaceAccessCompletedEvent.peer_id: String`.
    Completed {
        session_id: String,
        peer_id: String,
        success: bool,
        reason: Option<String>,
        ts: i64,
    },
    /// A P2P space access attempt completed.
    /// Maps to Tauri event "p2p-space-access-completed".
    P2PCompleted {
        session_id: String,
        peer_id: String,
        success: bool,
        reason: Option<String>,
        ts: i64,
    },
}

// ---------------------------------------------------------------------------
// HostEvent
// ---------------------------------------------------------------------------

/// Top-level host event enum — groups all in-scope semantic events by domain.
///
/// This is a pure Rust type with no serde annotations. Adapters are solely
/// responsible for serialization to frontend wire formats.
#[derive(Debug, Clone)]
pub enum HostEvent {
    Clipboard(ClipboardHostEvent),
    PeerDiscovery(PeerDiscoveryHostEvent),
    PeerConnection(PeerConnectionHostEvent),
    Transfer(TransferHostEvent),
    Pairing(PairingHostEvent),
    Realtime(RealtimeFrontendEvent),
    Setup(SetupHostEvent),
    SpaceAccess(SpaceAccessHostEvent),
}

// ---------------------------------------------------------------------------
// EmitError
// ---------------------------------------------------------------------------

/// Error returned when [`HostEventEmitterPort::emit`] fails.
///
/// Emit failures are best-effort — callers should log the error and continue.
#[derive(Debug, thiserror::Error)]
pub enum EmitError {
    #[error("emit failed: {0}")]
    Failed(String),
}

// ---------------------------------------------------------------------------
// HostEventEmitterPort
// ---------------------------------------------------------------------------

/// Abstract port for delivering host events to the runtime environment.
///
/// Implementations:
/// - `TauriEventEmitter` — wraps `AppHandle`, maps `HostEvent` → Tauri event name + camelCase DTO.
/// - `LoggingEventEmitter` — writes structured `tracing` output, always returns `Ok`.
///
/// The trait is synchronous — `tauri::Emitter::emit()` is non-async and
/// event delivery is fire-and-forget.
pub trait HostEventEmitterPort: Send + Sync {
    /// Deliver a host event to the runtime environment.
    ///
    /// On failure, the error is returned for the caller to log. The caller
    /// **must not** propagate the error as a business-logic failure.
    fn emit(&self, event: HostEvent) -> Result<(), EmitError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::transfer_progress::{TransferDirection, TransferProgress};
    use crate::setup::SetupState;
    use std::sync::Mutex;

    #[derive(Default)]
    struct RecordingEmitter {
        events: Mutex<Vec<HostEvent>>,
    }

    impl HostEventEmitterPort for RecordingEmitter {
        fn emit(&self, event: HostEvent) -> Result<(), EmitError> {
            self.events.lock().unwrap().push(event);
            Ok(())
        }
    }

    #[test]
    fn host_event_port_accepts_all_in_scope_events_without_infra_types() {
        let emitter = RecordingEmitter::default();

        let events = vec![
            // --- Clipboard (5 variants) ---
            HostEvent::Clipboard(ClipboardHostEvent::NewContent {
                entry_id: "entry-1".to_string(),
                preview: "hello".to_string(),
                origin: ClipboardOriginKind::Local,
            }),
            HostEvent::Clipboard(ClipboardHostEvent::InboundError {
                message_id: "msg-1".to_string(),
                origin_device_id: "device-1".to_string(),
                error: "decode failed".to_string(),
            }),
            HostEvent::Clipboard(ClipboardHostEvent::InboundSubscribeRecovered {
                recovered_after_attempts: 2,
            }),
            HostEvent::Clipboard(ClipboardHostEvent::InboundSubscribeError {
                attempt: 1,
                error: "subscribe error".to_string(),
            }),
            HostEvent::Clipboard(ClipboardHostEvent::InboundSubscribeRetry {
                attempt: 2,
                retry_in_ms: 500,
                error: "subscribe retry".to_string(),
            }),
            HostEvent::Clipboard(ClipboardHostEvent::DaemonReconnected),
            // --- PeerDiscovery (2 variants) ---
            HostEvent::PeerDiscovery(PeerDiscoveryHostEvent::Discovered {
                peer_id: "peer-1".to_string(),
                device_name: Some("Desk".to_string()),
                addresses: vec!["/ip4/127.0.0.1/tcp/42000".to_string()],
            }),
            HostEvent::PeerDiscovery(PeerDiscoveryHostEvent::Lost {
                peer_id: "peer-1".to_string(),
                device_name: None,
                addresses: vec![],
            }),
            // --- PeerConnection (3 variants) ---
            HostEvent::PeerConnection(PeerConnectionHostEvent::Connected {
                peer_id: "peer-2".to_string(),
                device_name: Some("Phone".to_string()),
            }),
            HostEvent::PeerConnection(PeerConnectionHostEvent::Disconnected {
                peer_id: "peer-2".to_string(),
                device_name: None,
            }),
            HostEvent::PeerConnection(PeerConnectionHostEvent::NameUpdated {
                peer_id: "peer-3".to_string(),
                device_name: "Updated".to_string(),
            }),
            // --- Transfer (3 variants) ---
            HostEvent::Transfer(TransferHostEvent::Progress(TransferProgress {
                transfer_id: "transfer-1".to_string(),
                peer_id: "peer-4".to_string(),
                direction: TransferDirection::Receiving,
                chunks_completed: 1,
                total_chunks: 3,
                bytes_transferred: 512,
                total_bytes: Some(1_024),
            })),
            HostEvent::Transfer(TransferHostEvent::Completed {
                transfer_id: "transfer-2".to_string(),
                filename: "note.txt".to_string(),
                peer_id: "peer-5".to_string(),
                file_size: 12,
                auto_pulled: false,
                file_path: "/tmp/note.txt".to_string(),
            }),
            HostEvent::Transfer(TransferHostEvent::StatusChanged {
                transfer_id: "transfer-3".to_string(),
                entry_id: "entry-3".to_string(),
                status: "pending".to_string(),
                reason: None,
            }),
            // --- Pairing (3 variants) ---
            HostEvent::Pairing(PairingHostEvent::Verification {
                session_id: "session-1".to_string(),
                kind: PairingVerificationKind::Request,
                peer_id: Some("peer-6".to_string()),
                device_name: Some("Desktop".to_string()),
                code: None,
                local_fingerprint: None,
                peer_fingerprint: None,
                error: None,
            }),
            HostEvent::Pairing(PairingHostEvent::SubscribeFailure {
                attempt: 1,
                retry_in_ms: 250,
                error: "subscribe failed".to_string(),
            }),
            HostEvent::Pairing(PairingHostEvent::SubscribeRecovered {
                recovered_after_attempts: 3,
            }),
            // --- Setup (1 variant) ---
            HostEvent::Setup(SetupHostEvent::StateChanged {
                state: SetupState::Welcome,
                session_id: None,
            }),
            // --- SpaceAccess (2 variants) ---
            HostEvent::SpaceAccess(SpaceAccessHostEvent::Completed {
                session_id: "sa-session-1".to_string(),
                peer_id: "peer-7".to_string(),
                success: true,
                reason: None,
                ts: 1_700_000_000,
            }),
            HostEvent::SpaceAccess(SpaceAccessHostEvent::P2PCompleted {
                session_id: "sa-session-2".to_string(),
                peer_id: "peer-8".to_string(),
                success: false,
                reason: Some("timeout".to_string()),
                ts: 1_700_000_001,
            }),
        ];

        for event in events {
            emitter.emit(event).expect("emit through port");
        }

        assert_eq!(
            emitter.events.lock().unwrap().len(),
            20,
            "all HostEvent variants should be deliverable through the core port"
        );
    }
}
