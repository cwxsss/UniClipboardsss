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

use std::sync::Arc;

use crate::facade::{
    ClipboardHistoryFacade, ClipboardRestoreFacade, DeviceFacade, EncryptionFacade,
    LifecycleFacade, MemberRosterFacade, ResourceFacade, SettingsFacade, SpaceSetupFacade,
    StorageFacade,
};

/// Aggregator exposing one field per business sub-facade.
///
/// Fields are `pub` on purpose — callers drive sub-facade methods
/// directly (`app.space_setup.initialize_space(..)`). The aggregator
/// carries no logic, so there are no invariants to guard.
pub struct AppFacade {
    pub space_setup: Option<Arc<SpaceSetupFacade>>,
    pub member_roster: Option<Arc<MemberRosterFacade>>,
    pub lifecycle: Arc<LifecycleFacade>,
    pub encryption: Arc<EncryptionFacade>,
    pub resource: Arc<ResourceFacade>,
    pub clipboard_history: Arc<ClipboardHistoryFacade>,
    pub clipboard_restore: Arc<ClipboardRestoreFacade>,
    pub settings: Arc<SettingsFacade>,
    pub device: Arc<DeviceFacade>,
    pub storage: Arc<StorageFacade>,
}

impl AppFacade {
    /// Compose from already-constructed sub-facades.
    ///
    /// Bootstrap builds each sub-facade from its own `*Deps` bundle and
    /// hands them here — the aggregator never sees raw ports.
    pub fn new(parts: AppFacadeParts) -> Self {
        Self {
            space_setup: parts.space_setup,
            member_roster: parts.member_roster,
            lifecycle: parts.lifecycle,
            encryption: parts.encryption,
            resource: parts.resource,
            clipboard_history: parts.clipboard_history,
            clipboard_restore: parts.clipboard_restore,
            settings: parts.settings,
            device: parts.device,
            storage: parts.storage,
        }
    }
}

pub struct AppFacadeParts {
    pub space_setup: Option<Arc<SpaceSetupFacade>>,
    pub member_roster: Option<Arc<MemberRosterFacade>>,
    pub lifecycle: Arc<LifecycleFacade>,
    pub encryption: Arc<EncryptionFacade>,
    pub resource: Arc<ResourceFacade>,
    pub clipboard_history: Arc<ClipboardHistoryFacade>,
    pub clipboard_restore: Arc<ClipboardRestoreFacade>,
    pub settings: Arc<SettingsFacade>,
    pub device: Arc<DeviceFacade>,
    pub storage: Arc<StorageFacade>,
}
