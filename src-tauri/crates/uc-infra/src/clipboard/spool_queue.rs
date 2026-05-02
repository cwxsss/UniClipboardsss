use tokio::sync::mpsc;
use uc_core::ports::clipboard::{SpoolQueuePort, SpoolRequest};

pub struct MpscSpoolQueue {
    sender: mpsc::Sender<SpoolRequest>,
}

impl MpscSpoolQueue {
    pub fn new(sender: mpsc::Sender<SpoolRequest>) -> Self {
        Self { sender }
    }
}

#[async_trait::async_trait]
impl SpoolQueuePort for MpscSpoolQueue {
    async fn enqueue(&self, request: SpoolRequest) -> anyhow::Result<()> {
        self.sender
            .send(request)
            .await
            .map_err(|err| anyhow::anyhow!("spool queue closed: {err}"))
    }
}
