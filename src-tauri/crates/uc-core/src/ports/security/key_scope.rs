use async_trait::async_trait;

use crate::crypto::model::KeyScope;

#[derive(Debug, thiserror::Error)]
pub enum ScopeError {
    #[error("failed to get current scope")]
    FailedToGetCurrentScope,
}

#[async_trait]
pub trait KeyScopePort: Send + Sync {
    async fn current_scope(&self) -> Result<KeyScope, ScopeError>;
}
