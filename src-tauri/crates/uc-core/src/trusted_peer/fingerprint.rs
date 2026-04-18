use serde::{Deserialize, Serialize};

/// Canonical fingerprint of a remote peer's public identity.
///
/// The derivation algorithm (hash function, encoding, truncation) lives in
/// the `network::pairing` protocol layer, not here: this domain only cares
/// that the value is stable across transports, sessions and restarts so it
/// can be used to re-verify "still the same peer" on reconnect.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PeerFingerprint(String);

impl PeerFingerprint {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for PeerFingerprint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
