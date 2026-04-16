use std::sync::Arc;

use async_trait::async_trait;
use tracing::{info, warn};

use crate::usecases::pairing::staged_paired_device_store::StagedPairedDeviceStore;
use uc_core::ids::{PeerId, SpaceId};
use uc_core::pairing::PairingState;
use uc_core::ports::paired_device_repository::PairedDeviceRepositoryPort;
use uc_core::ports::security::encryption_state::EncryptionStatePort;
use uc_core::ports::space::PersistencePort;

pub struct SpaceAccessPersistenceAdapter {
    encryption_state: Arc<dyn EncryptionStatePort>,
    paired_device_repo: Arc<dyn PairedDeviceRepositoryPort>,
    staged_store: Arc<StagedPairedDeviceStore>,
}

enum TrustPromotionSource {
    Staged,
    Repository,
}

impl SpaceAccessPersistenceAdapter {
    pub fn new(
        encryption_state: Arc<dyn EncryptionStatePort>,
        paired_device_repo: Arc<dyn PairedDeviceRepositoryPort>,
        staged_store: Arc<StagedPairedDeviceStore>,
    ) -> Self {
        Self {
            encryption_state,
            paired_device_repo,
            staged_store,
        }
    }

    async fn promote_peer_to_trusted(&self, peer_id: &str) -> anyhow::Result<TrustPromotionSource> {
        if let Some(mut staged_device) = self.staged_store.get_by_peer_id(peer_id) {
            staged_device.pairing_state = PairingState::Trusted;
            self.paired_device_repo.upsert(staged_device).await?;
            if self.staged_store.take_by_peer_id(peer_id).is_none() {
                warn!(
                    peer_id = %peer_id,
                    operation = "take_by_peer_id",
                    "take_by_peer_id failed: no staged state found"
                );
            }
            return Ok(TrustPromotionSource::Staged);
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
            TrustPromotionSource::Staged => info!(
                peer_id = %peer_id,
                source = "staged",
                target_state = "Trusted",
                "Joiner access persisted with staged paired device"
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
            TrustPromotionSource::Staged => info!(
                peer_id = %peer_id,
                source = "staged",
                target_state = "Trusted",
                "Sponsor access persisted with staged paired device"
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
