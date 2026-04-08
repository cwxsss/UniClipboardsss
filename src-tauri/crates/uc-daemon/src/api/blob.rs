//! HTTP endpoints for serving raw blob and thumbnail binary content.
//!
//! These endpoints return binary data with Content-Type headers,
//! replacing the uc:// custom protocol handler in uc-tauri.

use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use uc_app::usecases::CoreUseCases;
use uc_core::ids::RepresentationId;
use uc_core::BlobId;

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
    let Some(runtime) = state.runtime.clone() else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "daemon runtime unavailable",
        )
            .into_response();
    };

    let usecases = CoreUseCases::new(runtime.as_ref());
    let use_case = usecases.resolve_blob_resource();
    let parsed_blob_id = BlobId::from(blob_id.as_str());

    match use_case.execute(&parsed_blob_id).await {
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
            let msg = err.to_string().to_lowercase();
            if msg.contains("not found") {
                (StatusCode::NOT_FOUND, "blob not found").into_response()
            } else {
                tracing::error!(
                    error = %err,
                    blob_id = %blob_id,
                    "Failed to resolve blob resource"
                );
                (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response()
            }
        }
    }
}

async fn get_thumbnail(
    State(state): State<DaemonApiState>,
    Path(rep_id): Path<String>,
) -> impl IntoResponse {
    let Some(runtime) = state.runtime.clone() else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "daemon runtime unavailable",
        )
            .into_response();
    };

    let usecases = CoreUseCases::new(runtime.as_ref());
    let use_case = usecases.resolve_thumbnail_resource();
    let parsed_rep_id = RepresentationId::from(rep_id.as_str());

    match use_case.execute(&parsed_rep_id).await {
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
            let msg = err.to_string().to_lowercase();
            if msg.contains("not found") {
                (StatusCode::NOT_FOUND, "thumbnail not found").into_response()
            } else {
                tracing::error!(
                    error = %err,
                    rep_id = %rep_id,
                    "Failed to resolve thumbnail resource"
                );
                (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response()
            }
        }
    }
}
