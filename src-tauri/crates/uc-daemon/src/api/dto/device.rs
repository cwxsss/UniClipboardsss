use serde::Serialize;
use utoipa::ToSchema;

use uc_app::usecases::pairing::LocalDeviceInfo;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LocalDeviceInfoDto {
    pub peer_id: String,
    pub device_name: String,
}

impl From<LocalDeviceInfo> for LocalDeviceInfoDto {
    fn from(value: LocalDeviceInfo) -> Self {
        Self {
            peer_id: value.peer_id,
            device_name: value.device_name,
        }
    }
}

/// Response wrapper for GET /device/me.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetLocalDeviceInfoResponse {
    pub data: LocalDeviceInfoDto,
    pub ts: i64,
}
