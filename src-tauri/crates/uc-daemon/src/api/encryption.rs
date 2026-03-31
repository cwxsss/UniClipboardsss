//! HTTP route handlers for encryption state and session management endpoints.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::json;
use tokio::sync::broadcast::error::SendError;
use tracing::{info, warn};
use uc_app::usecases::unlock_encryption_with_passphrase::UnlockWithPassphraseError;
use uc_app::usecases::CoreUseCases;
use uc_core::network::daemon_api_strings::{ws_event, ws_topic};
use uc_core::security::model::Passphrase;
use uc_core::security::state::EncryptionState;
use utoipa;

use crate::api::dto::encryption::{
    EncryptionSessionReadyPayload, EncryptionStateResponse, UnlockRequest,
};
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
/// Attempts to unlock the encryption session using the provided passphrase.
/// On success, broadcasts the `encryption.session_ready` WebSocket event.
#[utoipa::path(
    post,
    path = "/encryption/unlock",
    tag = "encryption",
    request_body = UnlockRequest,
    responses(
        (status = 200, description = "Encryption session unlocked"),
        (status = 400, description = "Bad request", body = ApiErrorResponse),
        (status = 401, description = "Wrong passphrase", body = ApiErrorResponse),
        (status = 500, description = "Internal server error", body = ApiErrorResponse),
    )
)]
async fn unlock_handler(
    State(state): State<DaemonApiState>,
    payload: Result<Json<UnlockRequest>, axum::extract::rejection::JsonRejection>,
) -> impl IntoResponse {
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

    let Json(payload) = match payload {
        Ok(p) => p,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiErrorResponse {
                    code: "bad_request".to_string(),
                    message: "malformed request body".to_string(),
                }),
            )
                .into_response();
        }
    };

    let usecases = CoreUseCases::new(runtime.as_ref());
    let passphrase = Passphrase(payload.passphrase.clone());

    match usecases
        .unlock_encryption_with_passphrase()
        .execute(passphrase)
        .await
    {
        Ok(()) => {
            info!(?payload.passphrase, "encryption session unlocked via passphrase");

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
        Err(e) => {
            let msg = e.to_string();
            tracing::warn!(error = %e, "encryption unlock failed");

            // Map UnlockWithPassphraseError to HTTP status codes
            let (status, code, error_msg) = match &e {
                UnlockWithPassphraseError::NotInitialized => (
                    StatusCode::BAD_REQUEST,
                    "not_initialized",
                    "encryption has not been initialized",
                ),
                UnlockWithPassphraseError::UnwrapFailed(_) => (
                    StatusCode::UNAUTHORIZED,
                    "wrong_passphrase",
                    "wrong passphrase",
                ),
                // All other errors → 500
                _ => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    msg.as_str(),
                ),
            };

            (
                status,
                Json(json!({ "error": { "code": code, "message": error_msg } })),
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
