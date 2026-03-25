//! Host event emitter adapters: Tauri and Logging implementations.
//!
//! # TauriEventEmitter
//!
//! Converts [`HostEvent`] values to Tauri event name strings and serde-annotated
//! payload DTOs, then calls [`tauri::Emitter::emit`] on the `AppHandle`.
//!
//! Internal payload DTOs are module-private — they exist solely to reproduce
//! the existing frontend wire contracts (camelCase JSON, tagged enums, etc.).
//!
//! # LoggingEventEmitter
//!
//! Logs every event via `tracing` structured fields. Always returns `Ok(())`.
//! Used before the Tauri `AppHandle` is available (pre-setup) and in non-GUI modes.

use serde::Serialize;
use tauri::{AppHandle, Emitter, Runtime};
use uc_core::ports::host_event_emitter::{
    ClipboardHostEvent, ClipboardOriginKind, EmitError, HostEvent, HostEventEmitterPort,
    PairingHostEvent, PairingVerificationKind, PeerConnectionHostEvent, PeerDiscoveryHostEvent,
    SetupHostEvent, SpaceAccessHostEvent, TransferHostEvent,
};
use uc_core::ports::realtime::{
    RealtimeFrontendEvent, RealtimeFrontendPayload, RealtimeTopic, FRONTEND_REALTIME_EVENT,
};
use uc_core::setup::SetupState;

// ---------------------------------------------------------------------------
// Internal payload DTOs (module-private)
//
// These types reproduce the exact JSON shapes that the frontend consumes.
// Each struct/enum has the exact serde annotations needed to match the wire
// contract that currently exists at the call sites in wiring.rs and events/*.rs.
// ---------------------------------------------------------------------------

/// Clipboard event payload — uses `#[serde(tag = "type")]` with NO rename_all.
/// Produces snake_case field names, matching the existing ClipboardEvent in events/mod.rs.
/// JSON: { "type": "NewContent", "entry_id": "abc", "preview": "hello", "origin": "local" }
#[derive(Serialize)]
#[serde(tag = "type")]
enum ClipboardEventPayload {
    NewContent {
        entry_id: String,
        preview: String,
        origin: String,
    },
}

/// Inbound clipboard error payload — camelCase fields.
/// Matches InboundClipboardErrorPayload at wiring.rs:217-223.
/// JSON: { "messageId": "...", "originDeviceId": "...", "error": "..." }
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InboundClipboardErrorPayload {
    message_id: String,
    origin_device_id: String,
    error: String,
}

/// Inbound clipboard subscribe recovered payload — camelCase fields.
/// Matches InboundClipboardSubscribeRecoveredPayload at wiring.rs:240-244.
/// JSON: { "recoveredAfterAttempts": 3 }
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InboundClipboardSubscribeRecoveredPayload {
    recovered_after_attempts: u32,
}

/// Peer discovery changed payload — camelCase fields.
/// Matches P2PPeerDiscoveryEvent in events/p2p_peer.rs.
/// JSON: { "peerId": "...", "deviceName": null, "addresses": [...], "discovered": true }
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PeerDiscoveryChangedPayload {
    peer_id: String,
    device_name: Option<String>,
    addresses: Vec<String>,
    discovered: bool,
}

/// Peer connection changed payload — camelCase, NO ready field.
/// Matches P2PPeerConnectionEvent in events/p2p_peer.rs.
/// JSON: { "peerId": "...", "deviceName": "...", "connected": true }
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PeerConnectionChangedPayload {
    peer_id: String,
    device_name: Option<String>,
    connected: bool,
}

/// Peer name updated payload — camelCase fields.
/// Matches P2PPeerNameUpdatedEvent in events/p2p_peer.rs.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PeerNameUpdatedPayload {
    peer_id: String,
    device_name: String,
}

/// Transfer progress payload — camelCase fields.
/// Matches TransferProgressEvent in events/transfer_progress.rs.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TransferProgressPayload {
    transfer_id: String,
    peer_id: String,
    direction: String,
    chunks_completed: u32,
    total_chunks: u32,
    bytes_transferred: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    total_bytes: Option<u64>,
}

/// Transfer completed payload — camelCase fields.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TransferCompletedPayload {
    transfer_id: String,
    filename: String,
    peer_id: String,
    file_size: u64,
    auto_pulled: bool,
    file_path: String,
}

/// Transfer status changed payload — camelCase fields.
/// Matches FileTransferStatusPayload in bootstrap/file_transfer_wiring.rs.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TransferStatusChangedPayload {
    transfer_id: String,
    entry_id: String,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
}

/// Inbound clipboard subscribe error payload — camelCase fields.
/// Matches InboundClipboardSubscribeErrorPayload at wiring.rs:217-222.
/// JSON: { "attempt": 1, "error": "..." }
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InboundClipboardSubscribeErrorPayload {
    attempt: u32,
    error: String,
}

/// Inbound clipboard subscribe retry payload — camelCase fields.
/// Matches InboundClipboardSubscribeRetryPayload at wiring.rs:224-230.
/// JSON: { "attempt": 2, "retryInMs": 500, "error": "..." }
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InboundClipboardSubscribeRetryPayload {
    attempt: u32,
    retry_in_ms: u64,
    error: String,
}

/// Pairing verification payload — camelCase fields, kind as lowercase string.
/// JSON: { "sessionId": "...", "kind": "request", "peerId": null, ... }
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PairingVerificationPayload {
    session_id: String,
    kind: String, // "request"|"verification"|"verifying"|"complete"|"failed"
    #[serde(skip_serializing_if = "Option::is_none")]
    peer_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    device_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    local_fingerprint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    peer_fingerprint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// Pairing subscribe failure payload — camelCase fields.
/// JSON: { "attempt": 1, "retryInMs": 250, "error": "..." }
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PairingSubscribeFailurePayload {
    attempt: u32,
    retry_in_ms: u64,
    error: String,
}

/// Pairing subscribe recovered payload — camelCase fields.
/// JSON: { "recoveredAfterAttempts": 3 }
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PairingSubscribeRecoveredPayload {
    recovered_after_attempts: u32,
}

/// Setup state changed payload — camelCase fields.
/// IMPORTANT: `state` carries the full SetupState enum (not a String).
/// This preserves data-carrying variants like JoinSpaceConfirmPeer { short_code, ... }.
/// Matches SetupStateChangedPayload at wiring.rs:181-186.
/// JSON: { "state": { ... }, "sessionId": "..." }
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SetupStateChangedPayload {
    state: SetupState,
    #[serde(skip_serializing_if = "Option::is_none")]
    session_id: Option<String>,
}

/// Space access completed payload — camelCase fields.
/// IMPORTANT: `peer_id` is String (non-optional) matching the existing wire contract
/// and SpaceAccessCompletedEvent.peer_id: String.
/// Matches SpaceAccessCompletedPayload at wiring.rs:1889-1897.
/// JSON: { "sessionId": "...", "peerId": "...", "success": true, ... }
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SpaceAccessCompletedPayload {
    session_id: String,
    peer_id: String, // MUST be String (non-optional) — matches existing wire contract
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
    ts: i64,
}

// ---------------------------------------------------------------------------
// A helper enum so map_event can return heterogeneous payloads.
// We use erased serialization via serde_json::Value to avoid boxing.
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// TauriEventEmitter
// ---------------------------------------------------------------------------

/// Delivers [`HostEvent`]s to the Tauri frontend via `AppHandle::emit`.
///
/// Constructed only after the `AppHandle` is available. The emitter is
/// thread-safe (AppHandle implements Clone + Send + Sync) and can be
/// wrapped in `Arc<dyn HostEventEmitterPort>`.
pub struct TauriEventEmitter<R: Runtime> {
    app: AppHandle<R>,
}

impl<R: Runtime> TauriEventEmitter<R> {
    pub fn new(app: AppHandle<R>) -> Self {
        Self { app }
    }
}

impl<R: Runtime> HostEventEmitterPort for TauriEventEmitter<R> {
    fn emit(&self, event: HostEvent) -> Result<(), EmitError> {
        let (event_name, payload) = map_event_to_json(event);
        self.app
            .emit(event_name, payload)
            .map_err(|e| EmitError::Failed(e.to_string()))
    }
}

/// Map a [`PairingVerificationKind`] to its lowercase string representation.
///
/// Kind variants are all single words so camelCase = lowercase.
fn kind_to_str(kind: &PairingVerificationKind) -> &'static str {
    match kind {
        PairingVerificationKind::Request => "request",
        PairingVerificationKind::Verification => "verification",
        PairingVerificationKind::Verifying => "verifying",
        PairingVerificationKind::Complete => "complete",
        PairingVerificationKind::Failed => "failed",
    }
}

/// Convert a [`HostEvent`] into a (event_name, JSON payload) pair.
///
/// Event name strings MUST match exactly what the frontend listens on.
fn map_event_to_json(event: HostEvent) -> (&'static str, serde_json::Value) {
    match event {
        // -----------------------------------------------------------------------
        // Clipboard events
        // -----------------------------------------------------------------------
        HostEvent::Clipboard(ClipboardHostEvent::NewContent {
            entry_id,
            preview,
            origin,
        }) => {
            let origin_str = match origin {
                ClipboardOriginKind::Local => "local",
                ClipboardOriginKind::Remote => "remote",
            };
            let payload = ClipboardEventPayload::NewContent {
                entry_id,
                preview,
                origin: origin_str.to_string(),
            };
            (
                "clipboard://event",
                serde_json::to_value(payload).unwrap_or_default(),
            )
        }

        HostEvent::Clipboard(ClipboardHostEvent::InboundError {
            message_id,
            origin_device_id,
            error,
        }) => {
            let payload = InboundClipboardErrorPayload {
                message_id,
                origin_device_id,
                error,
            };
            (
                "inbound-clipboard-error",
                serde_json::to_value(payload).unwrap_or_default(),
            )
        }

        HostEvent::Clipboard(ClipboardHostEvent::InboundSubscribeRecovered {
            recovered_after_attempts,
        }) => {
            let payload = InboundClipboardSubscribeRecoveredPayload {
                recovered_after_attempts,
            };
            (
                "inbound-clipboard-subscribe-recovered",
                serde_json::to_value(payload).unwrap_or_default(),
            )
        }

        // -----------------------------------------------------------------------
        // Peer discovery events
        // -----------------------------------------------------------------------
        HostEvent::PeerDiscovery(PeerDiscoveryHostEvent::Discovered {
            peer_id,
            device_name,
            addresses,
        }) => (
            FRONTEND_REALTIME_EVENT,
            serde_json::json!({
                "topic": "peers",
                "type": "peers.changed",
                "ts": 0,
                "payload": {
                    "peers": [{
                        "peerId": peer_id,
                        "deviceName": device_name,
                        "connected": false,
                        "addresses": addresses,
                        "discovered": true
                    }]
                }
            }),
        ),

        HostEvent::PeerDiscovery(PeerDiscoveryHostEvent::Lost {
            peer_id,
            device_name,
            addresses,
        }) => (
            FRONTEND_REALTIME_EVENT,
            serde_json::json!({
                "topic": "peers",
                "type": "peers.changed",
                "ts": 0,
                "payload": {
                    "peers": [],
                    "removedPeer": {
                        "peerId": peer_id,
                        "deviceName": device_name,
                        "addresses": addresses,
                        "discovered": false
                    }
                }
            }),
        ),

        // -----------------------------------------------------------------------
        // Peer connection events
        // -----------------------------------------------------------------------
        HostEvent::PeerConnection(PeerConnectionHostEvent::Connected {
            peer_id,
            device_name,
        }) => (
            FRONTEND_REALTIME_EVENT,
            serde_json::json!({
                "topic": "peers",
                "type": "peers.connectionChanged",
                "ts": 0,
                "payload": {
                    "peerId": peer_id,
                    "deviceName": device_name,
                    "connected": true
                }
            }),
        ),

        HostEvent::PeerConnection(PeerConnectionHostEvent::Disconnected {
            peer_id,
            device_name,
        }) => (
            FRONTEND_REALTIME_EVENT,
            serde_json::json!({
                "topic": "peers",
                "type": "peers.connectionChanged",
                "ts": 0,
                "payload": {
                    "peerId": peer_id,
                    "deviceName": device_name,
                    "connected": false
                }
            }),
        ),

        HostEvent::PeerConnection(PeerConnectionHostEvent::NameUpdated {
            peer_id,
            device_name,
        }) => (
            FRONTEND_REALTIME_EVENT,
            serde_json::json!({
                "topic": "peers",
                "type": "peers.nameUpdated",
                "ts": 0,
                "payload": {
                    "peerId": peer_id,
                    "deviceName": device_name
                }
            }),
        ),

        // -----------------------------------------------------------------------
        // Transfer events
        // -----------------------------------------------------------------------
        HostEvent::Transfer(TransferHostEvent::Progress(progress)) => {
            let payload = TransferProgressPayload {
                transfer_id: progress.transfer_id,
                peer_id: progress.peer_id,
                direction: format!("{:?}", progress.direction),
                chunks_completed: progress.chunks_completed,
                total_chunks: progress.total_chunks,
                bytes_transferred: progress.bytes_transferred,
                total_bytes: progress.total_bytes,
            };
            (
                "file-transfer://progress",
                serde_json::to_value(payload).unwrap_or_default(),
            )
        }

        HostEvent::Transfer(TransferHostEvent::Completed {
            transfer_id,
            filename,
            peer_id,
            file_size,
            auto_pulled,
            file_path,
        }) => {
            let payload = TransferCompletedPayload {
                transfer_id,
                filename,
                peer_id,
                file_size,
                auto_pulled,
                file_path,
            };
            (
                "file-transfer://completed",
                serde_json::to_value(payload).unwrap_or_default(),
            )
        }

        HostEvent::Transfer(TransferHostEvent::StatusChanged {
            transfer_id,
            entry_id,
            status,
            reason,
        }) => {
            let payload = TransferStatusChangedPayload {
                transfer_id,
                entry_id,
                status,
                reason,
            };
            (
                "file-transfer://status-changed",
                serde_json::to_value(payload).unwrap_or_default(),
            )
        }

        // -----------------------------------------------------------------------
        // Clipboard subscribe events (new variants added in Phase 37)
        // -----------------------------------------------------------------------
        HostEvent::Clipboard(ClipboardHostEvent::InboundSubscribeError { attempt, error }) => {
            let payload = InboundClipboardSubscribeErrorPayload { attempt, error };
            (
                "inbound-clipboard-subscribe-error",
                serde_json::to_value(payload).unwrap_or_default(),
            )
        }

        HostEvent::Clipboard(ClipboardHostEvent::InboundSubscribeRetry {
            attempt,
            retry_in_ms,
            error,
        }) => {
            let payload = InboundClipboardSubscribeRetryPayload {
                attempt,
                retry_in_ms,
                error,
            };
            (
                "inbound-clipboard-subscribe-retry",
                serde_json::to_value(payload).unwrap_or_default(),
            )
        }

        // -----------------------------------------------------------------------
        // Pairing events
        // -----------------------------------------------------------------------
        HostEvent::Pairing(PairingHostEvent::Verification {
            session_id,
            kind,
            peer_id,
            device_name,
            code,
            local_fingerprint,
            peer_fingerprint,
            error,
        }) => {
            let payload = PairingVerificationPayload {
                session_id,
                kind: kind_to_str(&kind).to_string(),
                peer_id,
                device_name,
                code,
                local_fingerprint,
                peer_fingerprint,
                error,
            };
            match kind {
                PairingVerificationKind::Request | PairingVerificationKind::Verifying => (
                    FRONTEND_REALTIME_EVENT,
                    serde_json::json!({
                        "topic": "pairing",
                        "type": "pairing.updated",
                        "ts": 0,
                        "payload": {
                            "sessionId": payload.session_id,
                            "status": payload.kind,
                            "peerId": payload.peer_id,
                            "deviceName": payload.device_name
                        }
                    }),
                ),
                PairingVerificationKind::Verification => (
                    FRONTEND_REALTIME_EVENT,
                    serde_json::json!({
                        "topic": "pairing",
                        "type": "pairing.verificationRequired",
                        "ts": 0,
                        "payload": {
                            "sessionId": payload.session_id,
                            "peerId": payload.peer_id,
                            "deviceName": payload.device_name,
                            "code": payload.code,
                            "localFingerprint": payload.local_fingerprint,
                            "peerFingerprint": payload.peer_fingerprint
                        }
                    }),
                ),
                PairingVerificationKind::Complete => (
                    FRONTEND_REALTIME_EVENT,
                    serde_json::json!({
                        "topic": "pairing",
                        "type": "pairing.complete",
                        "ts": 0,
                        "payload": {
                            "sessionId": payload.session_id,
                            "peerId": payload.peer_id,
                            "deviceName": payload.device_name
                        }
                    }),
                ),
                PairingVerificationKind::Failed => (
                    FRONTEND_REALTIME_EVENT,
                    serde_json::json!({
                        "topic": "pairing",
                        "type": "pairing.failed",
                        "ts": 0,
                        "payload": {
                            "sessionId": payload.session_id,
                            "reason": payload.error
                        }
                    }),
                ),
            }
        }

        HostEvent::Pairing(PairingHostEvent::SubscribeFailure {
            attempt,
            retry_in_ms,
            error,
        }) => {
            let payload = PairingSubscribeFailurePayload {
                attempt,
                retry_in_ms,
                error,
            };
            (
                "pairing-events-subscribe-failure",
                serde_json::to_value(payload).unwrap_or_default(),
            )
        }

        HostEvent::Pairing(PairingHostEvent::SubscribeRecovered {
            recovered_after_attempts,
        }) => {
            let payload = PairingSubscribeRecoveredPayload {
                recovered_after_attempts,
            };
            (
                "pairing-events-subscribe-recovered",
                serde_json::to_value(payload).unwrap_or_default(),
            )
        }

        HostEvent::Realtime(event) => (FRONTEND_REALTIME_EVENT, realtime_event_to_json(event)),

        // -----------------------------------------------------------------------
        // Setup events
        // -----------------------------------------------------------------------
        HostEvent::Setup(SetupHostEvent::StateChanged { state, session_id }) => (
            FRONTEND_REALTIME_EVENT,
            serde_json::json!({
                "topic": "setup",
                "type": "setup.stateChanged",
                "ts": 0,
                "payload": serde_json::to_value(SetupStateChangedPayload { state, session_id }).unwrap_or_default(),
            }),
        ),

        // -----------------------------------------------------------------------
        // SpaceAccess events
        // -----------------------------------------------------------------------
        HostEvent::SpaceAccess(SpaceAccessHostEvent::Completed {
            session_id,
            peer_id,
            success,
            reason,
            ts,
        }) => (
            FRONTEND_REALTIME_EVENT,
            serde_json::json!({
                "topic": "setup",
                "type": "setup.spaceAccessCompleted",
                "ts": ts,
                "payload": serde_json::to_value(SpaceAccessCompletedPayload {
                    session_id,
                    peer_id,
                    success,
                    reason,
                    ts,
                }).unwrap_or_default(),
            }),
        ),

        HostEvent::SpaceAccess(SpaceAccessHostEvent::P2PCompleted {
            session_id,
            peer_id,
            success,
            reason,
            ts,
        }) => (
            FRONTEND_REALTIME_EVENT,
            serde_json::json!({
                "topic": "setup",
                "type": "setup.spaceAccessCompleted",
                "ts": ts,
                "payload": serde_json::to_value(SpaceAccessCompletedPayload {
                    session_id,
                    peer_id,
                    success,
                    reason,
                    ts,
                }).unwrap_or_default(),
            }),
        ),
    }
}

// ---------------------------------------------------------------------------
// LoggingEventEmitter
// ---------------------------------------------------------------------------

/// Delivers [`HostEvent`]s as structured `tracing` log output.
///
/// Always returns `Ok(())` — infallible by design. Used before the Tauri
/// `AppHandle` is available and in non-GUI runtime modes.
pub struct LoggingEventEmitter;

impl HostEventEmitterPort for LoggingEventEmitter {
    fn emit(&self, event: HostEvent) -> Result<(), EmitError> {
        match event {
            HostEvent::Clipboard(ClipboardHostEvent::NewContent {
                ref entry_id,
                ref origin,
                ..
            }) => {
                let origin_str = match origin {
                    ClipboardOriginKind::Local => "local",
                    ClipboardOriginKind::Remote => "remote",
                };
                tracing::info!(
                    event_type = "clipboard.new_content",
                    entry_id = %entry_id,
                    origin = %origin_str,
                );
            }

            HostEvent::Clipboard(ClipboardHostEvent::InboundError {
                ref message_id,
                ref origin_device_id,
                ref error,
            }) => {
                tracing::warn!(
                    event_type = "clipboard.inbound_error",
                    message_id = %message_id,
                    origin_device_id = %origin_device_id,
                    error = %error,
                );
            }

            HostEvent::Clipboard(ClipboardHostEvent::InboundSubscribeRecovered {
                recovered_after_attempts,
            }) => {
                tracing::debug!(
                    event_type = "clipboard.inbound_subscribe_recovered",
                    recovered_after_attempts = recovered_after_attempts,
                );
            }

            HostEvent::PeerDiscovery(PeerDiscoveryHostEvent::Discovered {
                ref peer_id,
                ref device_name,
                ..
            }) => {
                tracing::debug!(
                    event_type = "peer.discovered",
                    peer_id = %peer_id,
                    device_name = ?device_name,
                );
            }

            HostEvent::PeerDiscovery(PeerDiscoveryHostEvent::Lost {
                ref peer_id,
                ref device_name,
                ..
            }) => {
                tracing::debug!(
                    event_type = "peer.lost",
                    peer_id = %peer_id,
                    device_name = ?device_name,
                );
            }

            HostEvent::PeerConnection(PeerConnectionHostEvent::Connected {
                ref peer_id,
                ref device_name,
            }) => {
                tracing::debug!(
                    event_type = "peer.connected",
                    peer_id = %peer_id,
                    device_name = ?device_name,
                );
            }

            HostEvent::PeerConnection(PeerConnectionHostEvent::Disconnected {
                ref peer_id,
                ref device_name,
            }) => {
                tracing::debug!(
                    event_type = "peer.disconnected",
                    peer_id = %peer_id,
                    device_name = ?device_name,
                );
            }

            HostEvent::PeerConnection(PeerConnectionHostEvent::NameUpdated {
                ref peer_id,
                ref device_name,
            }) => {
                tracing::debug!(
                    event_type = "peer.name_updated",
                    peer_id = %peer_id,
                    device_name = %device_name,
                );
            }

            HostEvent::Transfer(TransferHostEvent::Progress(ref progress)) => {
                tracing::debug!(
                    event_type = "transfer.progress",
                    transfer_id = %progress.transfer_id,
                    peer_id = %progress.peer_id,
                    chunks_completed = progress.chunks_completed,
                    total_chunks = progress.total_chunks,
                );
            }

            HostEvent::Transfer(TransferHostEvent::Completed {
                ref transfer_id,
                ref filename,
                ref peer_id,
                file_size,
                auto_pulled,
                ..
            }) => {
                tracing::info!(
                    event_type = "transfer.completed",
                    transfer_id = %transfer_id,
                    filename = %filename,
                    peer_id = %peer_id,
                    file_size = file_size,
                    auto_pulled = auto_pulled,
                );
            }

            HostEvent::Transfer(TransferHostEvent::StatusChanged {
                ref transfer_id,
                ref entry_id,
                ref status,
                ..
            }) => {
                tracing::debug!(
                    event_type = "transfer.status_changed",
                    transfer_id = %transfer_id,
                    entry_id = %entry_id,
                    status = %status,
                );
            }

            HostEvent::Clipboard(ClipboardHostEvent::InboundSubscribeError {
                attempt,
                ref error,
            }) => {
                tracing::warn!(
                    event_type = "clipboard.inbound_subscribe_error",
                    attempt = attempt,
                    error = %error,
                );
            }

            HostEvent::Clipboard(ClipboardHostEvent::InboundSubscribeRetry {
                attempt,
                retry_in_ms,
                ref error,
            }) => {
                tracing::warn!(
                    event_type = "clipboard.inbound_subscribe_retry",
                    attempt = attempt,
                    retry_in_ms = retry_in_ms,
                    error = %error,
                );
            }

            HostEvent::Pairing(PairingHostEvent::Verification {
                ref session_id,
                ref kind,
                ref peer_id,
                ..
            }) => {
                tracing::debug!(
                    event_type = "pairing.verification",
                    session_id = %session_id,
                    kind = %kind_to_str(kind),
                    peer_id = ?peer_id,
                );
            }

            HostEvent::Pairing(PairingHostEvent::SubscribeFailure {
                attempt,
                retry_in_ms,
                ref error,
            }) => {
                tracing::warn!(
                    event_type = "pairing.subscribe_failure",
                    attempt = attempt,
                    retry_in_ms = retry_in_ms,
                    error = %error,
                );
            }

            HostEvent::Pairing(PairingHostEvent::SubscribeRecovered {
                recovered_after_attempts,
            }) => {
                tracing::info!(
                    event_type = "pairing.subscribe_recovered",
                    recovered_after_attempts = recovered_after_attempts,
                );
            }

            HostEvent::Realtime(RealtimeFrontendEvent {
                ref topic,
                ts,
                payload: _,
                ..
            }) => {
                tracing::debug!(
                    event_type = "realtime.event",
                    topic = %realtime_topic_to_str(topic),
                    ts = ts,
                );
            }

            HostEvent::Setup(SetupHostEvent::StateChanged {
                ref state,
                ref session_id,
            }) => {
                tracing::info!(
                    event_type = "setup.state_changed",
                    state = ?state,
                    session_id = ?session_id,
                );
            }

            HostEvent::SpaceAccess(SpaceAccessHostEvent::Completed {
                ref session_id,
                ref peer_id,
                success,
                ..
            }) => {
                tracing::info!(
                    event_type = "space_access.completed",
                    session_id = %session_id,
                    peer_id = %peer_id,
                    success = success,
                );
            }

            HostEvent::SpaceAccess(SpaceAccessHostEvent::P2PCompleted {
                ref session_id,
                ref peer_id,
                success,
                ..
            }) => {
                tracing::info!(
                    event_type = "space_access.p2p_completed",
                    session_id = %session_id,
                    peer_id = %peer_id,
                    success = success,
                );
            }
        }
        Ok(())
    }
}

fn realtime_event_to_json(event: RealtimeFrontendEvent) -> serde_json::Value {
    serde_json::json!({
        "topic": realtime_topic_to_str(&event.topic),
        "type": event.event_type(),
        "ts": event.ts,
        "payload": realtime_payload_to_json(event.payload),
    })
}

fn realtime_topic_to_str(topic: &RealtimeTopic) -> &'static str {
    match topic {
        RealtimeTopic::Pairing => "pairing",
        RealtimeTopic::Peers => "peers",
        RealtimeTopic::PairedDevices => "pairedDevices",
        RealtimeTopic::Setup => "setup",
        RealtimeTopic::SpaceAccess => "spaceAccess",
        RealtimeTopic::Clipboard => "clipboard",
    }
}

fn realtime_payload_to_json(payload: RealtimeFrontendPayload) -> serde_json::Value {
    match payload {
        RealtimeFrontendPayload::PairingUpdated(payload) => serde_json::json!({
            "sessionId": payload.session_id,
            "status": payload.status,
            "peerId": payload.peer_id,
            "deviceName": payload.device_name,
        }),
        RealtimeFrontendPayload::PairingVerificationRequired(payload) => serde_json::json!({
            "sessionId": payload.session_id,
            "peerId": payload.peer_id,
            "deviceName": payload.device_name,
            "code": payload.code,
            "localFingerprint": payload.local_fingerprint,
            "peerFingerprint": payload.peer_fingerprint,
        }),
        RealtimeFrontendPayload::PairingFailed(payload) => serde_json::json!({
            "sessionId": payload.session_id,
            "reason": payload.reason,
        }),
        RealtimeFrontendPayload::PairingComplete(payload) => serde_json::json!({
            "sessionId": payload.session_id,
            "peerId": payload.peer_id,
            "deviceName": payload.device_name,
        }),
        RealtimeFrontendPayload::PeersChanged(payload) => serde_json::json!({
            "peers": payload.peers.into_iter().map(|peer| serde_json::json!({
                "peerId": peer.peer_id,
                "deviceName": peer.device_name,
                "connected": peer.connected,
            })).collect::<Vec<_>>(),
        }),
        RealtimeFrontendPayload::PeersNameUpdated(payload) => serde_json::json!({
            "peerId": payload.peer_id,
            "deviceName": payload.device_name,
        }),
        RealtimeFrontendPayload::PeersConnectionChanged(payload) => serde_json::json!({
            "peerId": payload.peer_id,
            "deviceName": payload.device_name,
            "connected": payload.connected,
        }),
        RealtimeFrontendPayload::PairedDevicesChanged(payload) => serde_json::json!({
            "devices": payload.devices.into_iter().map(|device| serde_json::json!({
                "deviceId": device.device_id,
                "deviceName": device.device_name,
                "lastSeenTs": device.last_seen_ts,
            })).collect::<Vec<_>>(),
        }),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;
    use std::sync::{Arc, Mutex};
    use tauri::Listener;
    use uc_core::ports::transfer_progress::{TransferDirection, TransferProgress};

    #[derive(Clone, Default)]
    struct TestLogBuffer {
        inner: Arc<Mutex<Vec<u8>>>,
    }

    impl TestLogBuffer {
        fn content(&self) -> String {
            String::from_utf8(self.inner.lock().unwrap().clone()).unwrap_or_default()
        }
    }

    struct TestLogWriter {
        inner: Arc<Mutex<Vec<u8>>>,
    }

    impl io::Write for TestLogWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.inner.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl<'a> tracing_subscriber::fmt::writer::MakeWriter<'a> for TestLogBuffer {
        type Writer = TestLogWriter;

        fn make_writer(&'a self) -> Self::Writer {
            TestLogWriter {
                inner: self.inner.clone(),
            }
        }
    }

    // -----------------------------------------------------------------------
    // Contract test 1: clipboard new content — snake_case fields, type tag
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_clipboard_new_content_event_contract() {
        let app = tauri::test::mock_app();
        let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(1);

        app.handle()
            .listen("clipboard://event", move |event: tauri::Event| {
                let _ = tx.try_send(event.payload().to_string());
            });

        let emitter = TauriEventEmitter::new(app.handle().clone());
        emitter
            .emit(HostEvent::Clipboard(ClipboardHostEvent::NewContent {
                entry_id: "entry-abc".to_string(),
                preview: "Hello world".to_string(),
                origin: ClipboardOriginKind::Local,
            }))
            .expect("emit");

        let payload = rx.recv().await.expect("event payload");
        let json: serde_json::Value = serde_json::from_str(&payload).unwrap();

        // Must have "type" tag
        assert_eq!(json["type"], "NewContent");
        // Must use snake_case field names (not camelCase)
        assert_eq!(json["entry_id"], "entry-abc");
        assert_eq!(json["preview"], "Hello world");
        assert_eq!(json["origin"], "local");
        // Ensure camelCase variants are absent
        assert!(json.get("entryId").is_none());
    }

    // -----------------------------------------------------------------------
    // Contract test 2: inbound clipboard error — camelCase fields
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_inbound_clipboard_error_event_contract() {
        let app = tauri::test::mock_app();
        let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(1);

        app.handle()
            .listen("inbound-clipboard-error", move |event: tauri::Event| {
                let _ = tx.try_send(event.payload().to_string());
            });

        let emitter = TauriEventEmitter::new(app.handle().clone());
        emitter
            .emit(HostEvent::Clipboard(ClipboardHostEvent::InboundError {
                message_id: "msg-1".to_string(),
                origin_device_id: "device-xyz".to_string(),
                error: "decode failed".to_string(),
            }))
            .expect("emit");

        let payload = rx.recv().await.expect("event payload");
        let json: serde_json::Value = serde_json::from_str(&payload).unwrap();

        // camelCase keys
        assert_eq!(json["messageId"], "msg-1");
        assert_eq!(json["originDeviceId"], "device-xyz");
        assert_eq!(json["error"], "decode failed");
        // snake_case must be absent
        assert!(json.get("message_id").is_none());
        assert!(json.get("origin_device_id").is_none());
    }

    // -----------------------------------------------------------------------
    // Contract test 3: inbound clipboard subscribe recovered — camelCase
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_inbound_clipboard_subscribe_recovered_event_contract() {
        let app = tauri::test::mock_app();
        let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(1);

        app.handle().listen(
            "inbound-clipboard-subscribe-recovered",
            move |event: tauri::Event| {
                let _ = tx.try_send(event.payload().to_string());
            },
        );

        let emitter = TauriEventEmitter::new(app.handle().clone());
        emitter
            .emit(HostEvent::Clipboard(
                ClipboardHostEvent::InboundSubscribeRecovered {
                    recovered_after_attempts: 3,
                },
            ))
            .expect("emit");

        let payload = rx.recv().await.expect("event payload");
        let json: serde_json::Value = serde_json::from_str(&payload).unwrap();

        assert_eq!(json["recoveredAfterAttempts"], 3);
        assert!(json.get("recovered_after_attempts").is_none());
    }

    // -----------------------------------------------------------------------
    // Contract test 4: peer discovery changed — camelCase, discovered bool
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_peer_discovery_changed_event_contract() {
        let app = tauri::test::mock_app();
        let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(1);

        app.handle()
            .listen(FRONTEND_REALTIME_EVENT, move |event: tauri::Event| {
                if !event.payload().contains("\"type\":\"peers.changed\"") {
                    return;
                }
                let _ = tx.try_send(event.payload().to_string());
            });

        let emitter = TauriEventEmitter::new(app.handle().clone());
        emitter
            .emit(HostEvent::PeerDiscovery(
                PeerDiscoveryHostEvent::Discovered {
                    peer_id: "peer-1".to_string(),
                    device_name: Some("My Device".to_string()),
                    addresses: vec!["192.168.1.1:8080".to_string()],
                },
            ))
            .expect("emit");

        let payload = rx.recv().await.expect("event payload");
        let json: serde_json::Value = serde_json::from_str(&payload).unwrap();

        assert_eq!(json["topic"], "peers");
        assert_eq!(json["type"], "peers.changed");
        assert_eq!(json["payload"]["peers"][0]["peerId"], "peer-1");
        assert_eq!(json["payload"]["peers"][0]["deviceName"], "My Device");
        assert_eq!(
            json["payload"]["peers"][0]["addresses"][0],
            "192.168.1.1:8080"
        );
    }

    // -----------------------------------------------------------------------
    // Contract test 5: peer connection changed — camelCase, connected bool, NO ready
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_peer_connection_changed_event_contract() {
        let app = tauri::test::mock_app();
        let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(1);

        app.handle()
            .listen(FRONTEND_REALTIME_EVENT, move |event: tauri::Event| {
                if !event
                    .payload()
                    .contains("\"type\":\"peers.connectionChanged\"")
                {
                    return;
                }
                let _ = tx.try_send(event.payload().to_string());
            });

        let emitter = TauriEventEmitter::new(app.handle().clone());
        emitter
            .emit(HostEvent::PeerConnection(
                PeerConnectionHostEvent::Connected {
                    peer_id: "peer-2".to_string(),
                    device_name: Some("Laptop".to_string()),
                },
            ))
            .expect("emit");

        let payload = rx.recv().await.expect("event payload");
        let json: serde_json::Value = serde_json::from_str(&payload).unwrap();

        // camelCase
        assert_eq!(json["topic"], "peers");
        assert_eq!(json["type"], "peers.connectionChanged");
        assert_eq!(json["payload"]["peerId"], "peer-2");
        assert_eq!(json["payload"]["deviceName"], "Laptop");
        assert_eq!(json["payload"]["connected"], true);
        assert!(json["payload"].get("ready").is_none());
    }

    // -----------------------------------------------------------------------
    // Contract test 6: peer name updated — camelCase
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_peer_name_updated_event_contract() {
        let app = tauri::test::mock_app();
        let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(1);

        app.handle()
            .listen(FRONTEND_REALTIME_EVENT, move |event: tauri::Event| {
                if !event.payload().contains("\"type\":\"peers.nameUpdated\"") {
                    return;
                }
                let _ = tx.try_send(event.payload().to_string());
            });

        let emitter = TauriEventEmitter::new(app.handle().clone());
        emitter
            .emit(HostEvent::PeerConnection(
                PeerConnectionHostEvent::NameUpdated {
                    peer_id: "peer-3".to_string(),
                    device_name: "New Name".to_string(),
                },
            ))
            .expect("emit");

        let payload = rx.recv().await.expect("event payload");
        let json: serde_json::Value = serde_json::from_str(&payload).unwrap();

        assert_eq!(json["topic"], "peers");
        assert_eq!(json["type"], "peers.nameUpdated");
        assert_eq!(json["payload"]["peerId"], "peer-3");
        assert_eq!(json["payload"]["deviceName"], "New Name");
        assert!(json["payload"].get("peer_id").is_none());
    }

    // -----------------------------------------------------------------------
    // Contract test 7: transfer progress — camelCase
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_transfer_progress_event_contract() {
        let app = tauri::test::mock_app();
        let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(1);

        app.handle()
            .listen("file-transfer://progress", move |event: tauri::Event| {
                let _ = tx.try_send(event.payload().to_string());
            });

        let emitter = TauriEventEmitter::new(app.handle().clone());
        emitter
            .emit(HostEvent::Transfer(TransferHostEvent::Progress(
                TransferProgress {
                    transfer_id: "xfer-1".to_string(),
                    peer_id: "peer-4".to_string(),
                    direction: TransferDirection::Sending,
                    chunks_completed: 2,
                    total_chunks: 5,
                    bytes_transferred: 262_144,
                    total_bytes: Some(655_360),
                },
            )))
            .expect("emit");

        let payload = rx.recv().await.expect("event payload");
        let json: serde_json::Value = serde_json::from_str(&payload).unwrap();

        assert_eq!(json["transferId"], "xfer-1");
        assert_eq!(json["peerId"], "peer-4");
        assert_eq!(json["direction"], "Sending");
        assert_eq!(json["chunksCompleted"], 2);
        assert_eq!(json["totalChunks"], 5);
        assert_eq!(json["bytesTransferred"], 262_144);
        assert_eq!(json["totalBytes"], 655_360);
        assert!(json.get("transfer_id").is_none());
    }

    // -----------------------------------------------------------------------
    // Contract test 8: transfer completed — camelCase
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_transfer_completed_event_contract() {
        let app = tauri::test::mock_app();
        let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(1);

        app.handle()
            .listen("file-transfer://completed", move |event: tauri::Event| {
                let _ = tx.try_send(event.payload().to_string());
            });

        let emitter = TauriEventEmitter::new(app.handle().clone());
        emitter
            .emit(HostEvent::Transfer(TransferHostEvent::Completed {
                transfer_id: "xfer-2".to_string(),
                filename: "document.pdf".to_string(),
                peer_id: "peer-5".to_string(),
                file_size: 1_048_576,
                auto_pulled: true,
                file_path: "/tmp/document.pdf".to_string(),
            }))
            .expect("emit");

        let payload = rx.recv().await.expect("event payload");
        let json: serde_json::Value = serde_json::from_str(&payload).unwrap();

        assert_eq!(json["transferId"], "xfer-2");
        assert_eq!(json["filename"], "document.pdf");
        assert_eq!(json["peerId"], "peer-5");
        assert_eq!(json["fileSize"], 1_048_576);
        assert_eq!(json["autoPulled"], true);
        assert_eq!(json["filePath"], "/tmp/document.pdf");
        assert!(json.get("transfer_id").is_none());
    }

    // -----------------------------------------------------------------------
    // Contract test 9: transfer status changed — camelCase
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_transfer_status_changed_event_contract() {
        let app = tauri::test::mock_app();
        let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(1);

        app.handle().listen(
            "file-transfer://status-changed",
            move |event: tauri::Event| {
                let _ = tx.try_send(event.payload().to_string());
            },
        );

        let emitter = TauriEventEmitter::new(app.handle().clone());
        emitter
            .emit(HostEvent::Transfer(TransferHostEvent::StatusChanged {
                transfer_id: "xfer-3".to_string(),
                entry_id: "entry-1".to_string(),
                status: "failed".to_string(),
                reason: Some("timeout".to_string()),
            }))
            .expect("emit");

        let payload = rx.recv().await.expect("event payload");
        let json: serde_json::Value = serde_json::from_str(&payload).unwrap();

        assert_eq!(json["transferId"], "xfer-3");
        assert_eq!(json["entryId"], "entry-1");
        assert_eq!(json["status"], "failed");
        assert_eq!(json["reason"], "timeout");
        assert!(json.get("transfer_id").is_none());
    }

    // -----------------------------------------------------------------------
    // Contract test 10: LoggingEventEmitter always returns Ok
    // -----------------------------------------------------------------------

    #[test]
    fn test_logging_emitter_always_returns_ok() {
        let emitter = LoggingEventEmitter;
        use uc_core::ports::host_event_emitter::{
            PairingHostEvent, PairingVerificationKind, SetupHostEvent, SpaceAccessHostEvent,
        };
        use uc_core::ports::transfer_progress::{TransferDirection, TransferProgress};
        use uc_core::setup::SetupState;

        let events = vec![
            HostEvent::Clipboard(ClipboardHostEvent::NewContent {
                entry_id: "e1".to_string(),
                preview: "test".to_string(),
                origin: ClipboardOriginKind::Local,
            }),
            HostEvent::Clipboard(ClipboardHostEvent::InboundError {
                message_id: "m1".to_string(),
                origin_device_id: "d1".to_string(),
                error: "err".to_string(),
            }),
            HostEvent::Clipboard(ClipboardHostEvent::InboundSubscribeRecovered {
                recovered_after_attempts: 2,
            }),
            HostEvent::Clipboard(ClipboardHostEvent::InboundSubscribeError {
                attempt: 1,
                error: "sub error".to_string(),
            }),
            HostEvent::Clipboard(ClipboardHostEvent::InboundSubscribeRetry {
                attempt: 2,
                retry_in_ms: 500,
                error: "sub retry".to_string(),
            }),
            HostEvent::PeerDiscovery(PeerDiscoveryHostEvent::Discovered {
                peer_id: "p1".to_string(),
                device_name: None,
                addresses: vec![],
            }),
            HostEvent::PeerDiscovery(PeerDiscoveryHostEvent::Lost {
                peer_id: "p2".to_string(),
                device_name: None,
                addresses: vec![],
            }),
            HostEvent::PeerConnection(PeerConnectionHostEvent::Connected {
                peer_id: "p3".to_string(),
                device_name: None,
            }),
            HostEvent::PeerConnection(PeerConnectionHostEvent::Disconnected {
                peer_id: "p4".to_string(),
                device_name: None,
            }),
            HostEvent::PeerConnection(PeerConnectionHostEvent::NameUpdated {
                peer_id: "p5".to_string(),
                device_name: "Dev".to_string(),
            }),
            HostEvent::Transfer(TransferHostEvent::Progress(TransferProgress {
                transfer_id: "t1".to_string(),
                peer_id: "p6".to_string(),
                direction: TransferDirection::Receiving,
                chunks_completed: 1,
                total_chunks: 4,
                bytes_transferred: 100,
                total_bytes: None,
            })),
            HostEvent::Transfer(TransferHostEvent::Completed {
                transfer_id: "t2".to_string(),
                filename: "file.txt".to_string(),
                peer_id: "p7".to_string(),
                file_size: 500,
                auto_pulled: false,
                file_path: "/tmp/file.txt".to_string(),
            }),
            HostEvent::Transfer(TransferHostEvent::StatusChanged {
                transfer_id: "t3".to_string(),
                entry_id: "e2".to_string(),
                status: "pending".to_string(),
                reason: None,
            }),
            HostEvent::Pairing(PairingHostEvent::Verification {
                session_id: "s1".to_string(),
                kind: PairingVerificationKind::Request,
                peer_id: Some("p8".to_string()),
                device_name: None,
                code: None,
                local_fingerprint: None,
                peer_fingerprint: None,
                error: None,
            }),
            HostEvent::Pairing(PairingHostEvent::SubscribeFailure {
                attempt: 1,
                retry_in_ms: 250,
                error: "pairing sub fail".to_string(),
            }),
            HostEvent::Pairing(PairingHostEvent::SubscribeRecovered {
                recovered_after_attempts: 2,
            }),
            HostEvent::Setup(SetupHostEvent::StateChanged {
                state: SetupState::Welcome,
                session_id: None,
            }),
            HostEvent::SpaceAccess(SpaceAccessHostEvent::Completed {
                session_id: "sa1".to_string(),
                peer_id: "p9".to_string(),
                success: true,
                reason: None,
                ts: 1_700_000_000,
            }),
            HostEvent::SpaceAccess(SpaceAccessHostEvent::P2PCompleted {
                session_id: "sa2".to_string(),
                peer_id: "p10".to_string(),
                success: false,
                reason: Some("timeout".to_string()),
                ts: 1_700_000_001,
            }),
        ];

        for event in events {
            let result = emitter.emit(event);
            assert!(result.is_ok(), "LoggingEventEmitter must always return Ok");
        }
    }

    #[test]
    fn test_logging_emitter_writes_structured_tracing_fields() {
        let log_buffer = TestLogBuffer::default();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(log_buffer.clone())
            .with_max_level(tracing::Level::DEBUG)
            .with_ansi(false)
            .without_time()
            .finish();
        let _guard = tracing::subscriber::set_default(subscriber);

        let emitter = LoggingEventEmitter;
        emitter
            .emit(HostEvent::Clipboard(ClipboardHostEvent::InboundError {
                message_id: "msg-42".to_string(),
                origin_device_id: "device-7".to_string(),
                error: "decrypt failed".to_string(),
            }))
            .expect("emit should succeed");
        emitter
            .emit(HostEvent::Transfer(TransferHostEvent::Completed {
                transfer_id: "transfer-9".to_string(),
                filename: "report.pdf".to_string(),
                peer_id: "peer-11".to_string(),
                file_size: 4096,
                auto_pulled: true,
                file_path: "/tmp/report.pdf".to_string(),
            }))
            .expect("emit should succeed");

        let logs = log_buffer.content();
        assert!(logs.contains("event_type=\"clipboard.inbound_error\""));
        assert!(logs.contains("message_id=msg-42"));
        assert!(logs.contains("origin_device_id=device-7"));
        assert!(logs.contains("error=decrypt failed"));
        assert!(logs.contains("WARN"));
        assert!(logs.contains("event_type=\"transfer.completed\""));
        assert!(logs.contains("transfer_id=transfer-9"));
        assert!(logs.contains("filename=report.pdf"));
        assert!(logs.contains("peer_id=peer-11"));
        assert!(logs.contains("file_size=4096"));
        assert!(logs.contains("auto_pulled=true"));
        assert!(logs.contains("INFO"));
    }

    // -----------------------------------------------------------------------
    // Contract test 11: pairing verification request — camelCase, kind="request"
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_pairing_verification_request_event_contract() {
        use uc_core::ports::host_event_emitter::{PairingHostEvent, PairingVerificationKind};

        let app = tauri::test::mock_app();
        let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(1);

        app.handle()
            .listen(FRONTEND_REALTIME_EVENT, move |event: tauri::Event| {
                if !event.payload().contains("\"topic\":\"pairing\"") {
                    return;
                }
                let _ = tx.try_send(event.payload().to_string());
            });

        let emitter = TauriEventEmitter::new(app.handle().clone());
        emitter
            .emit(HostEvent::Pairing(PairingHostEvent::Verification {
                session_id: "session-42".to_string(),
                kind: PairingVerificationKind::Request,
                peer_id: Some("peer-a".to_string()),
                device_name: Some("Desktop A".to_string()),
                code: None,
                local_fingerprint: None,
                peer_fingerprint: None,
                error: None,
            }))
            .expect("emit");

        let payload = rx.recv().await.expect("event payload");
        let json: serde_json::Value = serde_json::from_str(&payload).unwrap();

        // camelCase keys
        assert_eq!(json["topic"], "pairing");
        assert_eq!(json["type"], "pairing.updated");
        assert_eq!(json["payload"]["sessionId"], "session-42");
        assert_eq!(json["payload"]["status"], "request");
        assert_eq!(json["payload"]["peerId"], "peer-a");
        assert_eq!(json["payload"]["deviceName"], "Desktop A");
        assert!(json["payload"].get("session_id").is_none());
        assert!(json["payload"].get("peer_id").is_none());
        assert!(json["payload"].get("device_name").is_none());
        assert!(json["payload"].get("code").is_none());
        assert!(json["payload"].get("localFingerprint").is_none());
        assert!(json["payload"].get("peerFingerprint").is_none());
        assert!(json["payload"].get("error").is_none());
    }

    // -----------------------------------------------------------------------
    // Contract test 12: pairing verification failed — error field present
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_pairing_verification_failed_event_contract() {
        use uc_core::ports::host_event_emitter::{PairingHostEvent, PairingVerificationKind};

        let app = tauri::test::mock_app();
        let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(1);

        app.handle()
            .listen(FRONTEND_REALTIME_EVENT, move |event: tauri::Event| {
                if !event.payload().contains("\"type\":\"pairing.failed\"") {
                    return;
                }
                let _ = tx.try_send(event.payload().to_string());
            });

        let emitter = TauriEventEmitter::new(app.handle().clone());
        emitter
            .emit(HostEvent::Pairing(PairingHostEvent::Verification {
                session_id: "session-43".to_string(),
                kind: PairingVerificationKind::Failed,
                peer_id: None,
                device_name: None,
                code: None,
                local_fingerprint: None,
                peer_fingerprint: None,
                error: Some("verification timed out".to_string()),
            }))
            .expect("emit");

        let payload = rx.recv().await.expect("event payload");
        let json: serde_json::Value = serde_json::from_str(&payload).unwrap();

        assert_eq!(json["topic"], "pairing");
        assert_eq!(json["type"], "pairing.failed");
        assert_eq!(json["payload"]["sessionId"], "session-43");
        assert_eq!(json["payload"]["reason"], "verification timed out");
        assert!(json["payload"].get("peerId").is_none());
    }

    // -----------------------------------------------------------------------
    // Contract test 13: pairing subscribe failure — camelCase, retryInMs
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_pairing_subscribe_failure_event_contract() {
        use uc_core::ports::host_event_emitter::PairingHostEvent;

        let app = tauri::test::mock_app();
        let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(1);

        app.handle().listen(
            "pairing-events-subscribe-failure",
            move |event: tauri::Event| {
                let _ = tx.try_send(event.payload().to_string());
            },
        );

        let emitter = TauriEventEmitter::new(app.handle().clone());
        emitter
            .emit(HostEvent::Pairing(PairingHostEvent::SubscribeFailure {
                attempt: 3,
                retry_in_ms: 2000,
                error: "connection refused".to_string(),
            }))
            .expect("emit");

        let payload = rx.recv().await.expect("event payload");
        let json: serde_json::Value = serde_json::from_str(&payload).unwrap();

        assert_eq!(json["attempt"], 3);
        assert_eq!(json["retryInMs"], 2000);
        assert_eq!(json["error"], "connection refused");
        // snake_case must be absent
        assert!(json.get("retry_in_ms").is_none());
    }

    // -----------------------------------------------------------------------
    // Contract test 14: pairing subscribe recovered — camelCase, recoveredAfterAttempts
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_pairing_subscribe_recovered_event_contract() {
        use uc_core::ports::host_event_emitter::PairingHostEvent;

        let app = tauri::test::mock_app();
        let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(1);

        app.handle().listen(
            "pairing-events-subscribe-recovered",
            move |event: tauri::Event| {
                let _ = tx.try_send(event.payload().to_string());
            },
        );

        let emitter = TauriEventEmitter::new(app.handle().clone());
        emitter
            .emit(HostEvent::Pairing(PairingHostEvent::SubscribeRecovered {
                recovered_after_attempts: 5,
            }))
            .expect("emit");

        let payload = rx.recv().await.expect("event payload");
        let json: serde_json::Value = serde_json::from_str(&payload).unwrap();

        assert_eq!(json["recoveredAfterAttempts"], 5);
        assert!(json.get("recovered_after_attempts").is_none());
    }

    // -----------------------------------------------------------------------
    // Contract test 15: setup state changed — state is object (not string), sessionId camelCase
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_setup_state_changed_event_contract() {
        use uc_core::ports::host_event_emitter::SetupHostEvent;
        use uc_core::setup::SetupState;

        let app = tauri::test::mock_app();
        let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(1);

        app.handle()
            .listen(FRONTEND_REALTIME_EVENT, move |event: tauri::Event| {
                if !event.payload().contains("\"type\":\"setup.stateChanged\"") {
                    return;
                }
                let _ = tx.try_send(event.payload().to_string());
            });

        let emitter = TauriEventEmitter::new(app.handle().clone());
        // Use data-carrying variant to verify object serialization (not just string)
        emitter
            .emit(HostEvent::Setup(SetupHostEvent::StateChanged {
                state: SetupState::JoinSpaceConfirmPeer {
                    short_code: "1234".to_string(),
                    peer_fingerprint: Some("fp-abc".to_string()),
                    error: None,
                },
                session_id: Some("session-setup-1".to_string()),
            }))
            .expect("emit");

        let payload = rx.recv().await.expect("event payload");
        let json: serde_json::Value = serde_json::from_str(&payload).unwrap();

        assert_eq!(json["topic"], "setup");
        assert_eq!(json["type"], "setup.stateChanged");
        assert!(json["payload"]["state"].is_object());
        assert_eq!(json["payload"]["sessionId"], "session-setup-1");
        assert!(json["payload"].get("session_id").is_none());
    }

    // -----------------------------------------------------------------------
    // Contract test 16: space-access-completed — peerId is String (not null), camelCase
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_space_access_completed_event_contract() {
        use uc_core::ports::host_event_emitter::SpaceAccessHostEvent;

        let app = tauri::test::mock_app();
        let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(1);

        app.handle()
            .listen(FRONTEND_REALTIME_EVENT, move |event: tauri::Event| {
                if !event
                    .payload()
                    .contains("\"type\":\"setup.spaceAccessCompleted\"")
                {
                    return;
                }
                let _ = tx.try_send(event.payload().to_string());
            });

        let emitter = TauriEventEmitter::new(app.handle().clone());
        emitter
            .emit(HostEvent::SpaceAccess(SpaceAccessHostEvent::Completed {
                session_id: "sa-session-1".to_string(),
                peer_id: "peer-xyz".to_string(),
                success: true,
                reason: None,
                ts: 1_700_000_000,
            }))
            .expect("emit");

        let payload = rx.recv().await.expect("event payload");
        let json: serde_json::Value = serde_json::from_str(&payload).unwrap();

        assert_eq!(json["topic"], "setup");
        assert_eq!(json["type"], "setup.spaceAccessCompleted");
        assert_eq!(json["payload"]["sessionId"], "sa-session-1");
        assert!(json["payload"]["peerId"].is_string());
        assert_eq!(json["payload"]["peerId"], "peer-xyz");
        assert_eq!(json["payload"]["success"], true);
        assert_eq!(json["ts"], 1_700_000_000_i64);
        assert!(json["payload"].get("reason").is_none());
        assert!(json["payload"].get("session_id").is_none());
        assert!(json["payload"].get("peer_id").is_none());
    }

    // -----------------------------------------------------------------------
    // Contract test 17: p2p-space-access-completed — correct event name
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_p2p_space_access_completed_event_contract() {
        use uc_core::ports::host_event_emitter::SpaceAccessHostEvent;

        let app = tauri::test::mock_app();
        let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(1);

        app.handle()
            .listen(FRONTEND_REALTIME_EVENT, move |event: tauri::Event| {
                if !event
                    .payload()
                    .contains("\"type\":\"setup.spaceAccessCompleted\"")
                {
                    return;
                }
                let _ = tx.try_send(event.payload().to_string());
            });

        let emitter = TauriEventEmitter::new(app.handle().clone());
        emitter
            .emit(HostEvent::SpaceAccess(SpaceAccessHostEvent::P2PCompleted {
                session_id: "sa-p2p-1".to_string(),
                peer_id: "peer-p2p".to_string(),
                success: false,
                reason: Some("peer rejected".to_string()),
                ts: 1_700_000_002,
            }))
            .expect("emit");

        let payload = rx.recv().await.expect("event payload");
        let json: serde_json::Value = serde_json::from_str(&payload).unwrap();

        assert_eq!(json["topic"], "setup");
        assert_eq!(json["type"], "setup.spaceAccessCompleted");
        assert_eq!(json["payload"]["sessionId"], "sa-p2p-1");
        assert_eq!(json["payload"]["peerId"], "peer-p2p");
        assert_eq!(json["payload"]["success"], false);
        assert_eq!(json["payload"]["reason"], "peer rejected");
    }
}
