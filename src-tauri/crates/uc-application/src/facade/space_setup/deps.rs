//! Port bundle consumed by [`super::SpaceSetupFacade::new`].
//!
//! Kept as a `struct` with `pub` fields so callers build it with a plain
//! literal (`SpaceSetupDeps { space_access, local_identity, … }`) and
//! adding a new dependency in a future slice is one line here plus an
//! explicit field in the caller — no cascading constructor churn.

use std::sync::Arc;

use uc_core::membership::MemberRepositoryPort;
use uc_core::ports::space::SpaceAccessPort;
use uc_core::ports::{
    ClockPort, DeviceIdentityPort, LocalIdentityPort, SettingsPort, SetupStatusPort,
};

/// Dependencies for [`super::SpaceSetupFacade`].
///
/// `SpaceAccessPort` / `SetupStatusPort` are shared between A1 and A2
/// because the underlying adapter keeps the active space / setup status
/// as process-wide singletons; the facade clones these `Arc`s when
/// constructing each use case.
pub struct SpaceSetupDeps {
    pub space_access: Arc<dyn SpaceAccessPort>,
    pub local_identity: Arc<dyn LocalIdentityPort>,
    pub device_identity: Arc<dyn DeviceIdentityPort>,
    pub member_repo: Arc<dyn MemberRepositoryPort>,
    pub setup_status: Arc<dyn SetupStatusPort>,
    pub settings: Arc<dyn SettingsPort>,
    pub clock: Arc<dyn ClockPort>,
}
