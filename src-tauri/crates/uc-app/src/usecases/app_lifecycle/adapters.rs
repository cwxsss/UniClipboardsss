//! In-memory adapter for the lifecycle status port.

use anyhow::Result;
use async_trait::async_trait;

use super::{LifecycleState, LifecycleStatusPort};

/// Stores lifecycle state in a `tokio::sync::Mutex`.
///
/// This adapter is intended to live as an `Arc<InMemoryLifecycleStatus>` inside
/// `CoreRuntime` so that all lifecycle status accessors share the same instance.
pub struct InMemoryLifecycleStatus {
    state: tokio::sync::Mutex<LifecycleState>,
}

impl InMemoryLifecycleStatus {
    pub fn new() -> Self {
        Self {
            state: tokio::sync::Mutex::new(LifecycleState::Idle),
        }
    }
}

impl Default for InMemoryLifecycleStatus {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl LifecycleStatusPort for InMemoryLifecycleStatus {
    async fn set_state(&self, state: LifecycleState) -> Result<()> {
        *self.state.lock().await = state;
        Ok(())
    }

    async fn get_state(&self) -> LifecycleState {
        self.state.lock().await.clone()
    }
}
