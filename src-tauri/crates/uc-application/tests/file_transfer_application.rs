use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use uc_application::file_transfer::{
    CancelTransfer, CompleteTransfer, FailTransfer, FileTransferApplicationError,
    FileTransferApplicationService, ReportTransferProgress, StartTransfer,
};
use uc_core::file_transfer::{FileTransferEventPublisherPort, FileTransferEventStorePort};
use uc_core::{
    FileTransferCancellationReason, FileTransferEvent, FileTransferFailureReason,
    FileTransferProgress,
};

#[derive(Default)]
struct InMemoryEventStore {
    events: Mutex<Vec<FileTransferEvent>>,
}

#[async_trait]
impl FileTransferEventStorePort for InMemoryEventStore {
    async fn load(&self, transfer_id: &str) -> Result<Vec<FileTransferEvent>> {
        Ok(self
            .events
            .lock()
            .unwrap()
            .iter()
            .filter(|event| transfer_id_of(event) == transfer_id)
            .cloned()
            .collect())
    }

    async fn append(&self, event: FileTransferEvent) -> Result<()> {
        self.events.lock().unwrap().push(event);
        Ok(())
    }
}

#[derive(Default)]
struct RecordingPublisher {
    published: Mutex<Vec<FileTransferEvent>>,
}

#[async_trait]
impl FileTransferEventPublisherPort for RecordingPublisher {
    async fn publish(&self, event: FileTransferEvent) -> Result<()> {
        self.published.lock().unwrap().push(event);
        Ok(())
    }
}

#[tokio::test]
async fn start_transfer_appends_and_publishes_started_event() {
    let store = Arc::new(InMemoryEventStore::default());
    let publisher = Arc::new(RecordingPublisher::default());
    let service = FileTransferApplicationService::new(store.clone(), publisher.clone());

    let event = service
        .start_transfer(StartTransfer {
            transfer_id: "transfer-1".into(),
            peer_id: "peer-1".into(),
            filename: "report.pdf".into(),
            file_size: 128,
        })
        .await
        .unwrap();

    assert_eq!(
        event,
        FileTransferEvent::started("transfer-1", "peer-1", "report.pdf", 128)
    );
    assert_eq!(store.load("transfer-1").await.unwrap(), vec![event.clone()]);
    assert_eq!(published_events(&publisher), vec![event]);
}

#[tokio::test]
async fn report_progress_after_start_appends_and_publishes_progress_event() {
    let store = Arc::new(InMemoryEventStore::default());
    let publisher = Arc::new(RecordingPublisher::default());
    let service = FileTransferApplicationService::new(store.clone(), publisher.clone());

    service
        .start_transfer(StartTransfer {
            transfer_id: "transfer-1".into(),
            peer_id: "peer-1".into(),
            filename: "report.pdf".into(),
            file_size: 128,
        })
        .await
        .unwrap();

    let event = service
        .report_progress(ReportTransferProgress {
            transfer_id: "transfer-1".into(),
            peer_id: "peer-1".into(),
            progress: FileTransferProgress {
                direction: uc_core::FileTransferDirection::Sending,
                bytes_transferred: 64,
                total_bytes: Some(128),
            },
        })
        .await
        .unwrap();

    assert_eq!(
        event,
        FileTransferEvent::Progress {
            transfer_id: "transfer-1".into(),
            peer_id: "peer-1".into(),
            progress: FileTransferProgress {
                direction: uc_core::FileTransferDirection::Sending,
                bytes_transferred: 64,
                total_bytes: Some(128),
            },
        }
    );
    assert_eq!(store.load("transfer-1").await.unwrap().len(), 2);
    assert_eq!(published_events(&publisher).len(), 2);
}

#[tokio::test]
async fn progress_before_start_is_rejected_without_side_effects() {
    let store = Arc::new(InMemoryEventStore::default());
    let publisher = Arc::new(RecordingPublisher::default());
    let service = FileTransferApplicationService::new(store.clone(), publisher.clone());

    let error = service
        .report_progress(ReportTransferProgress {
            transfer_id: "transfer-1".into(),
            peer_id: "peer-1".into(),
            progress: FileTransferProgress {
                direction: uc_core::FileTransferDirection::Sending,
                bytes_transferred: 64,
                total_bytes: Some(128),
            },
        })
        .await
        .unwrap_err();

    assert_eq!(
        error,
        FileTransferApplicationError::TransferNotStarted {
            transfer_id: "transfer-1".into(),
        }
    );
    assert!(store.load("transfer-1").await.unwrap().is_empty());
    assert!(published_events(&publisher).is_empty());
}

#[tokio::test]
async fn complete_transfer_after_start_appends_and_publishes_completed_event() {
    let store = Arc::new(InMemoryEventStore::default());
    let publisher = Arc::new(RecordingPublisher::default());
    let service = FileTransferApplicationService::new(store.clone(), publisher.clone());

    service
        .start_transfer(StartTransfer {
            transfer_id: "transfer-1".into(),
            peer_id: "peer-1".into(),
            filename: "report.pdf".into(),
            file_size: 128,
        })
        .await
        .unwrap();

    let event = service
        .complete_transfer(CompleteTransfer {
            transfer_id: "transfer-1".into(),
            peer_id: "peer-1".into(),
        })
        .await
        .unwrap();

    assert_eq!(event, FileTransferEvent::completed("transfer-1", "peer-1"));
    assert_eq!(store.load("transfer-1").await.unwrap().len(), 2);
    assert_eq!(published_events(&publisher).len(), 2);
}

#[tokio::test]
async fn terminal_transfer_rejects_follow_up_events_without_side_effects() {
    let store = Arc::new(InMemoryEventStore::default());
    let publisher = Arc::new(RecordingPublisher::default());
    let service = FileTransferApplicationService::new(store.clone(), publisher.clone());

    service
        .start_transfer(StartTransfer {
            transfer_id: "transfer-1".into(),
            peer_id: "peer-1".into(),
            filename: "report.pdf".into(),
            file_size: 128,
        })
        .await
        .unwrap();
    service
        .cancel_transfer(CancelTransfer {
            transfer_id: "transfer-1".into(),
            peer_id: "peer-1".into(),
            reason: FileTransferCancellationReason::LocalUser,
        })
        .await
        .unwrap();

    let error = service
        .fail_transfer(FailTransfer {
            transfer_id: "transfer-1".into(),
            peer_id: "peer-1".into(),
            reason: FileTransferFailureReason::TimedOut,
        })
        .await
        .unwrap_err();

    assert_eq!(
        error,
        FileTransferApplicationError::TransferAlreadyFinished {
            transfer_id: "transfer-1".into(),
        }
    );
    assert_eq!(store.load("transfer-1").await.unwrap().len(), 2);
    assert_eq!(published_events(&publisher).len(), 2);
}

fn published_events(publisher: &RecordingPublisher) -> Vec<FileTransferEvent> {
    publisher.published.lock().unwrap().clone()
}

fn transfer_id_of(event: &FileTransferEvent) -> &str {
    match event {
        FileTransferEvent::Started { transfer_id, .. }
        | FileTransferEvent::Progress { transfer_id, .. }
        | FileTransferEvent::Completed { transfer_id, .. }
        | FileTransferEvent::Failed { transfer_id, .. }
        | FileTransferEvent::Cancelled { transfer_id, .. } => transfer_id,
    }
}
