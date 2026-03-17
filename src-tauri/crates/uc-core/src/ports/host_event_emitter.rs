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

use crate::ports::transfer_progress::TransferProgress;

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
        ];

        for event in events {
            emitter.emit(event).expect("emit through port");
        }

        assert_eq!(
            emitter.events.lock().unwrap().len(),
            11,
            "all HostEvent variants should be deliverable through the core port"
        );
    }
}
