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
