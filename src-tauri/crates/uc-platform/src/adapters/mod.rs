//! # Platform Adapters / 平台适配器
//!
//! This module contains platform-specific implementations for ports.
//! 此模块包含端口的各种平台特定实现。
//!
//! Slice 4 P5c 起 libp2p 网络适配器(`libp2p_network/`、`pairing_stream/`、
//! `file_transfer/`)及 7 个废弃 trait 中的 6 个均已物理删除;
//! `network.rs::DisabledNetwork` 现在只为遗留的 `NetworkControlPort` 提供
//! no-op 桩,真正的网络栈走 `uc-infra/src/network/iroh`。

pub mod network;
pub mod protocol_ids;

pub use network::DisabledNetwork;
