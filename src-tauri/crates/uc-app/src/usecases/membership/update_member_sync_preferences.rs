//! Full-overwrite update of a member's sync preferences.
//!
//! 新路径（phase 4b PR-1）：写入 `MemberRepositoryPort` 而非 `PairedDeviceRepositoryPort`。
//! 语义为全量覆盖（`UpdateMemberSettingsUseCase` 约定）；若调用方只想改部分字段，
//! 应先 `GetMemberSyncPreferences` 取出当前值、合并后再调本用例。
//!
//! 与既有的 `UpdateDeviceSyncSettings` 并存，PR-4 移除旧 UC。

use anyhow::Result;
use std::sync::Arc;

use uc_application::membership::usecases::{UpdateMemberSettings, UpdateMemberSettingsUseCase};
use uc_core::{DeviceId, MemberRepositoryPort, MemberSyncPreferences, SpaceMember};

pub struct UpdateMemberSyncPreferences {
    member_repo: Arc<dyn MemberRepositoryPort>,
}

impl UpdateMemberSyncPreferences {
    pub fn from_ports(member_repo: Arc<dyn MemberRepositoryPort>) -> Self {
        Self { member_repo }
    }

    pub async fn execute(
        &self,
        device_id: &DeviceId,
        preferences: MemberSyncPreferences,
    ) -> Result<SpaceMember> {
        let uc = UpdateMemberSettingsUseCase::new(self.member_repo.clone());
        uc.execute(UpdateMemberSettings {
            device_id: device_id.clone(),
            sync_preferences: preferences,
        })
        .await
        .map_err(|e| anyhow::anyhow!("update_member_sync_preferences failed: {}", e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use chrono::Utc;
    use std::sync::Mutex;
    use uc_core::membership::MembershipError;
    use uc_core::settings::model::ContentTypes;

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

    fn sample_fingerprint() -> uc_core::security::IdentityFingerprint {
        uc_core::security::IdentityFingerprint::from_raw_string("FPAAAAAAAAAAAAAA").unwrap()
    }

    fn sample_member(peer_id: &str) -> SpaceMember {
        SpaceMember {
            device_id: DeviceId::new(peer_id),
            device_name: "alpha".to_string(),
            identity_fingerprint: sample_fingerprint(),
            joined_at: Utc::now(),
            sync_preferences: MemberSyncPreferences::default(),
        }
    }

    #[tokio::test]
    async fn overwrites_preferences_and_preserves_identity_fields() {
        let repo = Arc::new(FakeRepo::new(vec![sample_member("peer-1")]));
        let uc = UpdateMemberSyncPreferences::from_ports(repo.clone());

        let new_prefs = MemberSyncPreferences {
            send_enabled: false,
            receive_enabled: true,
            send_content_types: ContentTypes {
                text: true,
                image: false,
                link: false,
                file: false,
                code_snippet: false,
                rich_text: false,
            },
            receive_content_types: ContentTypes::default(),
        };

        let updated = uc
            .execute(&DeviceId::new("peer-1"), new_prefs.clone())
            .await
            .unwrap();

        assert_eq!(updated.device_name, "alpha");
        assert_eq!(updated.identity_fingerprint, sample_fingerprint());
        assert_eq!(updated.sync_preferences, new_prefs);

        let persisted = repo.get(&DeviceId::new("peer-1")).await.unwrap().unwrap();
        assert_eq!(persisted.sync_preferences, new_prefs);
    }

    #[tokio::test]
    async fn errors_when_member_missing() {
        let repo = Arc::new(FakeRepo::new(vec![]));
        let uc = UpdateMemberSyncPreferences::from_ports(repo);

        let err = uc
            .execute(
                &DeviceId::new("peer-ghost"),
                MemberSyncPreferences::default(),
            )
            .await
            .unwrap_err();
        assert!(err.to_string().contains("not found"));
    }
}
