use std::path::PathBuf;

use crate::ports::transfer_progress::TransferProgress;
use serde::{Deserialize, Serialize};

/// File transfer domain events.
///
/// This event model exists separately from `network::NetworkEvent` so file
/// transfer flow can migrate away from the network umbrella incrementally.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FileTransferEvent {
    Started {
        transfer_id: String,
        peer_id: String,
        filename: String,
        file_size: u64,
    },
    Completed {
        transfer_id: String,
        peer_id: String,
        filename: String,
        file_path: PathBuf,
        batch_id: Option<String>,
        batch_total: Option<u32>,
    },
    Failed {
        transfer_id: String,
        peer_id: String,
        error: String,
    },
    Cancelled {
        transfer_id: String,
        peer_id: String,
        reason: String,
    },
    Progress(TransferProgress),
}
