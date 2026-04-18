//! Host event emitter port — abstract event delivery for background tasks.
//!
//! This module defines the [`HostEventEmitterPort`] trait and the [`HostEvent`]
//! type system that background tasks use to deliver events to the host environment.
//!
//! The port is intentionally free of Tauri, serde, and any infrastructure
//! dependency. Adapters (e.g., `DaemonApiEventEmitter`) own the serialization
//! contract and event name mapping.
//!
//! # Design
//!
//! - [`HostEvent`] is a pure semantic model — no serde annotations.
//! - [`HostEventEmitterPort`] is synchronous (fire-and-forget semantics).
//! - Emit failures are best-effort: callers log the error and continue.

use uc_application::setup::SetupState;
use uc_core::file_transfer::FileTransferDirection;

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
}

// ---------------------------------------------------------------------------
// TransferHostEvent
// ---------------------------------------------------------------------------

/// Semantic events emitted by the file transfer subsystem.
#[derive(Debug, Clone)]
pub enum TransferHostEvent {
    /// The status of a transfer entry changed.
    StatusChanged {
        transfer_id: String,
        entry_id: String,
        status: String,
        reason: Option<String>,
    },
    /// Continuous transfer progress update.
    Progress {
        transfer_id: String,
        entry_id: Option<String>,
        peer_id: String,
        direction: FileTransferDirection,
        bytes_transferred: u64,
        total_bytes: Option<u64>,
    },
}

// ---------------------------------------------------------------------------
// SetupHostEvent
// ---------------------------------------------------------------------------

/// Semantic events emitted by the setup subsystem.
#[derive(Debug, Clone)]
pub enum SetupHostEvent {
    /// The setup wizard state changed.
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
    Transfer(TransferHostEvent),
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
/// - `DaemonApiEventEmitter` — broadcasts via daemon WebSocket.
/// - `LoggingEventEmitter` — writes structured `tracing` output, always returns `Ok`.
///
/// The trait is synchronous — event delivery is fire-and-forget.
pub trait HostEventEmitterPort: Send + Sync {
    /// Deliver a host event to the runtime environment.
    ///
    /// On failure, the error is returned for the caller to log. The caller
    /// **must not** propagate the error as a business-logic failure.
    fn emit(&self, event: HostEvent) -> Result<(), EmitError>;
}
