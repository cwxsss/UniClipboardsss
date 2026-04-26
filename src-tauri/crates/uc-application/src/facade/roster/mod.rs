//! Slice 2 Phase 1 · T7 — `MemberRosterFacade`.
//!
//! 对 UI / CLI 暴露已配对成员的列表,并给每条成员打上 presence 状态
//! (online / offline / unknown)和本机标记 (`is_local`)。是个典型的 thin
//! facade——不主动拨号、不管 rename / revoke、不做跨 use case 编排。主动
//! 拨号由 T6 `EnsureReachableAllUseCase` 负责,在
//! `SpaceSetupFacade::auto_start_network` (T8) 成功后一次性触发。
//!
//! ## 为什么 `list_with_presence` 不拨号
//!
//! 查询路径被频繁调用(CLI `members` 每次都会跑,未来 GUI 会做轮询或
//! subscribe)。如果每次查询都调 `presence.ensure_reachable()`,每个 peer
//! 都会产生一次 iroh dial,浪费带宽且让 UI 响应慢。`presence.current_state`
//! 是纯缓存读,O(1) 不触发 IO——查询路径应该总是走这一条。
//!
//! ## 模块导出
//!
//! 按 `uc-application/AGENTS.md` §11.4,只暴露 Facade + Query/Result/Error +
//! 订阅事件类型。`PresenceEvent` 从 `uc-core` 透出以便上层 crate 订阅
//! `subscribe_presence_events()` 时不用直接依赖 `uc-core::ports`。

mod commands;
mod errors;
mod facade;

pub use commands::{
    ContentTypesPatch, ContentTypesView, MemberSummary, MemberSyncPreferencesPatch,
    MemberSyncPreferencesView, RosterEntry,
};
pub use errors::RosterError;
pub use facade::{MemberRosterDeps, MemberRosterFacade};
pub use uc_core::ports::PresenceEvent;
