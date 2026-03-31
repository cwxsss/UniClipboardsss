//! HTTP route handlers for encryption state and session management endpoints.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::json;
use tokio::sync::broadcast::error::SendError;
use tracing::{info, warn};
use uc_app::usecases::CoreUseCases;
use uc_core::network::daemon_api_strings::{ws_event, ws_topic};
use uc_core::security::state::EncryptionState;
use utoipa;

use crate::api::dto::encryption::{EncryptionSessionReadyPayload, EncryptionStateResponse};
use crate::api::dto::error::ApiErrorResponse;
use crate::api::server::DaemonApiState;
use crate::api::types::DaemonWsEvent;

pub fn router() -> Router<DaemonApiState> {
    Router::new()
        .route("/encryption/state", get(get_encryption_state_handler))
        .route("/encryption/unlock", post(unlock_handler))
        .route("/encryption/lock", post(lock_handler))
}

/// GET /encryption/state
/// Returns the current encryption state and session readiness.
#[utoipa::path(
    get,
    path = "/encryption/state",
    tag = "encryption",
    responses(
        (status = 200, description = "Encryption state retrieved"),
        (status = 503, description = "Daemon runtime unavailable", body = ApiErrorResponse),
        (status = 500, description = "Internal server error", body = ApiErrorResponse),
    )
)]
async fn get_encryption_state_handler(State(state): State<DaemonApiState>) -> impl IntoResponse {
    let Some(runtime) = state.runtime.clone() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiErrorResponse {
                code: "runtime_unavailable".to_string(),
                message: "daemon runtime unavailable".to_string(),
            }),
        )
            .into_response();
    };

    let _usecases = CoreUseCases::new(runtime.as_ref());
    let deps = runtime.wiring_deps();

    // Load encryption state
    let state = deps.security.encryption_state.load_state().await;

    // Check session readiness
    let session_ready = deps.security.encryption_session.is_ready().await;

    let response = match state {
        Ok(EncryptionState::Initialized) => EncryptionStateResponse {
            initialized: true,
            session_ready,
        },
        Ok(_) => EncryptionStateResponse {
            initialized: false,
            session_ready: false,
        },
        Err(e) => {
            tracing::error!(error = %e, "failed to load encryption state");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiErrorResponse {
                    code: "internal_error".to_string(),
                    message: "failed to load encryption state".to_string(),
                }),
            )
                .into_response();
        }
    };

    let ts = chrono::Utc::now().timestamp_millis();
    Json(json!({ "data": response, "ts": ts })).into_response()
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
        (status = 500, description = "Internal server error", body = ApiErrorResponse),
    )
)]
async fn unlock_handler(State(state): State<DaemonApiState>) -> impl IntoResponse {
    let Some(runtime) = state.runtime.clone() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiErrorResponse {
                code: "runtime_unavailable".to_string(),
                message: "daemon runtime unavailable".to_string(),
            }),
        )
            .into_response();
    };

    let usecases = CoreUseCases::new(runtime.as_ref());

    match usecases.auto_unlock_encryption_session().execute().await {
        Ok(true) => {
            info!("encryption session auto-unlocked via keyring");

            // Broadcast encryption.session_ready WS event
            let event_payload = EncryptionSessionReadyPayload {
                ts: chrono::Utc::now().timestamp_millis(),
            };
            let event = DaemonWsEvent {
                topic: ws_topic::ENCRYPTION.to_string(),
                event_type: ws_event::ENCRYPTION_SESSION_READY.to_string(),
                session_id: None,
                ts: event_payload.ts,
                payload: serde_json::to_value(&event_payload).unwrap_or(serde_json::Value::Null),
            };
            if let Err(SendError(_)) = state.event_tx.send(event) {
                warn!("failed to broadcast encryption.session_ready event — no active subscribers");
            }

            let ts = chrono::Utc::now().timestamp_millis();
            (
                StatusCode::OK,
                Json(json!({ "data": { "success": true }, "ts": ts })),
            )
                .into_response()
        }
        Ok(false) => {
            // Encryption not initialized — not an error, just return success=false
            info!("encryption not initialized, skipping auto-unlock");
            let ts = chrono::Utc::now().timestamp_millis();
            (
                StatusCode::OK,
                Json(json!({ "data": { "success": false }, "ts": ts })),
            )
                .into_response()
        }
        Err(e) => {
            let msg = e.to_string();
            tracing::warn!(error = %e, "keyring auto-unlock failed");

            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": { "code": "auto_unlock_failed", "message": msg.as_str() } })),
            )
                .into_response()
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
        (status = 503, description = "Daemon runtime unavailable", body = ApiErrorResponse),
        (status = 500, description = "Internal server error", body = ApiErrorResponse),
    )
)]
async fn lock_handler(State(state): State<DaemonApiState>) -> impl IntoResponse {
    let Some(runtime) = state.runtime.clone() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiErrorResponse {
                code: "runtime_unavailable".to_string(),
                message: "daemon runtime unavailable".to_string(),
            }),
        )
            .into_response();
    };

    match runtime
        .wiring_deps()
        .security
        .encryption_session
        .clear()
        .await
    {
        Ok(()) => {
            info!("encryption session cleared (locked)");
            let ts = chrono::Utc::now().timestamp_millis();
            (
                StatusCode::OK,
                Json(json!({ "data": { "success": true }, "ts": ts })),
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!(error = %e, "failed to clear encryption session");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": { "code": "internal_error", "message": "failed to lock encryption" } })),
            )
                .into_response()
        }
    }
}
