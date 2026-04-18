//! Setup-facing ports for cross-crate use-case dependencies.
//!
//! `SetupOrchestrator` / `SetupActionExecutor` historically called into
//! `uc-app::usecases::InitializeEncryption` and
//! `uc-app::usecases::AppLifecycleCoordinator`. Now that setup lives in
//! `uc-application`, we cannot reach back into `uc-app` (that would create a
//! dependency cycle). These traits hide both concrete use-cases behind narrow,
//! setup-specific interfaces; `uc-app` supplies adapters.

use uc_core::crypto::model::Passphrase;

#[async_trait::async_trait]
pub trait SetupInitializeEncryptionPort: Send + Sync {
    async fn execute(&self, passphrase: Passphrase) -> anyhow::Result<()>;
}

#[async_trait::async_trait]
pub trait SetupAppLifecyclePort: Send + Sync {
    async fn ensure_ready(&self) -> anyhow::Result<()>;
}
