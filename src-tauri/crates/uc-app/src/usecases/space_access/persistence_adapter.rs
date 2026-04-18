use std::sync::Arc;

use async_trait::async_trait;
use tracing::info;

use uc_core::ids::{DeviceId, PeerId, SpaceId};
use uc_core::pairing::PairingState;
use uc_core::ports::paired_device_repository::PairedDeviceRepositoryPort;
use uc_core::ports::security::encryption_state::EncryptionStatePort;
use uc_core::ports::space::PersistencePort;
use uc_core::TrustedPeerRepositoryPort;

pub struct SpaceAccessPersistenceAdapter {
    encryption_state: Arc<dyn EncryptionStatePort>,
    paired_device_repo: Arc<dyn PairedDeviceRepositoryPort>,
    trusted_peer_repo: Arc<dyn TrustedPeerRepositoryPort>,
}

enum TrustPromotionSource {
    TrustedPeer,
    Repository,
}

impl SpaceAccessPersistenceAdapter {
    pub fn new(
        encryption_state: Arc<dyn EncryptionStatePort>,
        paired_device_repo: Arc<dyn PairedDeviceRepositoryPort>,
        trusted_peer_repo: Arc<dyn TrustedPeerRepositoryPort>,
    ) -> Self {
        Self {
            encryption_state,
            paired_device_repo,
            trusted_peer_repo,
        }
    }

    async fn promote_peer_to_trusted(&self, peer_id: &str) -> anyhow::Result<TrustPromotionSource> {
        let device_id = DeviceId::new(peer_id.to_string());
        if self
            .trusted_peer_repo
            .get(&device_id)
            .await
            .map_err(|err| anyhow::anyhow!("trusted_peer.get failed: {err}"))?
            .is_some()
        {
            return Ok(TrustPromotionSource::TrustedPeer);
        }

        self.paired_device_repo
            .set_state(&PeerId::from(peer_id), PairingState::Trusted)
            .await?;
        Ok(TrustPromotionSource::Repository)
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
        info!(peer_id = %peer_id, "Persisting joiner access and promoting peer trust");
        self.encryption_state.persist_initialized().await?;
        let source = self.promote_peer_to_trusted(peer_id).await?;
        match source {
            TrustPromotionSource::TrustedPeer => info!(
                peer_id = %peer_id,
                source = "trusted_peer",
                target_state = "Trusted",
                "Joiner access confirmed via trusted_peer repository"
            ),
            TrustPromotionSource::Repository => info!(
                peer_id = %peer_id,
                source = "repository",
                target_state = "Trusted",
                "Joiner access persisted with repository state update"
            ),
        }
        Ok(())
    }

    #[tracing::instrument(skip(self, _space_id), fields(peer_id = %peer_id))]
    async fn persist_sponsor_access(
        &mut self,
        _space_id: &SpaceId,
        peer_id: &str,
    ) -> anyhow::Result<()> {
        info!(peer_id = %peer_id, "Persisting sponsor access and promoting peer trust");
        let source = self.promote_peer_to_trusted(peer_id).await?;
        match source {
            TrustPromotionSource::TrustedPeer => info!(
                peer_id = %peer_id,
                source = "trusted_peer",
                target_state = "Trusted",
                "Sponsor access confirmed via trusted_peer repository"
            ),
            TrustPromotionSource::Repository => info!(
                peer_id = %peer_id,
                source = "repository",
                target_state = "Trusted",
                "Sponsor access persisted with repository state update"
            ),
        }
        Ok(())
    }
}
