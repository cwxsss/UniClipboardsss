//! `AppFacade` — Slice 1 cross-domain aggregator.
//!
//! Per `uc-application/AGENTS.md` §11.4 external consumers reach the
//! application layer exclusively through a facade. `AppFacade` is the
//! single outward-facing type; internally it just groups sub-facades,
//! each constructed from its own `*Deps` bundle, so adding a new
//! domain does not cascade into a constructor explosion.
//!
//! # Current scope (Slice 1 · P4)
//!
//! * [`SpaceSetupFacade`] — A1 `initialize_space`, A2 `unlock_space`
//!
//! # Deferred
//!
//! * `PairingFacade` (B1 / B2) → P7+
//! * `SyncFacade` (C1 / C2 / C3) → Slice 2
//! * F1 `on_startup` / F2 `on_shutdown` → P6 (lives inside the
//!   sub-facades once `StartNetwork` plumbing exists)
//! * Daemon / tauri / CLI switching from the legacy sub-facades
//!   (`SetupFacade`, `PairingFacade`, `SpaceAccessFacade`) to
//!   `AppFacade` → Slice 1.5 or later. Those sub-facades remain `pub`
//!   this slice to keep existing entry points working.

use crate::facade::space_setup::SpaceSetupFacade;

/// Aggregator exposing one field per business sub-facade.
///
/// Fields are `pub` on purpose — callers drive sub-facade methods
/// directly (`app.space_setup.initialize_space(..)`). The aggregator
/// carries no logic, so there are no invariants to guard.
pub struct AppFacade {
    pub space_setup: SpaceSetupFacade,
    // P7+: pub pairing: PairingFacade
    // Slice 2: pub sync: SyncFacade
}

impl AppFacade {
    /// Compose from already-constructed sub-facades.
    ///
    /// Bootstrap builds each sub-facade from its own `*Deps` bundle and
    /// hands them here — the aggregator never sees raw ports.
    pub fn new(space_setup: SpaceSetupFacade) -> Self {
        Self { space_setup }
    }
}
