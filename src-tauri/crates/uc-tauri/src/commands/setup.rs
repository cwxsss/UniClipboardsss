//! Setup-related Tauri commands
//! 设置流程相关的 Tauri 命令

use std::sync::Arc;

use crate::bootstrap::AppRuntime;
use crate::commands::error::CommandError;
use crate::commands::record_trace_fields;
use tauri::State;
use tracing::{info_span, Instrument};
use uc_app::usecases::setup::orchestrator::SetupError;
use uc_core::setup::SetupState;
use uc_platform::ports::observability::TraceMetadata;

/// Called by the frontend when the daemon emits `setup.spaceAccessCompleted` via
/// the WebSocket bridge. This bridges the gap between the daemon's space access
/// orchestrator completing and the app's setup orchestrator transitioning to
/// `Completed`.
///
/// Note: `setup.spaceAccessCompleted` fires on both sponsor and joiner sides.
/// For the sponsor (already Completed), `get_state()` seeds the context from
/// persisted status and returns `Completed` without dispatching any transition.
#[tauri::command]
pub async fn handle_space_access_completed(
    runtime: State<'_, Arc<AppRuntime>>,
    _trace: Option<TraceMetadata>,
) -> Result<SetupState, CommandError> {
    let span = info_span!(
        "command.setup.handle_space_access_completed",
        trace_id = tracing::field::Empty,
        trace_ts = tracing::field::Empty,
    );
    record_trace_fields(&span, &_trace);
    async {
        let orchestrator = runtime.usecases().setup_orchestrator();

        // Seed the in-process orchestrator state from persisted status before
        // dispatching. This ensures that a sponsor (already Completed) does not
        // receive an invalid JoinSpaceSucceeded transition that would reset the
        // frontend to the Welcome screen.
        let current_state = orchestrator.get_state().await;
        if matches!(current_state, SetupState::Completed) {
            tracing::debug!("handle_space_access_completed: setup already completed, returning Completed (sponsor role)");
            return Ok(SetupState::Completed);
        }

        orchestrator
            .complete_join_space()
            .await
            .map_err(|e| match e {
                SetupError::ActionNotImplemented(msg) => {
                    tracing::warn!(msg = %msg, "join space succeeded event not applicable in current state");
                    CommandError::internal(msg)
                }
                other => CommandError::internal(other.to_string()),
            })
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
