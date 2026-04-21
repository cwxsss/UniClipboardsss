//! Result models for `MemberRosterFacade`.
//!
//! Kept minimal by design:
//!
//! * 没有 `ListWithPresenceQuery` —— 当前没有任何过滤维度可暴露(单 space
//!   场景下返回"全部成员"是唯一语义)。plan §4.1 把它列为未来扩展点,等
//!   第一个过滤条件(按在线状态 / 按 fingerprint 前缀)出现再加,避免现
//!   在硬塞一个空 struct。
//! * 没有 `last_seen_at` —— 该字段需要 `PresencePort` 持久追踪"最后一次
//!   Online 的时间戳",但 Slice 2 Phase 1 的 presence port 只暴露
//!   `ReachabilityState`(离散三态),没有时间维度。T7 验收点也只要求
//!   `state` 三值正确,先不加,省得打出一个永远 `None` 的误导字段。

use uc_core::ids::DeviceId;
use uc_core::ports::ReachabilityState;

/// One row of the member roster view.
///
/// 字段顺序按"UI 最关心 → 诊断信息"排列,方便 CLI 直接按 `{entry.device_name}
/// ({entry.state})` 打印。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RosterEntry {
    /// Stable device id(跨 rename 不变)——作为 member 的主键。
    pub device_id: DeviceId,
    /// 用户在 A1 / B2 时设置的可读名字,仅展示用。
    pub device_name: String,
    /// 当前正好有一条 entry 的 `is_local == true` —— 即本机。pre-A1/B2
    /// 状态下本机身份还没生成,此时所有成员都会是 `false`(此窗口期内
    /// `list_with_presence` 返回的 roster 也应该是空,因为还没有 membership
    /// 记录,但防御性仍处理)。
    pub is_local: bool,
    /// 来自 `PresencePort::current_state` 的纯缓存读。首次拨号(F1 hook 触
    /// 发的 `ensure_reachable_all`)完成前,典型值是 `Unknown`。
    pub state: ReachabilityState,
}
