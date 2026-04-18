use std::sync::Arc;
use uc_core::network::{ConnectionPolicy, ResolvedConnectionPolicy};
use uc_core::pairing::PairingState;
use uc_core::ports::{ConnectionPolicyResolverError, ConnectionPolicyResolverPort};
use uc_core::{DeviceId, MemberRepositoryPort, PeerId};

pub struct ResolveConnectionPolicy {
    member_repo: Arc<dyn MemberRepositoryPort>,
}

#[derive(Debug, thiserror::Error)]
pub enum ResolveConnectionPolicyError {
    #[error("repository error: {0}")]
    Repository(String),
}

impl ResolveConnectionPolicy {
    pub fn new(member_repo: Arc<dyn MemberRepositoryPort>) -> Self {
        Self { member_repo }
    }

    pub async fn execute(
        &self,
        peer_id: PeerId,
    ) -> Result<ResolvedConnectionPolicy, ResolveConnectionPolicyError> {
        let device_id = DeviceId::new(peer_id.as_str());
        let state = match self.member_repo.get(&device_id).await {
            Ok(Some(_)) => PairingState::Trusted,
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
