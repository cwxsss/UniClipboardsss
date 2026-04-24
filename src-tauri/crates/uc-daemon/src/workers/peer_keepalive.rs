//! Peer keepalive worker — periodically refresh presence so iroh's magicsock
//! NAT binding and path cache never go idle.
//!
//! ## Why
//!
//! `IrohBlobTransferAdapter::fetch` opens a fresh QUIC connection to the
//! publisher every time a blob_ref comes in. When that peer hasn't been
//! dialed for ~60s the iroh endpoint's cached path has expired and the
//! connect attempt has to redo a full hole-punch + relay probe round. In
//! practice that takes ~33s and often terminates with `blob unavailable`
//! because the downloader's internal ConnectionPool also has a short
//! connect_timeout. Users observed "first copy after a while always fails".
//!
//! Refreshing presence on a short cadence keeps a warm PRESENCE_ALPN
//! connection alive per online peer, which in turn keeps the shared
//! magicsock layer (NAT binding, learned direct addrs) warm so the BLOBS
//! ALPN connection establishes on a hot path instead of cold-starting.
//!
//! ## Design
//!
//! * Delegates to `SpaceSetupFacade::refresh_presence()`, which internally
//!   runs `EnsureReachableAllUseCase` over every paired peer — reusing the
//!   existing dial path instead of introducing a second one.
//! * Ticker-only (no presence event subscription): the usecase is already
//!   idempotent and handles both dialing new peers and re-dialing stale
//!   connections. Subscribing would duplicate its scan.
//! * `MissedTickBehavior::Delay` avoids bursty catch-up if the previous
//!   refresh overran the interval (e.g. one peer's dial timing out).

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::time::MissedTickBehavior;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use uc_application::facade::SpaceSetupFacade;

use crate::service::{DaemonService, ServiceHealth};

/// Refresh cadence. Must sit comfortably below iroh's default QUIC idle
/// timeout (~60s) so the keepalive dial lands before the path is evicted.
/// 25s gives ~2× safety margin without flooding the network with probes.
const REFRESH_INTERVAL: Duration = Duration::from_secs(25);

pub struct PeerKeepAliveWorker {
    space_setup_facade: Arc<SpaceSetupFacade>,
}

impl PeerKeepAliveWorker {
    pub fn new(space_setup_facade: Arc<SpaceSetupFacade>) -> Self {
        Self { space_setup_facade }
    }
}

#[async_trait]
impl DaemonService for PeerKeepAliveWorker {
    fn name(&self) -> &str {
        "peer-keepalive"
    }

    async fn start(&self, cancel: CancellationToken) -> anyhow::Result<()> {
        let mut ticker = tokio::time::interval(REFRESH_INTERVAL);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        // Skip the immediate first tick — `SpaceSetupFacade::auto_start_network`
        // already fires one `ensure_reachable_all` right after network init, so
        // there's nothing to keep alive for the first 25s anyway.
        ticker.tick().await;

        info!(
            interval_secs = REFRESH_INTERVAL.as_secs(),
            "peer keepalive started"
        );

        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = ticker.tick() => {
                    match self.space_setup_facade.refresh_presence().await {
                        Ok(report) => {
                            debug!(
                                total = report.total,
                                online = report.online,
                                offline = report.offline,
                                errors = report.errors.len(),
                                "peer keepalive tick"
                            );
                        }
                        Err(err) => {
                            warn!(error = %err, "peer keepalive tick failed");
                        }
                    }
                }
            }
        }

        info!("peer keepalive cancelled");
        Ok(())
    }

    async fn stop(&self) -> anyhow::Result<()> {
        info!("peer keepalive stopped");
        Ok(())
    }

    fn health_check(&self) -> ServiceHealth {
        ServiceHealth::Healthy
    }
}
