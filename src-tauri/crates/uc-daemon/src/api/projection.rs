use uc_core::SpaceMember;

use crate::api::types::SpaceMemberDto;

pub trait IntoApiDto<T> {
    fn into_api_dto(self) -> T;
}

impl IntoApiDto<SpaceMemberDto> for SpaceMember {
    fn into_api_dto(self) -> SpaceMemberDto {
        SpaceMemberDto {
            peer_id: self.device_id.as_str().to_string(),
            device_name: self.device_name,
            pairing_state: "Trusted".to_string(),
            last_seen_at_ms: None,
            connected: false,
        }
    }
}
