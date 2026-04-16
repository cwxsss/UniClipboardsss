use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;
use std::sync::{Mutex, OnceLock};
use tokio::sync::mpsc;
use uc_core::network::{
    ClipboardMessage, ConnectedPeer, DiscoveredPeer, NetworkEvent, PairingMessage,
};
use uc_core::ports::{
    ClipboardTransportPort, NetworkEventPort, PairingTransportPort, PeerDirectoryPort,
};

type ClipboardSubscription = mpsc::Sender<(ClipboardMessage, Option<Vec<u8>>)>;
type EventSubscription = mpsc::Sender<NetworkEvent>;

fn clipboard_subscribers() -> &'static Mutex<Vec<ClipboardSubscription>> {
    static SUBSCRIBERS: OnceLock<Mutex<Vec<ClipboardSubscription>>> = OnceLock::new();
    SUBSCRIBERS.get_or_init(|| Mutex::new(Vec::new()))
}

fn event_subscribers() -> &'static Mutex<Vec<EventSubscription>> {
    static SUBSCRIBERS: OnceLock<Mutex<Vec<EventSubscription>>> = OnceLock::new();
    SUBSCRIBERS.get_or_init(|| Mutex::new(Vec::new()))
}

pub struct NoopClipboardTransport;

#[async_trait]
impl ClipboardTransportPort for NoopClipboardTransport {
    async fn send_clipboard(&self, _peer_id: &str, _encrypted_data: Arc<[u8]>) -> Result<()> {
        Ok(())
    }

    async fn broadcast_clipboard(&self, _encrypted_data: Arc<[u8]>) -> Result<()> {
        Ok(())
    }

    async fn subscribe_clipboard(
        &self,
    ) -> Result<mpsc::Receiver<(ClipboardMessage, Option<Vec<u8>>)>> {
        let (tx, rx) = mpsc::channel(1);
        clipboard_subscribers()
            .lock()
            .expect("clipboard subscribers mutex poisoned")
            .push(tx);
        Ok(rx)
    }
}

pub struct NoopPeerDirectory;

#[async_trait]
impl PeerDirectoryPort for NoopPeerDirectory {
    async fn get_discovered_peers(&self) -> Result<Vec<DiscoveredPeer>> {
        Ok(Vec::new())
    }

    async fn get_connected_peers(&self) -> Result<Vec<ConnectedPeer>> {
        Ok(Vec::new())
    }

    fn local_peer_id(&self) -> String {
        "noop-peer".to_string()
    }

    async fn announce_device_name(&self, _device_name: String) -> Result<()> {
        Ok(())
    }
}

pub struct NoopPairingTransport;

#[async_trait]
impl PairingTransportPort for NoopPairingTransport {
    async fn open_pairing_session(&self, _peer_id: String, _session_id: String) -> Result<()> {
        Ok(())
    }

    async fn send_pairing_on_session(&self, _message: PairingMessage) -> Result<()> {
        Ok(())
    }

    async fn close_pairing_session(
        &self,
        _session_id: String,
        _reason: Option<String>,
    ) -> Result<()> {
        Ok(())
    }

    async fn unpair_device(&self, _peer_id: String) -> Result<()> {
        Ok(())
    }
}

pub struct NoopNetworkEvents;

#[async_trait]
impl NetworkEventPort for NoopNetworkEvents {
    async fn subscribe_events(&self) -> Result<mpsc::Receiver<NetworkEvent>> {
        let (tx, rx) = mpsc::channel(1);
        event_subscribers()
            .lock()
            .expect("event subscribers mutex poisoned")
            .push(tx);
        Ok(rx)
    }
}

pub fn noop_network_ports() -> Arc<uc_app::deps::NetworkPorts> {
    Arc::new(uc_app::deps::NetworkPorts {
        clipboard: Arc::new(NoopClipboardTransport),
        peers: Arc::new(NoopPeerDirectory),
        pairing: Arc::new(NoopPairingTransport),
        events: Arc::new(NoopNetworkEvents),
        // TODO(phase-30): provide FileTransportPort adapter
        file_transfer: Arc::new(uc_core::ports::NoopFileTransportPort),
    })
}
