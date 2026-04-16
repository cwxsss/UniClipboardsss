use async_trait::async_trait;
use rand::RngCore;

use argon2::Argon2;
use chacha20poly1305::aead::Aead;
use chacha20poly1305::{KeyInit, XChaCha20Poly1305, XNonce};
use uc_core::ports::EncryptionPort;
use uc_core::security::model::{
    EncryptedBlob, EncryptionAlgo, EncryptionError, EncryptionFormatVersion, KdfAlgorithm,
    KdfParams, Kek, MasterKey, Passphrase,
};

pub struct EncryptionRepository;

const CURR_VERSION: EncryptionFormatVersion = EncryptionFormatVersion::V1;

fn aad_fingerprint(aad: &[u8]) -> Vec<u8> {
    blake3::hash(aad).as_bytes()[..16].to_vec()
}

#[async_trait]
impl EncryptionPort for EncryptionRepository {
    async fn derive_kek(
        &self,
        passphrase: &Passphrase,
        salt: &[u8],
        kdf: &KdfParams,
    ) -> Result<Kek, EncryptionError> {
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
                    .map_err(|_| EncryptionError::InvalidParameter(format!("{:?}", kdf.params)))?,
                );

                let mut okm = [0u8; 32];
                argon2
                    .hash_password_into(passphrase.as_bytes(), salt, &mut okm)
                    .map_err(|_| EncryptionError::KdfFailed)?;

                Kek::from_bytes(&okm)
            }
        }
    }
    async fn wrap_master_key(
        &self,
        kek: &Kek,
        master_key: &MasterKey,
        aead: EncryptionAlgo,
    ) -> Result<EncryptedBlob, EncryptionError> {
        let mut nonce = vec![0u8; 24];
        rand::rng().fill_bytes(&mut nonce);

        let ciphertext = match aead {
            EncryptionAlgo::XChaCha20Poly1305 => {
                let cipher = XChaCha20Poly1305::new_from_slice(kek.as_bytes())
                    .map_err(|_| EncryptionError::InvalidKey)?;
                cipher
                    .encrypt(XNonce::from_slice(&nonce), master_key.as_bytes())
                    .map_err(|_| EncryptionError::EncryptFailed)?
            }
        };

        Ok(EncryptedBlob {
            version: CURR_VERSION,
            nonce,
            ciphertext,
            aead,
            aad_fingerprint: None,
        })
    }

    async fn unwrap_master_key(
        &self,
        kek: &Kek,
        wrapped: &EncryptedBlob,
    ) -> Result<MasterKey, EncryptionError> {
        let plaintext = match wrapped.aead {
            EncryptionAlgo::XChaCha20Poly1305 => {
                let cipher = XChaCha20Poly1305::new_from_slice(kek.as_bytes())
                    .map_err(|_| EncryptionError::InvalidKey)?;
                cipher
                    .decrypt(
                        XNonce::from_slice(&wrapped.nonce),
                        wrapped.ciphertext.as_ref(),
                    )
                    .map_err(|_| EncryptionError::WrongPassphrase)?
            }
        };

        MasterKey::from_bytes(&plaintext)
    }

    async fn encrypt_blob(
        &self,
        master_key: &MasterKey,
        plaintext: &[u8],
        aad: &[u8],
        aead: EncryptionAlgo,
    ) -> Result<EncryptedBlob, EncryptionError> {
        let mut nonce = vec![0u8; 24];
        rand::rng().fill_bytes(&mut nonce);

        let ciphertext = match aead {
            EncryptionAlgo::XChaCha20Poly1305 => {
                let cipher = XChaCha20Poly1305::new_from_slice(master_key.as_bytes())
                    .map_err(|_| EncryptionError::InvalidKey)?;
                cipher
                    .encrypt(
                        XNonce::from_slice(&nonce),
                        chacha20poly1305::aead::Payload {
                            msg: plaintext,
                            aad,
                        },
                    )
                    .map_err(|_| EncryptionError::EncryptFailed)?
            }
        };

        let aad_fp = Some(aad_fingerprint(aad));

        Ok(EncryptedBlob {
            version: EncryptionFormatVersion::V1,
            nonce,
            ciphertext,
            aead,
            aad_fingerprint: aad_fp,
        })
    }

    async fn decrypt_blob(
        &self,
        master_key: &MasterKey,
        encrypted: &EncryptedBlob,
        aad: &[u8],
    ) -> Result<Vec<u8>, EncryptionError> {
        let plaintext = match encrypted.aead {
            EncryptionAlgo::XChaCha20Poly1305 => {
                let cipher = XChaCha20Poly1305::new_from_slice(master_key.as_bytes())
                    .map_err(|_| EncryptionError::InvalidKey)?;
                cipher
                    .decrypt(
                        XNonce::from_slice(&encrypted.nonce),
                        chacha20poly1305::aead::Payload {
                            msg: encrypted.ciphertext.as_ref(),
                            aad,
                        },
                    )
                    .map_err(|_| EncryptionError::CorruptedBlob)?
            }
        };

        Ok(plaintext)
    }
}
