//! Return the sync preferences recorded for a space member.
//!
//! 新路径（phase 4b PR-1）：从 `MemberRepositoryPort` 读取而非 `PairedDeviceRepositoryPort`。
//! 与既有的 `GetDeviceSyncSettings`（读 `paired_device`）并存，以便 daemon / 前端
//! 按需渐进切换，最终在 PR-4 移除旧 UC。

use anyhow::Result;
use std::sync::Arc;

use uc_application::membership::usecases::{GetMember, GetMemberUseCase};
use uc_core::{DeviceId, MemberRepositoryPort, MemberSyncPreferences};

pub struct GetMemberSyncPreferences {
    member_repo: Arc<dyn MemberRepositoryPort>,
}

impl GetMemberSyncPreferences {
    pub fn from_ports(member_repo: Arc<dyn MemberRepositoryPort>) -> Self {
        Self { member_repo }
    }

    pub async fn execute(&self, device_id: &DeviceId) -> Result<MemberSyncPreferences> {
        let uc = GetMemberUseCase::new(self.member_repo.clone());
        let member = uc
            .execute(GetMember {
                device_id: device_id.clone(),
            })
            .await
            .map_err(|e| anyhow::anyhow!("get_member_sync_preferences failed: {}", e))?;
        Ok(member.sync_preferences)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use chrono::Utc;
    use std::sync::Mutex;
    use uc_core::membership::MembershipError;
    use uc_core::SpaceMember;

    struct FakeRepo {
        members: Mutex<Vec<SpaceMember>>,
    }

    impl FakeRepo {
        fn new(seed: Vec<SpaceMember>) -> Self {
            Self {
                members: Mutex::new(seed),
            }
        }
    }

    #[async_trait]
    impl MemberRepositoryPort for FakeRepo {
        async fn get(&self, device_id: &DeviceId) -> Result<Option<SpaceMember>, MembershipError> {
            Ok(self
                .members
                .lock()
                .unwrap()
                .iter()
                .find(|m| &m.device_id == device_id)
                .cloned())
        }

        async fn list(&self) -> Result<Vec<SpaceMember>, MembershipError> {
            Ok(self.members.lock().unwrap().clone())
        }

        async fn save(&self, member: &SpaceMember) -> Result<(), MembershipError> {
            let mut guard = self.members.lock().unwrap();
            if let Some(existing) = guard.iter_mut().find(|m| m.device_id == member.device_id) {
                *existing = member.clone();
            } else {
                guard.push(member.clone());
            }
            Ok(())
        }

        async fn remove(&self, _device_id: &DeviceId) -> Result<bool, MembershipError> {
            Ok(false)
        }
    }

    fn sample_member(peer_id: &str) -> SpaceMember {
        SpaceMember {
            device_id: DeviceId::new(peer_id),
            device_name: "alpha".to_string(),
            identity_fingerprint: "fp".to_string(),
            joined_at: Utc::now(),
            sync_preferences: MemberSyncPreferences {
                send_enabled: false,
                receive_enabled: true,
                ..MemberSyncPreferences::default()
            },
        }
    }

    #[tokio::test]
    async fn returns_preferences_when_member_exists() {
        let repo = Arc::new(FakeRepo::new(vec![sample_member("peer-1")]));
        let uc = GetMemberSyncPreferences::from_ports(repo);

        let prefs = uc.execute(&DeviceId::new("peer-1")).await.unwrap();
        assert!(!prefs.send_enabled);
        assert!(prefs.receive_enabled);
    }

    #[tokio::test]
    async fn errors_when_member_missing() {
        let repo = Arc::new(FakeRepo::new(vec![]));
        let uc = GetMemberSyncPreferences::from_ports(repo);

        let err = uc.execute(&DeviceId::new("peer-ghost")).await.unwrap_err();
        assert!(err.to_string().contains("not found"));
    }
}
