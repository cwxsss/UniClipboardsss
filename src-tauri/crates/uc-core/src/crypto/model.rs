//! Security / Encryption domain models.
//!
//! Slice 6 (U6) 起保留以下符号:
//! - `Passphrase`: 用户提供的解锁口令;跨 crate 作为领域输入类型
//! - `KeyScope`: 仍保留(Slice 6 范围内);被 `KeyScopePort` trait 返回值引用,
//!   彻底下沉需同步重构 `KeyScopePort`(Slice 7 / U4 候选 B)
//! - `EncryptionError`: 跨 crate 错误类型(uc-infra `KeySlotStore` port 等返回)
//!
//! 其余数据结构(`KdfParams` / `KdfParamsV1` / `KeySlot` / `WrappedMasterKey` /
//! `EncryptedBlob` / `KeySlotFile` / `KeySlotConvertError`)已物理下沉到
//! `uc-infra/src/security/crypto_model.rs` —— 它们属于磁盘 JSON /
//! pairing wire format 的承载结构,是 uc-infra 的持久化职责。
//! 运行时密钥物料(`Kek` / `MasterKey`)在 Slice 4 (B.4.5) 已搬到
//! `uc-infra/src/security/secrets.rs`。

use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyScope {
    /// Profile ID (user profile)
    pub profile_id: String,
}

/// Passphrase provided by user. Only used to derive KEK inside use cases.
/// Avoid storing this beyond the unlock/initialize flow.
#[derive(Clone)]
pub struct Passphrase(pub String);

impl fmt::Debug for Passphrase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Passphrase([REDACTED])")
    }
}

impl Passphrase {
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EncryptionError {
    #[error("encryption is not initialized")]
    NotInitialized,

    #[error("encryption is locked")]
    Locked,

    #[error("wrong passphrase")]
    WrongPassphrase,

    #[error("unsupported keyslot version")]
    UnsupportedKeySlotVersion,

    #[error("unsupported blob format version")]
    UnsupportedBlobVersion,

    #[error("corrupted keyslot data")]
    CorruptedKeySlot,

    #[error("corrupted encrypted blob")]
    CorruptedBlob,

    #[error("internal crypto failure")]
    CryptoFailure,

    #[error("invalid key")]
    InvalidKey,

    #[error("invalid parameter: {0}")]
    InvalidParameter(String),

    #[error("KDF operation failed")]
    KdfFailed,

    #[error("unsupported KDF algorithm")]
    UnsupportedKdfAlgorithm,

    #[error("encryption failed")]
    EncryptFailed,
    /// Keyring / Key Material errors

    #[error("key material not found")]
    KeyNotFound, // keyring 或 keyslot 缺失

    #[error("key material is corrupt")]
    KeyMaterialCorrupt, // keyslot 或 keyring 内容损坏/长度不对/反序列化失败

    #[error("other encryption error: {0}")]
    KeyringError(String),

    #[error("permission denied for key material access")]
    PermissionDenied, // keyring 权限/系统拒绝

    #[error("I/O failure during key material access")]
    IoFailure, // 文件/DB IO

    #[error("unsupported version for key material")]
    UnsupportedVersion, // keyslot/blob 版本不支持
}
