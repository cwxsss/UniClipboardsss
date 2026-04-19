//! 空间元数据 port 的内存实现。
//!
//! 仅供单元测试与开发期使用。生产用的 SQLite 实现位于
//! `uc-infra::db::repositories::space_metadata_repo::DieselSpaceMetadataRepository`。

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;

use uc_core::ids::SpaceId;
use uc_core::ports::space_metadata_repository::{SpaceMetadataError, SpaceMetadataRepositoryPort};

/// 进程内内存实现 —— 清晰的 upsert 语义，不带任何持久化。
#[derive(Default, Clone)]
pub struct InMemorySpaceMetadataRepository {
    inner: Arc<RwLock<HashMap<SpaceId, Vec<u8>>>>,
}

impl InMemorySpaceMetadataRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl SpaceMetadataRepositoryPort for InMemorySpaceMetadataRepository {
    async fn save(&self, space_id: &SpaceId, payload: &[u8]) -> Result<(), SpaceMetadataError> {
        self.inner
            .write()
            .await
            .insert(space_id.clone(), payload.to_vec());
        Ok(())
    }

    async fn load(&self, space_id: &SpaceId) -> Result<Option<Vec<u8>>, SpaceMetadataError> {
        Ok(self.inner.read().await.get(space_id).cloned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn save_then_load_returns_same_bytes() {
        let repo = InMemorySpaceMetadataRepository::new();
        let id = SpaceId::from("abc".to_string());
        repo.save(&id, &[1, 2, 3]).await.unwrap();
        assert_eq!(repo.load(&id).await.unwrap(), Some(vec![1, 2, 3]));
    }

    #[tokio::test]
    async fn load_missing_returns_none() {
        let repo = InMemorySpaceMetadataRepository::new();
        assert!(repo
            .load(&SpaceId::from("missing".to_string()))
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn save_is_upsert() {
        let repo = InMemorySpaceMetadataRepository::new();
        let id = SpaceId::from("abc".to_string());
        repo.save(&id, &[1, 2, 3]).await.unwrap();
        repo.save(&id, &[9, 9, 9, 9]).await.unwrap();
        assert_eq!(repo.load(&id).await.unwrap(), Some(vec![9, 9, 9, 9]));
    }
}
