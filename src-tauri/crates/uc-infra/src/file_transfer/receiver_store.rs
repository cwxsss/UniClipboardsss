use anyhow::Result;
use diesel::Connection;

use crate::db::ports::DbExecutor;
use crate::file_transfer::event_store::sqlite::{append_event, load_events};
use crate::file_transfer::projection::sqlite::{apply_event, seed_receiver_context};
use crate::file_transfer::ReceiverTransferContext;
use uc_core::file_transfer::FileTransferEvent;

/// Receiver-side durable store that keeps event log and projection updates in one SQLite transaction.
pub struct SqliteReceiverFileTransferStore<E> {
    executor: E,
}

impl<E> SqliteReceiverFileTransferStore<E> {
    pub fn new(executor: E) -> Self {
        Self { executor }
    }
}

impl<E: DbExecutor> SqliteReceiverFileTransferStore<E> {
    pub async fn seed_receiver_context(&self, ctx: ReceiverTransferContext) -> Result<()> {
        self.executor.run(move |conn| {
            conn.transaction::<_, anyhow::Error, _>(|conn| seed_receiver_context(conn, &ctx))
        })
    }

    pub async fn append_event_and_project(&self, event: FileTransferEvent) -> Result<()> {
        self.executor.run(move |conn| {
            conn.transaction::<_, anyhow::Error, _>(|conn| {
                append_event(conn, event.clone())?;
                apply_event(conn, &event)?;
                Ok(())
            })
        })
    }

    pub async fn load_events(&self, transfer_id: &str) -> Result<Vec<FileTransferEvent>> {
        let transfer_id = transfer_id.to_string();
        self.executor
            .run(move |conn| load_events(conn, &transfer_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::executor::DieselSqliteExecutor;
    use crate::db::pool::init_db_pool;
    use crate::db::repositories::DieselFileTransferRepository;
    use tempfile::{tempdir, TempDir};
    use uc_core::ports::FileTransferRepositoryPort;
    use uc_core::{DeviceId, FileTransferDirection, FileTransferProgress};

    fn make_store() -> (
        SqliteReceiverFileTransferStore<DieselSqliteExecutor>,
        DieselFileTransferRepository<DieselSqliteExecutor>,
        TempDir,
    ) {
        let tempdir = tempdir().unwrap();
        let database_url = tempdir.path().join("receiver-file-transfer-store.sqlite");
        let pool = init_db_pool(database_url.to_str().unwrap()).unwrap();
        let store = SqliteReceiverFileTransferStore::new(DieselSqliteExecutor::new(pool.clone()));
        let repo = DieselFileTransferRepository::new(DieselSqliteExecutor::new(pool));

        (store, repo, tempdir)
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
    async fn append_event_and_project_updates_both_event_log_and_projection() {
        let (store, repo, _tempdir) = make_store();
        store
            .seed_receiver_context(receiver_context())
            .await
            .unwrap();

        let announced = FileTransferEvent::announced(
            "transfer-1",
            DeviceId::new("device-1"),
            "report.pdf",
            Some(128),
        );
        let progress = FileTransferEvent::Progress {
            transfer_id: "transfer-1".into(),
            peer_id: "peer-1".into(),
            progress: FileTransferProgress {
                direction: FileTransferDirection::Receiving,
                bytes_transferred: 64,
                total_bytes: Some(128),
            },
        };

        store
            .append_event_and_project(announced.clone())
            .await
            .unwrap();
        store
            .append_event_and_project(progress.clone())
            .await
            .unwrap();

        assert_eq!(
            store.load_events("transfer-1").await.unwrap(),
            vec![announced, progress]
        );

        let transfers = repo.list_transfers_for_entry("entry-1").await.unwrap();
        let transfer = &transfers[0];
        assert_eq!(transfer.file_size, Some(128));
        assert_eq!(
            transfer.status,
            uc_core::ports::file_transfer_repository::TrackedFileTransferStatus::Transferring
        );
    }

    #[tokio::test]
    async fn append_event_and_project_rolls_back_when_projection_fails() {
        let (store, _repo, _tempdir) = make_store();
        let event = FileTransferEvent::completed("missing-transfer", "peer-1");

        let err = store.append_event_and_project(event).await.unwrap_err();

        assert!(err
            .to_string()
            .contains("seed receiver context before applying events"));
        assert!(store
            .load_events("missing-transfer")
            .await
            .unwrap()
            .is_empty());
    }
}
