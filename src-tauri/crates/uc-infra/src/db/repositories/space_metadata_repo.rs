//! `SpaceMetadataRepositoryPort` 的 Diesel / SQLite 实现。
//!
//! Payload 是不透明字节 —— 具体格式由
//! `security::space_encryption::payload` 管理，这里只负责 upsert 与读取。

use async_trait::async_trait;
use chrono::Utc;
use diesel::prelude::*;

use uc_core::ids::SpaceId;
use uc_core::ports::space_metadata_repository::{SpaceMetadataError, SpaceMetadataRepositoryPort};

use crate::db::models::{NewSpaceMetadataRow, SpaceMetadataRow};
use crate::db::ports::DbExecutor;
use crate::db::schema::space_metadata::dsl::*;

pub struct DieselSpaceMetadataRepository<E> {
    executor: E,
}

impl<E> DieselSpaceMetadataRepository<E> {
    pub fn new(executor: E) -> Self {
        Self { executor }
    }
}

#[async_trait]
impl<E> SpaceMetadataRepositoryPort for DieselSpaceMetadataRepository<E>
where
    E: DbExecutor + Send + Sync,
{
    async fn save(
        &self,
        id_value: &SpaceId,
        payload_bytes: &[u8],
    ) -> Result<(), SpaceMetadataError> {
        let row = NewSpaceMetadataRow {
            space_id: id_value.as_str().to_string(),
            payload: payload_bytes.to_vec(),
            updated_at_ms: Utc::now().timestamp_millis(),
        };

        self.executor
            .run(move |conn| {
                diesel::insert_into(space_metadata)
                    .values(&row)
                    .on_conflict(space_id)
                    .do_update()
                    .set((
                        payload.eq(row.payload.clone()),
                        updated_at_ms.eq(row.updated_at_ms),
                    ))
                    .execute(conn)
                    .map_err(|e| anyhow::anyhow!(e.to_string()))?;
                Ok(())
            })
            .map_err(SpaceMetadataError::Backend)
    }

    async fn load(&self, id_value: &SpaceId) -> Result<Option<Vec<u8>>, SpaceMetadataError> {
        let id = id_value.as_str().to_string();
        self.executor
            .run(move |conn| {
                let row = space_metadata
                    .filter(space_id.eq(&id))
                    .first::<SpaceMetadataRow>(conn)
                    .optional()
                    .map_err(|e| anyhow::anyhow!(e.to_string()))?;
                Ok(row.map(|r| r.payload))
            })
            .map_err(SpaceMetadataError::Backend)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::executor::DieselSqliteExecutor;
    use crate::db::pool::init_db_pool;
    use tempfile::{tempdir, TempDir};

    fn make_repo() -> (DieselSpaceMetadataRepository<DieselSqliteExecutor>, TempDir) {
        let td = tempdir().unwrap();
        let url = td.path().join("space-metadata.sqlite");
        let pool = init_db_pool(url.to_str().unwrap()).unwrap();
        (
            DieselSpaceMetadataRepository::new(DieselSqliteExecutor::new(pool)),
            td,
        )
    }

    #[tokio::test]
    async fn save_then_load_roundtrip() {
        let (repo, _td) = make_repo();
        let id = SpaceId::from("space-a".to_string());
        repo.save(&id, &[1, 2, 3, 4]).await.unwrap();
        assert_eq!(repo.load(&id).await.unwrap(), Some(vec![1, 2, 3, 4]));
    }

    #[tokio::test]
    async fn load_missing_returns_none() {
        let (repo, _td) = make_repo();
        assert!(repo
            .load(&SpaceId::from("nope".to_string()))
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn save_is_upsert() {
        let (repo, _td) = make_repo();
        let id = SpaceId::from("space-x".to_string());
        repo.save(&id, &[1]).await.unwrap();
        repo.save(&id, &[9, 9, 9]).await.unwrap();
        assert_eq!(repo.load(&id).await.unwrap(), Some(vec![9, 9, 9]));
    }

    #[tokio::test]
    async fn independent_spaces_do_not_overwrite() {
        let (repo, _td) = make_repo();
        repo.save(&SpaceId::from("a".to_string()), &[1])
            .await
            .unwrap();
        repo.save(&SpaceId::from("b".to_string()), &[2])
            .await
            .unwrap();
        assert_eq!(
            repo.load(&SpaceId::from("a".to_string())).await.unwrap(),
            Some(vec![1])
        );
        assert_eq!(
            repo.load(&SpaceId::from("b".to_string())).await.unwrap(),
            Some(vec![2])
        );
    }
}
