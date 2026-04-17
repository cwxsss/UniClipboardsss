//! # Platform Adapters / 平台适配器
//!
//! This module contains platform-specific implementations for ports.
//! 此模块包含端口的各种平台特定实现。
//!
//! # Modules / 模块
//!
//! - `clipboard` - Placeholder clipboard materialization
//! - `encryption` - Placeholder encryption session management
//! - `network` - Placeholder P2P networking

pub mod encryption;
pub mod file_transfer;
pub mod libp2p_network;
pub mod network;
pub mod pairing_stream;
pub mod protocol_ids;

pub use encryption::InMemoryEncryptionSessionPort;
pub use libp2p_network::Libp2pNetworkAdapter;
pub use network::{DisabledPairingTransport, PairingRuntimeOwner};
