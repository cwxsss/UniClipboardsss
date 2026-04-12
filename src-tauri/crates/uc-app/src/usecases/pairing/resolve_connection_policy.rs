use std::sync::Arc;
use uc_core::network::{ConnectionPolicy, PairingState, ResolvedConnectionPolicy};
use uc_core::ports::{
    ConnectionPolicyResolverError, ConnectionPolicyResolverPort, PairedDeviceRepositoryPort,
};
use uc_core::PeerId;

pub struct ResolveConnectionPolicy {
    repo: Arc<dyn PairedDeviceRepositoryPort>,
}

#[derive(Debug, thiserror::Error)]
pub enum ResolveConnectionPolicyError {
    #[error("repository error: {0}")]
    Repository(String),
}

impl ResolveConnectionPolicy {
    pub fn new(repo: Arc<dyn PairedDeviceRepositoryPort>) -> Self {
        Self { repo }
    }

    pub async fn execute(
        &self,
        peer_id: PeerId,
    ) -> Result<ResolvedConnectionPolicy, ResolveConnectionPolicyError> {
        let state = match self.repo.get_by_peer_id(&peer_id).await {
            Ok(Some(device)) => device.pairing_state,
            Ok(None) => PairingState::Pending,
            Err(err) => return Err(ResolveConnectionPolicyError::Repository(err.to_string())),
        };

        Ok(ResolvedConnectionPolicy {
            pairing_state: state.clone(),
            allowed: ConnectionPolicy::allowed_protocols(state),
        })
    }
}

#[async_trait::async_trait]
impl ConnectionPolicyResolverPort for ResolveConnectionPolicy {
    async fn resolve_for_peer(
        &self,
        peer_id: &PeerId,
    ) -> Result<ResolvedConnectionPolicy, ConnectionPolicyResolverError> {
        self.execute(peer_id.clone())
            .await
            .map_err(|err| ConnectionPolicyResolverError::Repository(err.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_mocks::MockPairedDeviceRepository;
    use std::sync::Arc;
    use uc_core::network::{PairedDevice, PairingState, ProtocolKind};
    use uc_core::ports::PairedDeviceRepositoryError;

    #[tokio::test]
    async fn unpaired_peer_allows_pairing_only() {
        let mut repo = MockPairedDeviceRepository::new();
        repo.expect_get_by_peer_id().returning(|_| Ok(None));

        let uc = ResolveConnectionPolicy::new(Arc::new(repo));
        let resolved = uc.execute(PeerId::from("peer-1")).await.unwrap();
        assert_eq!(resolved.pairing_state, PairingState::Pending);
        assert!(resolved.allowed.allows(ProtocolKind::Pairing));
        assert!(!resolved.allowed.allows(ProtocolKind::Business));
    }

    #[tokio::test]
    async fn trusted_peer_allows_business() {
        let mut repo = MockPairedDeviceRepository::new();
        repo.expect_get_by_peer_id().returning(|peer_id| {
            Ok(Some(PairedDevice {
                peer_id: peer_id.clone(),
                pairing_state: PairingState::Trusted,
                identity_fingerprint: "fp".to_string(),
                paired_at: chrono::Utc::now(),
                last_seen_at: None,
                device_name: "Mock Device".to_string(),
                sync_settings: None,
            }))
        });

        let uc = ResolveConnectionPolicy::new(Arc::new(repo));
        let resolved = uc.execute(PeerId::from("peer-1")).await.unwrap();
        assert_eq!(resolved.pairing_state, PairingState::Trusted);
        assert!(resolved.allowed.allows(ProtocolKind::Business));
    }

    #[tokio::test]
    async fn repo_failure_returns_error() {
        let mut repo = MockPairedDeviceRepository::new();
        repo.expect_get_by_peer_id().returning(|_| {
            Err(PairedDeviceRepositoryError::Storage(
                "repo failure".to_string(),
            ))
        });

        let uc = ResolveConnectionPolicy::new(Arc::new(repo));
        let result = uc.execute(PeerId::from("peer-1")).await;
        assert!(result.is_err());
    }
}
