use uc_core::file_transfer::FileTransferDirection;

/// 剪贴板内容来源。
#[derive(Debug, Clone)]
pub enum ClipboardOriginKind {
    Local,
    Remote,
}

/// 剪贴板子系统发给宿主的语义事件。
#[derive(Debug, Clone)]
pub enum ClipboardHostEvent {
    NewContent {
        entry_id: String,
        preview: String,
        origin: ClipboardOriginKind,
    },
}

/// 文件传输子系统发给宿主的语义事件。
#[derive(Debug, Clone)]
pub enum TransferHostEvent {
    StatusChanged {
        transfer_id: String,
        entry_id: String,
        status: String,
        reason: Option<String>,
    },
    Progress {
        transfer_id: String,
        entry_id: Option<String>,
        peer_id: String,
        direction: FileTransferDirection,
        bytes_transferred: u64,
        total_bytes: Option<u64>,
    },
}

/// application 发给宿主环境的统一事件。
#[derive(Debug, Clone)]
pub enum HostEvent {
    Clipboard(ClipboardHostEvent),
    Transfer(TransferHostEvent),
}

/// 宿主事件发送失败。
#[derive(Debug, thiserror::Error)]
pub enum EmitError {
    #[error("emit failed: {0}")]
    Failed(String),
}

/// 宿主事件发送端口。
pub trait HostEventEmitterPort: Send + Sync {
    fn emit(&self, event: HostEvent) -> Result<(), EmitError>;
}
