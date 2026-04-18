//! Membership use cases (phase 4b entry points).
//!
//! 通过 `MemberRepositoryPort` 读写空间成员 sync preferences 的新路径。
//! 与 `usecases::pairing::{GetDeviceSyncSettings, UpdateDeviceSyncSettings}` 并存，
//! 由 daemon API / 前端按需切换，PR-4 移除旧 UC。

pub mod get_member_sync_preferences;
pub mod update_member_sync_preferences;

pub use get_member_sync_preferences::GetMemberSyncPreferences;
pub use update_member_sync_preferences::UpdateMemberSyncPreferences;
