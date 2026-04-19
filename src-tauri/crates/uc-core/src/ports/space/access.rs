//! 空间访问 port。
//!
//! 以"用户口令视角"把空间的初始化 / 解锁 / 换口令 / 加锁 /
//! 就绪查询合并到一个业务动作 port。签名里只出现领域中性类型
//! （`Passphrase` / `ActiveSpace`），密钥物料（KEK / MasterKey /
//! KeySlot / WrappedMasterKey 等）一律在 adapter 内部生成和持有，
//! 不穿过这一层。
//!
//! 合并了原先的 `EncryptionPort`（KDF + wrap/unwrap 部分）、
//! `EncryptionSessionPort`、`KeyMaterialPort`、
//! `space::CryptoPort::{export_keyslot_blob, derive_master_key_from_keyslot}`。

use async_trait::async_trait;

use crate::crypto::domain::{ActiveSpace, Passphrase};
use crate::crypto::model::MasterKey;
use crate::ids::SpaceId;
use crate::space_access::JoinOffer;

/// 业务语义级的空间访问失败。
///
/// 故意不带密码学细节（algo / nonce / tag / blob 偏移等）——
/// adapter 内部可以保留更细的错误，这里只暴露调用方需要分支处理的类别。
#[derive(Debug, thiserror::Error)]
pub enum SpaceAccessError {
    /// 空间尚未初始化，无法解锁或换口令。
    #[error("space not initialized")]
    NotInitialized,

    /// 空间已初始化，不能再次 `initialize`。
    #[error("space already initialized")]
    AlreadyInitialized,

    /// 口令不匹配——unlock 时校验失败。
    #[error("wrong passphrase")]
    WrongPassphrase,

    /// 空间当前未解锁，无法执行需要会话的操作。
    #[error("space not unlocked")]
    NotUnlocked,

    /// 持久化的密钥物料损坏或版本不支持——属于数据层故障，不可恢复。
    #[error("space key material corrupted or unsupported")]
    CorruptedKeyMaterial,

    /// 其它内部故障（底层 IO / 算法实现异常等）。
    #[error("space access internal error: {0}")]
    Internal(String),
}

/// 空间访问 port。
///
/// 所有方法都以"空间"为中心——adapter 需要把 `SpaceId` 作为
/// 内部会话/密钥物料查找的键，外部调用方不感知 KEK / MasterKey。
#[async_trait]
pub trait SpaceAccessPort: Send + Sync {
    /// 首次初始化一个空间。
    ///
    /// 语义：
    /// - 生成全新的密钥物料并用口令派生的 KEK 加以包装，持久化落盘。
    /// - 完成后内存会话进入"已解锁"状态，返回 `ActiveSpace` 作为凭据。
    /// - 若该空间已初始化过，应返回 [`SpaceAccessError::AlreadyInitialized`]。
    async fn initialize(
        &self,
        space_id: &SpaceId,
        passphrase: &Passphrase,
    ) -> Result<ActiveSpace, SpaceAccessError>;

    /// 用口令解锁一个已初始化的空间。
    ///
    /// 语义：
    /// - 从持久化中读出包装后的密钥物料，用 `passphrase` 解包。
    /// - 成功后内存会话持有解包后的密钥，返回 `ActiveSpace`。
    /// - 口令错误返回 [`SpaceAccessError::WrongPassphrase`]。
    /// - 空间未初始化返回 [`SpaceAccessError::NotInitialized`]。
    async fn unlock(
        &self,
        space_id: &SpaceId,
        passphrase: &Passphrase,
    ) -> Result<ActiveSpace, SpaceAccessError>;

    /// 查询当前是否已解锁。
    async fn is_unlocked(&self, space_id: &SpaceId) -> bool;

    /// 清除内存会话——持久化密钥物料不受影响,后续仍可 `unlock`。
    async fn lock(&self, space_id: &SpaceId) -> Result<(), SpaceAccessError>;

    /// Sponsor 侧：准备 pairing offer。
    ///
    /// 读取该空间的 keyslot 序列化字节 + 产生 32 字节挑战 nonce,打包给 joiner。
    /// 空间未初始化返回 [`SpaceAccessError::NotInitialized`]。
    async fn prepare_join_offer(&self, space_id: &SpaceId) -> Result<JoinOffer, SpaceAccessError>;

    /// Joiner 侧：用口令解开 offer 的 keyslot 字节,派生出构造 proof 所需的 MasterKey。
    ///
    /// ⚠️ **已知技术债务**：返回类型暂时是 `MasterKey`——pairing proof 协议
    /// 的下一步（`ProofPort::build_proof`）当前也接受 `MasterKey`。两者是同
    /// 一条链路上的邻居,将在后续阶段重构 `ProofPort` 时统一换成不透明凭据。
    /// **不要为此方法增加新调用方**——新代码应等待 `ProofPort` 修订完成后
    /// 使用新签名。
    async fn derive_master_key_for_proof(
        &self,
        offer: &JoinOffer,
        passphrase: &Passphrase,
    ) -> Result<MasterKey, SpaceAccessError>;
}
