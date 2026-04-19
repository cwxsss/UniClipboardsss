use async_trait::async_trait;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use uc_core::ids::{SessionId, SpaceId};
use uc_core::ports::space::ProofPort;
use uc_core::ports::EncryptionSessionPort;
use uc_core::space_access::{ProofDerivedKey, SpaceAccessProofArtifact};

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ProofCacheKey {
    pairing_session_id: String,
    space_id: String,
    challenge_nonce: [u8; 32],
}

pub struct HmacProofAdapter {
    key_cache: Mutex<HashMap<ProofCacheKey, [u8; 32]>>,
    encryption_session: Option<Arc<dyn EncryptionSessionPort>>,
}

impl HmacProofAdapter {
    pub fn new() -> Self {
        Self {
            key_cache: Mutex::new(HashMap::new()),
            encryption_session: None,
        }
    }

    pub fn new_with_encryption_session(encryption_session: Arc<dyn EncryptionSessionPort>) -> Self {
        Self {
            key_cache: Mutex::new(HashMap::new()),
            encryption_session: Some(encryption_session),
        }
    }

    fn payload(
        pairing_session_id: &SessionId,
        space_id: &SpaceId,
        challenge_nonce: [u8; 32],
    ) -> Vec<u8> {
        let session = pairing_session_id.as_str().as_bytes();
        let space = space_id.as_ref().as_bytes();

        let mut payload =
            Vec::with_capacity(8 + session.len() + space.len() + challenge_nonce.len());
        payload.extend_from_slice(&(session.len() as u32).to_be_bytes());
        payload.extend_from_slice(session);
        payload.extend_from_slice(&(space.len() as u32).to_be_bytes());
        payload.extend_from_slice(space);
        payload.extend_from_slice(&challenge_nonce);
        payload
    }

    fn cache_key(
        pairing_session_id: &SessionId,
        space_id: &SpaceId,
        challenge_nonce: [u8; 32],
    ) -> ProofCacheKey {
        ProofCacheKey {
            pairing_session_id: pairing_session_id.as_str().to_string(),
            space_id: space_id.as_ref().to_string(),
            challenge_nonce,
        }
    }

    fn compute_hmac(
        pairing_session_id: &SessionId,
        space_id: &SpaceId,
        challenge_nonce: [u8; 32],
        master_key_bytes: &[u8],
    ) -> anyhow::Result<Vec<u8>> {
        let payload = Self::payload(pairing_session_id, space_id, challenge_nonce);
        let mut mac = HmacSha256::new_from_slice(master_key_bytes)?;
        mac.update(&payload);
        Ok(mac.finalize().into_bytes().to_vec())
    }
}

#[async_trait]
impl ProofPort for HmacProofAdapter {
    async fn build_proof(
        &self,
        pairing_session_id: &SessionId,
        space_id: &SpaceId,
        challenge_nonce: [u8; 32],
        derived_key: &ProofDerivedKey,
    ) -> anyhow::Result<SpaceAccessProofArtifact> {
        let key_bytes = derived_key.as_bytes();
        let key_fingerprint = format!(
            "{:02x}{:02x}{:02x}{:02x}",
            key_bytes[0], key_bytes[1], key_bytes[2], key_bytes[3]
        );
        tracing::debug!(
            session_id = %pairing_session_id,
            space_id = %space_id,
            key_fingerprint,
            "building HMAC proof"
        );

        let proof_bytes =
            Self::compute_hmac(pairing_session_id, space_id, challenge_nonce, key_bytes)?;

        let cache_key = Self::cache_key(pairing_session_id, space_id, challenge_nonce);
        let mut cached = [0u8; 32];
        cached.copy_from_slice(key_bytes);
        self.key_cache.lock().await.insert(cache_key, cached);

        Ok(SpaceAccessProofArtifact {
            pairing_session_id: pairing_session_id.clone(),
            space_id: space_id.clone(),
            challenge_nonce,
            proof_bytes,
        })
    }

    async fn verify_proof(
        &self,
        proof: &SpaceAccessProofArtifact,
        expected_nonce: [u8; 32],
    ) -> anyhow::Result<bool> {
        if proof.challenge_nonce != expected_nonce {
            tracing::warn!(
                session_id = %proof.pairing_session_id,
                space_id = %proof.space_id,
                "proof verification failed: challenge nonce mismatch"
            );
            return Ok(false);
        }

        let cache_key = Self::cache_key(
            &proof.pairing_session_id,
            &proof.space_id,
            proof.challenge_nonce,
        );
        let master_key = {
            let cache = self.key_cache.lock().await;
            cache.get(&cache_key).copied()
        };

        let (master_key, key_source) = if let Some(master_key) = master_key {
            (Some(master_key), "cache")
        } else if let Some(encryption_session) = &self.encryption_session {
            match encryption_session.get_master_key().await {
                Ok(master_key) => {
                    let mut master_key_bytes = [0u8; 32];
                    master_key_bytes.copy_from_slice(master_key.as_bytes());
                    self.key_cache
                        .lock()
                        .await
                        .insert(cache_key, master_key_bytes);
                    (Some(master_key_bytes), "encryption_session")
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        session_id = %proof.pairing_session_id,
                        "proof verification failed: encryption session has no master key"
                    );
                    (None, "none")
                }
            }
        } else {
            tracing::warn!(
                session_id = %proof.pairing_session_id,
                "proof verification failed: no encryption session configured"
            );
            (None, "none")
        };

        let Some(master_key) = master_key else {
            return Ok(false);
        };

        let recomputed = Self::compute_hmac(
            &proof.pairing_session_id,
            &proof.space_id,
            proof.challenge_nonce,
            &master_key,
        )?;

        let mk_fingerprint = format!(
            "{:02x}{:02x}{:02x}{:02x}",
            master_key[0], master_key[1], master_key[2], master_key[3]
        );
        let matched = recomputed == proof.proof_bytes;
        if !matched {
            tracing::warn!(
                session_id = %proof.pairing_session_id,
                space_id = %proof.space_id,
                key_source,
                mk_fingerprint,
                proof_len = proof.proof_bytes.len(),
                recomputed_len = recomputed.len(),
                "proof verification failed: HMAC mismatch (master key from {key_source})"
            );
        } else {
            tracing::info!(
                session_id = %proof.pairing_session_id,
                key_source,
                "proof verification succeeded"
            );
        }

        Ok(matched)
    }
}
