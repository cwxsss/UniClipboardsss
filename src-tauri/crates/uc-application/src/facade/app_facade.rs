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

use thiserror::Error;
use tokio::sync::broadcast;

use crate::facade::roster::{MemberSummary, PeerSnapshotView, RosterError};
use crate::facade::{
    ClipboardHistoryFacade, ClipboardRestoreFacade, DeviceFacade, EncryptionFacade,
    LifecycleFacade, MemberRosterFacade, ResourceFacade, SearchFacade, SettingsFacade,
    SpaceSetupFacade, StorageFacade,
};
use uc_core::ports::{PresenceEvent, ReachabilityState};

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
    /// CLI / 仅查询场景下 daemon/Tauri 不构造 restore facade,这里是 None。
    /// daemon API handler 取出前需做存在性检查。
    pub clipboard_restore: Option<Arc<ClipboardRestoreFacade>>,
    pub search: Arc<SearchFacade>,
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
            search: parts.search,
            settings: parts.settings,
            device: parts.device,
            storage: parts.storage,
        }
    }

    /// 列出对外成员摘要。外部调用只经过 `AppFacade`,不直接依赖 roster 子 facade。
    pub async fn list_members(&self) -> Result<Vec<MemberSummary>, RosterError> {
        self.member_roster
            .as_ref()
            .ok_or(RosterError::Unavailable)?
            .list_members()
            .await
    }

    /// 列出对外 peer 快照。外部调用只经过 `AppFacade`,不直接依赖 roster 子 facade。
    pub async fn list_peer_snapshots(&self) -> Result<Vec<PeerSnapshotView>, RosterError> {
        self.member_roster
            .as_ref()
            .ok_or(RosterError::Unavailable)?
            .list_peer_snapshots()
            .await
    }

    /// 订阅成员在线状态变化。外部拿到的是 application 事件,不暴露 core 事件类型。
    pub fn subscribe_peer_presence_events(&self) -> Result<AppPresenceSubscription, RosterError> {
        let inner = self
            .member_roster
            .as_ref()
            .ok_or(RosterError::Unavailable)?
            .subscribe_presence_events();
        Ok(AppPresenceSubscription { inner })
    }
}

/// application 层 presence 事件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPresenceEvent {
    pub device_id: String,
    pub state: String,
    pub at_ms: i64,
}

/// application 层 presence 订阅错误。
#[derive(Debug, Error)]
pub enum AppPresenceSubscriptionError {
    #[error("presence event receiver lagged by {0} messages")]
    Lagged(u64),
    #[error("presence event receiver closed")]
    Closed,
}

/// application 层 presence 订阅句柄。
pub struct AppPresenceSubscription {
    inner: broadcast::Receiver<PresenceEvent>,
}

impl AppPresenceSubscription {
    pub async fn recv(&mut self) -> Result<AppPresenceEvent, AppPresenceSubscriptionError> {
        self.inner
            .recv()
            .await
            .map(presence_event_to_app)
            .map_err(|err| match err {
                broadcast::error::RecvError::Lagged(skipped) => {
                    AppPresenceSubscriptionError::Lagged(skipped)
                }
                broadcast::error::RecvError::Closed => AppPresenceSubscriptionError::Closed,
            })
    }
}

fn presence_event_to_app(event: PresenceEvent) -> AppPresenceEvent {
    AppPresenceEvent {
        device_id: event.device_id.as_str().to_string(),
        state: reachability_state_to_string(event.state),
        at_ms: event.at.timestamp_millis(),
    }
}

fn reachability_state_to_string(state: ReachabilityState) -> String {
    match state {
        ReachabilityState::Online => "online",
        ReachabilityState::Offline => "offline",
        ReachabilityState::Unknown => "unknown",
    }
    .to_string()
}

pub struct AppFacadeParts {
    pub space_setup: Option<Arc<SpaceSetupFacade>>,
    pub member_roster: Option<Arc<MemberRosterFacade>>,
    pub lifecycle: Arc<LifecycleFacade>,
    pub encryption: Arc<EncryptionFacade>,
    pub resource: Arc<ResourceFacade>,
    pub clipboard_history: Arc<ClipboardHistoryFacade>,
    pub clipboard_restore: Option<Arc<ClipboardRestoreFacade>>,
    pub search: Arc<SearchFacade>,
    pub settings: Arc<SettingsFacade>,
    pub device: Arc<DeviceFacade>,
    pub storage: Arc<StorageFacade>,
}
