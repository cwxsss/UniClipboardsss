//! 空间元数据的持久化格式。
//!
//! 与 `types::SpaceMetadataV2` 刻意分离：领域结构可以独立演化，格式版本
//! 由本层管理。当前只有 `v2` 一个版本；未来若需要切换格式，在此新增
//! `v3 / v4 ...` 分支并集中迁移。
//!
//! 格式：UTF-8 serde_json，字节域使用 base64 (URL-safe, no-pad) 字符串
//! 以保证可读性与体积折中。顶层字段 `version` 做格式判别。

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use uc_core::ids::SpaceId;

use super::types::{KdfParams, SpaceMetadataV2, SpaceSeed, WrappedDmk, AEAD_NONCE_LEN, KEY_LEN};

const PAYLOAD_VERSION_V2: u32 = 2;
const WRAPPED_DMK_LEN: usize = KEY_LEN + 16; // 32 字节 DMK + 16 字节 Poly1305 tag

/// 顶层 payload 结构。
#[derive(Serialize, Deserialize)]
#[serde(tag = "version")]
enum Payload {
    /// v2 格式 —— 当前唯一支持的格式。
    #[serde(rename = "2")]
    V2(PayloadV2),
}

#[derive(Serialize, Deserialize)]
struct PayloadV2 {
    space_id: String,
    space_seed_b64: String,
    kdf: KdfPayload,
    wrapped_dmk: WrappedDmkPayload,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize, Deserialize)]
struct KdfPayload {
    algorithm: String,
    mem_kib: u32,
    iters: u32,
    parallelism: u32,
}

#[derive(Serialize, Deserialize)]
struct WrappedDmkPayload {
    nonce_b64: String,
    ciphertext_b64: String,
}

#[derive(Debug, thiserror::Error)]
pub enum PayloadError {
    #[error("serde_json failure: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("base64 decode failure: {0}")]
    Base64(#[from] base64::DecodeError),
    #[error("payload field out of spec: {0}")]
    Shape(String),
}

/// 把 `SpaceMetadataV2` 编码为持久化字节。
pub fn encode(meta: &SpaceMetadataV2) -> Result<Vec<u8>, PayloadError> {
    let p = Payload::V2(PayloadV2 {
        space_id: meta.space_id.as_str().to_string(),
        space_seed_b64: URL_SAFE_NO_PAD.encode(meta.space_seed.as_bytes()),
        kdf: KdfPayload {
            algorithm: "argon2id".to_string(),
            mem_kib: meta.kdf_params.mem_kib,
            iters: meta.kdf_params.iters,
            parallelism: meta.kdf_params.parallelism,
        },
        wrapped_dmk: WrappedDmkPayload {
            nonce_b64: URL_SAFE_NO_PAD.encode(&meta.wrapped_dmk.nonce),
            ciphertext_b64: URL_SAFE_NO_PAD.encode(&meta.wrapped_dmk.ciphertext),
        },
        created_at: meta.created_at,
    });
    let _ = PAYLOAD_VERSION_V2; // 保留常量以便未来迁移跨版本用
    Ok(serde_json::to_vec(&p)?)
}

/// 把持久化字节解码为 `SpaceMetadataV2`。
pub fn decode(bytes: &[u8]) -> Result<SpaceMetadataV2, PayloadError> {
    let payload: Payload = serde_json::from_slice(bytes)?;
    let Payload::V2(p) = payload;

    if p.kdf.algorithm != "argon2id" {
        return Err(PayloadError::Shape(format!(
            "unexpected KDF algorithm: {}",
            p.kdf.algorithm
        )));
    }

    let seed_bytes = URL_SAFE_NO_PAD.decode(p.space_seed_b64.as_bytes())?;
    if seed_bytes.len() != super::types::SPACE_SEED_LEN {
        return Err(PayloadError::Shape(format!(
            "space_seed length {} != {}",
            seed_bytes.len(),
            super::types::SPACE_SEED_LEN
        )));
    }
    let mut seed_arr = [0u8; super::types::SPACE_SEED_LEN];
    seed_arr.copy_from_slice(&seed_bytes);

    let nonce = URL_SAFE_NO_PAD.decode(p.wrapped_dmk.nonce_b64.as_bytes())?;
    if nonce.len() != AEAD_NONCE_LEN {
        return Err(PayloadError::Shape(format!(
            "nonce length {} != {}",
            nonce.len(),
            AEAD_NONCE_LEN
        )));
    }
    let ciphertext = URL_SAFE_NO_PAD.decode(p.wrapped_dmk.ciphertext_b64.as_bytes())?;
    if ciphertext.len() != WRAPPED_DMK_LEN {
        return Err(PayloadError::Shape(format!(
            "wrapped_dmk length {} != {}",
            ciphertext.len(),
            WRAPPED_DMK_LEN
        )));
    }

    Ok(SpaceMetadataV2 {
        space_id: SpaceId::from(p.space_id),
        space_seed: SpaceSeed::from_bytes(seed_arr),
        kdf_params: KdfParams {
            mem_kib: p.kdf.mem_kib,
            iters: p.kdf.iters,
            parallelism: p.kdf.parallelism,
        },
        wrapped_dmk: WrappedDmk { nonce, ciphertext },
        created_at: p.created_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone as _;

    fn sample_metadata() -> SpaceMetadataV2 {
        SpaceMetadataV2 {
            space_id: SpaceId::from("space-xyz".to_string()),
            space_seed: SpaceSeed::from_bytes([7u8; 32]),
            kdf_params: KdfParams::default(),
            wrapped_dmk: WrappedDmk {
                nonce: vec![1u8; AEAD_NONCE_LEN],
                ciphertext: vec![2u8; WRAPPED_DMK_LEN],
            },
            created_at: chrono::Utc.timestamp_millis_opt(1_700_000_000_000).unwrap(),
        }
    }

    #[test]
    fn round_trip_preserves_all_fields() {
        let original = sample_metadata();
        let encoded = encode(&original).unwrap();
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded.space_id, original.space_id);
        assert_eq!(
            decoded.space_seed.as_bytes(),
            original.space_seed.as_bytes()
        );
        assert_eq!(decoded.kdf_params, original.kdf_params);
        assert_eq!(decoded.wrapped_dmk, original.wrapped_dmk);
        assert_eq!(decoded.created_at, original.created_at);
    }

    #[test]
    fn decode_rejects_wrong_algorithm() {
        let tampered = serde_json::json!({
            "version": "2",
            "space_id": "abc",
            "space_seed_b64": URL_SAFE_NO_PAD.encode([0u8; 32]),
            "kdf": { "algorithm": "scrypt", "mem_kib": 1, "iters": 1, "parallelism": 1 },
            "wrapped_dmk": {
                "nonce_b64": URL_SAFE_NO_PAD.encode([0u8; AEAD_NONCE_LEN]),
                "ciphertext_b64": URL_SAFE_NO_PAD.encode([0u8; WRAPPED_DMK_LEN]),
            },
            "created_at": "2026-01-01T00:00:00Z",
        });
        let err = decode(&serde_json::to_vec(&tampered).unwrap()).unwrap_err();
        assert!(matches!(err, PayloadError::Shape(_)));
    }

    #[test]
    fn decode_rejects_missing_version() {
        let tampered = serde_json::json!({
            "space_id": "abc",
            "space_seed_b64": "aa",
            "kdf": { "algorithm": "argon2id", "mem_kib": 1, "iters": 1, "parallelism": 1 },
            "wrapped_dmk": { "nonce_b64": "", "ciphertext_b64": "" },
            "created_at": "2026-01-01T00:00:00Z",
        });
        let err = decode(&serde_json::to_vec(&tampered).unwrap()).unwrap_err();
        assert!(matches!(err, PayloadError::Serde(_)));
    }

    #[test]
    fn decode_rejects_wrong_seed_length() {
        let tampered = serde_json::json!({
            "version": "2",
            "space_id": "abc",
            "space_seed_b64": URL_SAFE_NO_PAD.encode([0u8; 16]), // too short
            "kdf": { "algorithm": "argon2id", "mem_kib": 1, "iters": 1, "parallelism": 1 },
            "wrapped_dmk": {
                "nonce_b64": URL_SAFE_NO_PAD.encode([0u8; AEAD_NONCE_LEN]),
                "ciphertext_b64": URL_SAFE_NO_PAD.encode([0u8; WRAPPED_DMK_LEN]),
            },
            "created_at": "2026-01-01T00:00:00Z",
        });
        let err = decode(&serde_json::to_vec(&tampered).unwrap()).unwrap_err();
        assert!(matches!(err, PayloadError::Shape(_)));
    }
}
