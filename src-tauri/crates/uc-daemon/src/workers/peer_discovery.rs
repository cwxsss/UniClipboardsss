use std::sync::Arc;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};
use uc_bootstrap::resolve_pairing_device_name;
use uc_core::network::NetworkEvent;
use uc_core::ports::{NetworkControlPort, NetworkEventPort, PeerDirectoryPort, SettingsPort};

use crate::service::{DaemonService, ServiceHealth};

pub struct PeerDiscoveryWorker {
    network_control: Arc<dyn NetworkControlPort>,
    network_events: Arc<dyn NetworkEventPort>,
    peer_directory: Arc<dyn PeerDirectoryPort>,
    settings: Arc<dyn SettingsPort>,
}

impl PeerDiscoveryWorker {
    pub fn new(
        network_control: Arc<dyn NetworkControlPort>,
        network_events: Arc<dyn NetworkEventPort>,
        peer_directory: Arc<dyn PeerDirectoryPort>,
        settings: Arc<dyn SettingsPort>,
    ) -> Self {
        Self {
            network_control,
            network_events,
            peer_directory,
            settings,
        }
    }
}

#[async_trait]
impl DaemonService for PeerDiscoveryWorker {
    fn name(&self) -> &str {
        "peer-discovery"
    }

    async fn start(&self, cancel: CancellationToken) -> anyhow::Result<()> {
        let mut event_rx = self.network_events.subscribe_events().await?;
        self.network_control.start_network().await?;
        info!("peer discovery started");

        while !cancel.is_cancelled() {
            tokio::select! {
                _ = cancel.cancelled() => break,
                maybe_event = event_rx.recv() => {
                    let Some(event) = maybe_event else {
                        break;
                    };

                    if let NetworkEvent::PeerDiscovered(peer) = event {
                        let device_name = resolve_pairing_device_name(self.settings.clone()).await;
                        if let Err(err) = self.peer_directory.announce_device_name(device_name).await {
                            warn!(
                                error = %err,
                                peer_id = %peer.peer_id,
                                "failed to announce device name after daemon peer discovery"
                            );
                        }
                    }
                }
            }
        }

        info!("peer discovery cancelled");
        Ok(())
    }

    async fn stop(&self) -> anyhow::Result<()> {
        info!("peer discovery stopped");
        Ok(())
    }

    fn health_check(&self) -> ServiceHealth {
        ServiceHealth::Healthy
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    use mockall::mock;
    use tokio::sync::mpsc;
    use tokio::time::{timeout, Duration};
    use uc_core::network::{ConnectedPeer, DiscoveredPeer};
    use uc_core::ports::SettingsPort;
    use uc_core::settings::model::Settings;

    mock! {
        NetworkControl {}
        #[async_trait]
        impl NetworkControlPort for NetworkControl {
            async fn start_network(&self) -> anyhow::Result<()>;
        }
    }

    mock! {
        NetworkEvents {}
        #[async_trait]
        impl NetworkEventPort for NetworkEvents {
            async fn subscribe_events(&self) -> anyhow::Result<mpsc::Receiver<NetworkEvent>>;
        }
    }

    mock! {
        PeerDirectory {}
        #[async_trait]
        impl PeerDirectoryPort for PeerDirectory {
            async fn get_discovered_peers(&self) -> anyhow::Result<Vec<DiscoveredPeer>>;
            async fn get_connected_peers(&self) -> anyhow::Result<Vec<ConnectedPeer>>;
            fn local_peer_id(&self) -> String;
            async fn announce_device_name(&self, device_name: String) -> anyhow::Result<()>;
        }
    }

    mock! {
        Settings {}
        #[async_trait]
        impl SettingsPort for Settings {
            async fn load(&self) -> anyhow::Result<Settings>;
            async fn save(&self, settings: &Settings) -> anyhow::Result<()>;
        }
    }

    #[tokio::test]
    async fn peer_discovery_worker_starts_network_and_announces_device_name() {
        let announced_names = Arc::new(Mutex::new(Vec::new()));
        let (tx, rx) = mpsc::channel(8);
        let rx = Arc::new(Mutex::new(Some(rx)));

        let mut network_control = MockNetworkControl::new();
        network_control
            .expect_start_network()
            .times(1)
            .returning(|| Ok(()));

        let mut network_events = MockNetworkEvents::new();
        network_events
            .expect_subscribe_events()
            .times(1)
            .returning({
                let rx = Arc::clone(&rx);
                move || {
                    rx.lock()
                        .unwrap()
                        .take()
                        .ok_or_else(|| anyhow::anyhow!("receiver already taken"))
                }
            });

        let mut peer_directory = MockPeerDirectory::new();
        peer_directory
            .expect_get_discovered_peers()
            .returning(|| Ok(vec![]));
        peer_directory
            .expect_get_connected_peers()
            .returning(|| Ok(vec![]));
        peer_directory
            .expect_local_peer_id()
            .returning(|| "local-peer".to_string());
        peer_directory
            .expect_announce_device_name()
            .times(1)
            .returning({
                let announced_names = Arc::clone(&announced_names);
                move |device_name| {
                    announced_names.lock().unwrap().push(device_name);
                    Ok(())
                }
            });

        let mut settings = MockSettings::new();
        settings.expect_load().times(1).returning(|| {
            let mut settings = Settings::default();
            settings.general.device_name = Some("Daemon Desk".to_string());
            Ok(settings)
        });

        let worker = PeerDiscoveryWorker::new(
            Arc::new(network_control),
            Arc::new(network_events),
            Arc::new(peer_directory),
            Arc::new(settings),
        );

        let cancel = CancellationToken::new();
        let worker_cancel = cancel.clone();
        let task = tokio::spawn(async move { worker.start(worker_cancel).await });

        tx.send(NetworkEvent::PeerDiscovered(DiscoveredPeer {
            peer_id: "peer-1".to_string(),
            device_name: None,
            device_id: None,
            addresses: vec![],
            discovered_at: chrono::Utc::now(),
            last_seen: chrono::Utc::now(),
            is_paired: false,
        }))
        .await
        .unwrap();

        timeout(Duration::from_secs(1), async {
            loop {
                if !announced_names.lock().unwrap().is_empty() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("worker should announce device name");

        assert_eq!(
            announced_names.lock().unwrap().as_slice(),
            ["Daemon Desk".to_string()]
        );

        cancel.cancel();
        task.await.unwrap().unwrap();
    }
}
