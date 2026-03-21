//! Pairing-related Tauri commands
//! 配对相关的 Tauri 命令

use crate::bootstrap::AppRuntime;
use crate::bootstrap::DaemonConnectionState;
use crate::commands::error::CommandError;
use crate::commands::record_trace_fields;
use crate::daemon_client::{DaemonPairingRequestError, TauriDaemonPairingClient};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tauri::{Emitter, State};
use tracing::{error, info_span, warn, Instrument};
use uc_app::usecases::LocalDeviceInfo;
use uc_core::network::{ConnectedPeer, DiscoveredPeer, PairedDevice, PairingState};
use uc_core::PeerId;
use uc_platform::ports::observability::TraceMetadata;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct P2PPeerInfo {
    pub peer_id: String,
    pub device_name: Option<String>,
    pub addresses: Vec<String>,
    pub is_paired: bool,
    pub connected: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairedPeer {
    pub peer_id: String,
    pub device_name: String,
    pub shared_secret: Vec<u8>,
    pub paired_at: String,
    pub last_seen: Option<String>,
    pub last_known_addresses: Vec<String>,
    pub connected: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct P2PPairingRequest {
    pub peer_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct P2PPairingResponse {
    pub session_id: String,
    pub success: bool,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct P2PPinVerifyRequest {
    pub session_id: String,
    pub pin_matches: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct P2PCommandErrorEvent {
    command: String,
    message: String,
}

#[derive(Debug, Clone)]
struct PairingCommandErrorContext {
    code: String,
    message: String,
    user_message: String,
}

/// List paired devices
/// 列出已配对设备
#[tauri::command]
pub async fn list_paired_devices(
    runtime: State<'_, Arc<AppRuntime>>,
    _trace: Option<TraceMetadata>,
) -> Result<Vec<PairedPeer>, CommandError> {
    let span = info_span!(
        "command.pairing.list",
        trace_id = tracing::field::Empty,
        trace_ts = tracing::field::Empty,
    );
    record_trace_fields(&span, &_trace);
    async {
        let uc = runtime.usecases().list_paired_devices();
        let devices = uc.execute().await.map_err(|e| {
            tracing::error!(error = %e, "Failed to list paired devices");
            let message = e.to_string();
            emit_command_error(&runtime, "list_paired_devices", &message);
            CommandError::InternalError(message)
        })?;
        let peers: Vec<PairedPeer> = devices
            .into_iter()
            .map(|d| map_paired_device_to_peer(d, None, false))
            .collect();
        Ok(peers)
    }
    .instrument(span)
    .await
}

#[tauri::command]
pub async fn get_local_peer_id(
    runtime: State<'_, Arc<AppRuntime>>,
    _trace: Option<TraceMetadata>,
) -> Result<String, CommandError> {
    let span = info_span!(
        "command.pairing.get_local_peer_id",
        trace_id = tracing::field::Empty,
        trace_ts = tracing::field::Empty,
    );
    record_trace_fields(&span, &_trace);
    async { Ok(runtime.usecases().get_local_peer_id().execute()) }
        .instrument(span)
        .await
}

#[tauri::command]
pub async fn get_local_device_info(
    runtime: State<'_, Arc<AppRuntime>>,
    _trace: Option<TraceMetadata>,
) -> Result<LocalDeviceInfo, CommandError> {
    let span = info_span!(
        "command.pairing.get_local_device_info",
        trace_id = tracing::field::Empty,
        trace_ts = tracing::field::Empty,
    );
    record_trace_fields(&span, &_trace);
    async {
        runtime
            .usecases()
            .get_local_device_info()
            .execute()
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "Failed to get local device info");
                let message = e.to_string();
                emit_command_error(&runtime, "get_local_device_info", &message);
                CommandError::InternalError(message)
            })
    }
    .instrument(span)
    .await
}

#[tauri::command]
pub async fn get_p2p_peers(
    runtime: State<'_, Arc<AppRuntime>>,
    _trace: Option<TraceMetadata>,
) -> Result<Vec<P2PPeerInfo>, CommandError> {
    let span = info_span!(
        "command.pairing.get_p2p_peers",
        trace_id = tracing::field::Empty,
        trace_ts = tracing::field::Empty,
    );
    record_trace_fields(&span, &_trace);
    async {
        let snapshot = runtime
            .usecases()
            .get_p2p_peers_snapshot()
            .execute()
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "Failed to get p2p peers snapshot");
                let message = format!("get_p2p_peers_snapshot: {}", e);
                emit_command_error(&runtime, "get_p2p_peers", &message);
                CommandError::InternalError(e.to_string())
            })?;

        tracing::info!(
            peer_count = snapshot.len(),
            "assembled p2p peer snapshot from shared use case"
        );

        Ok(snapshot
            .into_iter()
            .map(|p| P2PPeerInfo {
                peer_id: p.peer_id,
                device_name: p.device_name,
                addresses: p.addresses,
                is_paired: p.is_paired,
                connected: p.is_connected,
            })
            .collect())
    }
    .instrument(span)
    .await
}

#[tauri::command]
pub async fn get_paired_peers(
    runtime: State<'_, Arc<AppRuntime>>,
    _trace: Option<TraceMetadata>,
) -> Result<Vec<PairedPeer>, CommandError> {
    get_paired_peers_with_status(runtime, _trace).await
}

#[tauri::command]
pub async fn get_paired_peers_with_status(
    runtime: State<'_, Arc<AppRuntime>>,
    _trace: Option<TraceMetadata>,
) -> Result<Vec<PairedPeer>, CommandError> {
    let span = info_span!(
        "command.pairing.get_paired_peers_with_status",
        trace_id = tracing::field::Empty,
        trace_ts = tracing::field::Empty,
    );
    record_trace_fields(&span, &_trace);
    async {
        let snapshot = runtime
            .usecases()
            .get_p2p_peers_snapshot()
            .execute()
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "Failed to get p2p peers snapshot");
                let message = format!("get_p2p_peers_snapshot: {}", e);
                emit_command_error(&runtime, "get_paired_peers_with_status", &message);
                CommandError::InternalError(e.to_string())
            })?;

        tracing::info!(
            paired_peer_count = snapshot.iter().filter(|p| p.is_paired).count(),
            "assembled paired peers with status from shared use case"
        );

        Ok(snapshot
            .into_iter()
            .filter(|p| p.is_paired)
            .map(|p| PairedPeer {
                peer_id: p.peer_id,
                device_name: p
                    .device_name
                    .unwrap_or_else(|| "Unknown Device".to_string()),
                shared_secret: vec![],
                paired_at: "".to_string(), // Not available in snapshot
                last_seen: None,
                last_known_addresses: p.addresses,
                connected: p.is_connected,
            })
            .collect())
    }
    .instrument(span)
    .await
}

fn map_paired_device_to_peer(
    device: PairedDevice,
    discovered_peer: Option<&DiscoveredPeer>,
    connected: bool,
) -> PairedPeer {
    let peer_id = device.peer_id.as_str().to_string();

    // Use persisted device_name as primary, fallback to discovered name, then to "Unknown Device"
    let device_name = if !device.device_name.is_empty() {
        device.device_name.clone()
    } else {
        discovered_peer
            .and_then(|peer| peer.device_name.clone())
            .unwrap_or_else(|| "Unknown Device".to_string())
    };

    let addresses = discovered_peer
        .map(|peer| peer.addresses.clone())
        .unwrap_or_default();

    PairedPeer {
        peer_id,
        device_name,
        shared_secret: vec![],
        paired_at: device.paired_at.to_rfc3339(),
        last_seen: device.last_seen_at.map(|time| time.to_rfc3339()),
        last_known_addresses: addresses,
        connected,
    }
}

/// Update pairing state for a peer
/// 更新对等端配对状态
#[tauri::command]
pub async fn set_pairing_state(
    runtime: State<'_, Arc<AppRuntime>>,
    peer_id: String,
    state: PairingState,
    _trace: Option<TraceMetadata>,
) -> Result<(), CommandError> {
    let span = info_span!(
        "command.pairing.set_state",
        trace_id = tracing::field::Empty,
        trace_ts = tracing::field::Empty,
        peer_id = %peer_id,
    );
    record_trace_fields(&span, &_trace);
    async {
        let uc = runtime.usecases().set_pairing_state();
        uc.execute(PeerId::from(peer_id.as_str()), state)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "Failed to set pairing state");
                let message = e.to_string();
                emit_command_error(&runtime, "set_pairing_state", &message);
                CommandError::InternalError(message)
            })?;
        Ok(())
    }
    .instrument(span)
    .await
}

#[tauri::command]
pub async fn initiate_p2p_pairing(
    request: P2PPairingRequest,
    runtime: State<'_, Arc<AppRuntime>>,
    daemon_connection: State<'_, DaemonConnectionState>,
    _trace: Option<TraceMetadata>,
) -> Result<P2PPairingResponse, CommandError> {
    let span = info_span!(
        "command.pairing.initiate",
        trace_id = tracing::field::Empty,
        trace_ts = tracing::field::Empty,
        peer_id = %request.peer_id,
    );
    record_trace_fields(&span, &_trace);
    async {
        match TauriDaemonPairingClient::new(daemon_connection.inner().clone())
            .initiate_pairing(request.peer_id)
            .await
        {
            Ok(response) => Ok(P2PPairingResponse {
                session_id: response.session_id,
                success: true,
                error: None,
            }),
            Err(error) => {
                let mapped = map_pairing_command_error(&error);
                log_pairing_command_error("initiate_p2p_pairing", &mapped);
                emit_command_error(&runtime, "initiate_p2p_pairing", &mapped.user_message);
                Ok(P2PPairingResponse {
                    session_id: String::new(),
                    success: false,
                    error: Some(mapped.user_message),
                })
            }
        }
    }
    .instrument(span)
    .await
}

#[tauri::command]
pub async fn accept_p2p_pairing(
    session_id: String,
    runtime: State<'_, Arc<AppRuntime>>,
    daemon_connection: State<'_, DaemonConnectionState>,
    _trace: Option<TraceMetadata>,
) -> Result<(), CommandError> {
    let span = info_span!(
        "command.pairing.accept",
        trace_id = tracing::field::Empty,
        trace_ts = tracing::field::Empty,
        session_id = %session_id,
    );
    record_trace_fields(&span, &_trace);
    async {
        TauriDaemonPairingClient::new(daemon_connection.inner().clone())
            .accept_pairing(&session_id)
            .await
            .map_err(|error| {
                let mapped = map_pairing_command_error(&error);
                log_pairing_command_error("accept_p2p_pairing", &mapped);
                emit_command_error(&runtime, "accept_p2p_pairing", &mapped.user_message);
                CommandError::InternalError(mapped.user_message)
            })?;
        Ok(())
    }
    .instrument(span)
    .await
}

#[tauri::command]
pub async fn reject_p2p_pairing(
    session_id: String,
    runtime: State<'_, Arc<AppRuntime>>,
    daemon_connection: State<'_, DaemonConnectionState>,
    _trace: Option<TraceMetadata>,
) -> Result<(), CommandError> {
    let span = info_span!(
        "command.pairing.reject",
        trace_id = tracing::field::Empty,
        trace_ts = tracing::field::Empty,
        session_id = %session_id,
    );
    record_trace_fields(&span, &_trace);
    async {
        TauriDaemonPairingClient::new(daemon_connection.inner().clone())
            .reject_pairing(&session_id)
            .await
            .map_err(|error| {
                let mapped = map_pairing_command_error(&error);
                log_pairing_command_error("reject_p2p_pairing", &mapped);
                emit_command_error(&runtime, "reject_p2p_pairing", &mapped.user_message);
                CommandError::InternalError(mapped.user_message)
            })?;
        Ok(())
    }
    .instrument(span)
    .await
}

#[tauri::command]
pub async fn verify_p2p_pairing_pin(
    request: P2PPinVerifyRequest,
    runtime: State<'_, Arc<AppRuntime>>,
    daemon_connection: State<'_, DaemonConnectionState>,
    _trace: Option<TraceMetadata>,
) -> Result<(), CommandError> {
    let span = info_span!(
        "command.pairing.verify_pin",
        trace_id = tracing::field::Empty,
        trace_ts = tracing::field::Empty,
        session_id = %request.session_id,
    );
    record_trace_fields(&span, &_trace);
    async {
        TauriDaemonPairingClient::new(daemon_connection.inner().clone())
            .verify_pairing(&request.session_id, request.pin_matches)
            .await
            .map_err(|error| {
                let mapped = map_pairing_command_error(&error);
                log_pairing_command_error("verify_p2p_pairing_pin", &mapped);
                emit_command_error(&runtime, "verify_p2p_pairing_pin", &mapped.user_message);
                CommandError::InternalError(mapped.user_message)
            })?;
        Ok(())
    }
    .instrument(span)
    .await
}

#[tauri::command]
pub async fn unpair_p2p_device(
    peer_id: String,
    runtime: State<'_, Arc<AppRuntime>>,
    _trace: Option<TraceMetadata>,
) -> Result<(), CommandError> {
    let span = info_span!(
        "command.pairing.unpair",
        trace_id = tracing::field::Empty,
        trace_ts = tracing::field::Empty,
        peer_id = %peer_id,
    );
    record_trace_fields(&span, &_trace);
    async {
        let uc = runtime.usecases().unpair_device();
        uc.execute(peer_id.clone()).await.map_err(|e| {
            tracing::error!(error = %e, peer_id = %peer_id, "Failed to unpair P2P device");
            let message = e.to_string();
            emit_command_error(&runtime, "unpair_p2p_device", &message);
            CommandError::InternalError(message)
        })
    }
    .instrument(span)
    .await
}

/// Get resolved sync settings for a specific device.
/// Returns per-device overrides if set, otherwise global defaults.
#[tauri::command]
pub async fn get_device_sync_settings(
    runtime: State<'_, Arc<AppRuntime>>,
    peer_id: String,
    _trace: Option<TraceMetadata>,
) -> Result<uc_core::settings::model::SyncSettings, CommandError> {
    let span = info_span!(
        "command.pairing.get_device_sync_settings",
        trace_id = tracing::field::Empty,
        trace_ts = tracing::field::Empty,
        peer_id = %peer_id,
    );
    record_trace_fields(&span, &_trace);
    async {
        let uc = runtime.usecases().get_device_sync_settings();
        uc.execute(&PeerId::from(peer_id.as_str()))
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "Failed to get device sync settings");
                let message = e.to_string();
                emit_command_error(&runtime, "get_device_sync_settings", &message);
                CommandError::InternalError(message)
            })
    }
    .instrument(span)
    .await
}

/// Update or clear per-device sync settings.
/// Passing `null` for settings resets to global defaults.
#[tauri::command]
pub async fn update_device_sync_settings(
    runtime: State<'_, Arc<AppRuntime>>,
    peer_id: String,
    settings: Option<uc_core::settings::model::SyncSettings>,
    _trace: Option<TraceMetadata>,
) -> Result<(), CommandError> {
    let span = info_span!(
        "command.pairing.update_device_sync_settings",
        trace_id = tracing::field::Empty,
        trace_ts = tracing::field::Empty,
        peer_id = %peer_id,
    );
    record_trace_fields(&span, &_trace);
    async {
        let uc = runtime.usecases().update_device_sync_settings();
        uc.execute(&PeerId::from(peer_id.as_str()), settings)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "Failed to update device sync settings");
                let message = e.to_string();
                emit_command_error(&runtime, "update_device_sync_settings", &message);
                CommandError::InternalError(message)
            })
    }
    .instrument(span)
    .await
}

fn emit_command_error(runtime: &AppRuntime, command: &str, message: &str) {
    if let Some(app) = runtime.app_handle().as_ref() {
        let payload = P2PCommandErrorEvent {
            command: command.to_string(),
            message: message.to_string(),
        };
        if let Err(err) = app.emit("p2p-command-error", payload) {
            tracing::warn!(error = %err, command = %command, "Failed to emit p2p command error");
        }
    } else {
        tracing::debug!(
            command = %command,
            "AppHandle not available, skipping p2p command error emission"
        );
    }
}

fn map_pairing_command_error(error: &anyhow::Error) -> PairingCommandErrorContext {
    if let Some(pairing_error) = error.downcast_ref::<DaemonPairingRequestError>() {
        let code = pairing_error
            .code
            .clone()
            .unwrap_or_else(|| "internal".to_string());
        let user_message = match code.as_str() {
            "active_session_exists" => "active pairing session exists".to_string(),
            "no_local_participant" => "no local pairing participant ready".to_string(),
            "host_not_discoverable" => "host not discoverable".to_string(),
            "session_not_found" => "pairing session not found".to_string(),
            _ => pairing_error.message.clone(),
        };
        return PairingCommandErrorContext {
            code,
            message: pairing_error.message.clone(),
            user_message,
        };
    }

    PairingCommandErrorContext {
        code: "internal".to_string(),
        message: error.to_string(),
        user_message: error.to_string(),
    }
}

fn log_pairing_command_error(command: &'static str, mapped: &PairingCommandErrorContext) {
    match mapped.code.as_str() {
        "active_session_exists"
        | "no_local_participant"
        | "host_not_discoverable"
        | "session_not_found" => warn!(
            command,
            code = %mapped.code,
            message = %mapped.message,
            "daemon pairing command returned handled error"
        ),
        _ => error!(
            command,
            code = %mapped.code,
            message = %mapped.message,
            "daemon pairing command failed"
        ),
    }
}

fn discovered_peer_map(peers: &[DiscoveredPeer]) -> HashMap<String, DiscoveredPeer> {
    peers
        .iter()
        .map(|peer| (peer.peer_id.clone(), peer.clone()))
        .collect()
}

fn connected_peer_ids(peers: &[ConnectedPeer]) -> HashMap<String, ConnectedPeer> {
    peers
        .iter()
        .map(|peer| (peer.peer_id.clone(), peer.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uc_core::network::{DiscoveredPeer, PairedDevice, PairingState};
    use uc_core::PeerId;

    #[test]
    fn test_map_paired_device_to_peer_uses_persisted_name() {
        let device = PairedDevice {
            peer_id: PeerId::from("peer-1"),
            pairing_state: PairingState::Trusted,
            identity_fingerprint: "fp".to_string(),
            paired_at: Utc::now(),
            last_seen_at: None,
            device_name: "Persisted Name".to_string(),
            sync_settings: None,
        };

        let discovered = DiscoveredPeer {
            peer_id: "peer-1".to_string(),
            device_name: Some("Discovered Name".to_string()),
            device_id: None,
            addresses: vec!["127.0.0.1:1234".to_string()],
            discovered_at: Utc::now(),
            last_seen: Utc::now(),
            is_paired: true,
        };

        let result = map_paired_device_to_peer(device, Some(&discovered), true);

        assert_eq!(result.device_name, "Persisted Name");
        assert_eq!(result.last_known_addresses, vec!["127.0.0.1:1234"]);
        assert!(result.connected);
    }

    #[test]
    fn test_map_paired_device_to_peer_falls_back_to_discovered_name() {
        let device = PairedDevice {
            peer_id: PeerId::from("peer-1"),
            pairing_state: PairingState::Trusted,
            identity_fingerprint: "fp".to_string(),
            paired_at: Utc::now(),
            last_seen_at: None,
            device_name: "".to_string(),
            sync_settings: None,
        };

        let discovered = DiscoveredPeer {
            peer_id: "peer-1".to_string(),
            device_name: Some("Discovered Name".to_string()),
            device_id: None,
            addresses: vec![],
            discovered_at: Utc::now(),
            last_seen: Utc::now(),
            is_paired: true,
        };

        let result = map_paired_device_to_peer(device, Some(&discovered), false);

        assert_eq!(result.device_name, "Discovered Name");
        assert!(!result.connected);
    }

    #[test]
    fn test_map_paired_device_to_peer_falls_back_to_unknown_device() {
        let device = PairedDevice {
            peer_id: PeerId::from("peer-1"),
            pairing_state: PairingState::Trusted,
            identity_fingerprint: "fp".to_string(),
            paired_at: Utc::now(),
            last_seen_at: None,
            device_name: "".to_string(),
            sync_settings: None,
        };

        let result = map_paired_device_to_peer(device, None, false);

        assert_eq!(result.device_name, "Unknown Device");
    }
}
