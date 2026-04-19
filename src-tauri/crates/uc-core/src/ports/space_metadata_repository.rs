//! 空间元数据持久化 port。
//!
//! port 签名**只看字节**，不知道持久化格式结构——adapter 在内部做
//! 序列化/反序列化。这样 uc-core 完全与具体格式/数据库脱耦。
//!
//! 当前 port 仅含 `save` / `load`（由 Phase 3.1 的 `create_space` adapter 呼出）。
//! `delete` / `list` 等操作留给后续 usecase 呼出时按需加入。

use async_trait::async_trait;
use thiserror::Error;

use crate::ids::SpaceId;

#[derive(Debug, Error)]
pub enum SpaceMetadataError {
    /// 底层存储未能识别 payload（不符合当前格式或损坏）。
    #[error("space metadata payload corrupted: {0}")]
    Corrupted(String),

    /// 存储后端失败（数据库、文件系统、IO 等）。
    #[error("space metadata backend failure: {0}")]
    Backend(#[from] anyhow::Error),
}

/// 空间元数据持久化 port。
///
/// 实现约定：
/// - `save` 为 upsert 语义——相同 space_id 覆盖旧 payload。
/// - `load` 在不存在时返回 `Ok(None)`，而不是错误。
/// - 不保证跨进程或跨设备的并发一致性；上层保证同一 space_id 不并行写。
#[async_trait]
pub trait SpaceMetadataRepositoryPort: Send + Sync {
    async fn save(&self, space_id: &SpaceId, payload: &[u8]) -> Result<(), SpaceMetadataError>;
    async fn load(&self, space_id: &SpaceId) -> Result<Option<Vec<u8>>, SpaceMetadataError>;
}
