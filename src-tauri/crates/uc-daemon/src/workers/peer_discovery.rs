use std::sync::Arc;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};
use uc_bootstrap::resolve_pairing_device_name;
use uc_core::network::NetworkEvent;
use uc_core::ports::{NetworkControlPort, NetworkEventPort, PeerDirectoryPort, SettingsPort};

use crate::worker::{DaemonWorker, WorkerHealth};

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
impl DaemonWorker for PeerDiscoveryWorker {
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

    fn health_check(&self) -> WorkerHealth {
        WorkerHealth::Healthy
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    use tokio::sync::mpsc;
    use tokio::time::{timeout, Duration};
    use uc_core::network::{ConnectedPeer, DiscoveredPeer};
    use uc_core::ports::SettingsPort;
    use uc_core::settings::model::Settings;

    struct MockNetworkControl {
        calls: Arc<Mutex<u32>>,
    }

    #[async_trait]
    impl NetworkControlPort for MockNetworkControl {
        async fn start_network(&self) -> anyhow::Result<()> {
            *self.calls.lock().unwrap() += 1;
            Ok(())
        }
    }

    struct MockNetworkEvents {
        rx: Mutex<Option<mpsc::Receiver<NetworkEvent>>>,
    }

    #[async_trait]
    impl NetworkEventPort for MockNetworkEvents {
        async fn subscribe_events(&self) -> anyhow::Result<mpsc::Receiver<NetworkEvent>> {
            self.rx
                .lock()
                .unwrap()
                .take()
                .ok_or_else(|| anyhow::anyhow!("receiver already taken"))
        }
    }

    struct MockPeerDirectory {
        announced_names: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait]
    impl PeerDirectoryPort for MockPeerDirectory {
        async fn get_discovered_peers(&self) -> anyhow::Result<Vec<DiscoveredPeer>> {
            Ok(vec![])
        }

        async fn get_connected_peers(&self) -> anyhow::Result<Vec<ConnectedPeer>> {
            Ok(vec![])
        }

        fn local_peer_id(&self) -> String {
            "local-peer".to_string()
        }

        async fn announce_device_name(&self, device_name: String) -> anyhow::Result<()> {
            self.announced_names.lock().unwrap().push(device_name);
            Ok(())
        }
    }

    struct MockSettings;

    #[async_trait]
    impl SettingsPort for MockSettings {
        async fn load(&self) -> anyhow::Result<Settings> {
            let mut settings = Settings::default();
            settings.general.device_name = Some("Daemon Desk".to_string());
            Ok(settings)
        }

        async fn save(&self, _settings: &Settings) -> anyhow::Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn peer_discovery_worker_starts_network_and_announces_device_name() {
        let start_calls = Arc::new(Mutex::new(0));
        let announced_names = Arc::new(Mutex::new(Vec::new()));
        let (tx, rx) = mpsc::channel(8);

        let worker = PeerDiscoveryWorker::new(
            Arc::new(MockNetworkControl {
                calls: start_calls.clone(),
            }),
            Arc::new(MockNetworkEvents {
                rx: Mutex::new(Some(rx)),
            }),
            Arc::new(MockPeerDirectory {
                announced_names: announced_names.clone(),
            }),
            Arc::new(MockSettings),
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

        assert_eq!(*start_calls.lock().unwrap(), 1);
        assert_eq!(
            announced_names.lock().unwrap().as_slice(),
            ["Daemon Desk".to_string()]
        );

        cancel.cancel();
        task.await.unwrap().unwrap();
    }
}
