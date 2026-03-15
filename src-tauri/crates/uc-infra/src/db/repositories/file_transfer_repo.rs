use async_trait::async_trait;
use diesel::prelude::*;
use tracing::debug_span;

use crate::db::models::{FileTransferRow, NewFileTransferRow};
use crate::db::ports::DbExecutor;
use crate::db::schema::file_transfer;
use uc_core::ports::file_transfer_repository::{
    compute_aggregate_status, EntryTransferSummary, ExpiredInflightTransfer,
    FileTransferRepositoryPort, PendingInboundTransfer, TrackedFileTransfer,
    TrackedFileTransferStatus,
};

/// SQLite adapter for `FileTransferRepositoryPort`.
pub struct DieselFileTransferRepository<E> {
    executor: E,
}

impl<E> DieselFileTransferRepository<E> {
    pub fn new(executor: E) -> Self {
        Self { executor }
    }
}

fn row_to_domain(row: &FileTransferRow) -> TrackedFileTransfer {
    let status = TrackedFileTransferStatus::from_str_value(&row.status)
        .unwrap_or(TrackedFileTransferStatus::Pending);
    TrackedFileTransfer {
        transfer_id: row.transfer_id.clone(),
        entry_id: row.entry_id.clone(),
        origin_device_id: row.source_device.clone(),
        filename: row.filename.clone(),
        cached_path: row.cached_path.clone().unwrap_or_default(),
        status,
        failure_reason: row.failure_reason.clone(),
        file_size: row.file_size,
        content_hash: row.content_hash.clone(),
        updated_at_ms: row.updated_at_ms,
        created_at_ms: row.created_at_ms,
    }
}

fn row_to_expired(row: &FileTransferRow) -> ExpiredInflightTransfer {
    let status = TrackedFileTransferStatus::from_str_value(&row.status)
        .unwrap_or(TrackedFileTransferStatus::Pending);
    ExpiredInflightTransfer {
        transfer_id: row.transfer_id.clone(),
        entry_id: row.entry_id.clone(),
        cached_path: row.cached_path.clone().unwrap_or_default(),
        status,
    }
}

#[async_trait]
impl<E: DbExecutor> FileTransferRepositoryPort for DieselFileTransferRepository<E> {
    async fn insert_pending_transfers(
        &self,
        transfers: &[PendingInboundTransfer],
    ) -> anyhow::Result<()> {
        let span = debug_span!(
            "infra.sqlite.insert_pending_transfers",
            count = transfers.len()
        );
        let rows: Vec<NewFileTransferRow> = transfers
            .iter()
            .map(|t| NewFileTransferRow {
                transfer_id: t.transfer_id.clone(),
                entry_id: t.entry_id.clone(),
                filename: t.filename.clone(),
                file_size: None,
                content_hash: None,
                status: TrackedFileTransferStatus::Pending.as_str().to_string(),
                source_device: t.origin_device_id.clone(),
                cached_path: Some(t.cached_path.clone()),
                failure_reason: None,
                created_at_ms: t.created_at_ms,
                updated_at_ms: t.created_at_ms,
            })
            .collect();

        span.in_scope(|| {
            self.executor.run(|conn| {
                for row in &rows {
                    diesel::insert_into(file_transfer::table)
                        .values(row)
                        .execute(conn)?;
                }
                Ok(())
            })
        })
    }

    async fn backfill_announce_metadata(
        &self,
        transfer_id: &str,
        file_size: i64,
        content_hash: &str,
    ) -> anyhow::Result<()> {
        let span = debug_span!(
            "infra.sqlite.backfill_announce_metadata",
            transfer_id = transfer_id
        );
        let tid = transfer_id.to_string();
        let hash = content_hash.to_string();
        span.in_scope(|| {
            self.executor.run(move |conn| {
                diesel::update(file_transfer::table.filter(file_transfer::transfer_id.eq(&tid)))
                    .set((
                        file_transfer::file_size.eq(Some(file_size)),
                        file_transfer::content_hash.eq(Some(&hash)),
                    ))
                    .execute(conn)?;
                Ok(())
            })
        })
    }

    async fn mark_transferring(&self, transfer_id: &str, now_ms: i64) -> anyhow::Result<bool> {
        let span = debug_span!("infra.sqlite.mark_transferring", transfer_id = transfer_id);
        let tid = transfer_id.to_string();
        span.in_scope(|| {
            self.executor.run(move |conn| {
                let affected = diesel::update(
                    file_transfer::table
                        .filter(file_transfer::transfer_id.eq(&tid))
                        .filter(
                            file_transfer::status
                                .eq(TrackedFileTransferStatus::Pending.as_str())
                                .or(file_transfer::status
                                    .eq(TrackedFileTransferStatus::Transferring.as_str())),
                        ),
                )
                .set((
                    file_transfer::status.eq(TrackedFileTransferStatus::Transferring.as_str()),
                    file_transfer::updated_at_ms.eq(now_ms),
                ))
                .execute(conn)?;
                Ok(affected > 0)
            })
        })
    }

    async fn refresh_activity(&self, transfer_id: &str, now_ms: i64) -> anyhow::Result<()> {
        let tid = transfer_id.to_string();
        self.executor.run(move |conn| {
            diesel::update(file_transfer::table.filter(file_transfer::transfer_id.eq(&tid)))
                .set(file_transfer::updated_at_ms.eq(now_ms))
                .execute(conn)?;
            Ok(())
        })
    }

    async fn mark_completed(
        &self,
        transfer_id: &str,
        content_hash: Option<&str>,
        now_ms: i64,
    ) -> anyhow::Result<()> {
        let tid = transfer_id.to_string();
        let hash = content_hash.map(|h| h.to_string());
        self.executor.run(move |conn| {
            // Always set status and updated_at_ms; optionally set content_hash
            if let Some(h) = &hash {
                diesel::update(file_transfer::table.filter(file_transfer::transfer_id.eq(&tid)))
                    .set((
                        file_transfer::status.eq(TrackedFileTransferStatus::Completed.as_str()),
                        file_transfer::updated_at_ms.eq(now_ms),
                        file_transfer::content_hash.eq(Some(h)),
                    ))
                    .execute(conn)?;
            } else {
                diesel::update(file_transfer::table.filter(file_transfer::transfer_id.eq(&tid)))
                    .set((
                        file_transfer::status.eq(TrackedFileTransferStatus::Completed.as_str()),
                        file_transfer::updated_at_ms.eq(now_ms),
                    ))
                    .execute(conn)?;
            }
            Ok(())
        })
    }

    async fn mark_failed(
        &self,
        transfer_id: &str,
        reason: &str,
        now_ms: i64,
    ) -> anyhow::Result<()> {
        let tid = transfer_id.to_string();
        let reason = reason.to_string();
        self.executor.run(move |conn| {
            diesel::update(file_transfer::table.filter(file_transfer::transfer_id.eq(&tid)))
                .set((
                    file_transfer::status.eq(TrackedFileTransferStatus::Failed.as_str()),
                    file_transfer::failure_reason.eq(Some(&reason)),
                    file_transfer::updated_at_ms.eq(now_ms),
                ))
                .execute(conn)?;
            Ok(())
        })
    }

    async fn list_expired_inflight(
        &self,
        pending_cutoff_ms: i64,
        transferring_cutoff_ms: i64,
    ) -> anyhow::Result<Vec<ExpiredInflightTransfer>> {
        self.executor.run(move |conn| {
            let rows = file_transfer::table
                .filter(
                    file_transfer::status
                        .eq(TrackedFileTransferStatus::Pending.as_str())
                        .and(file_transfer::updated_at_ms.lt(pending_cutoff_ms))
                        .or(file_transfer::status
                            .eq(TrackedFileTransferStatus::Transferring.as_str())
                            .and(file_transfer::updated_at_ms.lt(transferring_cutoff_ms))),
                )
                .load::<FileTransferRow>(conn)?;
            Ok(rows.iter().map(row_to_expired).collect())
        })
    }

    async fn bulk_fail_inflight(
        &self,
        reason: &str,
        now_ms: i64,
    ) -> anyhow::Result<Vec<ExpiredInflightTransfer>> {
        let reason = reason.to_string();
        self.executor.run(move |conn| {
            // First, select all in-flight rows
            let rows = file_transfer::table
                .filter(
                    file_transfer::status
                        .eq(TrackedFileTransferStatus::Pending.as_str())
                        .or(file_transfer::status
                            .eq(TrackedFileTransferStatus::Transferring.as_str())),
                )
                .load::<FileTransferRow>(conn)?;

            let targets: Vec<ExpiredInflightTransfer> = rows.iter().map(row_to_expired).collect();

            // Then bulk-update them to failed
            if !targets.is_empty() {
                diesel::update(
                    file_transfer::table.filter(
                        file_transfer::status
                            .eq(TrackedFileTransferStatus::Pending.as_str())
                            .or(file_transfer::status
                                .eq(TrackedFileTransferStatus::Transferring.as_str())),
                    ),
                )
                .set((
                    file_transfer::status.eq(TrackedFileTransferStatus::Failed.as_str()),
                    file_transfer::failure_reason.eq(Some(&reason)),
                    file_transfer::updated_at_ms.eq(now_ms),
                ))
                .execute(conn)?;
            }

            Ok(targets)
        })
    }

    async fn get_entry_transfer_summary(
        &self,
        entry_id: &str,
    ) -> anyhow::Result<Option<EntryTransferSummary>> {
        let eid = entry_id.to_string();
        self.executor.run(move |conn| {
            let rows = file_transfer::table
                .filter(file_transfer::entry_id.eq(&eid))
                .load::<FileTransferRow>(conn)?;

            if rows.is_empty() {
                return Ok(None);
            }

            let statuses: Vec<TrackedFileTransferStatus> = rows
                .iter()
                .map(|r| {
                    TrackedFileTransferStatus::from_str_value(&r.status)
                        .unwrap_or(TrackedFileTransferStatus::Pending)
                })
                .collect();

            let aggregate_status = match compute_aggregate_status(&statuses) {
                Some(s) => s,
                None => return Ok(None),
            };

            // Pick failure_reason from any failed transfer
            let failure_reason = if aggregate_status == TrackedFileTransferStatus::Failed {
                rows.iter()
                    .find(|r| r.status == TrackedFileTransferStatus::Failed.as_str())
                    .and_then(|r| r.failure_reason.clone())
            } else {
                None
            };

            let transfer_ids = rows.iter().map(|r| r.transfer_id.clone()).collect();

            Ok(Some(EntryTransferSummary {
                entry_id: eid,
                aggregate_status,
                failure_reason,
                transfer_ids,
            }))
        })
    }

    async fn list_transfers_for_entry(
        &self,
        entry_id: &str,
    ) -> anyhow::Result<Vec<TrackedFileTransfer>> {
        let eid = entry_id.to_string();
        self.executor.run(move |conn| {
            let rows = file_transfer::table
                .filter(file_transfer::entry_id.eq(&eid))
                .load::<FileTransferRow>(conn)?;
            Ok(rows.iter().map(row_to_domain).collect())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::pool::init_db_pool;
    use std::sync::Arc;

    /// In-memory test executor for testing repositories.
    #[derive(Clone)]
    struct TestDbExecutor {
        pool: Arc<crate::db::pool::DbPool>,
    }

    impl TestDbExecutor {
        fn new() -> Self {
            let pool = Arc::new(init_db_pool(":memory:").expect("Failed to create test DB pool"));
            Self { pool }
        }
    }

    impl DbExecutor for TestDbExecutor {
        fn run<T>(
            &self,
            f: impl FnOnce(&mut diesel::SqliteConnection) -> anyhow::Result<T>,
        ) -> anyhow::Result<T> {
            let mut conn = self.pool.get()?;
            f(&mut conn)
        }
    }

    fn make_repo() -> DieselFileTransferRepository<TestDbExecutor> {
        DieselFileTransferRepository::new(TestDbExecutor::new())
    }

    fn pending_transfer(transfer_id: &str, entry_id: &str) -> PendingInboundTransfer {
        PendingInboundTransfer {
            transfer_id: transfer_id.to_string(),
            entry_id: entry_id.to_string(),
            origin_device_id: "device-A".to_string(),
            filename: format!("{}.txt", transfer_id),
            cached_path: format!("/cache/{}/{}.txt", transfer_id, transfer_id),
            created_at_ms: 1000,
        }
    }

    #[tokio::test]
    async fn test_seed_creates_pending_rows_with_entry_id() {
        let repo = make_repo();
        let transfers = vec![
            pending_transfer("t1", "entry-1"),
            pending_transfer("t2", "entry-1"),
        ];

        repo.insert_pending_transfers(&transfers)
            .await
            .expect("insert should succeed");

        let list = repo
            .list_transfers_for_entry("entry-1")
            .await
            .expect("list should succeed");

        assert_eq!(list.len(), 2);
        for t in &list {
            assert_eq!(t.entry_id, "entry-1");
            assert_eq!(t.status, TrackedFileTransferStatus::Pending);
            assert!(
                t.file_size.is_none(),
                "file_size should be null before announce"
            );
            assert!(
                t.content_hash.is_none(),
                "content_hash should be null before announce"
            );
        }
    }

    #[tokio::test]
    async fn test_announce_backfill_fills_metadata() {
        let repo = make_repo();
        repo.insert_pending_transfers(&[pending_transfer("t1", "entry-1")])
            .await
            .unwrap();

        repo.backfill_announce_metadata("t1", 4096, "blake3:abc123")
            .await
            .unwrap();

        let list = repo.list_transfers_for_entry("entry-1").await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].file_size, Some(4096));
        assert_eq!(list[0].content_hash.as_deref(), Some("blake3:abc123"));
    }

    #[tokio::test]
    async fn test_failed_rows_retain_failure_reason() {
        let repo = make_repo();
        repo.insert_pending_transfers(&[pending_transfer("t1", "entry-1")])
            .await
            .unwrap();

        repo.mark_failed("t1", "hash mismatch", 2000).await.unwrap();

        let list = repo.list_transfers_for_entry("entry-1").await.unwrap();
        assert_eq!(list[0].status, TrackedFileTransferStatus::Failed);
        assert_eq!(list[0].failure_reason.as_deref(), Some("hash mismatch"));
    }

    #[tokio::test]
    async fn test_aggregate_summary_failed_over_others() {
        let repo = make_repo();
        repo.insert_pending_transfers(&[
            pending_transfer("t1", "entry-1"),
            pending_transfer("t2", "entry-1"),
            pending_transfer("t3", "entry-1"),
        ])
        .await
        .unwrap();

        // t1 = transferring, t2 = failed, t3 = pending
        repo.mark_transferring("t1", 2000).await.unwrap();
        repo.mark_failed("t2", "timeout", 2000).await.unwrap();

        let summary = repo
            .get_entry_transfer_summary("entry-1")
            .await
            .unwrap()
            .expect("summary should exist");

        assert_eq!(summary.aggregate_status, TrackedFileTransferStatus::Failed);
        assert_eq!(summary.failure_reason.as_deref(), Some("timeout"));
        assert_eq!(summary.transfer_ids.len(), 3);
    }

    #[tokio::test]
    async fn test_timeout_query_returns_expired_pending_and_transferring() {
        let repo = make_repo();
        // Two pending at t=1000, one transferring at t=1000
        repo.insert_pending_transfers(&[
            pending_transfer("t1", "entry-1"),
            pending_transfer("t2", "entry-1"),
        ])
        .await
        .unwrap();
        repo.mark_transferring("t2", 1000).await.unwrap();

        // pending_cutoff = 1060000 (60s after t=1000 in ms)
        // transferring_cutoff = 1300000 (5min after t=1000 in ms)
        // But t1 updated_at = 1000 which is < 1060000 => expired pending
        // t2 updated_at = 1000 which is < 1300000 => expired transferring
        let expired = repo.list_expired_inflight(1060001, 1300001).await.unwrap();

        assert_eq!(expired.len(), 2);
        let statuses: Vec<_> = expired.iter().map(|e| e.status).collect();
        assert!(statuses.contains(&TrackedFileTransferStatus::Pending));
        assert!(statuses.contains(&TrackedFileTransferStatus::Transferring));
    }

    #[tokio::test]
    async fn test_startup_reconciliation_only_touches_inflight() {
        let repo = make_repo();
        repo.insert_pending_transfers(&[
            pending_transfer("t1", "entry-1"),
            pending_transfer("t2", "entry-1"),
            pending_transfer("t3", "entry-1"),
        ])
        .await
        .unwrap();

        // t1 stays pending, t2 goes to transferring, t3 goes to completed
        repo.mark_transferring("t2", 2000).await.unwrap();
        repo.mark_completed("t3", Some("hash"), 3000).await.unwrap();

        let cleanup = repo
            .bulk_fail_inflight("orphaned on startup", 5000)
            .await
            .unwrap();

        // Only t1 (pending) and t2 (transferring) should be affected
        assert_eq!(cleanup.len(), 2);
        let ids: Vec<_> = cleanup.iter().map(|c| c.transfer_id.as_str()).collect();
        assert!(ids.contains(&"t1"));
        assert!(ids.contains(&"t2"));
        assert!(!ids.contains(&"t3"));

        // Verify t3 is still completed
        let list = repo.list_transfers_for_entry("entry-1").await.unwrap();
        let t3 = list.iter().find(|t| t.transfer_id == "t3").unwrap();
        assert_eq!(t3.status, TrackedFileTransferStatus::Completed);

        // t1 and t2 should now be failed
        let t1 = list.iter().find(|t| t.transfer_id == "t1").unwrap();
        assert_eq!(t1.status, TrackedFileTransferStatus::Failed);
        assert_eq!(t1.failure_reason.as_deref(), Some("orphaned on startup"));
    }

    #[tokio::test]
    async fn test_mark_completed_is_idempotent() {
        let repo = make_repo();
        repo.insert_pending_transfers(&[pending_transfer("t1", "entry-1")])
            .await
            .unwrap();

        repo.mark_completed("t1", Some("hash1"), 2000)
            .await
            .unwrap();
        // Second call should not fail
        repo.mark_completed("t1", Some("hash1"), 3000)
            .await
            .unwrap();

        let list = repo.list_transfers_for_entry("entry-1").await.unwrap();
        assert_eq!(list[0].status, TrackedFileTransferStatus::Completed);
    }

    #[tokio::test]
    async fn test_mark_failed_is_idempotent() {
        let repo = make_repo();
        repo.insert_pending_transfers(&[pending_transfer("t1", "entry-1")])
            .await
            .unwrap();

        repo.mark_failed("t1", "reason1", 2000).await.unwrap();
        repo.mark_failed("t1", "reason2", 3000).await.unwrap();

        let list = repo.list_transfers_for_entry("entry-1").await.unwrap();
        assert_eq!(list[0].status, TrackedFileTransferStatus::Failed);
        // Last reason wins
        assert_eq!(list[0].failure_reason.as_deref(), Some("reason2"));
    }

    #[tokio::test]
    async fn test_summary_returns_none_for_unknown_entry() {
        let repo = make_repo();
        let summary = repo
            .get_entry_transfer_summary("nonexistent")
            .await
            .unwrap();
        assert!(summary.is_none());
    }

    #[tokio::test]
    async fn test_refresh_activity_bumps_updated_at() {
        let repo = make_repo();
        repo.insert_pending_transfers(&[pending_transfer("t1", "entry-1")])
            .await
            .unwrap();

        repo.refresh_activity("t1", 9999).await.unwrap();

        let list = repo.list_transfers_for_entry("entry-1").await.unwrap();
        assert_eq!(list[0].updated_at_ms, 9999);
    }

    #[tokio::test]
    async fn test_aggregate_all_completed() {
        let repo = make_repo();
        repo.insert_pending_transfers(&[
            pending_transfer("t1", "entry-1"),
            pending_transfer("t2", "entry-1"),
        ])
        .await
        .unwrap();

        repo.mark_completed("t1", None, 2000).await.unwrap();
        repo.mark_completed("t2", None, 3000).await.unwrap();

        let summary = repo
            .get_entry_transfer_summary("entry-1")
            .await
            .unwrap()
            .unwrap();

        assert_eq!(
            summary.aggregate_status,
            TrackedFileTransferStatus::Completed
        );
        assert!(summary.failure_reason.is_none());
    }
}
