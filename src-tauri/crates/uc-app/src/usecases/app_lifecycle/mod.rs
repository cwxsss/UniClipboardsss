pub mod adapters;
pub use adapters::{
    DeviceNameAnnouncer, InMemoryLifecycleStatus, LoggingLifecycleEventEmitter,
    LoggingSessionReadyEmitter,
};

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use tracing::{info, info_span, warn, Instrument};

use super::StartNetworkAfterUnlock;

// ---------------------------------------------------------------------------
// Lifecycle state
// ---------------------------------------------------------------------------

/// Represents the current state of the application lifecycle.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum LifecycleState {
    /// Initial state – no lifecycle attempt has been made yet.
    Idle,
    /// Lifecycle boot is in progress.
    Pending,
    /// All subsystems are running and ready.
    Ready,
    /// The network runtime failed to start.
    NetworkFailed,
}

// ---------------------------------------------------------------------------
// Lifecycle event
// ---------------------------------------------------------------------------

/// Events emitted during the lifecycle boot process.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum LifecycleEvent {
    /// The network runtime failed to start. Contains the error message.
    NetworkFailed(String),
    /// All subsystems booted successfully.
    Ready,
}

// ---------------------------------------------------------------------------
// Ports
// ---------------------------------------------------------------------------

/// Port for persisting and querying lifecycle state.
#[async_trait]
pub trait LifecycleStatusPort: Send + Sync {
    /// Persist a new lifecycle state.
    async fn set_state(&self, state: LifecycleState) -> Result<()>;
    /// Retrieve the current lifecycle state.
    async fn get_state(&self) -> LifecycleState;
}

/// Port for emitting lifecycle events (failures and readiness).
#[async_trait]
pub trait LifecycleEventEmitter: Send + Sync {
    /// Emit a lifecycle event to interested consumers.
    async fn emit_lifecycle_event(&self, event: LifecycleEvent) -> Result<()>;
}

/// Port for emitting a session-ready signal to the frontend.
#[async_trait]
pub trait SessionReadyEmitter: Send + Sync {
    async fn emit_ready(&self) -> Result<()>;
}

/// Port for announcing the local device after the network starts.
///
/// Implementations typically resolve the device name from settings and
/// broadcast it over the network so that peers can discover this device.
#[async_trait]
pub trait DeviceAnnouncer: Send + Sync {
    async fn announce(&self) -> Result<()>;
}

// ---------------------------------------------------------------------------
// Coordinator
// ---------------------------------------------------------------------------

/// Coordinates application lifecycle readiness by orchestrating
/// the network runtime and session ready emission.
///
/// On each call to [`ensure_ready`](Self::ensure_ready) the coordinator:
/// 1. Sets the lifecycle state to **Pending**.
/// 2. Attempts to start the network runtime.
/// 3. If it succeeds, sets state to **Ready** and emits a `Ready` event.
/// 4. On failure, sets the appropriate failed state and emits a failure event.
pub struct AppLifecycleCoordinator {
    network: Arc<StartNetworkAfterUnlock>,
    announcer: Option<Arc<dyn DeviceAnnouncer>>,
    emitter: Arc<dyn SessionReadyEmitter>,
    status: Arc<dyn LifecycleStatusPort>,
    lifecycle_emitter: Arc<dyn LifecycleEventEmitter>,
}

/// Helper for constructing the coordinator with explicit dependency fields.
pub struct AppLifecycleCoordinatorDeps {
    pub network: Arc<StartNetworkAfterUnlock>,
    pub announcer: Option<Arc<dyn DeviceAnnouncer>>,
    pub emitter: Arc<dyn SessionReadyEmitter>,
    pub status: Arc<dyn LifecycleStatusPort>,
    pub lifecycle_emitter: Arc<dyn LifecycleEventEmitter>,
}

impl AppLifecycleCoordinator {
    /// Create a new coordinator instance.
    pub fn new(
        network: Arc<StartNetworkAfterUnlock>,
        announcer: Option<Arc<dyn DeviceAnnouncer>>,
        emitter: Arc<dyn SessionReadyEmitter>,
        status: Arc<dyn LifecycleStatusPort>,
        lifecycle_emitter: Arc<dyn LifecycleEventEmitter>,
    ) -> Self {
        Self {
            network,
            announcer,
            emitter,
            status,
            lifecycle_emitter,
        }
    }

    /// Construct a coordinator from dependency bundle.
    pub fn from_deps(deps: AppLifecycleCoordinatorDeps) -> Self {
        let AppLifecycleCoordinatorDeps {
            network,
            announcer,
            emitter,
            status,
            lifecycle_emitter,
        } = deps;

        Self::new(network, announcer, emitter, status, lifecycle_emitter)
    }

    /// Ensure the application lifecycle is ready by booting
    /// network and emitting the ready event.
    ///
    /// State transitions:
    /// - `Idle` / any → `Pending` → `Ready` (on success)
    /// - `Idle` / any → `Pending` → `NetworkFailed` (if network fails)
    pub async fn ensure_ready(&self) -> Result<()> {
        let span = info_span!("usecase.app_lifecycle_coordinator.ensure_ready");

        async {
            let current_state = self.status.get_state().await;
            info!(state = ?current_state, "Lifecycle ensure_ready invoked");
            if matches!(current_state, LifecycleState::Ready) {
                info!("Lifecycle already Ready; skipping duplicate ensure_ready call");
                return Ok(());
            }
            // 1. Mark as pending
            self.status.set_state(LifecycleState::Pending).await?;
            info!("Lifecycle state set to Pending");

            // 2. Start network
            if let Err(e) = self.network.execute().await {
                let msg = e.to_string();
                if msg.to_ascii_lowercase().contains("already started") {
                    info!(error = %msg, "network already started; skip");
                } else {
                    warn!(error = %msg, "Network failed to start");
                    self.status.set_state(LifecycleState::NetworkFailed).await?;
                    self.lifecycle_emitter
                        .emit_lifecycle_event(LifecycleEvent::NetworkFailed(msg.clone()))
                        .await?;
                    return Err(anyhow::anyhow!(msg));
                }
            }

            // 2.5. Announce device name (best-effort, failure is non-fatal)
            if let Some(announcer) = &self.announcer {
                if let Err(e) = announcer.announce().await {
                    warn!(error = %e, "Failed to announce device name after network start");
                }
            }

            // 3. All good – mark ready and emit events
            self.status.set_state(LifecycleState::Ready).await?;
            self.lifecycle_emitter
                .emit_lifecycle_event(LifecycleEvent::Ready)
                .await?;
            self.emitter.emit_ready().await?;
            info!("Lifecycle state set to Ready");

            Ok(())
        }
        .instrument(span)
        .await
    }
}
