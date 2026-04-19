//! SRK 派生（Argon2id）与子密钥派生（HKDF-SHA256）。
//!
//! 本模块不涉及领域层任何类型——输入/输出都是基础设施字节串或 infra 内部类型。

use argon2::Argon2;
use hkdf::Hkdf;
use sha2::{Digest, Sha256};

use super::types::{KdfParams, SpaceSeed, Srk, KEY_LEN};

const SRK_SALT_DOMAIN: &[u8] = b"uniclipboard-salt";

/// KDF 过程中可能出现的错误。
#[derive(Debug, thiserror::Error)]
pub enum KdfError {
    #[error("invalid Argon2 parameters")]
    InvalidParams,
    #[error("Argon2id derivation failed")]
    Argon2Failed,
    #[error("HKDF expand failed")]
    HkdfFailed,
}

/// 根据 space_id + seed 计算 Argon2id 的盐值。
///
/// 公式（Phase 0 决策）：`salt = SHA256("uniclipboard-salt" || space_id || space_seed)`
pub fn derive_srk_salt(space_id: &str, seed: &SpaceSeed) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(SRK_SALT_DOMAIN);
    h.update(space_id.as_bytes());
    h.update(seed.as_bytes());
    let out = h.finalize();
    let mut salt = [0u8; 32];
    salt.copy_from_slice(&out);
    salt
}

/// 从 passphrase 派生 SRK。
///
/// Argon2id v0x13，参数见 `KdfParams`；输出 32 字节。
pub fn derive_srk(
    passphrase: &[u8],
    space_id: &str,
    seed: &SpaceSeed,
    params: &KdfParams,
) -> Result<Srk, KdfError> {
    let salt = derive_srk_salt(space_id, seed);
    let argon2 = Argon2::new(
        argon2::Algorithm::Argon2id,
        argon2::Version::V0x13,
        argon2::Params::new(
            params.mem_kib,
            params.iters,
            params.parallelism,
            Some(KEY_LEN),
        )
        .map_err(|_| KdfError::InvalidParams)?,
    );
    let mut okm = [0u8; KEY_LEN];
    argon2
        .hash_password_into(passphrase, &salt, &mut okm)
        .map_err(|_| KdfError::Argon2Failed)?;
    Ok(Srk::from_bytes(okm))
}

/// 从 SRK 派生的三路子密钥。
///
/// HKDF-SHA256 派生；salt 和 info 按 Phase 0 决策补齐：
/// - salt = space_id || space_seed
/// - info ∈ {"uc/v2/dmk-wrap", "uc/v2/metadata", "uc/v2/auth"}
pub struct Subkeys {
    pub dmk_wrap_key: [u8; KEY_LEN],
    pub metadata_key: [u8; KEY_LEN],
    pub auth_key: [u8; KEY_LEN],
}

const INFO_DMK_WRAP: &[u8] = b"uc/v2/dmk-wrap";
const INFO_METADATA: &[u8] = b"uc/v2/metadata";
const INFO_AUTH: &[u8] = b"uc/v2/auth";

pub fn derive_subkeys(srk: &Srk, space_id: &str, seed: &SpaceSeed) -> Result<Subkeys, KdfError> {
    let mut salt = Vec::with_capacity(space_id.len() + seed.as_bytes().len());
    salt.extend_from_slice(space_id.as_bytes());
    salt.extend_from_slice(seed.as_bytes());

    let hk = Hkdf::<Sha256>::new(Some(&salt), srk.as_bytes());
    let mut dmk_wrap = [0u8; KEY_LEN];
    let mut metadata = [0u8; KEY_LEN];
    let mut auth = [0u8; KEY_LEN];
    hk.expand(INFO_DMK_WRAP, &mut dmk_wrap)
        .map_err(|_| KdfError::HkdfFailed)?;
    hk.expand(INFO_METADATA, &mut metadata)
        .map_err(|_| KdfError::HkdfFailed)?;
    hk.expand(INFO_AUTH, &mut auth)
        .map_err(|_| KdfError::HkdfFailed)?;
    Ok(Subkeys {
        dmk_wrap_key: dmk_wrap,
        metadata_key: metadata,
        auth_key: auth,
    })
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srk_salt_is_deterministic() {
        let seed = SpaceSeed::from_bytes([7u8; 32]);
        let a = derive_srk_salt("space-xyz", &seed);
        let b = derive_srk_salt("space-xyz", &seed);
        assert_eq!(a, b);
    }

    #[test]
    fn srk_salt_varies_by_space_id() {
        let seed = SpaceSeed::from_bytes([7u8; 32]);
        let a = derive_srk_salt("space-A", &seed);
        let b = derive_srk_salt("space-B", &seed);
        assert_ne!(a, b);
    }

    #[test]
    fn srk_salt_varies_by_seed() {
        let s1 = SpaceSeed::from_bytes([1u8; 32]);
        let s2 = SpaceSeed::from_bytes([2u8; 32]);
        let a = derive_srk_salt("space", &s1);
        let b = derive_srk_salt("space", &s2);
        assert_ne!(a, b);
    }

    #[test]
    fn derive_srk_is_deterministic_with_test_params() {
        let params = KdfParams::insecure_test_defaults();
        let seed = SpaceSeed::from_bytes([9u8; 32]);
        let srk_a = derive_srk(b"correct horse", "space", &seed, &params).unwrap();
        let srk_b = derive_srk(b"correct horse", "space", &seed, &params).unwrap();
        assert_eq!(srk_a.as_bytes(), srk_b.as_bytes());
    }

    #[test]
    fn derive_srk_differs_by_passphrase() {
        let params = KdfParams::insecure_test_defaults();
        let seed = SpaceSeed::from_bytes([9u8; 32]);
        let a = derive_srk(b"alpha", "space", &seed, &params).unwrap();
        let b = derive_srk(b"beta", "space", &seed, &params).unwrap();
        assert_ne!(a.as_bytes(), b.as_bytes());
    }

    #[test]
    fn subkeys_three_ways_are_distinct() {
        let params = KdfParams::insecure_test_defaults();
        let seed = SpaceSeed::from_bytes([9u8; 32]);
        let srk = derive_srk(b"pp", "space", &seed, &params).unwrap();
        let sk = derive_subkeys(&srk, "space", &seed).unwrap();
        assert_ne!(sk.dmk_wrap_key, sk.metadata_key);
        assert_ne!(sk.metadata_key, sk.auth_key);
        assert_ne!(sk.dmk_wrap_key, sk.auth_key);
    }

    #[test]
    fn subkeys_deterministic_with_same_inputs() {
        let params = KdfParams::insecure_test_defaults();
        let seed = SpaceSeed::from_bytes([9u8; 32]);
        let srk1 = derive_srk(b"pp", "space", &seed, &params).unwrap();
        let srk2 = derive_srk(b"pp", "space", &seed, &params).unwrap();
        let sk1 = derive_subkeys(&srk1, "space", &seed).unwrap();
        let sk2 = derive_subkeys(&srk2, "space", &seed).unwrap();
        assert_eq!(sk1.dmk_wrap_key, sk2.dmk_wrap_key);
        assert_eq!(sk1.metadata_key, sk2.metadata_key);
        assert_eq!(sk1.auth_key, sk2.auth_key);
    }
}
