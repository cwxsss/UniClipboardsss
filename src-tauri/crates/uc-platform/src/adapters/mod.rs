//! # Platform Adapters / 平台适配器
//!
//! This module contains platform-specific implementations for ports.
//! 此模块包含端口的各种平台特定实现。
//!
//! Slice 4 P5b 起 libp2p 网络适配器(`libp2p_network/`、`pairing_stream/`、
//! `file_transfer/`)已物理删除;`network.rs::DisabledNetwork` 桩满足
//! `NetworkPorts` 类型图,真正的网络栈走 `uc-infra/src/network/iroh`。

pub mod network;
pub mod protocol_ids;

pub use network::DisabledNetwork;
