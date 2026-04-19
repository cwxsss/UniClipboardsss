//! v2 空间加密的基础设施类型。
//!
//! 全部为基础设施细节（密码学产物、持久化结构）——不得进入 `uc-core`。

use rand::RngCore;
use uc_core::ids::SpaceId;
use zeroize::Zeroize as _;

// ---------------------------------------------------------------------------
// 固定密钥长度
// ---------------------------------------------------------------------------

/// 所有 32 字节密钥（SRK / DMK / HKDF 子密钥）共用的长度常量。
pub const KEY_LEN: usize = 32;

/// AEAD nonce 长度（XChaCha20-Poly1305 / 24 字节）。
pub const AEAD_NONCE_LEN: usize = 24;

/// 元数据 space_seed 长度。
pub const SPACE_SEED_LEN: usize = 32;

// ---------------------------------------------------------------------------
// SRK
// ---------------------------------------------------------------------------

/// Space Root Key —— 由 passphrase 经 Argon2id 派生得到的 32 字节根密钥。
///
/// Drop 时自动清零；不 Clone / Serialize。
pub struct Srk([u8; KEY_LEN]);

impl Srk {
    pub fn from_bytes(b: [u8; KEY_LEN]) -> Self {
        Self(b)
    }

    pub fn as_bytes(&self) -> &[u8; KEY_LEN] {
        &self.0
    }
}

impl Drop for Srk {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

// ---------------------------------------------------------------------------
// DMK
// ---------------------------------------------------------------------------

/// Data Master Key —— 随机生成的 32 字节数据加密密钥。
///
/// Drop 时自动清零；不 Clone / Serialize。
pub struct Dmk([u8; KEY_LEN]);

impl Dmk {
    /// 通过 OS CSPRNG 生成新的 DMK。
    pub fn generate() -> Self {
        let mut b = [0u8; KEY_LEN];
        rand::rng().fill_bytes(&mut b);
        Self(b)
    }

    pub fn from_bytes(b: [u8; KEY_LEN]) -> Self {
        Self(b)
    }

    pub fn as_bytes(&self) -> &[u8; KEY_LEN] {
        &self.0
    }
}

impl Drop for Dmk {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

// ---------------------------------------------------------------------------
// SpaceSeed
// ---------------------------------------------------------------------------

/// 空间种子 —— 创建空间时随机生成，与 space_id 一起作为 SRK 派生盐的一部分。
///
/// 不是秘密（会落盘在元数据中）；但为防止调试日志误打印，仍保持不透明。
#[derive(Clone)]
pub struct SpaceSeed([u8; SPACE_SEED_LEN]);

impl SpaceSeed {
    pub fn generate() -> Self {
        let mut b = [0u8; SPACE_SEED_LEN];
        rand::rng().fill_bytes(&mut b);
        Self(b)
    }

    pub fn from_bytes(b: [u8; SPACE_SEED_LEN]) -> Self {
        Self(b)
    }

    pub fn as_bytes(&self) -> &[u8; SPACE_SEED_LEN] {
        &self.0
    }
}

impl std::fmt::Debug for SpaceSeed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SpaceSeed({} bytes)", self.0.len())
    }
}

// ---------------------------------------------------------------------------
// KdfParams
// ---------------------------------------------------------------------------

/// Argon2id 参数。
///
/// 默认值对齐 Phase 0 决策 D2：保持现有强度（128 MiB / iters=3 / par=4）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KdfParams {
    pub mem_kib: u32,
    pub iters: u32,
    pub parallelism: u32,
}

impl Default for KdfParams {
    fn default() -> Self {
        Self {
            mem_kib: 128 * 1024,
            iters: 3,
            parallelism: 4,
        }
    }
}

impl KdfParams {
    /// 用于测试场景的最低成本参数（~KiB 级内存），大幅加速单测。
    ///
    /// 仅限测试使用；生产代码应通过 `Default` 取得生产参数。
    #[cfg(test)]
    pub fn insecure_test_defaults() -> Self {
        Self {
            mem_kib: 8,
            iters: 1,
            parallelism: 1,
        }
    }
}

// ---------------------------------------------------------------------------
// WrappedDmk
// ---------------------------------------------------------------------------

/// DMK 的 AEAD 包装产物。
///
/// 包含 nonce + 密文（Poly1305 tag 内联在密文末尾）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WrappedDmk {
    pub nonce: Vec<u8>,
    pub ciphertext: Vec<u8>,
}

// ---------------------------------------------------------------------------
// SpaceMetadataV2
// ---------------------------------------------------------------------------

/// v2 空间元数据。
///
/// 持久化形态：目前仅内存；Phase 3.1.b 会落到 SQLite `space_metadata` 表。
#[derive(Clone, Debug)]
pub struct SpaceMetadataV2 {
    pub space_id: SpaceId,
    pub space_seed: SpaceSeed,
    pub kdf_params: KdfParams,
    pub wrapped_dmk: WrappedDmk,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dmk_generate_uses_entropy() {
        let a = Dmk::generate();
        let b = Dmk::generate();
        assert_ne!(a.as_bytes(), b.as_bytes(), "两次生成的 DMK 应不相等");
    }

    #[test]
    fn space_seed_generate_uses_entropy() {
        let a = SpaceSeed::generate();
        let b = SpaceSeed::generate();
        assert_ne!(a.as_bytes(), b.as_bytes());
    }

    #[test]
    fn kdf_params_default_matches_d2_decision() {
        let p = KdfParams::default();
        assert_eq!(p.mem_kib, 128 * 1024);
        assert_eq!(p.iters, 3);
        assert_eq!(p.parallelism, 4);
    }

    #[test]
    fn space_seed_debug_does_not_leak_bytes() {
        let s = SpaceSeed::generate();
        let dbg = format!("{:?}", s);
        assert!(dbg.contains("32 bytes"));
        // 即使种子不是秘密也不应在 Debug 里打印字节
        assert!(!dbg.contains("["));
    }
}
