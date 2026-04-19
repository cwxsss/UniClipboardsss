//! V1 加密原语集中点（pub(crate)）。
//!
//! 把 KEK 派生 / MasterKey 包装拆解 / blob AEAD 三组算法封装成纯函数,
//! 供 `BlobCipherAdapter` / `EncryptedBlobStore` / 后续 SpaceAccessAdapter
//! 内联使用。算法行为与历史 `EncryptionRepository` 一致——Argon2id KDF +
//! XChaCha20-Poly1305 AEAD,nonce 为 24 字节随机。
//!
//! V1 加密不变量 ironclad 保留——这里只是把同一份算法搬到一处,杜绝
//! 多个 adapter 各自实现的行为漂移风险。
//!
//! 注：本 commit (Slice 3 - C2) 只让 `BlobCipherAdapter` 消费
//! `encrypt_blob_xchacha` / `decrypt_blob_xchacha`；其余 3 个函数
//! （derive_kek_argon2id / wrap_master_key_xchacha / unwrap_master_key_xchacha）
//! 在 C8 删除 `EncryptionPort` 后由 SpaceAccessAdapter 内联消费,
//! 期间允许 dead_code。

#![allow(dead_code)]

use argon2::Argon2;
use chacha20poly1305::aead::Aead;
use chacha20poly1305::{KeyInit, XChaCha20Poly1305, XNonce};
use rand::RngCore;
use uc_core::crypto::model::{
    EncryptedBlob, EncryptionAlgo, EncryptionFormatVersion, KdfAlgorithm, KdfParams, Kek,
    MasterKey, Passphrase,
};

#[derive(Debug, thiserror::Error)]
pub(crate) enum AeadError {
    #[error("invalid key length")]
    InvalidKey,
    #[error("AEAD encryption failed")]
    EncryptFailed,
    #[error("AEAD decryption failed (key mismatch / corrupted ciphertext)")]
    DecryptFailed,
}

/// Argon2id 派生 KEK。
pub(crate) fn derive_kek_argon2id(
    passphrase: &Passphrase,
    salt: &[u8],
    kdf: &KdfParams,
) -> Result<Kek, String> {
    match kdf.alg {
        KdfAlgorithm::Argon2id => {
            let argon2 = Argon2::new(
                argon2::Algorithm::Argon2id,
                argon2::Version::V0x13,
                argon2::Params::new(
                    kdf.params.mem_kib,
                    kdf.params.iters,
                    kdf.params.parallelism,
                    Some(32),
                )
                .map_err(|e| format!("argon2 params: {e}"))?,
            );
            let mut okm = [0u8; 32];
            argon2
                .hash_password_into(passphrase.as_bytes(), salt, &mut okm)
                .map_err(|e| format!("argon2 hash: {e}"))?;
            Kek::from_bytes(&okm).map_err(|e| format!("Kek::from_bytes: {e}"))
        }
    }
}

/// XChaCha20-Poly1305 包装 MasterKey。
///
/// 输出 `EncryptedBlob` 直接对接 KeySlot.wrapped_master_key（V1 格式）。
pub(crate) fn wrap_master_key_xchacha(
    kek: &Kek,
    master_key: &MasterKey,
) -> Result<EncryptedBlob, AeadError> {
    let mut nonce = vec![0u8; 24];
    rand::rng().fill_bytes(&mut nonce);

    let cipher =
        XChaCha20Poly1305::new_from_slice(kek.as_bytes()).map_err(|_| AeadError::InvalidKey)?;
    let ciphertext = cipher
        .encrypt(XNonce::from_slice(&nonce), master_key.as_bytes())
        .map_err(|_| AeadError::EncryptFailed)?;

    Ok(EncryptedBlob {
        version: EncryptionFormatVersion::V1,
        aead: EncryptionAlgo::XChaCha20Poly1305,
        nonce,
        ciphertext,
        aad_fingerprint: None,
    })
}

/// XChaCha20-Poly1305 解包 MasterKey。
pub(crate) fn unwrap_master_key_xchacha(
    kek: &Kek,
    wrapped: &EncryptedBlob,
) -> Result<MasterKey, AeadError> {
    let cipher =
        XChaCha20Poly1305::new_from_slice(kek.as_bytes()).map_err(|_| AeadError::InvalidKey)?;
    let plaintext = cipher
        .decrypt(
            XNonce::from_slice(&wrapped.nonce),
            wrapped.ciphertext.as_ref(),
        )
        .map_err(|_| AeadError::DecryptFailed)?;
    MasterKey::from_bytes(&plaintext).map_err(|_| AeadError::DecryptFailed)
}

/// XChaCha20-Poly1305 加密业务 blob,返回完整的 `EncryptedBlob`。
///
/// 调用方决定 AAD（业务上下文绑定,例如条目 id / blob id）。
pub(crate) fn encrypt_blob_xchacha(
    master_key: &MasterKey,
    plaintext: &[u8],
    aad: &[u8],
) -> Result<EncryptedBlob, AeadError> {
    let mut nonce = vec![0u8; 24];
    rand::rng().fill_bytes(&mut nonce);

    let cipher = XChaCha20Poly1305::new_from_slice(master_key.as_bytes())
        .map_err(|_| AeadError::InvalidKey)?;
    let ciphertext = cipher
        .encrypt(
            XNonce::from_slice(&nonce),
            chacha20poly1305::aead::Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| AeadError::EncryptFailed)?;

    let aad_fp = Some(blake3::hash(aad).as_bytes()[..16].to_vec());

    Ok(EncryptedBlob {
        version: EncryptionFormatVersion::V1,
        aead: EncryptionAlgo::XChaCha20Poly1305,
        nonce,
        ciphertext,
        aad_fingerprint: aad_fp,
    })
}

/// XChaCha20-Poly1305 解密业务 blob。
///
/// 接收 nonce + ciphertext + AAD 三件直接调底层 AEAD,绕过 EncryptedBlob
/// 结构（让 EncryptedBlobStore 用 UCBL 二进制头时也能直接消费）。
pub(crate) fn decrypt_blob_xchacha(
    master_key: &MasterKey,
    nonce: &[u8],
    ciphertext: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>, AeadError> {
    if nonce.len() != 24 {
        return Err(AeadError::DecryptFailed);
    }
    let cipher = XChaCha20Poly1305::new_from_slice(master_key.as_bytes())
        .map_err(|_| AeadError::InvalidKey)?;
    cipher
        .decrypt(
            XNonce::from_slice(nonce),
            chacha20poly1305::aead::Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map_err(|_| AeadError::DecryptFailed)
}
