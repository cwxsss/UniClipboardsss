//! Placeholder network port implementation
//! 占位符网络端口实现

use crate::ports::IdentityStorePort;
use anyhow::Result;
use async_trait::async_trait;
use libp2p::PeerId;
use tracing::error;
use uc_core::network::{ConnectedPeer, DiscoveredPeer, NetworkEvent, PairingMessage};
use uc_core::ports::{
    ClipboardInboundMessageSource, ClipboardInboundTransportPort, ClipboardOutboundTransportPort,
    ClipboardTransportError, NetworkControlPort, NetworkEventPort, OutboundClipboardFrame,
    PairingTransportPort, PeerDirectoryPort, SyncTargetId,
};

use crate::identity_store::load_or_create_identity;

const DISABLED_PAIRING_RUNTIME_ERROR: &str = "local pairing runtime is disabled in this process";

/// Declares which process owns the local pairing runtime.
/// 声明哪个进程拥有本地 pairing runtime。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairingRuntimeOwner {
    CurrentProcess,
    ExternalDaemon,
}

/// Pairing transport that always fails because this process does not host pairing runtime.
/// 当前进程不承载 pairing runtime 时使用的失败型 pairing transport。
#[derive(Debug, Default, Clone, Copy)]
pub struct DisabledPairingTransport;

#[async_trait]
impl PairingTransportPort for DisabledPairingTransport {
    async fn open_pairing_session(&self, _peer_id: String, _session_id: String) -> Result<()> {
        Err(anyhow::anyhow!(DISABLED_PAIRING_RUNTIME_ERROR))
    }

    async fn send_pairing_on_session(&self, _message: PairingMessage) -> Result<()> {
        Err(anyhow::anyhow!(DISABLED_PAIRING_RUNTIME_ERROR))
    }

    async fn close_pairing_session(
        &self,
        _session_id: String,
        _reason: Option<String>,
    ) -> Result<()> {
        Err(anyhow::anyhow!(DISABLED_PAIRING_RUNTIME_ERROR))
    }

    async fn unpair_device(&self, _peer_id: String) -> Result<()> {
        Err(anyhow::anyhow!(DISABLED_PAIRING_RUNTIME_ERROR))
    }
}

/// Placeholder network port implementation
/// 占位符网络端口实现
#[derive(Debug, Clone)]
pub struct PlaceholderNetworkPort {
    local_peer_id: PeerId,
}

impl PlaceholderNetworkPort {
    pub fn new(identity_store: std::sync::Arc<dyn IdentityStorePort>) -> Result<Self> {
        let keypair = load_or_create_identity(identity_store.as_ref())
            .map_err(|e| anyhow::anyhow!("failed to load libp2p identity: {e}"))?;
        let local_peer_id = PeerId::from(keypair.public());
        Ok(Self { local_peer_id })
    }

    pub fn local_peer_id(&self) -> &PeerId {
        &self.local_peer_id
    }
}

#[async_trait]
impl ClipboardOutboundTransportPort for PlaceholderNetworkPort {
    async fn send_clipboard(
        &self,
        _target: &SyncTargetId,
        _frame: OutboundClipboardFrame,
    ) -> std::result::Result<(), ClipboardTransportError> {
        Err(ClipboardTransportError::Unsupported)
    }
}

#[async_trait]
impl ClipboardInboundTransportPort for PlaceholderNetworkPort {
    async fn subscribe_clipboard(
        &self,
    ) -> std::result::Result<Box<dyn ClipboardInboundMessageSource>, ClipboardTransportError> {
        error!("ClipboardInboundTransportPort::subscribe_clipboard not implemented");
        Err(ClipboardTransportError::Unsupported)
    }
}

#[async_trait]
impl PeerDirectoryPort for PlaceholderNetworkPort {
    // === Peer operations ===

    async fn get_discovered_peers(&self) -> Result<Vec<DiscoveredPeer>> {
        Ok(Vec::new())
    }

    async fn get_connected_peers(&self) -> Result<Vec<ConnectedPeer>> {
        Ok(Vec::new())
    }

    fn local_peer_id(&self) -> String {
        self.local_peer_id.to_string()
    }

    async fn announce_device_name(&self, _device_name: String) -> Result<()> {
        Err(anyhow::anyhow!(
            "PeerDirectoryPort::announce_device_name not implemented yet"
        ))
    }
}

#[async_trait]
impl PairingTransportPort for PlaceholderNetworkPort {
    async fn open_pairing_session(&self, _peer_id: String, _session_id: String) -> Result<()> {
        Err(anyhow::anyhow!(
            "PairingTransportPort::open_pairing_session not implemented yet"
        ))
    }

    async fn send_pairing_on_session(&self, _message: PairingMessage) -> Result<()> {
        Err(anyhow::anyhow!(
            "PairingTransportPort::send_pairing_on_session not implemented yet"
        ))
    }

    async fn close_pairing_session(
        &self,
        _session_id: String,
        _reason: Option<String>,
    ) -> Result<()> {
        Err(anyhow::anyhow!(
            "PairingTransportPort::close_pairing_session not implemented yet"
        ))
    }

    async fn unpair_device(&self, _peer_id: String) -> Result<()> {
        Err(anyhow::anyhow!(
            "PairingTransportPort::unpair_device not implemented yet"
        ))
    }
}

#[async_trait]
impl NetworkEventPort for PlaceholderNetworkPort {
    async fn subscribe_events(&self) -> Result<tokio::sync::mpsc::Receiver<NetworkEvent>> {
        error!("NetworkEventPort::subscribe_events not implemented");
        Err(anyhow::anyhow!(
            "NetworkEventPort::subscribe_events not implemented yet"
        ))
    }
}

#[async_trait]
impl NetworkControlPort for PlaceholderNetworkPort {
    async fn start_network(&self) -> Result<()> {
        Ok(())
    }
}
