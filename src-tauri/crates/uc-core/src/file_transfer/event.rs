use crate::ports::transfer_progress::{
    TransferDirection as TransportTransferDirection, TransferProgress as TransportTransferProgress,
};
use serde::{Deserialize, Serialize};

/// Business-facing direction of a file transfer.
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileTransferProgress {
    pub direction: FileTransferDirection,
    pub bytes_transferred: u64,
    /// Total bytes for the transfer when known.
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum FileTransferEvent {
    Started {
        transfer_id: String,
        peer_id: String,
        filename: String,
        file_size: u64,
    },
    Progress {
        transfer_id: String,
        peer_id: String,
        progress: FileTransferProgress,
    },
    Completed {
        transfer_id: String,
        peer_id: String,
    },
    Failed {
        transfer_id: String,
        peer_id: String,
        reason: FileTransferFailureReason,
    },
    Cancelled {
        transfer_id: String,
        peer_id: String,
        reason: FileTransferCancellationReason,
    },
}

impl FileTransferEvent {
    pub fn started(
        transfer_id: impl Into<String>,
        peer_id: impl Into<String>,
        filename: impl Into<String>,
        file_size: u64,
    ) -> Self {
        Self::Started {
            transfer_id: transfer_id.into(),
            peer_id: peer_id.into(),
            filename: filename.into(),
            file_size,
        }
    }

    pub fn from_progress(progress: TransportTransferProgress) -> Self {
        Self::Progress {
            transfer_id: progress.transfer_id.clone(),
            peer_id: progress.peer_id.clone(),
            progress: progress.into(),
        }
    }

    pub fn completed(transfer_id: impl Into<String>, peer_id: impl Into<String>) -> Self {
        Self::Completed {
            transfer_id: transfer_id.into(),
            peer_id: peer_id.into(),
        }
    }

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
