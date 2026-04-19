//! v2 空间加密 adapter 模块。
//!
//! 实现 `uc-core::ports::space_encryption::SpaceCryptoPort`。
//!
//! 当前只有内存版本（Phase 3.1.a），仅支持 `create_space`。后续：
//! - 3.1.b 添加 SQLite 持久化与 Keychain 集成；
//! - 3.2 起扩展 `unlock / encrypt / decrypt / change_passphrase / join_space`。

mod adapter;
mod kdf;
mod types;

pub use adapter::InMemorySpaceCryptoAdapter;
pub use kdf::{derive_srk, derive_srk_salt, derive_subkeys, KdfError, Subkeys};
pub use types::{
    Dmk, KdfParams, SpaceMetadataV2, SpaceSeed, Srk, WrappedDmk, AEAD_NONCE_LEN, KEY_LEN,
    SPACE_SEED_LEN,
};
