pub mod adapters;
pub use adapters::InMemoryLifecycleStatus;

use anyhow::Result;
use async_trait::async_trait;

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
