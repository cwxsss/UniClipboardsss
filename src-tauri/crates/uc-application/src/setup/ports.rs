//! Setup-facing ports for cross-crate use-case dependencies.
//!
//! `SetupOrchestrator` / `SetupActionExecutor` historically called into
//! `uc-app::usecases::AppLifecycleCoordinator`. Now that setup lives in
//! `uc-application`, we cannot reach back into `uc-app` (that would create a
//! dependency cycle). The trait hides the concrete use-case behind a narrow,
//! setup-specific interface; `uc-app` supplies the adapter.
//!
//! Phase C (2026-04-19): `SetupInitializeEncryptionPort` was removed — setup
//! action `CreateEncryptedSpace` now calls `SpaceAccessPort.initialize`
//! directly (the old trait was a redundant adapter over a thin wrapper).

#[async_trait::async_trait]
pub trait SetupAppLifecyclePort: Send + Sync {
    async fn ensure_ready(&self) -> anyhow::Result<()>;
}
