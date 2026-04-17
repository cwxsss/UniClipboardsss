use super::event::FileTransferEvent;
use anyhow::Result;
use async_trait::async_trait;

#[async_trait]
pub trait FileTransferEventStorePort: Send + Sync {
    async fn load(&self, transfer_id: &str) -> Result<Vec<FileTransferEvent>>;

    async fn append(&self, event: FileTransferEvent) -> Result<()>;
}

#[async_trait]
pub trait FileTransferEventPublisherPort: Send + Sync {
    async fn publish(&self, event: FileTransferEvent) -> Result<()>;
}

/// Inbound stream of file-transfer domain events produced by the transport /
/// platform layer (e.g. libp2p) for consumption by the application layer.
///
/// 由传输 / 平台层（如 libp2p）产生、供应用层消费的文件传输领域事件入站流。
///
/// Distinct from [`FileTransferEventPublisherPort`]: that port pushes events
/// out to host-facing listeners (UI, daemon WS). This port is the adapter-
/// produced bus that the application layer subscribes to in order to advance
/// the transfer lifecycle.
#[async_trait]
pub trait FileTransferEventInboundPort: Send + Sync {
    /// Subscribe to the inbound file-transfer event stream.
    ///
    /// Contract: adapters may expose this as a single-consumer stream.
    async fn subscribe(&self) -> Result<tokio::sync::mpsc::Receiver<FileTransferEvent>>;
}
