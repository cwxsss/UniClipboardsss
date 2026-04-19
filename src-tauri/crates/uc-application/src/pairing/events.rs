use async_trait::async_trait;
use tokio::sync::mpsc;

use uc_core::TrustAbortReason;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PairingDomainEvent {
    /// Slice 6 (U6) 起 `keyslot_payload` 以不透明 `serde_json::Value` 承载——
    /// 应用层直接透传到 `setup::action_executor` 边界再 `to_vec` 成字节喂给
    /// `SpaceAccessJoinerOffer.keyslot_blob`,避免 `KeySlotFile` → `KeySlot` →
    /// `serde_json::to_vec` 的 round-trip。`space_id` 由 wire 层独立携带(sender
    /// 提供,旧对端缺失时 orchestrator 从 payload 内 `scope.profile_id` fall back)。
    KeyslotReceived {
        session_id: String,
        peer_id: String,
        keyslot_payload: serde_json::Value,
        space_id: String,
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
