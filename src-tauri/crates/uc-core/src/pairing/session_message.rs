//! Slice 1 pairing session-level domain messages.
//!
//! Pure domain types carried by [`PairingSessionPort`] and surfaced by
//! [`PairingEventPort`]. Adapters own wire encoding — these types have no
//! `serde` derives, no protocol ids, no libp2p / iroh leakage.
//!
//! Shape tracks the Slice 1 handshake:
//!
//! ```text
//!   Joiner → Sponsor : Request
//!   Sponsor → Joiner : KeyslotOffer
//!   Joiner → Sponsor : ChallengeResponse
//!   Sponsor → Joiner : Confirm      (or Reject at any step, either side)
//! ```
//!
//! Legacy libp2p-era equivalents live in [`crate::network::protocol::pairing`]
//! and carry a different — PIN-based, `peer_id`-leaky — shape. Slice 5 will
//! delete that module together with the libp2p adapter.
//!
//! [`PairingSessionPort`]: crate::ports::pairing::PairingSessionPort
//! [`PairingEventPort`]: crate::ports::pairing::PairingEventPort

use super::invitation::InvitationCode;
use crate::ids::{DeviceId, SpaceId};
use crate::ports::pairing::PairingSessionId;
use crate::security::IdentityFingerprint;

/// All pairing session-level messages for the Slice 1 iroh-native flow.
#[derive(Debug, Clone)]
pub enum PairingSessionMessage {
    Request(JoinerRequest),
    KeyslotOffer(SponsorKeyslotOffer),
    ChallengeResponse(JoinerChallengeResponse),
    Confirm(SponsorConfirm),
    Reject(PairingReject),
}

/// Joiner → sponsor. First message on the bi-stream (B2 step 5).
#[derive(Debug, Clone)]
pub struct JoinerRequest {
    /// Code the joiner redeemed. Sponsor orchestrator matches it against
    /// the in-memory pending invitation (Q-B1-3 / F-041).
    pub invitation_code: InvitationCode,
    /// Joiner's stable business device id (F-036 concept 1).
    pub device_id: DeviceId,
    /// Joiner's device name for sponsor-side UI / persistence.
    pub device_name: String,
    /// Joiner's identity fingerprint (F-036 concept 2). Derived at the
    /// adapter from the Ed25519 pubkey used by the session's transport.
    pub identity_fingerprint: IdentityFingerprint,
    /// Handshake transcript nonce.
    pub nonce: Vec<u8>,
    /// 不透明传输地址 blob（Slice 2 Phase 1 · T5）。
    ///
    /// 由 joiner 端 adapter 用自身的 transport 编码（iroh adapter 用
    /// postcard 编码 `EndpointAddr`）。core 不解析内容，只把字节作为
    /// 透传字段交给 sponsor 端写入 [`PeerAddressRepositoryPort`]。
    /// 空 `Vec` 表示 joiner 端 adapter 无法提供地址（旧客户端或尚未
    /// publish direct addrs），sponsor 端降级为跳过 upsert。
    ///
    /// [`PeerAddressRepositoryPort`]: crate::ports::PeerAddressRepositoryPort
    pub transport_address_blob: Vec<u8>,
}

/// Sponsor → joiner. Hands the joiner an offer they can unseal with the
/// shared passphrase (B2 step 6).
#[derive(Debug, Clone)]
pub struct SponsorKeyslotOffer {
    /// The space this offer belongs to.
    pub space_id: SpaceId,
    /// Opaque keyslot payload. Infra serializes the historical
    /// `KeySlotFile` JSON here; core treats the blob as bytes.
    pub keyslot_blob: Vec<u8>,
    /// 32-byte challenge nonce the joiner combines with the derived
    /// master key and `pairing_session_id` to compute an HMAC proof
    /// ([`ProofPort::build_proof`](crate::ports::space::ProofPort)).
    /// Sponsor keeps a copy in per-session state and feeds the same
    /// value to `verify_proof` on receipt.
    pub challenge: Vec<u8>,
    /// Sponsor-minted session identifier replayed verbatim into the
    /// joiner's proof payload so the sponsor-side `verify_proof` can
    /// bind the HMAC to the live pairing session (replay defence).
    pub pairing_session_id: PairingSessionId,
}

/// Joiner → sponsor. Challenge decrypt proof (B2 step 8).
#[derive(Debug, Clone)]
pub struct JoinerChallengeResponse {
    pub encrypted_challenge: Vec<u8>,
}

/// Sponsor → joiner. Final success message + sponsor identity facts the
/// joiner persists as a `SpaceMember` + `TrustedPeer` (B2 step 9/10).
#[derive(Debug, Clone)]
pub struct SponsorConfirm {
    pub space_id: SpaceId,
    pub sender_device_id: DeviceId,
    pub sender_device_name: String,
    pub sender_identity_fingerprint: IdentityFingerprint,
    /// 不透明传输地址 blob（Slice 2 Phase 1 · T5）。
    ///
    /// sponsor 端 adapter 填入自身的 transport 编码（iroh adapter 为
    /// postcard 编码的 `EndpointAddr`）。joiner 端只把字节直传
    /// [`PeerAddressRepositoryPort`]。空 `Vec` 表示 sponsor adapter
    /// 尚未发布 direct addrs，joiner 端降级为跳过 upsert，留待下轮
    /// `ensure_reachable_all` 从 rendezvous 再拉取。
    ///
    /// [`PeerAddressRepositoryPort`]: crate::ports::PeerAddressRepositoryPort
    pub transport_address_blob: Vec<u8>,
}

/// Either side → other. Terminal message with a structured reason so the
/// orchestrator can pick the right UI error / `PairingError` variant.
#[derive(Debug, Clone)]
pub struct PairingReject {
    pub reason: PairingRejectReason,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PairingRejectReason {
    /// Sponsor: incoming code didn't match any pending invitation (stale
    /// rendezvous entry or attacker replay).
    InvitationMismatch,
    /// Sponsor: joiner's challenge response didn't decrypt — wrong
    /// passphrase.
    PassphraseMismatch,
    /// Sponsor: user declined (reserved; Slice 1 doesn't surface an
    /// approval prompt but the enum leaves room for it).
    UserRejected,
    /// Sponsor: handshake未在 TTL 内完成（`begin` 后既没看到 `confirm`
    /// 也没看到 `reject` / `close`）。与 `Internal(String)` 分开是
    /// 因为 timeout 是一个稳定、可观测的产品语义（UI 可以直接展示
    /// "配对超时"），不是字符串化的兜底错误。
    Timeout,
    /// Protocol-level violation; message is for logs only.
    Internal(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reject_reason_equality_is_structural() {
        assert_eq!(
            PairingRejectReason::InvitationMismatch,
            PairingRejectReason::InvitationMismatch
        );
        assert_ne!(
            PairingRejectReason::Internal("a".into()),
            PairingRejectReason::Internal("b".into())
        );
    }
}
