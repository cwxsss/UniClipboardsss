#![allow(deprecated)] // frozen libp2p NetworkEventPort consumer; replaced in Slice 5

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
