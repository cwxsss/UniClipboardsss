//! Setup is a business phase.
//!
//! **Deprecated re-export shim (phase B.2).** All setup code moved to
//! [`uc_application::setup`]. This module exists only to keep existing
//! `uc_app::usecases::setup::*` imports compiling during the incremental
//! migration; it will be removed in phase C.
//!
//! In addition to re-exports, this module supplies the adapter impls that
//! bridge two uc-app-level use-cases to their uc-application setup ports
//! (`SetupInitializeEncryptionPort`, `SetupAppLifecyclePort`) so that
//! bootstrap can wire `SetupOrchestrator` without inverting the
//! `uc-app → uc-application` dependency direction.

#[deprecated(
    since = "0.6.0",
    note = "setup moved to uc-application; import from uc_application::setup"
)]
#[allow(deprecated)]
pub use uc_application::setup::{
    MarkSetupComplete, SetupAppLifecyclePort, SetupInitializeEncryptionPort, SetupOrchestrator,
    SetupOrchestratorError as SetupError, SetupPairingFacadePort,
};

mod adapters;
