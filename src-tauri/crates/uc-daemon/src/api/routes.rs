//! Read-only HTTP route handlers for the daemon API.

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use serde_json::json;

use crate::api::server::DaemonApiState;

pub fn router() -> Router<DaemonApiState> {
    Router::new()
        .route("/health", get(health))
        .route("/status", get(status))
        .route("/peers", get(peers))
        .route("/paired-devices", get(paired_devices))
        .route("/pairing/sessions/:session_id", get(pairing_session))
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

fn unauthorized() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({"error": "unauthorized"})),
    )
}

fn internal_error(error: anyhow::Error) -> (StatusCode, Json<serde_json::Value>) {
    tracing::error!(error = %error, "daemon API request failed");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({"error": "internal_error"})),
    )
}
