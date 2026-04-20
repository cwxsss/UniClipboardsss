//! Port bundle consumed by [`super::SpaceSetupFacade::new`].
//!
//! Kept as a `struct` with `pub` fields so callers build it with a plain
//! literal (`SpaceSetupDeps { space_access, local_identity, … }`) and
//! adding a new dependency in a future slice is one line here plus an
//! explicit field in the caller — no cascading constructor churn.

use std::sync::Arc;

use uc_core::membership::MemberRepositoryPort;
use uc_core::ports::pairing::{PairingEventPort, PairingSessionPort};
use uc_core::ports::pairing_invitation::PairingInvitationPort;
use uc_core::ports::space::SpaceAccessPort;
use uc_core::ports::{
    ClockPort, DeviceIdentityPort, LocalIdentityPort, NetworkControlPort, SettingsPort,
    SetupStatusPort,
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
    /// Network runtime lifecycle. Auto-started on A1/A2 success (F1) and
    /// stopped by [`super::SpaceSetupFacade::on_shutdown`] (F2).
    pub network_control: Arc<dyn NetworkControlPort>,
    /// Sponsor-side rendezvous client for issuing invitation codes (B1)
    /// and notifying the rendezvous of successful consumes (P7e inbound
    /// path).
    ///
    /// The accompanying in-memory holder for parked invitations is
    /// constructed **inside** [`super::SpaceSetupFacade::new`] and kept
    /// `pub(crate)` so application-internal implementation details
    /// (`uc-application/AGENTS.md` §11.4) stay off the bootstrap surface.
    pub pairing_invitation: Arc<dyn PairingInvitationPort>,
    /// Session-level transport used by the sponsor-side inbound orchestrator
    /// to send `PairingReject` and close sessions that fail code matching.
    /// Joiner-side uses the same port to dial; Slice 1 wires a single
    /// adapter (`IrohPairingSessionAdapter`) for both roles.
    pub pairing_session: Arc<dyn PairingSessionPort>,
    /// Sponsor-side subscription to inbound pairing events. Drives the
    /// [`crate::pairing_inbound`] orchestrator; the facade spawns the event
    /// loop during construction and stops it on shutdown.
    pub pairing_events: Arc<dyn PairingEventPort>,
}
