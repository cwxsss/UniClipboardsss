//! # Disabled network stubs / 失能网络桩实现
//!
//! Slice 4 P5b 起 libp2p adapter 已物理删除,iroh 栈
//! (`uc-infra/src/network/iroh`) 承担所有真实传输。`uc-app::deps::NetworkPorts`
//! 的 7 个 trait dyn 字段 + `AppDeps.network_control` 仍是 Slice 5
//! 待清扫的旧消费者(sync_outbound/clipboard、sync_outbound/file_sync、
//! `StartNetworkAfterUnlock` 等)的编译时依赖面;在 5c 物理删除这些
//! 消费者前,统一用 [`DisabledNetwork`] 满足类型图。
//!
//! 任何 live(iroh)路径都不应触达本桩;若被触达,显式失败而非静默
//! 降级,以便上层 tracing 能立即定位到错误调用点。

#![allow(deprecated)] // 满足 Slice 5 待删除的 libp2p-era port traits

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use tokio::sync::mpsc;

use uc_core::file_transfer::{FileTransferEvent, FileTransferEventInboundPort};
use uc_core::network::protocol::FileTransferMessage;
use uc_core::network::{ConnectedPeer, DiscoveredPeer, NetworkEvent, PairingMessage};
use uc_core::ports::{
    ClipboardInboundMessageSource, ClipboardInboundTransportPort, ClipboardOutboundTransportPort,
    ClipboardTransportError, FileTransportPort, NetworkControlPort, NetworkEventPort,
    OutboundClipboardFrame, PairingTransportPort, PeerDirectoryPort, SyncTargetId,
};

const DISABLED_MSG: &str = "network adapter is disabled in this build (libp2p adapter removed in Slice 4 P5b; iroh stack handles real transport)";

/// Zero-sized 桩,实现 `NetworkPorts` 的全部 7 个 trait 加 `NetworkControlPort`。
/// 用于在 Slice 5c 删除上层旧消费者前满足类型图。
#[derive(Debug, Default, Clone, Copy)]
pub struct DisabledNetwork;

#[async_trait]
impl ClipboardOutboundTransportPort for DisabledNetwork {
    async fn send_clipboard(
        &self,
        _target: &SyncTargetId,
        _frame: OutboundClipboardFrame,
    ) -> std::result::Result<(), ClipboardTransportError> {
        Err(ClipboardTransportError::Unsupported)
    }
}

#[async_trait]
impl ClipboardInboundTransportPort for DisabledNetwork {
    async fn subscribe_clipboard(
        &self,
    ) -> std::result::Result<Box<dyn ClipboardInboundMessageSource>, ClipboardTransportError> {
        Err(ClipboardTransportError::Unsupported)
    }
}

#[async_trait]
impl PeerDirectoryPort for DisabledNetwork {
    async fn get_discovered_peers(&self) -> Result<Vec<DiscoveredPeer>> {
        Ok(Vec::new())
    }

    async fn get_connected_peers(&self) -> Result<Vec<ConnectedPeer>> {
        Ok(Vec::new())
    }

    fn local_peer_id(&self) -> String {
        String::new()
    }

    async fn announce_device_name(&self, _device_name: String) -> Result<()> {
        Err(anyhow!(DISABLED_MSG))
    }
}

#[async_trait]
impl PairingTransportPort for DisabledNetwork {
    async fn open_pairing_session(&self, _peer_id: String, _session_id: String) -> Result<()> {
        Err(anyhow!(DISABLED_MSG))
    }

    async fn send_pairing_on_session(&self, _message: PairingMessage) -> Result<()> {
        Err(anyhow!(DISABLED_MSG))
    }

    async fn close_pairing_session(
        &self,
        _session_id: String,
        _reason: Option<String>,
    ) -> Result<()> {
        Err(anyhow!(DISABLED_MSG))
    }

    async fn unpair_device(&self, _peer_id: String) -> Result<()> {
        Err(anyhow!(DISABLED_MSG))
    }
}

#[async_trait]
impl NetworkEventPort for DisabledNetwork {
    async fn subscribe_events(&self) -> Result<mpsc::Receiver<NetworkEvent>> {
        // 立即关闭的通道:任何 polling 消费者立刻拿到 `None` 并干净退出,
        // 而不是无限阻塞。
        let (_tx, rx) = mpsc::channel::<NetworkEvent>(1);
        Ok(rx)
    }
}

#[async_trait]
impl NetworkControlPort for DisabledNetwork {
    async fn start_network(&self) -> Result<()> {
        // GUI lifecycle 的 `StartNetworkAfterUnlock` 仍会调到这里;
        // 真正的 iroh endpoint 由 `SpaceSetupAssembly` 单独驱动,
        // 这里返回 Ok 即可避免 unlock 后误报错误。
        tracing::debug!(
            "DisabledNetwork::start_network — no-op (iroh stack drives real transport)"
        );
        Ok(())
    }
}

#[async_trait]
impl FileTransportPort for DisabledNetwork {
    async fn send_file_announce(
        &self,
        _peer_id: &str,
        _announce: FileTransferMessage,
    ) -> Result<()> {
        Err(anyhow!(DISABLED_MSG))
    }

    async fn send_file_data(&self, _peer_id: &str, _data: FileTransferMessage) -> Result<()> {
        Err(anyhow!(DISABLED_MSG))
    }

    async fn send_file_complete(
        &self,
        _peer_id: &str,
        _complete: FileTransferMessage,
    ) -> Result<()> {
        Err(anyhow!(DISABLED_MSG))
    }

    async fn cancel_transfer(&self, _peer_id: &str, _cancel: FileTransferMessage) -> Result<()> {
        Err(anyhow!(DISABLED_MSG))
    }

    async fn send_file(
        &self,
        _peer_id: &str,
        _file_path: std::path::PathBuf,
        _transfer_id: String,
        _batch_id: Option<String>,
        _batch_total: Option<u32>,
    ) -> Result<()> {
        Err(anyhow!(DISABLED_MSG))
    }
}

#[async_trait]
impl FileTransferEventInboundPort for DisabledNetwork {
    async fn subscribe(&self) -> Result<mpsc::Receiver<FileTransferEvent>> {
        let (_tx, rx) = mpsc::channel::<FileTransferEvent>(1);
        Ok(rx)
    }
}
