//! # uc-core
//!
//! Core domain models and business logic for UniClipboard.
//!
//! This crate contains pure business logic without any infrastructure dependencies.

// Public module exports
pub mod app_dirs;
pub mod clipboard;
pub mod config;
pub mod crypto;
pub mod device;
pub mod file_transfer;
pub mod ids;
pub mod network;
pub mod pairing;
pub mod ports;
pub mod search;
pub mod settings;
pub mod setup;
pub mod space_access;

// Re-export commonly used types at the crate root
pub use clipboard::*;
pub use config::AppConfig;
pub use device::DeviceId;
pub use file_transfer::{
    FileTransferCancellationReason, FileTransferDirection, FileTransferEvent,
    FileTransferFailureReason, FileTransferProgress,
};
pub use ids::BlobId;
pub use ids::{PeerId, SessionId};
pub use network::{NetworkEvent, NetworkStatus, ProtocolMessage};

// 不知道如何分类，临时定义在这里

pub struct EncryptionMeta {
    pub algo: String,   // "xchacha20poly1305"
    pub key_id: String, // keyslot id / key version
    pub nonce_b64: String,
    pub aad_b64: Option<String>,
}

#[derive(Debug, Clone)]
pub enum MaterializedPayload {
    /// 直接可交付
    Inline {
        mime: Option<String>,
        bytes: Vec<u8>,
    },

    /// 已经落 blob，可交付 blob 引用
    Blob {
        mime: Option<String>,
        blob_id: BlobId,
    },
}
