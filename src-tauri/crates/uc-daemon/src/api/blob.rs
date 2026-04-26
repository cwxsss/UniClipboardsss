//! HTTP endpoints for serving raw blob and thumbnail binary content.
//!
//! These endpoints return binary data with Content-Type headers,
//! replacing the uc:// custom protocol handler in uc-tauri.

use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use uc_application::facade::ResourceFacadeError;

use crate::api::server::DaemonApiState;

pub fn router() -> Router<DaemonApiState> {
    Router::new()
        .route("/clipboard/blobs/:blob_id", get(get_blob))
        .route("/clipboard/thumbnails/:rep_id", get(get_thumbnail))
}

async fn get_blob(
    State(state): State<DaemonApiState>,
    Path(blob_id): Path<String>,
) -> impl IntoResponse {
    let facade = match state.resource_facade_or_error() {
        Ok(facade) => facade,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "daemon resource facade unavailable",
            )
                .into_response();
        }
    };

    match facade.blob(&blob_id).await {
        Ok(result) => {
            let content_type = result
                .mime_type
                .as_deref()
                .unwrap_or("application/octet-stream");
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, content_type.to_string())],
                result.bytes,
            )
                .into_response()
        }
        Err(err) => {
            if !matches!(err, ResourceFacadeError::NotFound) {
                tracing::error!(
                    error = %err,
                    blob_id = %blob_id,
                    "Failed to resolve blob resource"
                );
            }
            map_resource_error(err, "blob not found")
        }
    }
}

async fn get_thumbnail(
    State(state): State<DaemonApiState>,
    Path(rep_id): Path<String>,
) -> impl IntoResponse {
    let facade = match state.resource_facade_or_error() {
        Ok(facade) => facade,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "daemon resource facade unavailable",
            )
                .into_response();
        }
    };

    match facade.thumbnail(&rep_id).await {
        Ok(result) => {
            let content_type = result
                .mime_type
                .as_deref()
                .unwrap_or("application/octet-stream");
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, content_type.to_string())],
                result.bytes,
            )
                .into_response()
        }
        Err(err) => {
            if !matches!(err, ResourceFacadeError::NotFound) {
                tracing::error!(
                    error = %err,
                    rep_id = %rep_id,
                    "Failed to resolve thumbnail resource"
                );
            }
            map_resource_error(err, "thumbnail not found")
        }
    }
}

fn map_resource_error(
    error: ResourceFacadeError,
    not_found_message: &'static str,
) -> axum::response::Response {
    match error {
        ResourceFacadeError::NotFound => (StatusCode::NOT_FOUND, not_found_message).into_response(),
        ResourceFacadeError::Mismatch(_) | ResourceFacadeError::Internal(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response()
        }
    }
}
