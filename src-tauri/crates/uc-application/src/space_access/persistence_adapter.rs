use std::sync::Arc;

use async_trait::async_trait;
use tracing::{info, warn};

use uc_core::ids::{DeviceId, SpaceId};
use uc_core::ports::space::PersistencePort;
use uc_core::TrustedPeerRepositoryPort;

pub struct SpaceAccessPersistenceAdapter {
    trusted_peer_repo: Arc<dyn TrustedPeerRepositoryPort>,
}

impl SpaceAccessPersistenceAdapter {
    pub fn new(trusted_peer_repo: Arc<dyn TrustedPeerRepositoryPort>) -> Self {
        Self { trusted_peer_repo }
    }

    /// 确认对端已存在于 `trusted_peer` 表。
    ///
    /// 阶段 0.5 起 `paired_device` 表进入只读，本方法不再回填旧表；
    /// 若 `trusted_peer` 表未命中则说明尚未完成 pairing 协议，
    /// 记 WARN 继续放行（与 pairing 双写时代的语义一致，不阻塞 space_access）。
    async fn ensure_peer_trusted(&self, peer_id: &str) -> anyhow::Result<()> {
        let device_id = DeviceId::new(peer_id.to_string());
        let hit = self
            .trusted_peer_repo
            .get(&device_id)
            .await
            .map_err(|err| anyhow::anyhow!("trusted_peer.get failed: {err}"))?
            .is_some();
        if !hit {
            warn!(
                peer_id = %peer_id,
                "trusted_peer record missing during space_access persistence; continuing anyway"
            );
        }
        Ok(())
    }
}

#[async_trait]
impl PersistencePort for SpaceAccessPersistenceAdapter {
    #[tracing::instrument(skip(self, _space_id), fields(peer_id = %peer_id))]
    async fn persist_joiner_access(
        &mut self,
        _space_id: &SpaceId,
        peer_id: &str,
    ) -> anyhow::Result<()> {
        info!(peer_id = %peer_id, "Confirming joiner peer trust");
        // Phase C 起不再写 `.initialized_encryption` marker;"本机已初始化"
        // 由磁盘 keyslot 文件存在性回答, setup 完成由 `SetupStatusPort.has_completed`
        // 在 setup flow 统一收口标记。
        self.ensure_peer_trusted(peer_id).await?;
        info!(
            peer_id = %peer_id,
            source = "trusted_peer",
            "Joiner access confirmed via trusted_peer repository"
        );
        Ok(())
    }

    #[tracing::instrument(skip(self, _space_id), fields(peer_id = %peer_id))]
    async fn persist_sponsor_access(
        &mut self,
        _space_id: &SpaceId,
        peer_id: &str,
    ) -> anyhow::Result<()> {
        info!(peer_id = %peer_id, "Persisting sponsor access and confirming peer trust");
        self.ensure_peer_trusted(peer_id).await?;
        info!(
            peer_id = %peer_id,
            source = "trusted_peer",
            "Sponsor access confirmed via trusted_peer repository"
        );
        Ok(())
    }
}
