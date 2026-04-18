use std::sync::Arc;

use chrono::{DateTime, Utc};
use uc_core::{DeviceId, MemberRepositoryPort, MemberSyncPreferences, SpaceMember};

use crate::membership::errors::MembershipApplicationError;

/// Input for admitting a new member.
///
/// 典型触发点：`space_access` 完成后，由上层流程把对端设备登记进本机成员列表。
/// 本机成员关系是**本地自治**的，不会广播给其他设备。
#[derive(Debug, Clone)]
pub struct AdmitMember {
    pub device_id: DeviceId,
    pub device_name: String,
    pub identity_fingerprint: String,
    pub joined_at: DateTime<Utc>,
    pub sync_preferences: MemberSyncPreferences,
}

/// 将新设备接纳为本机空间成员。
///
/// 幂等策略：同一 `device_id` 重复 admit 会返回 `AlreadyAdmitted` 错误。
pub struct AdmitMemberUseCase<R> {
    repository: Arc<R>,
}

impl<R> AdmitMemberUseCase<R>
where
    R: MemberRepositoryPort,
{
    pub fn new(repository: Arc<R>) -> Self {
        Self { repository }
    }

    pub async fn execute(
        &self,
        input: AdmitMember,
    ) -> Result<SpaceMember, MembershipApplicationError> {
        if self.repository.get(&input.device_id).await?.is_some() {
            return Err(MembershipApplicationError::AlreadyAdmitted(input.device_id));
        }

        let member = SpaceMember {
            device_id: input.device_id,
            device_name: input.device_name,
            identity_fingerprint: input.identity_fingerprint,
            joined_at: input.joined_at,
            sync_preferences: input.sync_preferences,
        };

        self.repository.save(&member).await?;
        Ok(member)
    }
}
