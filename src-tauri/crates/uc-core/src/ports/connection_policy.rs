use crate::ids::PeerId;
use crate::network::connection_policy::ResolvedConnectionPolicy;
use async_trait::async_trait;

#[derive(Debug, thiserror::Error)]
pub enum ConnectionPolicyResolverError {
    #[error("repository error: {0}")]
    Repository(String),
}

#[async_trait]
pub trait ConnectionPolicyResolverPort: Send + Sync {
    async fn resolve_for_peer(
        &self,
        peer_id: &PeerId,
    ) -> Result<ResolvedConnectionPolicy, ConnectionPolicyResolverError>;
}
