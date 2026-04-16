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
