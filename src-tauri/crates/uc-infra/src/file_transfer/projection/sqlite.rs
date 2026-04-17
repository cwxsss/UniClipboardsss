use anyhow::{bail, Result};
use chrono::Utc;
use diesel::prelude::*;
use diesel::upsert::excluded;

use crate::db::models::NewFileTransferRow;
use crate::db::ports::DbExecutor;
use crate::db::schema::file_transfer;
use uc_core::file_transfer::{
    FileTransferCancellationReason, FileTransferEvent, FileTransferFailureReason,
};
use uc_core::ports::file_transfer_repository::TrackedFileTransferStatus;

/// Receiver-side local context required to materialize the `file_transfer` projection.
///
/// 这些字段属于接收侧本地上下文，不进入 `uc-core::file_transfer` 事件模型。
#[derive(Debug, Clone)]
pub struct ReceiverTransferContext {
    pub transfer_id: String,
    pub entry_id: String,
    pub origin_device_id: String,
    pub filename: String,
    pub cached_path: String,
    pub created_at_ms: i64,
}

/// SQLite projection updater for receiver-side file transfer snapshots.
pub struct SqliteReceiverFileTransferProjectionUpdater<E> {
    executor: E,
}

impl<E> SqliteReceiverFileTransferProjectionUpdater<E> {
    pub fn new(executor: E) -> Self {
        Self { executor }
    }
}

impl<E: DbExecutor> SqliteReceiverFileTransferProjectionUpdater<E> {
    pub async fn seed_receiver_context(&self, ctx: ReceiverTransferContext) -> Result<()> {
        self.executor
            .run(move |conn| seed_receiver_context(conn, &ctx))
    }

    pub async fn apply_event(&self, event: &FileTransferEvent) -> Result<()> {
        let event = event.clone();
        self.executor.run(move |conn| apply_event(conn, &event))
    }
}

fn seed_receiver_context(
    conn: &mut diesel::sqlite::SqliteConnection,
    ctx: &ReceiverTransferContext,
) -> Result<()> {
    let row = NewFileTransferRow {
        transfer_id: ctx.transfer_id.clone(),
        entry_id: ctx.entry_id.clone(),
        filename: ctx.filename.clone(),
        file_size: None,
        content_hash: None,
        status: TrackedFileTransferStatus::Pending.as_str().to_string(),
        source_device: ctx.origin_device_id.clone(),
        cached_path: Some(ctx.cached_path.clone()),
        failure_reason: None,
        created_at_ms: ctx.created_at_ms,
        updated_at_ms: ctx.created_at_ms,
    };

    diesel::insert_into(file_transfer::table)
        .values(&row)
        .on_conflict(file_transfer::transfer_id)
        .do_update()
        .set((
            file_transfer::entry_id.eq(excluded(file_transfer::entry_id)),
            file_transfer::filename.eq(excluded(file_transfer::filename)),
            file_transfer::source_device.eq(excluded(file_transfer::source_device)),
            file_transfer::cached_path.eq(excluded(file_transfer::cached_path)),
        ))
        .execute(conn)?;

    Ok(())
}

fn apply_event(
    conn: &mut diesel::sqlite::SqliteConnection,
    event: &FileTransferEvent,
) -> Result<()> {
    let now_ms = Utc::now().timestamp_millis();
    let transfer_id = transfer_id_of(event).to_string();

    let affected = match event {
        FileTransferEvent::Announced {
            transfer_id,
            origin_device_id,
            filename,
            file_size,
        } => {
            diesel::update(file_transfer::table.filter(file_transfer::transfer_id.eq(transfer_id)))
                .set((
                    file_transfer::status.eq(TrackedFileTransferStatus::Pending.as_str()),
                    file_transfer::filename.eq(filename.as_str()),
                    file_transfer::source_device.eq(origin_device_id.to_string()),
                    file_transfer::file_size.eq(file_size.map(u64_to_i64)),
                    file_transfer::updated_at_ms.eq(now_ms),
                ))
                .execute(conn)?
        }
        FileTransferEvent::Started {
            transfer_id,
            filename,
            file_size,
            ..
        } => {
            diesel::update(file_transfer::table.filter(file_transfer::transfer_id.eq(transfer_id)))
                .set((
                    file_transfer::status.eq(TrackedFileTransferStatus::Transferring.as_str()),
                    file_transfer::filename.eq(filename.as_str()),
                    file_transfer::file_size.eq(file_size.map(u64_to_i64)),
                    file_transfer::updated_at_ms.eq(now_ms),
                ))
                .execute(conn)?
        }
        FileTransferEvent::Progress { transfer_id, .. } => {
            diesel::update(file_transfer::table.filter(file_transfer::transfer_id.eq(transfer_id)))
                .set((
                    file_transfer::status.eq(TrackedFileTransferStatus::Transferring.as_str()),
                    file_transfer::updated_at_ms.eq(now_ms),
                ))
                .execute(conn)?
        }
        FileTransferEvent::Completed { transfer_id, .. } => {
            diesel::update(file_transfer::table.filter(file_transfer::transfer_id.eq(transfer_id)))
                .set((
                    file_transfer::status.eq(TrackedFileTransferStatus::Completed.as_str()),
                    file_transfer::updated_at_ms.eq(now_ms),
                ))
                .execute(conn)?
        }
        FileTransferEvent::Failed {
            transfer_id,
            reason,
            ..
        } => {
            diesel::update(file_transfer::table.filter(file_transfer::transfer_id.eq(transfer_id)))
                .set((
                    file_transfer::status.eq(TrackedFileTransferStatus::Failed.as_str()),
                    file_transfer::failure_reason.eq(Some(failure_reason_of(*reason))),
                    file_transfer::updated_at_ms.eq(now_ms),
                ))
                .execute(conn)?
        }
        FileTransferEvent::Cancelled {
            transfer_id,
            reason,
            ..
        } => {
            diesel::update(file_transfer::table.filter(file_transfer::transfer_id.eq(transfer_id)))
                .set((
                    file_transfer::status.eq(TrackedFileTransferStatus::Failed.as_str()),
                    file_transfer::failure_reason.eq(Some(cancellation_reason_of(*reason))),
                    file_transfer::updated_at_ms.eq(now_ms),
                ))
                .execute(conn)?
        }
    };

    if affected == 0 {
        bail!(
            "receiver-side file transfer projection row missing for `{}`; seed receiver context before applying events",
            transfer_id
        );
    }

    Ok(())
}

fn transfer_id_of(event: &FileTransferEvent) -> &str {
    match event {
        FileTransferEvent::Announced { transfer_id, .. }
        | FileTransferEvent::Started { transfer_id, .. }
        | FileTransferEvent::Progress { transfer_id, .. }
        | FileTransferEvent::Completed { transfer_id, .. }
        | FileTransferEvent::Failed { transfer_id, .. }
        | FileTransferEvent::Cancelled { transfer_id, .. } => transfer_id,
    }
}

fn u64_to_i64(value: u64) -> i64 {
    value.min(i64::MAX as u64) as i64
}

fn failure_reason_of(reason: FileTransferFailureReason) -> &'static str {
    match reason {
        FileTransferFailureReason::NetworkUnavailable => "network_unavailable",
        FileTransferFailureReason::TimedOut => "timed_out",
        FileTransferFailureReason::AccessDenied => "access_denied",
        FileTransferFailureReason::StorageUnavailable => "storage_unavailable",
        FileTransferFailureReason::IntegrityCheckFailed => "integrity_check_failed",
        FileTransferFailureReason::Unknown => "unknown",
    }
}

fn cancellation_reason_of(reason: FileTransferCancellationReason) -> &'static str {
    match reason {
        FileTransferCancellationReason::LocalUser => "cancelled:local_user",
        FileTransferCancellationReason::RemotePeer => "cancelled:remote_peer",
        FileTransferCancellationReason::Replaced => "cancelled:replaced",
        FileTransferCancellationReason::Unknown => "cancelled:unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::executor::DieselSqliteExecutor;
    use crate::db::pool::init_db_pool;
    use crate::db::repositories::DieselFileTransferRepository;
    use tempfile::{tempdir, TempDir};
    use uc_core::file_transfer::FileTransferProgress;
    use uc_core::ports::FileTransferRepositoryPort;
    use uc_core::{DeviceId, FileTransferDirection};

    fn make_updater() -> (
        SqliteReceiverFileTransferProjectionUpdater<DieselSqliteExecutor>,
        DieselFileTransferRepository<DieselSqliteExecutor>,
        TempDir,
    ) {
        let tempdir = tempdir().unwrap();
        let database_url = tempdir.path().join("file-transfer-projection.sqlite");
        let pool = init_db_pool(database_url.to_str().unwrap()).unwrap();
        let updater = SqliteReceiverFileTransferProjectionUpdater::new(DieselSqliteExecutor::new(
            pool.clone(),
        ));
        let repo = DieselFileTransferRepository::new(DieselSqliteExecutor::new(pool));

        (updater, repo, tempdir)
    }

    fn receiver_context() -> ReceiverTransferContext {
        ReceiverTransferContext {
            transfer_id: "transfer-1".into(),
            entry_id: "entry-1".into(),
            origin_device_id: "device-1".into(),
            filename: "report.pdf".into(),
            cached_path: "/tmp/report.pdf".into(),
            created_at_ms: 10,
        }
    }

    #[tokio::test]
    async fn seed_receiver_context_creates_pending_projection_row() {
        let (updater, repo, _tempdir) = make_updater();

        updater
            .seed_receiver_context(receiver_context())
            .await
            .unwrap();

        let transfers = repo.list_transfers_for_entry("entry-1").await.unwrap();
        assert_eq!(transfers.len(), 1);
        let transfer = &transfers[0];
        assert_eq!(transfer.transfer_id, "transfer-1");
        assert_eq!(transfer.origin_device_id, "device-1");
        assert_eq!(transfer.status, TrackedFileTransferStatus::Pending);
        assert_eq!(transfer.cached_path, "/tmp/report.pdf");
        assert_eq!(transfer.file_size, None);
    }

    #[tokio::test]
    async fn apply_event_projects_announced_started_and_completed_states() {
        let (updater, repo, _tempdir) = make_updater();
        updater
            .seed_receiver_context(receiver_context())
            .await
            .unwrap();

        updater
            .apply_event(&FileTransferEvent::announced(
                "transfer-1",
                DeviceId::new("device-1"),
                "report.pdf",
                Some(128),
            ))
            .await
            .unwrap();
        updater
            .apply_event(&FileTransferEvent::started(
                "transfer-1",
                "peer-1",
                "report.pdf",
                Some(128),
            ))
            .await
            .unwrap();
        updater
            .apply_event(&FileTransferEvent::completed("transfer-1", "peer-1"))
            .await
            .unwrap();

        let transfers = repo.list_transfers_for_entry("entry-1").await.unwrap();
        let transfer = &transfers[0];
        assert_eq!(transfer.status, TrackedFileTransferStatus::Completed);
        assert_eq!(transfer.file_size, Some(128));
    }

    #[tokio::test]
    async fn progress_and_cancelled_events_update_projection_as_expected() {
        let (updater, repo, _tempdir) = make_updater();
        updater
            .seed_receiver_context(receiver_context())
            .await
            .unwrap();

        updater
            .apply_event(&FileTransferEvent::Progress {
                transfer_id: "transfer-1".into(),
                peer_id: "peer-1".into(),
                progress: FileTransferProgress {
                    direction: FileTransferDirection::Receiving,
                    bytes_transferred: 64,
                    total_bytes: Some(128),
                },
            })
            .await
            .unwrap();
        updater
            .apply_event(&FileTransferEvent::cancelled(
                "transfer-1",
                "peer-1",
                FileTransferCancellationReason::RemotePeer,
            ))
            .await
            .unwrap();

        let transfers = repo.list_transfers_for_entry("entry-1").await.unwrap();
        let transfer = &transfers[0];
        assert_eq!(transfer.status, TrackedFileTransferStatus::Failed);
        assert_eq!(
            transfer.failure_reason.as_deref(),
            Some("cancelled:remote_peer")
        );
    }

    #[tokio::test]
    async fn apply_event_requires_seeded_receiver_context() {
        let (updater, _repo, _tempdir) = make_updater();

        let err = updater
            .apply_event(&FileTransferEvent::completed("missing-transfer", "peer-1"))
            .await
            .unwrap_err();

        assert!(err
            .to_string()
            .contains("seed receiver context before applying events"));
    }
}
