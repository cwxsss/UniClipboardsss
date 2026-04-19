//! # Platform Adapters / 平台适配器
//!
//! This module contains platform-specific implementations for ports.
//! 此模块包含端口的各种平台特定实现。
//!
//! # Modules / 模块
//!
//! - `clipboard` - Placeholder clipboard materialization
//! - `network` - P2P networking
//!
//! 注: 历史 `encryption` 模块（InMemoryEncryptionSessionPort）已下沉为
//! uc-infra 内部具体类型 `InMemorySession`,Slice 3 - C8 完成。

pub mod file_transfer;
pub mod libp2p_network;
pub mod network;
pub mod pairing_stream;
pub mod protocol_ids;

pub use libp2p_network::Libp2pNetworkAdapter;
pub use network::{DisabledPairingTransport, PairingRuntimeOwner};
