//! Setup-related Tauri commands
//! 设置流程相关的 Tauri 命令

use crate::bootstrap::DaemonConnectionState;
use crate::commands::error::CommandError;
use crate::commands::record_trace_fields;
use crate::daemon_client::TauriDaemonSetupClient;
use tauri::State;
use tracing::{info_span, Instrument};
use uc_core::setup::{SetupError, SetupState};
use uc_platform::ports::observability::TraceMetadata;

fn deserialize_setup_state(value: serde_json::Value) -> Result<SetupState, CommandError> {
    serde_json::from_value::<SetupState>(value).map_err(CommandError::internal)
}

fn daemon_setup_client(
    daemon_connection: &State<'_, DaemonConnectionState>,
) -> TauriDaemonSetupClient {
    TauriDaemonSetupClient::new(daemon_connection.inner().clone())
}

/// Get current setup state.
/// 获取当前设置流程状态。
#[tauri::command]
pub async fn get_setup_state(
    daemon_connection: State<'_, DaemonConnectionState>,
    _trace: Option<TraceMetadata>,
) -> Result<SetupState, CommandError> {
    let span = info_span!(
        "command.setup.get_state",
        trace_id = tracing::field::Empty,
        trace_ts = tracing::field::Empty,
    );
    record_trace_fields(&span, &_trace);
    async {
        let response = daemon_setup_client(&daemon_connection)
            .get_setup_state()
            .await
            .map_err(CommandError::internal)?;
        deserialize_setup_state(response.state)
    }
    .instrument(span)
    .await
}

#[tauri::command]
pub async fn start_new_space(
    daemon_connection: State<'_, DaemonConnectionState>,
    _trace: Option<TraceMetadata>,
) -> Result<SetupState, CommandError> {
    let span = info_span!(
        "command.setup.start_new_space",
        trace_id = tracing::field::Empty,
        trace_ts = tracing::field::Empty,
    );
    record_trace_fields(&span, &_trace);
    async {
        let response = daemon_setup_client(&daemon_connection)
            .start_new_space()
            .await
            .map_err(CommandError::internal)?;
        deserialize_setup_state(response.state)
    }
    .instrument(span)
    .await
}

#[tauri::command]
pub async fn start_join_space(
    daemon_connection: State<'_, DaemonConnectionState>,
    _trace: Option<TraceMetadata>,
) -> Result<SetupState, CommandError> {
    let span = info_span!(
        "command.setup.start_join_space",
        trace_id = tracing::field::Empty,
        trace_ts = tracing::field::Empty,
    );
    record_trace_fields(&span, &_trace);
    async {
        let response = daemon_setup_client(&daemon_connection)
            .start_join_space()
            .await
            .map_err(CommandError::internal)?;
        deserialize_setup_state(response.state)
    }
    .instrument(span)
    .await
}

#[tauri::command]
pub async fn select_device(
    daemon_connection: State<'_, DaemonConnectionState>,
    peer_id: String,
    _trace: Option<TraceMetadata>,
) -> Result<SetupState, CommandError> {
    let span = info_span!(
        "command.setup.select_device",
        trace_id = tracing::field::Empty,
        trace_ts = tracing::field::Empty,
    );
    record_trace_fields(&span, &_trace);
    async {
        let response = daemon_setup_client(&daemon_connection)
            .select_device(peer_id)
            .await
            .map_err(CommandError::internal)?;
        deserialize_setup_state(response.state)
    }
    .instrument(span)
    .await
}

#[tauri::command]
pub async fn submit_passphrase(
    daemon_connection: State<'_, DaemonConnectionState>,
    passphrase1: String,
    passphrase2: String,
    _trace: Option<TraceMetadata>,
) -> Result<SetupState, CommandError> {
    let span = info_span!(
        "command.setup.submit_passphrase",
        trace_id = tracing::field::Empty,
        trace_ts = tracing::field::Empty,
    );
    record_trace_fields(&span, &_trace);
    async {
        if passphrase1 != passphrase2 {
            return Ok(SetupState::CreateSpaceInputPassphrase {
                error: Some(SetupError::PassphraseMismatch),
            });
        }

        let response = daemon_setup_client(&daemon_connection)
            .submit_passphrase(passphrase1)
            .await
            .map_err(CommandError::internal)?;
        deserialize_setup_state(response.state)
    }
    .instrument(span)
    .await
}

#[tauri::command]
pub async fn verify_passphrase(
    daemon_connection: State<'_, DaemonConnectionState>,
    passphrase: String,
    _trace: Option<TraceMetadata>,
) -> Result<SetupState, CommandError> {
    let span = info_span!(
        "command.setup.verify_passphrase",
        trace_id = tracing::field::Empty,
        trace_ts = tracing::field::Empty,
    );
    record_trace_fields(&span, &_trace);
    async {
        let response = daemon_setup_client(&daemon_connection)
            .submit_passphrase(passphrase)
            .await
            .map_err(CommandError::internal)?;
        deserialize_setup_state(response.state)
    }
    .instrument(span)
    .await
}

#[tauri::command]
pub async fn confirm_peer_trust(
    daemon_connection: State<'_, DaemonConnectionState>,
    _trace: Option<TraceMetadata>,
) -> Result<SetupState, CommandError> {
    let span = info_span!(
        "command.setup.confirm_peer_trust",
        trace_id = tracing::field::Empty,
        trace_ts = tracing::field::Empty,
    );
    record_trace_fields(&span, &_trace);
    async {
        let response = daemon_setup_client(&daemon_connection)
            .confirm_peer_trust()
            .await
            .map_err(CommandError::internal)?;
        deserialize_setup_state(response.state)
    }
    .instrument(span)
    .await
}

#[tauri::command]
pub async fn cancel_setup(
    daemon_connection: State<'_, DaemonConnectionState>,
    _trace: Option<TraceMetadata>,
) -> Result<SetupState, CommandError> {
    let span = info_span!(
        "command.setup.cancel_setup",
        trace_id = tracing::field::Empty,
        trace_ts = tracing::field::Empty,
    );
    record_trace_fields(&span, &_trace);
    async {
        let response = daemon_setup_client(&daemon_connection)
            .cancel_setup()
            .await
            .map_err(CommandError::internal)?;
        deserialize_setup_state(response.state)
    }
    .instrument(span)
    .await
}

#[cfg(test)]
mod tests {
    use uc_core::setup::SetupState;

    #[test]
    fn setup_state_welcome_serializes_as_string_json() {
        let value = serde_json::to_value(&SetupState::Welcome).expect("serialize failed");
        assert_eq!(value, serde_json::json!("Welcome"));
    }

    #[test]
    fn setup_state_create_space_passphrase_serializes_correctly() {
        let value = serde_json::to_value(&SetupState::CreateSpaceInputPassphrase { error: None })
            .expect("serialize failed");
        assert_eq!(
            value,
            serde_json::json!({"CreateSpaceInputPassphrase": {"error": null}})
        );
    }
}
