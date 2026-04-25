//! HTTP route handlers for pairing endpoints.
//!
//! Slice 4 P5a-4: 旧 pairing 协议（initiate/accept/reject/verify/sessions/...）
//! 随 libp2p 一起下线。本文件仅保留 `/pairing/unpair`，前端 DevicesPage 通过
//! `MemberRepositoryPort` 走本地撤销路径。

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use utoipa;

use uc_application::membership::usecases::{RevokeMember, RevokeMemberUseCase};
use uc_core::DeviceId;

use crate::api::dto::error::ApiError;
use crate::api::dto::pairing::UnpairDeviceRequest;
use crate::api::server::DaemonApiState;

pub fn router() -> Router<DaemonApiState> {
    Router::new().route("/pairing/unpair", post(handle_unpair_device))
}

/// POST /pairing/unpair
#[utoipa::path(
    post,
    path = "/pairing/unpair",
    tag = "pairing",
    request_body = UnpairDeviceRequest,
    responses(
        (status = 204, description = "Device unpaired"),
        (status = 400, description = "Bad request"),
        (status = 503, description = "Runtime unavailable"),
        (status = 500, description = "Internal error"),
    )
)]
pub(crate) async fn handle_unpair_device(
    State(state): State<DaemonApiState>,
    Json(payload): Json<UnpairDeviceRequest>,
) -> Result<StatusCode, ApiError> {
    let runtime = state.runtime_or_error()?;
    let deps = runtime.wiring_deps();
    let peer_id = payload.peer_id;

    // Slice 4 P5a-1: 取消配对 = 删除本机成员记录。libp2p 时代的
    // `PairingTransportPort::unpair_device` 通知对端的能力随 libp2p 一同下线；
    // 本地自治模型下不再广播给对端（对端发现后会自行清理）。
    RevokeMemberUseCase::new(deps.device.member_repo.clone())
        .execute(RevokeMember {
            device_id: DeviceId::new(peer_id.as_str()),
        })
        .await
        .map_err(|e| {
            tracing::error!(error = %e, peer_id = %peer_id, "daemon unpair: revoke member failed");
            ApiError::internal(e.to_string())
        })?;

    Ok(StatusCode::NO_CONTENT)
}
