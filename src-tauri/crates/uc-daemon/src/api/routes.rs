//! HTTP route handlers for the daemon API.
//!
//! Router is split into two tiers:
//! - L1 (router_l1): public endpoints requiring no authentication (health check)
//! - L2+ (router_l2_plus): protected endpoints behind auth_extractor + rate_limit middleware
//!
//! Auth middleware chain (layer order):
//!   auth_extractor (innermost) runs FIRST -> validates JWT + PID whitelist -> sets client_id
//!   rate_limit (outermost) runs SECOND -> checks rate limit using client_id from extensions
//!
//! L3/L4 permission enforcement is NOT implemented in Phase 75 (deferred to future phases).

use std::sync::Arc;

use axum::extract::rejection::JsonRejection;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::middleware;
use axum::response::IntoResponse;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde_json::json;
use uc_app::usecases::CoreUseCases;
use uc_core::network::daemon_api_strings::{http_route, pairing_error_code, pairing_stage};

use crate::api::pairing::{
    AckedPairingCommandResponse, InitiatePairingRequest, InitiatePairingResponse,
    PairingApiErrorResponse, PairingGuiLeaseRequest, PairingSessionCommandRequest,
    SetPairingDiscoverabilityRequest, SetPairingParticipantRequest, UnpairDeviceRequest,
    VerifyPairingRequest,
};
use crate::api::server::{map_daemon_pairing_error, DaemonApiState};
use crate::pairing::host::{DaemonPairingHost, DaemonPairingHostError};
use crate::security::middleware::{auth_extractor_middleware, rate_limit_middleware};

/// Build the L1 (public) router - no auth required.
/// Contains only the health check endpoint.
///
/// Takes state to return Router<DaemonApiState> so it can be merged
/// with router_l2_plus without type mismatch.
pub fn router_l1(state: DaemonApiState) -> Router<DaemonApiState> {
    let mut router = Router::new()
        .route("/health", get(health))
        .with_state(state.clone())
        .layer(middleware::from_fn(crate::api::server::cors_middleware));

    #[cfg(debug_assertions)]
    {
        router = router.merge(crate::api::dev::router(state));
    }

    router
}

/// Build the L2+ (protected) router - requires valid session token.
/// All routes are behind auth_extractor -> rate_limit middleware layers.
///
/// LAYER ORDER (FINDING-2): In Axum, the LAST `.layer()` call wraps closest to the
/// handler and runs FIRST on incoming requests. We want auth_extractor to run FIRST
/// (to validate the JWT and set client_id in extensions), then rate_limit to run SECOND
/// (to check the rate limit using the client_id from extensions).
///
/// Therefore, auth_extractor_middleware must be the LAST .layer() call:
///   .layer(rate_limit_middleware)      // first in code -> outer -> runs SECOND
///   .layer(auth_extractor_middleware)  // last in code -> inner -> runs FIRST
///
/// This means rate limiting applies to already-authenticated requests (by validated PID).
/// It is NOT a pre-auth gate - that is a deliberate design choice for Phase 75.
///
/// NOTE on L3/L4: Phase 75 does NOT implement L3/L4 permission enforcement.
/// The middleware chain enforces only L2 (valid JWT + PID whitelist).
/// L3/L4 checks (encryption_ready state) are reserved for future phases.
pub fn router_l2_plus(state: DaemonApiState) -> Router<DaemonApiState> {
    let router = Router::new()
        .merge(crate::api::clipboard::router())
        .merge(crate::api::device::router())
        .merge(crate::api::settings::router())
        .merge(crate::api::setup::router())
        .merge(crate::api::encryption::router())
        .merge(crate::api::storage::router())
        .route("/status", get(status))
        .route("/peers", get(peers))
        .route("/paired-devices", get(paired_devices))
        .route("/space-access/state", get(space_access_state_handler))
        .route("/pairing/initiate", post(handle_initiate_pairing))
        .route("/pairing/accept", post(handle_accept_pairing))
        .route("/pairing/reject", post(handle_reject_pairing))
        .route("/pairing/cancel", post(handle_cancel_pairing))
        .route("/pairing/unpair", post(handle_unpair_device))
        .route("/pairing/gui/lease", post(handle_pairing_gui_lease))
        .route(
            "/pairing/discoverability/current",
            put(set_pairing_discoverability),
        )
        .route(
            "/pairing/participants/current",
            put(set_pairing_participant),
        )
        .route("/pairing/sessions", post(initiate_pairing))
        .route("/pairing/sessions/:session_id", get(pairing_session))
        .route("/pairing/sessions/:session_id/accept", post(accept_pairing))
        .route("/pairing/sessions/:session_id/reject", post(reject_pairing))
        .route("/pairing/sessions/:session_id/cancel", post(cancel_pairing))
        .route("/pairing/sessions/:session_id/verify", post(verify_pairing))
        .merge(crate::api::lifecycle::router())
        .route(
            &format!("{}/:entry_id", http_route::CLIPBOARD_RESTORE),
            post(restore_clipboard_entry_handler),
        )
        .with_state(state.clone());

    // Apply middleware layers.
    // auth_extractor (innermost, runs first) sets client_id in extensions.
    // rate_limit (outermost, runs second) checks the rate limit using client_id.
    // See detailed comment above for layer order explanation.
    let state_for_middleware = Arc::new(state);
    router
        .layer(middleware::from_fn(crate::api::server::cors_middleware))
        .layer(middleware::from_fn_with_state(
            state_for_middleware.clone(),
            rate_limit_middleware,
        ))
        .layer(middleware::from_fn_with_state(
            state_for_middleware,
            auth_extractor_middleware,
        ))
}

async fn health(State(state): State<DaemonApiState>) -> impl IntoResponse {
    Json(state.query_service.health().await)
}

async fn restore_clipboard_entry_handler(
    State(state): State<DaemonApiState>,
    Path(entry_id): Path<String>,
) -> impl IntoResponse {
    let Some(runtime) = state.runtime.clone() else {
        return internal_error(anyhow::anyhow!("daemon runtime unavailable")).into_response();
    };

    let parsed_id = uc_core::ids::EntryId::from(entry_id.clone());
    let usecases = CoreUseCases::new(runtime.as_ref());

    tracing::info!(entry_id = %entry_id, "daemon restore request received");

    // Restore to OS clipboard first - this calls set_next_origin(LocalRestore) in-process.
    // The daemon's ClipboardWatcherWorker will detect the write, but CaptureClipboardUseCase
    // skips capture for LocalRestore origin - no duplicate DB entry, no outbound sync.
    // This is correct behavior: restored content is already in DB and was previously synced.
    // Do NOT call SyncOutboundClipboardUseCase here - it would cause unwanted duplicate sync.
    let restore_uc = match usecases.restore_clipboard_selection() {
        Ok(uc) => uc,
        Err(e) => {
            tracing::warn!(entry_id = %entry_id, error = %e, "clipboard_write_coordinator unavailable for restore");
            return internal_error(e).into_response();
        }
    };
    match restore_uc.execute(&parsed_id).await {
        Ok(()) => {
            tracing::info!(entry_id = %entry_id, "daemon restore request succeeded");
        }
        Err(e) => {
            // Map "entry not found" errors to 404 (not 500).
            // RestoreClipboardSelectionUseCase returns anyhow error with "not found" text
            // when entry or representations are missing.
            let msg = e.to_string().to_lowercase();
            tracing::warn!(entry_id = %entry_id, error = %e, "daemon restore request failed");
            if msg.contains("not found") {
                return (StatusCode::NOT_FOUND, Json(json!({"error": "not_found"})))
                    .into_response();
            }
            return internal_error(e).into_response();
        }
    }

    // Touch after successful restore to bump active_time (avoids stale timestamp on failed restore)
    match usecases.touch_clipboard_entry().execute(&parsed_id).await {
        Ok(_) => {}
        Err(e) => {
            tracing::warn!(error = %e, entry_id = %entry_id, "touch_clipboard_entry failed after restore");
        }
    }

    (StatusCode::OK, Json(json!({"success": true}))).into_response()
}

async fn status(State(state): State<DaemonApiState>) -> impl IntoResponse {
    match state.query_service.status().await {
        Ok(response) => Json(response).into_response(),
        Err(error) => internal_error(error).into_response(),
    }
}

async fn peers(State(state): State<DaemonApiState>) -> impl IntoResponse {
    match state.query_service.peers().await {
        Ok(response) => Json(response).into_response(),
        Err(error) => internal_error(error).into_response(),
    }
}

async fn paired_devices(State(state): State<DaemonApiState>) -> impl IntoResponse {
    match state.query_service.paired_devices().await {
        Ok(response) => Json(response).into_response(),
        Err(error) => internal_error(error).into_response(),
    }
}

async fn space_access_state_handler(State(state): State<DaemonApiState>) -> impl IntoResponse {
    Json(
        state
            .query_service
            .space_access_state(state.space_access_orchestrator().as_deref())
            .await,
    )
    .into_response()
}

async fn pairing_session(
    State(state): State<DaemonApiState>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    match state.query_service.pairing_session(&session_id).await {
        Ok(Some(response)) => Json(response).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "not_found", "sessionId": session_id})),
        )
            .into_response(),
        Err(error) => internal_error(error).into_response(),
    }
}

async fn set_pairing_discoverability(
    State(state): State<DaemonApiState>,
    payload: Result<Json<SetPairingDiscoverabilityRequest>, JsonRejection>,
) -> impl IntoResponse {
    let Some(pairing_host) = pairing_host(&state).into_response_ok() else {
        return daemon_pairing_unavailable().into_response();
    };
    let Json(payload) = match payload {
        Ok(payload) => payload,
        Err(_) => return bad_request("malformed_request_body").into_response(),
    };

    pairing_host
        .set_discoverability(
            payload.client_kind,
            payload.discoverable,
            payload.lease_ttl_ms,
        )
        .await;

    acknowledged(
        "current".to_string(),
        payload.discoverable,
        "discoverability_updated",
    )
    .into_response()
}

async fn set_pairing_participant(
    State(state): State<DaemonApiState>,
    payload: Result<Json<SetPairingParticipantRequest>, JsonRejection>,
) -> impl IntoResponse {
    let Some(pairing_host) = pairing_host(&state).into_response_ok() else {
        return daemon_pairing_unavailable().into_response();
    };
    let Json(payload) = match payload {
        Ok(payload) => payload,
        Err(_) => return bad_request("malformed_request_body").into_response(),
    };

    pairing_host
        .set_participant_ready(payload.client_kind, payload.ready, payload.lease_ttl_ms)
        .await;

    acknowledged("current".to_string(), payload.ready, "participant_updated").into_response()
}

async fn initiate_pairing(
    State(state): State<DaemonApiState>,
    payload: Result<Json<InitiatePairingRequest>, JsonRejection>,
) -> impl IntoResponse {
    let Some(pairing_host) = pairing_host(&state).into_response_ok() else {
        return daemon_pairing_unavailable().into_response();
    };
    let Json(payload) = match payload {
        Ok(payload) => payload,
        Err(_) => return bad_request("malformed_request_body").into_response(),
    };

    match pairing_host.initiate_pairing(payload.peer_id).await {
        Ok(session_id) => acknowledged(session_id, true, pairing_stage::REQUEST).into_response(),
        Err(error) => map_pairing_command_error(error).into_response(),
    }
}

async fn handle_initiate_pairing(
    State(state): State<DaemonApiState>,
    payload: Result<Json<InitiatePairingRequest>, JsonRejection>,
) -> impl IntoResponse {
    let Some(pairing_host) = pairing_host(&state).into_response_ok() else {
        return daemon_pairing_unavailable().into_response();
    };
    let Json(payload) = match payload {
        Ok(payload) => payload,
        Err(_) => {
            return pairing_api_error(
                StatusCode::BAD_REQUEST,
                pairing_error_code::BAD_REQUEST,
                "malformed request body",
            )
            .into_response();
        }
    };

    match pairing_host.initiate_pairing(payload.peer_id).await {
        Ok(session_id) => {
            (StatusCode::OK, Json(InitiatePairingResponse { session_id })).into_response()
        }
        Err(error) => pairing_http_error(error).into_response(),
    }
}

async fn accept_pairing(
    State(state): State<DaemonApiState>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    let Some(pairing_host) = pairing_host(&state).into_response_ok() else {
        return daemon_pairing_unavailable().into_response();
    };

    match pairing_host.accept_pairing(&session_id).await {
        Ok(()) => acknowledged(session_id, true, pairing_stage::VERIFYING).into_response(),
        Err(error) => map_pairing_command_error(error).into_response(),
    }
}

async fn handle_accept_pairing(
    State(state): State<DaemonApiState>,
    payload: Result<Json<PairingSessionCommandRequest>, JsonRejection>,
) -> impl IntoResponse {
    handle_session_command(state, payload, |pairing_host, session_id| async move {
        pairing_host.accept_pairing(&session_id).await
    })
    .await
}

async fn reject_pairing(
    State(state): State<DaemonApiState>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    let Some(pairing_host) = pairing_host(&state).into_response_ok() else {
        return daemon_pairing_unavailable().into_response();
    };

    match pairing_host.reject_pairing(&session_id).await {
        Ok(()) => acknowledged(session_id, true, pairing_stage::FAILED).into_response(),
        Err(error) => map_pairing_command_error(error).into_response(),
    }
}

async fn handle_reject_pairing(
    State(state): State<DaemonApiState>,
    payload: Result<Json<PairingSessionCommandRequest>, JsonRejection>,
) -> impl IntoResponse {
    handle_session_command(state, payload, |pairing_host, session_id| async move {
        pairing_host.reject_pairing(&session_id).await
    })
    .await
}

async fn cancel_pairing(
    State(state): State<DaemonApiState>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    let Some(pairing_host) = pairing_host(&state).into_response_ok() else {
        return daemon_pairing_unavailable().into_response();
    };

    match pairing_host.cancel_pairing(&session_id).await {
        Ok(()) => acknowledged(session_id, true, pairing_stage::FAILED).into_response(),
        Err(error) => map_pairing_command_error(error).into_response(),
    }
}

async fn handle_cancel_pairing(
    State(state): State<DaemonApiState>,
    payload: Result<Json<PairingSessionCommandRequest>, JsonRejection>,
) -> impl IntoResponse {
    handle_session_command(state, payload, |pairing_host, session_id| async move {
        pairing_host.cancel_pairing(&session_id).await
    })
    .await
}

async fn handle_unpair_device(
    State(state): State<DaemonApiState>,
    payload: Result<Json<UnpairDeviceRequest>, JsonRejection>,
) -> impl IntoResponse {
    let Some(runtime) = state.runtime.clone() else {
        return pairing_api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            pairing_error_code::RUNTIME_UNAVAILABLE,
            "daemon runtime unavailable",
        )
        .into_response();
    };
    let Json(payload) = match payload {
        Ok(payload) => payload,
        Err(_) => {
            return pairing_api_error(
                StatusCode::BAD_REQUEST,
                pairing_error_code::BAD_REQUEST,
                "malformed request body",
            )
            .into_response();
        }
    };

    let usecases = CoreUseCases::new(runtime.as_ref());
    match usecases.unpair_device().execute(payload.peer_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => {
            let message = error.to_string();
            tracing::error!(error = %error, "daemon unpair command failed");
            pairing_api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                pairing_error_code::INTERNAL,
                &message,
            )
            .into_response()
        }
    }
}

async fn verify_pairing(
    State(state): State<DaemonApiState>,
    Path(session_id): Path<String>,
    payload: Result<Json<VerifyPairingRequest>, JsonRejection>,
) -> impl IntoResponse {
    let Some(pairing_host) = pairing_host(&state).into_response_ok() else {
        return daemon_pairing_unavailable().into_response();
    };
    let Json(payload) = match payload {
        Ok(payload) => payload,
        Err(_) => return bad_request("malformed_request_body").into_response(),
    };

    match pairing_host
        .verify_pairing(&session_id, payload.pin_matches)
        .await
    {
        Ok(()) => acknowledged(
            session_id,
            true,
            if payload.pin_matches {
                pairing_stage::VERIFYING
            } else {
                pairing_stage::FAILED
            },
        )
        .into_response(),
        Err(error) => map_pairing_command_error(error).into_response(),
    }
}

async fn handle_pairing_gui_lease(
    State(state): State<DaemonApiState>,
    payload: Result<Json<PairingGuiLeaseRequest>, JsonRejection>,
) -> impl IntoResponse {
    let Some(pairing_host) = pairing_host(&state).into_response_ok() else {
        return daemon_pairing_unavailable().into_response();
    };
    let Json(payload) = match payload {
        Ok(payload) => payload,
        Err(_) => {
            return pairing_api_error(
                StatusCode::BAD_REQUEST,
                pairing_error_code::BAD_REQUEST,
                "malformed request body",
            )
            .into_response();
        }
    };

    match pairing_host
        .register_gui_participant(payload.enabled, payload.lease_ttl_ms)
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => pairing_http_error(error).into_response(),
    }
}

fn pairing_host(state: &DaemonApiState) -> Result<Arc<DaemonPairingHost>, ()> {
    state.pairing_host().ok_or(())
}

async fn handle_session_command<F, Fut>(
    state: DaemonApiState,
    payload: Result<Json<PairingSessionCommandRequest>, JsonRejection>,
    handler: F,
) -> axum::response::Response
where
    F: FnOnce(Arc<DaemonPairingHost>, String) -> Fut,
    Fut: std::future::Future<Output = Result<(), DaemonPairingHostError>>,
{
    let Some(pairing_host) = pairing_host(&state).into_response_ok() else {
        return daemon_pairing_unavailable().into_response();
    };
    let Json(payload) = match payload {
        Ok(payload) => payload,
        Err(_) => {
            return pairing_api_error(
                StatusCode::BAD_REQUEST,
                pairing_error_code::BAD_REQUEST,
                "malformed request body",
            )
            .into_response();
        }
    };

    match handler(pairing_host, payload.session_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => pairing_http_error(error).into_response(),
    }
}

/// NOTE: The `unauthorized()` helper is kept for backward compatibility with modules
/// that may still reference it. The middleware layer now handles authentication
/// before requests reach handlers, so individual handlers no longer call this.
pub(crate) fn unauthorized() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({"error": "unauthorized"})),
    )
}

pub(crate) fn daemon_pairing_unavailable() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({"error": "pairing_host_unavailable"})),
    )
}

fn bad_request(error: &str) -> (StatusCode, Json<serde_json::Value>) {
    (StatusCode::BAD_REQUEST, Json(json!({ "error": error })))
}

pub(crate) fn internal_error(error: anyhow::Error) -> (StatusCode, Json<serde_json::Value>) {
    tracing::error!(error = %error, "daemon API request failed");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({"error": "internal_error"})),
    )
}

pub(crate) fn map_pairing_command_error(
    error: DaemonPairingHostError,
) -> (StatusCode, Json<serde_json::Value>) {
    let (status, body) = map_daemon_pairing_error(error);
    (
        status,
        Json(json!({
            "code": body.code,
            "message": body.message,
        })),
    )
}

fn pairing_http_error(
    error: DaemonPairingHostError,
) -> (StatusCode, Json<PairingApiErrorResponse>) {
    let (status, body) = map_daemon_pairing_error(error);
    (status, Json(body))
}

fn pairing_api_error(
    status: StatusCode,
    code: &str,
    message: &str,
) -> (StatusCode, Json<PairingApiErrorResponse>) {
    (
        status,
        Json(PairingApiErrorResponse {
            code: code.to_string(),
            message: message.to_string(),
        }),
    )
}

fn acknowledged(
    session_id: String,
    accepted: bool,
    state: &'static str,
) -> (StatusCode, Json<AckedPairingCommandResponse>) {
    (
        StatusCode::ACCEPTED,
        Json(AckedPairingCommandResponse {
            session_id,
            accepted,
            state: state.to_string(),
            error: None,
        }),
    )
}

trait IntoResponseOk<T> {
    fn into_response_ok(self) -> Option<T>;
}

impl<T, E> IntoResponseOk<T> for Result<T, E> {
    fn into_response_ok(self) -> Option<T> {
        self.ok()
    }
}
