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
    PeerConnectionHostEvent, PeerDiscoveryHostEvent, TransferHostEvent,
};

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
        }) => {
            let payload = PeerDiscoveryChangedPayload {
                peer_id,
                device_name,
                addresses,
                discovered: true,
            };
            (
                "p2p-peer-discovery-changed",
                serde_json::to_value(payload).unwrap_or_default(),
            )
        }

        HostEvent::PeerDiscovery(PeerDiscoveryHostEvent::Lost {
            peer_id,
            device_name,
            addresses,
        }) => {
            let payload = PeerDiscoveryChangedPayload {
                peer_id,
                device_name,
                addresses,
                discovered: false,
            };
            (
                "p2p-peer-discovery-changed",
                serde_json::to_value(payload).unwrap_or_default(),
            )
        }

        // -----------------------------------------------------------------------
        // Peer connection events
        // -----------------------------------------------------------------------
        HostEvent::PeerConnection(PeerConnectionHostEvent::Connected {
            peer_id,
            device_name,
        }) => {
            let payload = PeerConnectionChangedPayload {
                peer_id,
                device_name,
                connected: true,
            };
            (
                "p2p-peer-connection-changed",
                serde_json::to_value(payload).unwrap_or_default(),
            )
        }

        HostEvent::PeerConnection(PeerConnectionHostEvent::Disconnected {
            peer_id,
            device_name,
        }) => {
            let payload = PeerConnectionChangedPayload {
                peer_id,
                device_name,
                connected: false,
            };
            (
                "p2p-peer-connection-changed",
                serde_json::to_value(payload).unwrap_or_default(),
            )
        }

        HostEvent::PeerConnection(PeerConnectionHostEvent::NameUpdated {
            peer_id,
            device_name,
        }) => {
            let payload = PeerNameUpdatedPayload {
                peer_id,
                device_name,
            };
            (
                "p2p-peer-name-updated",
                serde_json::to_value(payload).unwrap_or_default(),
            )
        }

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
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tauri::Listener;
    use uc_core::ports::transfer_progress::{TransferDirection, TransferProgress};

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
            .listen("p2p-peer-discovery-changed", move |event: tauri::Event| {
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

        assert_eq!(json["peerId"], "peer-1");
        assert_eq!(json["deviceName"], "My Device");
        assert_eq!(json["addresses"][0], "192.168.1.1:8080");
        assert_eq!(json["discovered"], true);
        assert!(json.get("peer_id").is_none());
    }

    // -----------------------------------------------------------------------
    // Contract test 5: peer connection changed — camelCase, connected bool, NO ready
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_peer_connection_changed_event_contract() {
        let app = tauri::test::mock_app();
        let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(1);

        app.handle()
            .listen("p2p-peer-connection-changed", move |event: tauri::Event| {
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
        assert_eq!(json["peerId"], "peer-2");
        assert_eq!(json["deviceName"], "Laptop");
        assert_eq!(json["connected"], true);
        // Must NOT have "ready" field
        assert!(json.get("ready").is_none());
        // Must NOT have "type" discriminator
        assert!(json.get("type").is_none());
    }

    // -----------------------------------------------------------------------
    // Contract test 6: peer name updated — camelCase
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_peer_name_updated_event_contract() {
        let app = tauri::test::mock_app();
        let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(1);

        app.handle()
            .listen("p2p-peer-name-updated", move |event: tauri::Event| {
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

        assert_eq!(json["peerId"], "peer-3");
        assert_eq!(json["deviceName"], "New Name");
        assert!(json.get("peer_id").is_none());
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
        use uc_core::ports::transfer_progress::{TransferDirection, TransferProgress};

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
        ];

        for event in events {
            let result = emitter.emit(event);
            assert!(result.is_ok(), "LoggingEventEmitter must always return Ok");
        }
    }
}
