//! HTTP route handlers for device info endpoints.
//!
//! Provides read-only access to local device identity (peer ID + device name).

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use utoipa;

use uc_app::usecases::CoreUseCases;

use crate::api::dto::device::{GetLocalDeviceInfoResponse, LocalDeviceInfoDto};
use crate::api::dto::error::ApiError;
use crate::api::server::DaemonApiState;

pub fn router() -> Router<DaemonApiState> {
    Router::new().route("/device/me", get(get_local_device_info_handler))
}

/// GET /device/me
/// Returns the local device's peer ID and resolved device name.
#[utoipa::path(
    get,
    path = "/device/me",
    tag = "device",
    responses(
        (status = 200, body = GetLocalDeviceInfoResponse),
        (status = 500, description = "Internal server error", body = crate::api::dto::error::ApiErrorResponse)
    )
)]
async fn get_local_device_info_handler(
    State(state): State<DaemonApiState>,
) -> Result<Json<GetLocalDeviceInfoResponse>, ApiError> {
    let runtime = state.runtime_or_error()?;
    let usecases = CoreUseCases::new(runtime.as_ref());

    let info = usecases
        .get_local_device_info()
        .execute()
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;

    Ok(Json(GetLocalDeviceInfoResponse {
        data: info.into(),
        ts: chrono::Utc::now().timestamp_millis(),
    }))
}
