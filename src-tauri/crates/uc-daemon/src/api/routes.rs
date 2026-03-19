//! HTTP route handlers for the daemon API.

use std::sync::Arc;

use axum::extract::rejection::JsonRejection;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde_json::json;

use crate::api::pairing::{
    AckedPairingCommandResponse, InitiatePairingRequest, SetPairingDiscoverabilityRequest,
    SetPairingParticipantRequest, VerifyPairingRequest,
};
use crate::api::server::DaemonApiState;
use crate::pairing::host::{DaemonPairingHost, DaemonPairingHostError};

pub fn router() -> Router<DaemonApiState> {
    Router::new()
        .route("/health", get(health))
        .route("/status", get(status))
        .route("/peers", get(peers))
        .route("/paired-devices", get(paired_devices))
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
}

async fn health(State(state): State<DaemonApiState>) -> impl IntoResponse {
    Json(state.query_service.health().await)
}

async fn status(State(state): State<DaemonApiState>, headers: HeaderMap) -> impl IntoResponse {
    if !state.is_authorized(&headers) {
        return unauthorized().into_response();
    }

    match state.query_service.status().await {
        Ok(response) => Json(response).into_response(),
        Err(error) => internal_error(error).into_response(),
    }
}

async fn peers(State(state): State<DaemonApiState>, headers: HeaderMap) -> impl IntoResponse {
    if !state.is_authorized(&headers) {
        return unauthorized().into_response();
    }

    match state.query_service.peers().await {
        Ok(response) => Json(response).into_response(),
        Err(error) => internal_error(error).into_response(),
    }
}

async fn paired_devices(
    State(state): State<DaemonApiState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if !state.is_authorized(&headers) {
        return unauthorized().into_response();
    }

    match state.query_service.paired_devices().await {
        Ok(response) => Json(response).into_response(),
        Err(error) => internal_error(error).into_response(),
    }
}

async fn pairing_session(
    State(state): State<DaemonApiState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    if !state.is_authorized(&headers) {
        return unauthorized().into_response();
    }

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
    headers: HeaderMap,
    payload: Result<Json<SetPairingDiscoverabilityRequest>, JsonRejection>,
) -> impl IntoResponse {
    if !state.is_authorized(&headers) {
        return unauthorized().into_response();
    }
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
    headers: HeaderMap,
    payload: Result<Json<SetPairingParticipantRequest>, JsonRejection>,
) -> impl IntoResponse {
    if !state.is_authorized(&headers) {
        return unauthorized().into_response();
    }
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
    headers: HeaderMap,
    payload: Result<Json<InitiatePairingRequest>, JsonRejection>,
) -> impl IntoResponse {
    if !state.is_authorized(&headers) {
        return unauthorized().into_response();
    }
    let Some(pairing_host) = pairing_host(&state).into_response_ok() else {
        return daemon_pairing_unavailable().into_response();
    };
    let Json(payload) = match payload {
        Ok(payload) => payload,
        Err(_) => return bad_request("malformed_request_body").into_response(),
    };

    match pairing_host.initiate_pairing(payload.peer_id).await {
        Ok(session_id) => acknowledged(session_id, true, "request").into_response(),
        Err(error) => pairing_command_error(error).into_response(),
    }
}

async fn accept_pairing(
    State(state): State<DaemonApiState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    if !state.is_authorized(&headers) {
        return unauthorized().into_response();
    }
    let Some(pairing_host) = pairing_host(&state).into_response_ok() else {
        return daemon_pairing_unavailable().into_response();
    };

    match pairing_host.accept_pairing(&session_id).await {
        Ok(()) => acknowledged(session_id, true, "verifying").into_response(),
        Err(error) => pairing_command_error(error).into_response(),
    }
}

async fn reject_pairing(
    State(state): State<DaemonApiState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    if !state.is_authorized(&headers) {
        return unauthorized().into_response();
    }
    let Some(pairing_host) = pairing_host(&state).into_response_ok() else {
        return daemon_pairing_unavailable().into_response();
    };

    match pairing_host.reject_pairing(&session_id).await {
        Ok(()) => acknowledged(session_id, true, "failed").into_response(),
        Err(error) => pairing_command_error(error).into_response(),
    }
}

async fn cancel_pairing(
    State(state): State<DaemonApiState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    if !state.is_authorized(&headers) {
        return unauthorized().into_response();
    }
    let Some(pairing_host) = pairing_host(&state).into_response_ok() else {
        return daemon_pairing_unavailable().into_response();
    };

    match pairing_host.cancel_pairing(&session_id).await {
        Ok(()) => acknowledged(session_id, true, "failed").into_response(),
        Err(error) => pairing_command_error(error).into_response(),
    }
}

async fn verify_pairing(
    State(state): State<DaemonApiState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    payload: Result<Json<VerifyPairingRequest>, JsonRejection>,
) -> impl IntoResponse {
    if !state.is_authorized(&headers) {
        return unauthorized().into_response();
    }
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
                "verifying"
            } else {
                "failed"
            },
        )
        .into_response(),
        Err(error) => pairing_command_error(error).into_response(),
    }
}

fn pairing_host(state: &DaemonApiState) -> Result<Arc<DaemonPairingHost>, ()> {
    state.pairing_host.clone().ok_or(())
}

fn unauthorized() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({"error": "unauthorized"})),
    )
}

fn daemon_pairing_unavailable() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({"error": "pairing_host_unavailable"})),
    )
}

fn bad_request(error: &str) -> (StatusCode, Json<serde_json::Value>) {
    (StatusCode::BAD_REQUEST, Json(json!({ "error": error })))
}

fn internal_error(error: anyhow::Error) -> (StatusCode, Json<serde_json::Value>) {
    tracing::error!(error = %error, "daemon API request failed");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({"error": "internal_error"})),
    )
}

fn pairing_command_error(error: DaemonPairingHostError) -> (StatusCode, Json<serde_json::Value>) {
    match error {
        DaemonPairingHostError::ActivePairingSessionExists => (
            StatusCode::CONFLICT,
            Json(json!({"error": "active_pairing_session_exists"})),
        ),
        DaemonPairingHostError::HostNotDiscoverable => (
            StatusCode::CONFLICT,
            Json(json!({"error": "host_not_discoverable"})),
        ),
        DaemonPairingHostError::NoLocalPairingParticipantReady => (
            StatusCode::PRECONDITION_FAILED,
            Json(json!({"error": "no_local_pairing_participant_ready"})),
        ),
        DaemonPairingHostError::SessionNotFound(session_id) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "not_found", "sessionId": session_id})),
        ),
        DaemonPairingHostError::Internal(message) => {
            tracing::error!(error = %message, "daemon pairing command failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "internal_error"})),
            )
        }
    }
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

#[allow(dead_code)]
fn _route_markers() -> [&'static str; 10] {
    [
        "/pairing/discoverability/current",
        "/pairing/participants/current",
        "/pairing/sessions",
        "/pairing/sessions/:session_id/accept",
        "/pairing/sessions/:session_id/reject",
        "/pairing/sessions/:session_id/cancel",
        "/pairing/sessions/:session_id/verify",
        "active_pairing_session_exists",
        "no_local_pairing_participant_ready",
        "host_not_discoverable",
    ]
}

#[allow(dead_code)]
fn _response_marker(_: AckedPairingCommandResponse) -> StatusCode {
    StatusCode::ACCEPTED
}
