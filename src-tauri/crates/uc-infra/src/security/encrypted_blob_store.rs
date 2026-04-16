//! Encrypted blob store decorator.
//!
//! Wraps an inner BlobStorePort and transparently encrypts/decrypts
//! blob data using the session's MasterKey. Uses UCBL binary format
//! with zstd compression for efficient on-disk storage.
//!
//! # Binary Format (V2)
//!
//! ```text
//! [UCBL magic: 4B] [version: 1B] [nonce: 24B] [ciphertext: NB]
//! ```
//!
//! Total header: 29 bytes before ciphertext.

use anyhow::{Context, Result};
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, info_span, Instrument};

use uc_core::{
    ports::{BlobStorePort, EncryptionPort, EncryptionSessionPort},
    security::aad,
    security::model::{EncryptedBlob, EncryptionAlgo, EncryptionFormatVersion},
    BlobId,
};

/// Magic bytes identifying a UniClipboard blob file ("UCBL")
const BLOB_MAGIC: [u8; 4] = [0x55, 0x43, 0x42, 0x4C];
/// Binary format version (v1 of the binary format, not to be confused with AAD v2)
const BLOB_FORMAT_VERSION: u8 = 0x01;
/// Header size: magic(4) + version(1) + nonce(24) = 29 bytes
const BLOB_HEADER_SIZE: usize = 4 + 1 + 24;
/// zstd compression level (3 = default, good speed/ratio balance)
const ZSTD_LEVEL: i32 = 3;
/// Maximum decompressed size to prevent zip bombs (500 MB)
const MAX_DECOMPRESSED_SIZE: usize = 500 * 1024 * 1024;

/// Serializes a nonce and ciphertext into the UCBL binary format.
fn serialize_blob(nonce: &[u8; 24], ciphertext: &[u8]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(BLOB_HEADER_SIZE + ciphertext.len());
    buf.extend_from_slice(&BLOB_MAGIC);
    buf.push(BLOB_FORMAT_VERSION);
    buf.extend_from_slice(nonce);
    buf.extend_from_slice(ciphertext);
    buf
}

/// Parses the UCBL binary format, extracting nonce and ciphertext.
fn parse_blob(data: &[u8]) -> Result<(&[u8; 24], &[u8])> {
    if data.len() < BLOB_HEADER_SIZE {
        return Err(anyhow::anyhow!(
            "blob file truncated: {} bytes < {} header",
            data.len(),
            BLOB_HEADER_SIZE
        ));
    }
    if data[0..4] != BLOB_MAGIC {
        return Err(anyhow::anyhow!("invalid blob magic bytes"));
    }
    if data[4] != BLOB_FORMAT_VERSION {
        return Err(anyhow::anyhow!(
            "unsupported blob format version: {}",
            data[4]
        ));
    }
    let nonce: &[u8; 24] = data[5..29]
        .try_into()
        .map_err(|_| anyhow::anyhow!("nonce extraction failed"))?;
    Ok((nonce, &data[29..]))
}

/// Decorator that encrypts/decrypts blob data transparently.
///
/// Uses UCBL binary format with zstd compression:
/// - Write: compress -> encrypt -> serialize to binary
/// - Read: parse binary -> decrypt -> decompress
pub struct EncryptedBlobStore {
    inner: Arc<dyn BlobStorePort>,
    encryption: Arc<dyn EncryptionPort>,
    session: Arc<dyn EncryptionSessionPort>,
}

impl EncryptedBlobStore {
    pub fn new(
        inner: Arc<dyn BlobStorePort>,
        encryption: Arc<dyn EncryptionPort>,
        session: Arc<dyn EncryptionSessionPort>,
    ) -> Self {
        Self {
            inner,
            encryption,
            session,
        }
    }
}

#[async_trait]
impl BlobStorePort for EncryptedBlobStore {
    async fn put(&self, blob_id: &BlobId, data: &[u8]) -> Result<(PathBuf, Option<i64>)> {
        let plaintext_size = data.len();

        // 1. Get master key from session
        let master_key = self
            .session
            .get_master_key()
            .await
            .context("encryption session not ready - cannot encrypt blob")?;

        // 2. Compress plaintext with zstd
        let compressed =
            zstd::bulk::compress(data, ZSTD_LEVEL).context("failed to compress blob data")?;
        let compressed_size = compressed.len();

        // 3. Build AAD with v2 format
        let aad = aad::for_blob_v2(blob_id);

        // 4. Encrypt compressed data
        let encrypted_blob = self
            .encryption
            .encrypt_blob(
                &master_key,
                &compressed,
                &aad,
                EncryptionAlgo::XChaCha20Poly1305,
            )
            .await
            .context("failed to encrypt blob data")?;

        // 5. Extract nonce as [u8; 24]
        let nonce: [u8; 24] = encrypted_blob
            .nonce
            .as_slice()
            .try_into()
            .context("encrypted blob nonce is not 24 bytes")?;

        // 6. Serialize to UCBL binary format
        let binary_data = serialize_blob(&nonce, &encrypted_blob.ciphertext);
        let on_disk_size = binary_data.len() as i64;

        // 7. Write to inner store
        let (path, _) = self
            .inner
            .put(blob_id, &binary_data)
            .instrument(info_span!("inner_blob_put", blob_id = %blob_id.as_ref()))
            .await?;

        debug!(
            blob_id = %blob_id.as_ref(),
            plaintext_size,
            compressed_size,
            on_disk_size,
            "Wrote V2 blob (compress -> encrypt -> UCBL binary)"
        );

        // 8. Return path and on-disk size
        Ok((path, Some(on_disk_size)))
    }

    async fn get(&self, blob_id: &BlobId) -> Result<Vec<u8>> {
        // 1. Read binary data from inner store
        let binary_data = self
            .inner
            .get(blob_id)
            .instrument(info_span!("inner_blob_get", blob_id = %blob_id.as_ref()))
            .await
            .context("failed to read encrypted blob from storage")?;

        // 2. Parse UCBL binary header
        let (nonce, ciphertext) = parse_blob(&binary_data)?;

        // 3. Reconstruct EncryptedBlob struct
        let encrypted_blob = EncryptedBlob {
            version: EncryptionFormatVersion::V1,
            aead: EncryptionAlgo::XChaCha20Poly1305,
            nonce: nonce.to_vec(),
            ciphertext: ciphertext.to_vec(),
            aad_fingerprint: None,
        };

        // 4. Get master key from session
        let master_key = self
            .session
            .get_master_key()
            .await
            .context("encryption session not ready - cannot decrypt blob")?;

        // 5. Build AAD with v2 format
        let aad = aad::for_blob_v2(blob_id);

        // 6. Decrypt
        let compressed = self
            .encryption
            .decrypt_blob(&master_key, &encrypted_blob, &aad)
            .await
            .context("failed to decrypt blob - key mismatch or data corrupted")?;

        // 7. Decompress
        let plaintext = zstd::bulk::decompress(&compressed, MAX_DECOMPRESSED_SIZE)
            .context("failed to decompress blob data - data may be corrupted")?;

        debug!(
            blob_id = %blob_id.as_ref(),
            on_disk_size = binary_data.len(),
            compressed_size = compressed.len(),
            plaintext_size = plaintext.len(),
            "Read V2 blob (UCBL binary -> decrypt -> decompress)"
        );

        Ok(plaintext)
    }
}
