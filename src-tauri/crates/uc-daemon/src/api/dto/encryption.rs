use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Request payload for POST /encryption/unlock.
#[derive(Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UnlockRequest {
    pub passphrase: String,
}

/// Response payload for GET /encryption/state.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EncryptionStateResponse {
    pub initialized: bool,
    pub session_ready: bool,
}

/// Internal event payload for the encryption.session_ready WS event.
/// Serialized as part of DaemonWsEvent payload.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EncryptionSessionReadyPayload {
    pub ts: i64,
}
