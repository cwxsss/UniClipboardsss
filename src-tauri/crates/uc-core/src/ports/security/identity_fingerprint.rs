//! Identity fingerprint factory port.
//!
//! Derives a short, stable, human-readable fingerprint string from a raw
//! identity public key. Used during pairing for out-of-band verification.
//!
//! The trait returns a plain `String` because all current callers use
//! the rendered display form. Concrete implementation (SHA-256 + Base32
//! grouping) lives in `uc-infra`.

use anyhow::Result;

pub trait IdentityFingerprintFactoryPort: Send + Sync {
    fn from_public_key(&self, public_key: &[u8]) -> Result<String>;
}
