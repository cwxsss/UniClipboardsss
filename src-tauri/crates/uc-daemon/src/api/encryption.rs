//! HTTP route handlers for encryption state and session management endpoints.

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::json;
use tokio::sync::broadcast::error::SendError;
use tracing::{info, warn};
use uc_app::usecases::CoreUseCases;
use uc_core::crypto::state::EncryptionState;
use uc_daemon_contract::constants::{ws_event, ws_topic};
use utoipa;

use crate::api::dto::encryption::{
    EncryptionSessionReadyPayload, EncryptionStateResponse, KeychainAccessResponse,
};
use crate::api::dto::error::ApiError;
use crate::api::server::DaemonApiState;
use crate::api::types::DaemonWsEvent;

pub fn router() -> Router<DaemonApiState> {
    Router::new()
        .route("/encryption/state", get(get_encryption_state_handler))
        .route("/encryption/unlock", post(unlock_handler))
        .route("/encryption/lock", post(lock_handler))
        .route(
            "/encryption/keychain-access",
            get(verify_keychain_access_handler),
        )
}

/// GET /encryption/state
/// Returns the current encryption state and session readiness.
#[utoipa::path(
    get,
    path = "/encryption/state",
    tag = "encryption",
    responses(
        (status = 200, description = "Encryption state retrieved"),
        (status = 503, description = "Daemon runtime unavailable"),
        (status = 500, description = "Internal server error"),
    )
)]
async fn get_encryption_state_handler(
    State(state): State<DaemonApiState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let runtime = state.runtime_or_error()?;
    let deps = runtime.wiring_deps();

    let enc_state = deps
        .security
        .encryption_state
        .load_state()
        .await
        .map_err(|e| ApiError::internal(format!("failed to load encryption state: {e}")))?;
    let space_id = uc_core::ids::SpaceId::from("space");
    let session_ready = deps.security.space_access.is_unlocked(&space_id).await;

    let (initialized, session_ready) = match enc_state {
        EncryptionState::Initialized => (true, session_ready),
        _ => (false, false),
    };

    let ts = chrono::Utc::now().timestamp_millis();
    Ok(Json(json!({
        "data": EncryptionStateResponse { initialized, session_ready },
        "ts": ts
    })))
}

/// POST /encryption/unlock
/// Attempts to auto-unlock the encryption session using keyring-stored KEK.
/// No passphrase is required — credentials are retrieved from the OS keychain.
/// On success, broadcasts the `encryption.session_ready` WebSocket event.
#[utoipa::path(
    post,
    path = "/encryption/unlock",
    tag = "encryption",
    responses(
        (status = 200, description = "Encryption session unlocked (or already ready)"),
        (status = 500, description = "Internal server error"),
    )
)]
async fn unlock_handler(
    State(state): State<DaemonApiState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let runtime = state.runtime_or_error()?;
    let usecases = CoreUseCases::new(runtime.as_ref());

    match usecases.auto_unlock_encryption_session().execute().await {
        Ok(true) => {
            info!("encryption session auto-unlocked via keyring");

            let ts = chrono::Utc::now().timestamp_millis();
            let event_payload = EncryptionSessionReadyPayload { ts };
            let event = DaemonWsEvent {
                topic: ws_topic::ENCRYPTION.to_string(),
                event_type: ws_event::ENCRYPTION_SESSION_READY.to_string(),
                session_id: None,
                ts,
                payload: serde_json::to_value(&event_payload).unwrap_or(serde_json::Value::Null),
            };
            if let Err(SendError(_)) = state.event_tx.send(event) {
                warn!("failed to broadcast encryption.session_ready event — no active subscribers");
            }

            Ok(Json(json!({ "data": { "success": true }, "ts": ts })))
        }
        Ok(false) => {
            info!("encryption not initialized, skipping auto-unlock");
            let ts = chrono::Utc::now().timestamp_millis();
            Ok(Json(json!({ "data": { "success": false }, "ts": ts })))
        }
        Err(e) => {
            warn!(error = %e, "keyring auto-unlock failed");
            Err(ApiError::internal(format!("auto-unlock failed: {e}")))
        }
    }
}

/// POST /encryption/lock
/// Locks the encryption session by clearing the master key.
#[utoipa::path(
    post,
    path = "/encryption/lock",
    tag = "encryption",
    responses(
        (status = 200, description = "Encryption session locked"),
        (status = 503, description = "Daemon runtime unavailable"),
        (status = 500, description = "Internal server error"),
    )
)]
async fn lock_handler(
    State(state): State<DaemonApiState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let runtime = state.runtime_or_error()?;

    let space_id = uc_core::ids::SpaceId::from("space");
    runtime
        .wiring_deps()
        .security
        .space_access
        .lock(&space_id)
        .await
        .map_err(|e| ApiError::internal(format!("failed to lock encryption: {e}")))?;

    info!("encryption session cleared (locked)");
    let ts = chrono::Utc::now().timestamp_millis();
    Ok(Json(json!({ "data": { "success": true }, "ts": ts })))
}

/// GET /encryption/keychain-access
/// Verifies macOS Keychain "Always Allow" permission for this app.
/// Returns `granted: true` if Keychain access succeeds silently, `false` if permission denied.
#[utoipa::path(
    get,
    path = "/encryption/keychain-access",
    tag = "encryption",
    responses(
        (status = 200, description = "Keychain access verified"),
        (status = 503, description = "Daemon runtime unavailable"),
        (status = 500, description = "Internal server error"),
    )
)]
async fn verify_keychain_access_handler(
    State(state): State<DaemonApiState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let runtime = state.runtime_or_error()?;
    let usecases = CoreUseCases::new(runtime.as_ref());

    let granted = usecases
        .verify_keychain_access()
        .execute()
        .await
        .map_err(|e| ApiError::internal(format!("keychain access check failed: {e}")))?;

    let ts = chrono::Utc::now().timestamp_millis();
    Ok(Json(json!({
        "data": KeychainAccessResponse { granted },
        "ts": ts
    })))
}
