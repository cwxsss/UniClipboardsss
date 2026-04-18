use serde::Serialize;
use utoipa::ToSchema;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LocalDeviceInfoDto {
    pub peer_id: String,
    pub device_name: String,
}

/// Response wrapper for GET /device/me.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetLocalDeviceInfoResponse {
    pub data: LocalDeviceInfoDto,
    pub ts: i64,
}
