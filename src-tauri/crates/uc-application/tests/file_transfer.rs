use std::sync::Arc;

use uc_application::file_transfer::{
    CancelTransfer, CancelTransferUseCase, CompleteTransfer, CompleteTransferUseCase, FailTransfer,
    FailTransferUseCase, ReportTransferProgress, ReportTransferProgressUseCase, StartTransfer,
    StartTransferUseCase,
};
use uc_core::file_transfer::FileTransferEventStorePort;
use uc_core::{
    FileTransferCancellationReason, FileTransferDirection, FileTransferEvent,
    FileTransferFailureReason, FileTransferProgress,
};
use uc_infra::file_transfer::{InMemoryEventPublisher, InMemoryEventStore};

#[path = "file_transfer/error_cases.rs"]
mod error_cases;
#[path = "file_transfer/full_flow.rs"]
mod full_flow;
#[path = "file_transfer/start_transfer.rs"]
mod start_transfer;

struct TestContext {
    store: Arc<InMemoryEventStore>,
    publisher: Arc<InMemoryEventPublisher>,
    start_transfer: StartTransferUseCase<InMemoryEventStore, InMemoryEventPublisher>,
    report_progress: ReportTransferProgressUseCase<InMemoryEventStore, InMemoryEventPublisher>,
    complete_transfer: CompleteTransferUseCase<InMemoryEventStore, InMemoryEventPublisher>,
    fail_transfer: FailTransferUseCase<InMemoryEventStore, InMemoryEventPublisher>,
    cancel_transfer: CancelTransferUseCase<InMemoryEventStore, InMemoryEventPublisher>,
}

fn build_context() -> TestContext {
    let store = Arc::new(InMemoryEventStore::new());
    let publisher = Arc::new(InMemoryEventPublisher::new());

    TestContext {
        start_transfer: StartTransferUseCase::new(store.clone(), publisher.clone()),
        report_progress: ReportTransferProgressUseCase::new(store.clone(), publisher.clone()),
        complete_transfer: CompleteTransferUseCase::new(store.clone(), publisher.clone()),
        fail_transfer: FailTransferUseCase::new(store.clone(), publisher.clone()),
        cancel_transfer: CancelTransferUseCase::new(store.clone(), publisher.clone()),
        store,
        publisher,
    }
}

async fn transfer_history(ctx: &TestContext, transfer_id: &str) -> Vec<FileTransferEvent> {
    ctx.store.load(transfer_id).await.unwrap()
}

fn published_events(ctx: &TestContext) -> Vec<FileTransferEvent> {
    ctx.publisher.published_events().unwrap()
}

fn sending_progress(bytes_transferred: u64, total_bytes: u64) -> FileTransferProgress {
    FileTransferProgress {
        direction: FileTransferDirection::Sending,
        bytes_transferred,
        total_bytes: Some(total_bytes),
    }
}

fn start_input(transfer_id: &str, peer_id: &str) -> StartTransfer {
    StartTransfer {
        transfer_id: transfer_id.into(),
        peer_id: peer_id.into(),
        filename: "report.pdf".into(),
        file_size: Some(128),
    }
}

fn progress_input(
    transfer_id: &str,
    peer_id: &str,
    bytes_transferred: u64,
) -> ReportTransferProgress {
    ReportTransferProgress {
        transfer_id: transfer_id.into(),
        peer_id: peer_id.into(),
        progress: sending_progress(bytes_transferred, 128),
    }
}

fn complete_input(transfer_id: &str, peer_id: &str) -> CompleteTransfer {
    CompleteTransfer {
        transfer_id: transfer_id.into(),
        peer_id: peer_id.into(),
    }
}

fn fail_input(transfer_id: &str, peer_id: &str, reason: FileTransferFailureReason) -> FailTransfer {
    FailTransfer {
        transfer_id: transfer_id.into(),
        peer_id: peer_id.into(),
        reason,
    }
}

fn cancel_input(
    transfer_id: &str,
    peer_id: &str,
    reason: FileTransferCancellationReason,
) -> CancelTransfer {
    CancelTransfer {
        transfer_id: transfer_id.into(),
        peer_id: peer_id.into(),
        reason,
    }
}
