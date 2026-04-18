//! Adapter impls wiring uc-app use-cases to `uc-application::setup` ports.
//!
//! Setup's orchestrator lives in `uc-application` (phase B.2) but still needs
//! to drive two uc-app–level use-cases: `InitializeEncryption` and
//! `AppLifecycleCoordinator`. Rather than pull those use-cases into
//! `uc-application` (which would scope-creep into encryption/lifecycle
//! migration), we expose them through narrow setup-facing ports and adapt
//! here.

use async_trait::async_trait;

use uc_application::setup::{SetupAppLifecyclePort, SetupInitializeEncryptionPort};
use uc_core::crypto::model::Passphrase;

use crate::usecases::app_lifecycle::AppLifecycleCoordinator;
use crate::usecases::initialize_encryption::InitializeEncryption;

#[async_trait]
impl SetupInitializeEncryptionPort for InitializeEncryption {
    async fn execute(&self, passphrase: Passphrase) -> anyhow::Result<()> {
        InitializeEncryption::execute(self, passphrase)
            .await
            .map_err(anyhow::Error::new)
    }
}

#[async_trait]
impl SetupAppLifecyclePort for AppLifecycleCoordinator {
    async fn ensure_ready(&self) -> anyhow::Result<()> {
        AppLifecycleCoordinator::ensure_ready(self).await
    }
}
