use async_trait::async_trait;
use tokio::sync::mpsc;

use uc_core::crypto::model::KeySlotFile;
use uc_core::TrustAbortReason;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PairingDomainEvent {
    KeyslotReceived {
        session_id: String,
        peer_id: String,
        keyslot_file: KeySlotFile,
        challenge: Vec<u8>,
    },
    PairingVerificationRequired {
        session_id: String,
        peer_id: String,
        short_code: String,
        local_fingerprint: String,
        peer_fingerprint: String,
    },
    PairingVerifying {
        session_id: String,
        peer_id: String,
    },
    PairingSucceeded {
        session_id: String,
        peer_id: String,
    },
    /// Pairing flow terminated without reaching `Paired`.
    ///
    /// `reason` is intentionally collapsed to the three `TrustAbortReason`
    /// categories (2026-04-17 decision D24): detailed transport / crypto
    /// / persistence messages are logged at the source and not propagated
    /// through this domain event.
    PairingFailed {
        session_id: String,
        peer_id: String,
        reason: TrustAbortReason,
    },
}

#[async_trait]
pub trait PairingEventPort: Send + Sync {
    async fn subscribe(&self) -> anyhow::Result<mpsc::Receiver<PairingDomainEvent>>;
}
