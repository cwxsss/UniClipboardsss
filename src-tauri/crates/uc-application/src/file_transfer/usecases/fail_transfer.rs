use std::sync::Arc;

use uc_core::file_transfer::{FileTransferEventPublisherPort, FileTransferEventStorePort};
use uc_core::{FileTransferEvent, FileTransferFailureReason};

use crate::file_transfer::errors::FileTransferApplicationError;
use crate::file_transfer::timeline::{load_timeline, persist_and_publish};

/// Input for failing a transfer.
///
/// 标记传输失败时的应用层输入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailTransfer {
    pub transfer_id: String,
    pub peer_id: String,
    pub reason: FileTransferFailureReason,
}

/// Mark a transfer as failed and emit a `Failed` event.
///
/// 将传输标记为失败，并产出 `Failed` 事件。
pub struct FailTransferUseCase<S, P> {
    store: Arc<S>,
    publisher: Arc<P>,
}

impl<S, P> FailTransferUseCase<S, P>
where
    S: FileTransferEventStorePort,
    P: FileTransferEventPublisherPort,
{
    pub fn new(store: Arc<S>, publisher: Arc<P>) -> Self {
        Self { store, publisher }
    }

    pub async fn execute(
        &self,
        input: FailTransfer,
    ) -> Result<FileTransferEvent, FileTransferApplicationError> {
        let timeline = load_timeline(self.store.as_ref(), &input.transfer_id).await?;
        timeline.ensure_active(&input.transfer_id, &input.peer_id)?;

        let event = FileTransferEvent::failed(input.transfer_id, input.peer_id, input.reason);
        persist_and_publish(self.store.as_ref(), self.publisher.as_ref(), event).await
    }
}
