//! HTTP handlers for per-member sync preferences (phase 4b PR-2).
//!
//! 新路径，与 `api::device::{get,update}_device_sync_settings_handler` 并存；
//! 读写 `SpaceMember.sync_preferences`（`MemberRepositoryPort`），PR-4 移除旧端点。

use axum::extract::{Path, State};
use axum::routing::{get, patch};
use axum::{Json, Router};

use uc_app::usecases::CoreUseCases;
use uc_core::membership::MemberSyncPreferences;
use uc_core::DeviceId;

use crate::api::dto::error::ApiError;
use crate::api::dto::member::{
    GetMemberSyncPreferencesResponse, MemberSyncPreferencesPatchDto,
    UpdateMemberSyncPreferencesResponse,
};
use crate::api::server::DaemonApiState;

pub fn router() -> Router<DaemonApiState> {
    Router::new()
        .route(
            "/member/:device_id/sync-preferences",
            get(get_member_sync_preferences_handler),
        )
        .route(
            "/member/:device_id/sync-preferences",
            patch(update_member_sync_preferences_handler),
        )
}

/// GET /member/:device_id/sync-preferences
/// 返回已接纳成员的同步偏好（双向 send/receive + 双套 content_types）。
#[utoipa::path(
    get,
    path = "/member/{device_id}/sync-preferences",
    tag = "member",
    params(
        ("device_id" = String, Path, description = "Space member's device ID (same string as peer_id, D5)")
    ),
    responses(
        (status = 200, body = GetMemberSyncPreferencesResponse),
        (status = 404, description = "Member not found", body = crate::api::dto::error::ApiErrorResponse),
        (status = 500, description = "Internal server error", body = crate::api::dto::error::ApiErrorResponse)
    )
)]
pub async fn get_member_sync_preferences_handler(
    State(state): State<DaemonApiState>,
    Path(device_id): Path<String>,
) -> Result<Json<GetMemberSyncPreferencesResponse>, ApiError> {
    let runtime = state.runtime_or_error()?;
    let usecases = CoreUseCases::new(runtime.as_ref());

    let prefs = usecases
        .get_member_sync_preferences()
        .execute(&DeviceId::new(device_id.clone()))
        .await
        .map_err(|e| map_member_error(&device_id, "get_member_sync_preferences", e))?;

    Ok(Json(GetMemberSyncPreferencesResponse {
        data: prefs.into(),
        ts: chrono::Utc::now().timestamp_millis(),
    }))
}

/// PATCH /member/:device_id/sync-preferences
/// 部分更新成员的同步偏好；未提供的字段保留当前值。
///
/// 内部 `get → merge → save`，保持与 `UpdateMemberSettingsUseCase` 的全量覆盖语义对齐。
#[utoipa::path(
    patch,
    path = "/member/{device_id}/sync-preferences",
    tag = "member",
    params(
        ("device_id" = String, Path, description = "Space member's device ID")
    ),
    request_body = MemberSyncPreferencesPatchDto,
    responses(
        (status = 200, body = UpdateMemberSyncPreferencesResponse),
        (status = 400, description = "Invalid request", body = crate::api::dto::error::ApiErrorResponse),
        (status = 404, description = "Member not found", body = crate::api::dto::error::ApiErrorResponse),
        (status = 500, description = "Internal server error", body = crate::api::dto::error::ApiErrorResponse)
    )
)]
pub async fn update_member_sync_preferences_handler(
    State(state): State<DaemonApiState>,
    Path(device_id): Path<String>,
    Json(payload): Json<MemberSyncPreferencesPatchDto>,
) -> Result<Json<UpdateMemberSyncPreferencesResponse>, ApiError> {
    let runtime = state.runtime_or_error()?;
    let usecases = CoreUseCases::new(runtime.as_ref());

    let existing = usecases
        .get_member_sync_preferences()
        .execute(&DeviceId::new(device_id.clone()))
        .await
        .map_err(|e| map_member_error(&device_id, "load existing member sync preferences", e))?;

    let merged = merge_member_sync_preferences_patch(existing, payload);

    let updated_member = usecases
        .update_member_sync_preferences()
        .execute(&DeviceId::new(device_id.clone()), merged)
        .await
        .map_err(|e| map_member_error(&device_id, "update_member_sync_preferences", e))?;

    Ok(Json(UpdateMemberSyncPreferencesResponse {
        success: true,
        data: updated_member.sync_preferences.into(),
        ts: chrono::Utc::now().timestamp_millis(),
    }))
}

fn merge_member_sync_preferences_patch(
    mut existing: MemberSyncPreferences,
    patch: MemberSyncPreferencesPatchDto,
) -> MemberSyncPreferences {
    if let Some(v) = patch.send_enabled {
        existing.send_enabled = v;
    }
    if let Some(v) = patch.receive_enabled {
        existing.receive_enabled = v;
    }
    if let Some(send) = patch.send_content_types {
        apply_content_types_patch(&mut existing.send_content_types, send);
    }
    if let Some(receive) = patch.receive_content_types {
        apply_content_types_patch(&mut existing.receive_content_types, receive);
    }
    existing
}

fn apply_content_types_patch(
    target: &mut uc_core::settings::model::ContentTypes,
    patch: crate::api::dto::device::ContentTypesPatchDto,
) {
    if let Some(v) = patch.text {
        target.text = v;
    }
    if let Some(v) = patch.image {
        target.image = v;
    }
    if let Some(v) = patch.link {
        target.link = v;
    }
    if let Some(v) = patch.file {
        target.file = v;
    }
    if let Some(v) = patch.code_snippet {
        target.code_snippet = v;
    }
    if let Some(v) = patch.rich_text {
        target.rich_text = v;
    }
}

/// 把 uc-app UseCase 的 anyhow 错误映射为 HTTP 状态。
///
/// `MembershipApplicationError::NotFound` 在 uc-app 层被包成 anyhow 时保留了
/// "not found" 文案（见 `GetMemberSyncPreferences::execute`），这里做字符串匹配
/// 把它提升为 404；其余视为 500。
fn map_member_error(device_id: &str, context: &str, err: anyhow::Error) -> ApiError {
    let msg = err.to_string();
    tracing::error!(error = %msg, device_id = %device_id, context = context, "member handler failed");
    if msg.to_lowercase().contains("not found") {
        ApiError::not_found(format!("member `{device_id}` not found"))
    } else {
        ApiError::internal(msg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uc_core::settings::model::ContentTypes;

    #[test]
    fn merge_applies_only_provided_fields() {
        let base = MemberSyncPreferences {
            send_enabled: true,
            receive_enabled: true,
            send_content_types: ContentTypes::default(),
            receive_content_types: ContentTypes::default(),
        };
        let patch = MemberSyncPreferencesPatchDto {
            send_enabled: Some(false),
            receive_enabled: None,
            send_content_types: None,
            receive_content_types: None,
        };
        let merged = merge_member_sync_preferences_patch(base.clone(), patch);
        assert!(!merged.send_enabled);
        assert!(merged.receive_enabled);
        assert_eq!(merged.send_content_types, base.send_content_types);
        assert_eq!(merged.receive_content_types, base.receive_content_types);
    }

    #[test]
    fn merge_partial_content_types_patch_retains_unmentioned_fields() {
        let mut base_ct = ContentTypes::default();
        base_ct.text = false;
        base_ct.image = true;
        let base = MemberSyncPreferences {
            send_enabled: true,
            receive_enabled: true,
            send_content_types: base_ct,
            receive_content_types: ContentTypes::default(),
        };
        let patch = MemberSyncPreferencesPatchDto {
            send_enabled: None,
            receive_enabled: None,
            send_content_types: Some(crate::api::dto::device::ContentTypesPatchDto {
                text: Some(true),
                image: None,
                link: None,
                file: None,
                code_snippet: None,
                rich_text: None,
            }),
            receive_content_types: None,
        };
        let merged = merge_member_sync_preferences_patch(base, patch);
        assert!(merged.send_content_types.text);
        assert!(merged.send_content_types.image); // 未提供 → 保留
    }
}
