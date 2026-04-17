//! File-transfer lifecycle wiring.
//!
//! Groups the durable event store, host-event publisher, and the six
//! lifecycle use cases so the composition root can hand a single bundle
//! to background workers.

use std::sync::{Arc, RwLock};

use uc_app::shared::host_event::HostEventEmitterPort;
use uc_app::shared::host_event_publisher::FileTransferHostEventPublisher;
use uc_application::file_transfer::{
    AnnounceTransferUseCase, CancelTransferUseCase, CompleteTransferUseCase, FailTransferUseCase,
    ReportTransferProgressUseCase, StartTransferUseCase,
};
use uc_core::ports::FileTransferRepositoryPort;
use uc_infra::db::executor::DieselSqliteExecutor;
use uc_infra::file_transfer::SqliteReceiverFileTransferStore;

pub type FileTransferEventStore = SqliteReceiverFileTransferStore<Arc<DieselSqliteExecutor>>;

pub type FileTransferAnnounceUseCase =
    AnnounceTransferUseCase<FileTransferEventStore, FileTransferHostEventPublisher>;
pub type FileTransferStartUseCase =
    StartTransferUseCase<FileTransferEventStore, FileTransferHostEventPublisher>;
pub type FileTransferProgressUseCase =
    ReportTransferProgressUseCase<FileTransferEventStore, FileTransferHostEventPublisher>;
pub type FileTransferCompleteUseCase =
    CompleteTransferUseCase<FileTransferEventStore, FileTransferHostEventPublisher>;
pub type FileTransferFailUseCase =
    FailTransferUseCase<FileTransferEventStore, FileTransferHostEventPublisher>;
pub type FileTransferCancelUseCase =
    CancelTransferUseCase<FileTransferEventStore, FileTransferHostEventPublisher>;

/// Bundle of the durable store + publisher + 6 lifecycle use cases.
///
/// `store` is exposed as the concrete type so the receiver-side worker can
/// call `seed_receiver_context` on it; the use cases only need the
/// `FileTransferEventStorePort` surface.
pub struct FileTransferLifecycle {
    pub store: Arc<FileTransferEventStore>,
    pub publisher: Arc<FileTransferHostEventPublisher>,
    pub announce: Arc<FileTransferAnnounceUseCase>,
    pub start: Arc<FileTransferStartUseCase>,
    pub report_progress: Arc<FileTransferProgressUseCase>,
    pub complete: Arc<FileTransferCompleteUseCase>,
    pub fail: Arc<FileTransferFailUseCase>,
    pub cancel: Arc<FileTransferCancelUseCase>,
}

pub fn build_file_transfer_lifecycle(
    store: Arc<FileTransferEventStore>,
    emitter_cell: Arc<RwLock<Arc<dyn HostEventEmitterPort>>>,
    file_transfer_repo: Arc<dyn FileTransferRepositoryPort>,
) -> FileTransferLifecycle {
    let publisher = Arc::new(FileTransferHostEventPublisher::new(
        emitter_cell,
        file_transfer_repo,
    ));

    let announce = Arc::new(AnnounceTransferUseCase::new(
        Arc::clone(&store),
        Arc::clone(&publisher),
    ));
    let start = Arc::new(StartTransferUseCase::new(
        Arc::clone(&store),
        Arc::clone(&publisher),
    ));
    let report_progress = Arc::new(ReportTransferProgressUseCase::new(
        Arc::clone(&store),
        Arc::clone(&publisher),
    ));
    let complete = Arc::new(CompleteTransferUseCase::new(
        Arc::clone(&store),
        Arc::clone(&publisher),
    ));
    let fail = Arc::new(FailTransferUseCase::new(
        Arc::clone(&store),
        Arc::clone(&publisher),
    ));
    let cancel = Arc::new(CancelTransferUseCase::new(
        Arc::clone(&store),
        Arc::clone(&publisher),
    ));

    FileTransferLifecycle {
        store,
        publisher,
        announce,
        start,
        report_progress,
        complete,
        fail,
        cancel,
    }
}
