//! File transfer adapter module.
//!
//! Provides chunked file transfer over libp2p streams with Blake3 hash verification.

mod framing;
pub mod protocol;
pub mod queue;
pub mod retry;
pub mod service;

pub use queue::FileTransferQueue;
pub use service::FileTransferService;
