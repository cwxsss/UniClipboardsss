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

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::middleware;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::json;
use uc_app::usecases::CoreUseCases;
use uc_core::network::daemon_api_strings::http_route;

use crate::api::server::DaemonApiState;
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
        .merge(crate::api::pairing::router())
        .route("/status", get(status))
        .route("/peers", get(peers))
        .route("/paired-devices", get(paired_devices))
        .route("/space-access/state", get(space_access_state_handler))
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

/// NOTE: The `unauthorized()` helper is kept for backward compatibility with modules
/// that may still reference it. The middleware layer now handles authentication
/// before requests reach handlers, so individual handlers no longer call this.
pub(crate) fn unauthorized() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({"error": "unauthorized"})),
    )
}

pub(crate) fn internal_error(error: anyhow::Error) -> (StatusCode, Json<serde_json::Value>) {
    tracing::error!(error = %error, "daemon API request failed");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({"error": "internal_error"})),
    )
}
