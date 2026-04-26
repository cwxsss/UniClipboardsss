//! # Platform Adapters / 平台适配器
//!
//! This module contains platform-specific implementations for ports.
//! 此模块包含端口的各种平台特定实现。
//!
//! Slice 4 P5c 完成:libp2p 网络适配器(`libp2p_network/`、`pairing_stream/`、
//! `file_transfer/`)及 7 个废弃 trait 全部物理删除,真正的网络栈走
//! `uc-infra/src/network/iroh`。本目录现在只剩 protocol_ids(uc-app 仍用作
//! 协议常量),其余平台适配器(`autostart/`、`clipboard/`、`secure_storage/` 等)
//! 散落在 `uc-platform/src/` 下不同子模块。

pub mod protocol_ids;
