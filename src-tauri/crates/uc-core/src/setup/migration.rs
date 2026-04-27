//! Switch-space 重加密迁移的领域类型。
//!
//! 设备已经完成 setup 后再加入另一个 sponsor 的空间时，要把本地剪贴板
//! 历史从旧 master_key 转加密到新 master_key（详见
//! `docs/architecture/ports.md` 的 switch-space 章节）。整个流程分四阶段，
//! 每完成一个阶段就把状态写回 `MigrationStatePort` 的实现：
//!
//! | 阶段        | 已落盘的事实                                         |
//! |-------------|------------------------------------------------------|
//! | `Prepared`  | backup 表已写满，migration_key 在 keyring，主表未动  |
//! | `HandshakeDone` | session/磁盘 keyslot/keyring KEK 已切到新空间        |
//! | `Swapped`   | 主表已用新 master_key 重写，backup 表 + migration_key 仍在 |
//! | `None`      | 阶段 4 完成：backup 表清空、migration_key 销毁       |
//!
//! 故障恢复策略由 `SwitchSpaceUseCase` 在启动时按当前阶段判断，详见
//! 各变体 doc。
//!
//! 这两个类型只承载领域语义，不绑定具体存储实现——具体落盘位置由
//! `MigrationStatePort` 的 adapter 决定。

use serde::{Deserialize, Serialize};

use crate::ids::SpaceId;

/// 一次 switch-space 迁移运行的稳定标识。
///
/// 由 `KeyMigrationPort::prepare_migration_key` 在阶段 1 开始时生成
/// （时间戳 + 随机后缀），随后被序列化进 `MigrationPhase`，落盘后即使
/// daemon 崩溃重启也能用同一 id 找回 keyring 里的 migration key。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MigrationRunId(String);

impl MigrationRunId {
    /// 由 adapter 在生成 migration key 时构造。app/use-case 层不应自己
    /// 拼造——run_id 与 keyring entry 名一一对应，乱造会破坏密钥定位。
    pub fn new(raw: impl Into<String>) -> Self {
        Self(raw.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for MigrationRunId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// 一次迁移当前所处的阶段。
///
/// 值唯一持久化点是 `MigrationStatePort`，整个生命周期里只有 4 种合法值：
/// `None` / `Prepared` / `HandshakeDone` / `Swapped`，每个变体都携带
/// `run_id`（让 adapter 找到 keyring 里的 migration key）和
/// `target_space_id`（最终要切到的空间 id，phase 4 写回 `SetupStatus`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MigrationPhase {
    /// 阶段 1 已完成：所有 representation 已用 migration_key 重加密
    /// 写入 backup 表。主表内容未变，旧 master_key 仍在 session/磁盘/keyring。
    ///
    /// 故障恢复：可以选择继续走阶段 2，也可以直接放弃（清空 backup 表 +
    /// 销毁 migration_key），旧空间数据完整。
    Prepared {
        run_id: MigrationRunId,
        target_space_id: SpaceId,
    },

    /// 阶段 2 已完成：sponsor handshake 走完，session / 磁盘 keyslot /
    /// keyring KEK 三处都换成新空间的 master_key。主表里的密文都是用
    /// 旧 master_key 加密的——此时主表已不可读，必须依靠 backup 表 +
    /// migration_key 走完阶段 3 才能恢复。
    ///
    /// 故障恢复：自动重试阶段 3。无法回退（旧 master_key 已不可恢复）。
    HandshakeDone {
        run_id: MigrationRunId,
        target_space_id: SpaceId,
    },

    /// 阶段 3 已完成：主表所有 representation 都用新 master_key 重写。
    /// backup 表 + migration_key 仍在，等待阶段 4 清理。
    ///
    /// 故障恢复：自动补做阶段 4（写 admit/trust/setup_status，清掉
    /// backup 表和 migration_key）。
    Swapped {
        run_id: MigrationRunId,
        target_space_id: SpaceId,
    },
}

impl MigrationPhase {
    pub fn run_id(&self) -> &MigrationRunId {
        match self {
            Self::Prepared { run_id, .. }
            | Self::HandshakeDone { run_id, .. }
            | Self::Swapped { run_id, .. } => run_id,
        }
    }

    pub fn target_space_id(&self) -> &SpaceId {
        match self {
            Self::Prepared {
                target_space_id, ..
            }
            | Self::HandshakeDone {
                target_space_id, ..
            }
            | Self::Swapped {
                target_space_id, ..
            } => target_space_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_id_round_trips_string() {
        let id = MigrationRunId::new("mig-2026-04-27-abc");
        assert_eq!(id.as_str(), "mig-2026-04-27-abc");
        assert_eq!(id.to_string(), "mig-2026-04-27-abc");
    }

    #[test]
    fn migration_phase_serde_round_trip_prepared() {
        let phase = MigrationPhase::Prepared {
            run_id: MigrationRunId::new("run-1"),
            target_space_id: SpaceId::from_str("space-target"),
        };
        let json = serde_json::to_string(&phase).unwrap();
        let parsed: MigrationPhase = serde_json::from_str(&json).unwrap();
        assert_eq!(phase, parsed);
        // 标签字段名稳定性 ironclad：磁盘格式契约。
        assert!(json.contains("\"kind\":\"prepared\""));
    }

    #[test]
    fn migration_phase_accessors_match_variants() {
        let run_id = MigrationRunId::new("run-x");
        let space = SpaceId::from_str("space-x");
        let phase = MigrationPhase::HandshakeDone {
            run_id: run_id.clone(),
            target_space_id: space.clone(),
        };
        assert_eq!(phase.run_id(), &run_id);
        assert_eq!(phase.target_space_id(), &space);
    }
}
