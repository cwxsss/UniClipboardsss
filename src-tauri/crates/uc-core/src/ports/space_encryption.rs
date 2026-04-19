//! 空间加密能力 port。
//!
//! 该 port 用"单一业务动作"的粒度封装空间加密流程——adapter 在内部完成
//! KDF 派生 / 密钥生成 / 包装 / 持久化 / 会话登记 / saga 回滚，领域层只看到
//! 业务语义（`create_space(passphrase) -> ActiveSpace` 等）。
//!
//! 当前仅包含 `create_space`。其他动作（`unlock / change_passphrase /
//! encrypt / decrypt / join_space`）随后续子阶段按需加入。
//!
//! 本 port 是 Phase 3 usecase-driven 设计的产物，形状由 Orchestrator/adapter
//! 的真实调用点反推——未来可能再次演化。

use async_trait::async_trait;

use crate::crypto::domain::{ActiveSpace, Passphrase};

/// 空间加密动作可能产生的错误。
#[derive(Debug, thiserror::Error)]
pub enum SpaceCryptoError {
    /// 基础设施层的意外失败（KDF / AEAD / 持久化）。
    ///
    /// 上层通常将此错误翻译为"创建空间失败，请重试"。
    #[error("space crypto internal failure: {0}")]
    Internal(#[from] anyhow::Error),
}

/// 空间加密端口——封装空间级别的加密业务动作。
///
/// 实现类（例如基础设施层的 adapter）应在每个方法内部保证 saga 的原子性：
/// 任何步骤失败必须回滚前序副作用，不允许把中间状态暴露给上层。
#[async_trait]
pub trait SpaceCryptoPort: Send + Sync {
    /// 创建一个新空间。
    ///
    /// 契约：
    /// - 输入口令不会被记录或持久化（adapter 实现必须遵守）
    /// - 成功时返回的 `ActiveSpace` 担保该空间已进入"已解锁"会话
    /// - 失败时不得留下半成品的元数据、密钥或会话条目
    async fn create_space(&self, passphrase: &Passphrase) -> Result<ActiveSpace, SpaceCryptoError>;
}
