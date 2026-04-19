//! v2 空间加密 adapter 模块。
//!
//! 实现 `uc-core::ports::space_encryption::SpaceCryptoPort`。
//!
//! 当前仅支持 `create_space`（Phase 3.1.b：含 SQLite 元数据持久化）。
//! 后续：3.2 起扩展 `unlock / encrypt / decrypt / change_passphrase / join_space`，
//! 并视需要把 Keychain SRK 缓存接入 `SecureStoragePort`。

mod adapter;
mod kdf;
mod payload;
mod repository;
mod types;

pub use adapter::SpaceCryptoAdapter;
pub use kdf::{derive_srk, derive_srk_salt, derive_subkeys, KdfError, Subkeys};
pub use payload::{decode as decode_payload, encode as encode_payload, PayloadError};
pub use repository::InMemorySpaceMetadataRepository;
pub use types::{
    Dmk, KdfParams, SpaceMetadataV2, SpaceSeed, Srk, WrappedDmk, AEAD_NONCE_LEN, KEY_LEN,
    SPACE_SEED_LEN,
};
