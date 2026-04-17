use crate::ports::transfer_progress::{
    TransferDirection as TransportTransferDirection, TransferProgress as TransportTransferProgress,
};
use crate::DeviceId;
use serde::{Deserialize, Serialize};

/// Business-facing direction of a file transfer.
///
/// 文件传输的业务方向。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum FileTransferDirection {
    Sending,
    Receiving,
}

impl From<TransportTransferDirection> for FileTransferDirection {
    fn from(value: TransportTransferDirection) -> Self {
        match value {
            TransportTransferDirection::Sending => Self::Sending,
            TransportTransferDirection::Receiving => Self::Receiving,
        }
    }
}

/// Business-facing progress snapshot for an active transfer.
///
/// 文件传输的业务进度快照。
///
/// 这里故意只保留 byte-level 进度语义：
/// - `bytes_transferred`
/// - `total_bytes`
///
/// 不保留 chunk-level 字段（如 `chunks_completed`、`total_chunks`），
/// 因为 chunk 是当前传输实现的技术细节，不属于 `uc-core` 应固化的业务模型。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileTransferProgress {
    pub direction: FileTransferDirection,
    pub bytes_transferred: u64,
    /// Total bytes for the transfer when known.
    ///
    /// 传输总字节数；如果当前还未知，则为 `None`。
    pub total_bytes: Option<u64>,
}

impl From<TransportTransferProgress> for FileTransferProgress {
    fn from(value: TransportTransferProgress) -> Self {
        Self {
            direction: value.direction.into(),
            bytes_transferred: value.bytes_transferred,
            total_bytes: value.total_bytes,
        }
    }
}

/// Stable business reason for a failed file transfer.
///
/// 文件传输失败的稳定业务原因。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum FileTransferFailureReason {
    NetworkUnavailable,
    TimedOut,
    AccessDenied,
    StorageUnavailable,
    IntegrityCheckFailed,
    Unknown,
}

/// Stable business reason for a cancelled file transfer.
///
/// 文件传输取消的稳定业务原因。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum FileTransferCancellationReason {
    LocalUser,
    RemotePeer,
    Replaced,
    Unknown,
}

/// File transfer domain events.
///
/// This event model captures business facts only. Transport details such as
/// chunk counters, local file paths, and raw infrastructure errors stay out of
/// this boundary.
///
/// 文件传输领域事件。
///
/// 这些事件只表达业务事实，不表达底层传输实现细节。
/// 例如：
/// - 可以表达“传输已声明”“传输已开始”“传输已完成”
/// - 不应表达某个协议帧、chunk 编号、底层流实现或本地文件路径
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum FileTransferEvent {
    /// A transfer has been declared and may be referenced before content bytes arrive.
    ///
    /// 一笔文件传输已经被声明。
    ///
    /// 此时系统已经知道：
    /// - `transfer_id`
    /// - 来源设备 `origin_device_id`
    /// - 文件名 `filename`
    /// - 以及可能已经知道的 `file_size`
    ///
    /// 但这并不表示文件内容已经开始传输。
    Announced {
        transfer_id: String,
        origin_device_id: DeviceId,
        filename: String,
        file_size: Option<u64>,
    },
    /// The transfer content has started flowing.
    ///
    /// 文件内容传输已经开始。
    ///
    /// `Started` 表示这笔传输已经从“可被引用的声明状态”进入“内容开始流动”的状态。
    /// 它与 `Announced` 不同，后者只表示“系统已知道这笔传输存在”。
    Started {
        transfer_id: String,
        peer_id: String,
        filename: String,
        file_size: Option<u64>,
    },
    /// Business progress for an active transfer.
    ///
    /// 活跃传输的业务进度。
    Progress {
        transfer_id: String,
        peer_id: String,
        progress: FileTransferProgress,
    },
    /// The transfer has completed successfully.
    ///
    /// 传输已成功完成。
    Completed {
        transfer_id: String,
        peer_id: String,
    },
    /// The transfer has failed.
    ///
    /// 传输已失败。
    Failed {
        transfer_id: String,
        peer_id: String,
        reason: FileTransferFailureReason,
    },
    /// The transfer has been cancelled.
    ///
    /// 传输已被取消。
    Cancelled {
        transfer_id: String,
        peer_id: String,
        reason: FileTransferCancellationReason,
    },
}

impl FileTransferEvent {
    /// Create an `Announced` event.
    ///
    /// 构造“传输已声明”事件。
    pub fn announced(
        transfer_id: impl Into<String>,
        origin_device_id: DeviceId,
        filename: impl Into<String>,
        file_size: Option<u64>,
    ) -> Self {
        Self::Announced {
            transfer_id: transfer_id.into(),
            origin_device_id,
            filename: filename.into(),
            file_size,
        }
    }

    /// Create a `Started` event.
    ///
    /// 构造“传输已开始”事件。
    pub fn started(
        transfer_id: impl Into<String>,
        peer_id: impl Into<String>,
        filename: impl Into<String>,
        file_size: Option<u64>,
    ) -> Self {
        Self::Started {
            transfer_id: transfer_id.into(),
            peer_id: peer_id.into(),
            filename: filename.into(),
            file_size,
        }
    }

    /// Convert transport-layer progress into a business-facing `Progress` event.
    ///
    /// 将传输层进度转换为业务层 `Progress` 事件。
    pub fn from_progress(progress: TransportTransferProgress) -> Self {
        Self::Progress {
            transfer_id: progress.transfer_id.clone(),
            peer_id: progress.peer_id.clone(),
            progress: progress.into(),
        }
    }

    /// Create a `Completed` event.
    ///
    /// 构造“传输已完成”事件。
    pub fn completed(transfer_id: impl Into<String>, peer_id: impl Into<String>) -> Self {
        Self::Completed {
            transfer_id: transfer_id.into(),
            peer_id: peer_id.into(),
        }
    }

    /// Create a `Failed` event.
    ///
    /// 构造“传输已失败”事件。
    pub fn failed(
        transfer_id: impl Into<String>,
        peer_id: impl Into<String>,
        reason: FileTransferFailureReason,
    ) -> Self {
        Self::Failed {
            transfer_id: transfer_id.into(),
            peer_id: peer_id.into(),
            reason,
        }
    }

    /// Create a `Cancelled` event.
    ///
    /// 构造“传输已取消”事件。
    pub fn cancelled(
        transfer_id: impl Into<String>,
        peer_id: impl Into<String>,
        reason: FileTransferCancellationReason,
    ) -> Self {
        Self::Cancelled {
            transfer_id: transfer_id.into(),
            peer_id: peer_id.into(),
            reason,
        }
    }
}
