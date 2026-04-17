use std::sync::Arc;

use uc_core::file_transfer::{FileTransferEventPublisherPort, FileTransferEventStorePort};
use uc_core::{DeviceId, FileTransferEvent};

use crate::file_transfer::errors::FileTransferApplicationError;
use crate::file_transfer::timeline::{load_timeline, persist_and_publish};

/// Input for announcing a transfer before content bytes start flowing.
///
/// 在文件内容开始传输前，先声明一笔文件传输。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnnounceTransfer {
    pub transfer_id: String,
    pub origin_device_id: DeviceId,
    pub filename: String,
    pub file_size: Option<u64>,
}

/// Declare a file transfer so downstream consumers can reference it before
/// the binary content is actually transferred.
///
/// 先声明一笔文件传输，使接收方在文件内容到达前就能建立引用关系。
pub struct AnnounceTransferUseCase<S, P> {
    store: Arc<S>,
    publisher: Arc<P>,
}

impl<S, P> AnnounceTransferUseCase<S, P>
where
    S: FileTransferEventStorePort,
    P: FileTransferEventPublisherPort,
{
    pub fn new(store: Arc<S>, publisher: Arc<P>) -> Self {
        Self { store, publisher }
    }

    pub async fn execute(
        &self,
        input: AnnounceTransfer,
    ) -> Result<FileTransferEvent, FileTransferApplicationError> {
        let timeline = load_timeline(self.store.as_ref(), &input.transfer_id).await?;

        if timeline.started {
            return Err(FileTransferApplicationError::TransferAlreadyStarted {
                transfer_id: input.transfer_id,
            });
        }

        let event = FileTransferEvent::announced(
            input.transfer_id,
            input.origin_device_id,
            input.filename,
            input.file_size,
        );

        persist_and_publish(self.store.as_ref(), self.publisher.as_ref(), event).await
    }
}
