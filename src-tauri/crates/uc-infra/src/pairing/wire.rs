//! Binary codec for [`PairingSessionMessage`].
//!
//! Slice 1 pairing session wire format (postcard + explicit version byte).
//! Runs over an iroh bi-directional stream; P7c.2 layers length-prefixed
//! framing on top of this codec before hitting the stream.
//!
//! Design notes:
//!
//! * **Wire types are infra-local.** The core [`PairingSessionMessage`]
//!   carries no `serde` derives (§6.3). This module owns mirror structs with
//!   serde derives and maps them at the boundary.
//! * **Envelope carries a version byte from day 1.** Slice 2+ will extend
//!   the enum (e.g. keep-alives, resume tokens); `v` lets us distinguish
//!   "old peer sent unknown variant" from "data corruption".
//! * **postcard, not JSON.** postcard gives ~40% smaller payloads than
//!   JSON for this shape (mainly because keyslot / challenge / nonce are
//!   binary bytes). Rendezvous tickets are already ~500 bytes — saving here
//!   is worth the binary opaqueness.
//! * **IdentityFingerprint on the wire uses the display form**
//!   (`ABCD-EFGH-IJKL-MNOP`) — stable, printable in logs, round-trips
//!   through [`IdentityFingerprint::from_display_string`].
//!
//! [`PairingSessionMessage`]: uc_core::pairing::PairingSessionMessage

use serde::{Deserialize, Serialize};
use thiserror::Error;

use uc_core::ids::{DeviceId, SpaceId};
use uc_core::pairing::{
    InvitationCode, JoinerChallengeResponse, JoinerRequest, PairingReject, PairingRejectReason,
    PairingSessionMessage, SponsorConfirm, SponsorKeyslotOffer,
};
use uc_core::ports::pairing::PairingSessionId;
use uc_core::security::IdentityFingerprint;

const WIRE_VERSION: u8 = 1;

// ============================================================================
// Wire types (infra-local)
// ============================================================================

#[derive(Serialize, Deserialize, Debug)]
struct WireEnvelope {
    v: u8,
    body: WireBody,
}

#[derive(Serialize, Deserialize, Debug)]
enum WireBody {
    Request(WireJoinerRequest),
    KeyslotOffer(WireSponsorKeyslotOffer),
    ChallengeResponse(WireJoinerChallengeResponse),
    Confirm(WireSponsorConfirm),
    Reject(WirePairingReject),
}

#[derive(Serialize, Deserialize, Debug)]
struct WireJoinerRequest {
    invitation_code: String,
    device_id: String,
    device_name: String,
    identity_fingerprint: String,
    nonce: Vec<u8>,
}

#[derive(Serialize, Deserialize, Debug)]
struct WireSponsorKeyslotOffer {
    space_id: String,
    keyslot_blob: Vec<u8>,
    challenge: Vec<u8>,
    pairing_session_id: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct WireJoinerChallengeResponse {
    encrypted_challenge: Vec<u8>,
}

#[derive(Serialize, Deserialize, Debug)]
struct WireSponsorConfirm {
    space_id: String,
    sender_device_id: String,
    sender_device_name: String,
    sender_identity_fingerprint: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct WirePairingReject {
    reason: WireRejectReason,
}

#[derive(Serialize, Deserialize, Debug)]
enum WireRejectReason {
    InvitationMismatch,
    PassphraseMismatch,
    UserRejected,
    Internal(String),
}

// ============================================================================
// Errors
// ============================================================================

#[derive(Debug, Error)]
pub enum WireEncodeError {
    #[error("postcard encode failed: {0}")]
    Postcard(#[from] postcard::Error),
}

#[derive(Debug, Error)]
pub enum WireDecodeError {
    #[error("postcard decode failed: {0}")]
    Postcard(postcard::Error),

    #[error("unsupported wire version {got} (this build understands {expected})")]
    UnsupportedVersion { got: u8, expected: u8 },

    #[error("invalid identity fingerprint on wire: {0}")]
    InvalidFingerprint(String),
}

impl From<postcard::Error> for WireDecodeError {
    fn from(err: postcard::Error) -> Self {
        WireDecodeError::Postcard(err)
    }
}

// ============================================================================
// Public API
// ============================================================================

/// Serialize a [`PairingSessionMessage`] for transport.
pub fn encode(message: &PairingSessionMessage) -> Result<Vec<u8>, WireEncodeError> {
    let envelope = WireEnvelope {
        v: WIRE_VERSION,
        body: to_wire(message),
    };
    Ok(postcard::to_allocvec(&envelope)?)
}

/// Deserialize a [`PairingSessionMessage`] from bytes produced by
/// [`encode`] (or a peer running a compatible version).
pub fn decode(bytes: &[u8]) -> Result<PairingSessionMessage, WireDecodeError> {
    let envelope: WireEnvelope = postcard::from_bytes(bytes)?;
    if envelope.v != WIRE_VERSION {
        return Err(WireDecodeError::UnsupportedVersion {
            got: envelope.v,
            expected: WIRE_VERSION,
        });
    }
    from_wire(envelope.body)
}

// ============================================================================
// Conversions
// ============================================================================

fn to_wire(msg: &PairingSessionMessage) -> WireBody {
    match msg {
        PairingSessionMessage::Request(r) => WireBody::Request(WireJoinerRequest {
            invitation_code: r.invitation_code.as_str().to_string(),
            device_id: r.device_id.as_str().to_string(),
            device_name: r.device_name.clone(),
            identity_fingerprint: r.identity_fingerprint.as_display().to_string(),
            nonce: r.nonce.clone(),
        }),
        PairingSessionMessage::KeyslotOffer(o) => WireBody::KeyslotOffer(WireSponsorKeyslotOffer {
            space_id: o.space_id.inner().clone(),
            keyslot_blob: o.keyslot_blob.clone(),
            challenge: o.challenge.clone(),
            pairing_session_id: o.pairing_session_id.as_str().to_string(),
        }),
        PairingSessionMessage::ChallengeResponse(c) => {
            WireBody::ChallengeResponse(WireJoinerChallengeResponse {
                encrypted_challenge: c.encrypted_challenge.clone(),
            })
        }
        PairingSessionMessage::Confirm(c) => WireBody::Confirm(WireSponsorConfirm {
            space_id: c.space_id.inner().clone(),
            sender_device_id: c.sender_device_id.as_str().to_string(),
            sender_device_name: c.sender_device_name.clone(),
            sender_identity_fingerprint: c.sender_identity_fingerprint.as_display().to_string(),
        }),
        PairingSessionMessage::Reject(r) => WireBody::Reject(WirePairingReject {
            reason: match &r.reason {
                PairingRejectReason::InvitationMismatch => WireRejectReason::InvitationMismatch,
                PairingRejectReason::PassphraseMismatch => WireRejectReason::PassphraseMismatch,
                PairingRejectReason::UserRejected => WireRejectReason::UserRejected,
                PairingRejectReason::Internal(s) => WireRejectReason::Internal(s.clone()),
            },
        }),
    }
}

fn from_wire(body: WireBody) -> Result<PairingSessionMessage, WireDecodeError> {
    match body {
        WireBody::Request(r) => Ok(PairingSessionMessage::Request(JoinerRequest {
            invitation_code: InvitationCode::new(r.invitation_code),
            device_id: DeviceId::new(r.device_id),
            device_name: r.device_name,
            identity_fingerprint: parse_fingerprint(&r.identity_fingerprint)?,
            nonce: r.nonce,
        })),
        WireBody::KeyslotOffer(o) => Ok(PairingSessionMessage::KeyslotOffer(SponsorKeyslotOffer {
            space_id: SpaceId::from_string(o.space_id),
            keyslot_blob: o.keyslot_blob,
            challenge: o.challenge,
            pairing_session_id: PairingSessionId::new(o.pairing_session_id),
        })),
        WireBody::ChallengeResponse(c) => Ok(PairingSessionMessage::ChallengeResponse(
            JoinerChallengeResponse {
                encrypted_challenge: c.encrypted_challenge,
            },
        )),
        WireBody::Confirm(c) => Ok(PairingSessionMessage::Confirm(SponsorConfirm {
            space_id: SpaceId::from_string(c.space_id),
            sender_device_id: DeviceId::new(c.sender_device_id),
            sender_device_name: c.sender_device_name,
            sender_identity_fingerprint: parse_fingerprint(&c.sender_identity_fingerprint)?,
        })),
        WireBody::Reject(r) => Ok(PairingSessionMessage::Reject(PairingReject {
            reason: match r.reason {
                WireRejectReason::InvitationMismatch => PairingRejectReason::InvitationMismatch,
                WireRejectReason::PassphraseMismatch => PairingRejectReason::PassphraseMismatch,
                WireRejectReason::UserRejected => PairingRejectReason::UserRejected,
                WireRejectReason::Internal(s) => PairingRejectReason::Internal(s),
            },
        })),
    }
}

fn parse_fingerprint(s: &str) -> Result<IdentityFingerprint, WireDecodeError> {
    IdentityFingerprint::from_display_string(s)
        .map_err(|e| WireDecodeError::InvalidFingerprint(e.to_string()))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_fingerprint() -> IdentityFingerprint {
        IdentityFingerprint::from_raw_string("ABCDEFGHIJKLMNOP").unwrap()
    }

    fn round_trip(msg: PairingSessionMessage) -> PairingSessionMessage {
        let bytes = encode(&msg).expect("encode");
        decode(&bytes).expect("decode")
    }

    #[test]
    fn request_round_trips() {
        let original = PairingSessionMessage::Request(JoinerRequest {
            invitation_code: InvitationCode::new("CODE-1234"),
            device_id: DeviceId::new("dev-001"),
            device_name: "Alice's laptop".to_string(),
            identity_fingerprint: sample_fingerprint(),
            nonce: vec![1, 2, 3, 4, 5],
        });

        let decoded = round_trip(original);
        match decoded {
            PairingSessionMessage::Request(r) => {
                assert_eq!(r.invitation_code.as_str(), "CODE-1234");
                assert_eq!(r.device_id.as_str(), "dev-001");
                assert_eq!(r.device_name, "Alice's laptop");
                assert_eq!(r.identity_fingerprint, sample_fingerprint());
                assert_eq!(r.nonce, vec![1, 2, 3, 4, 5]);
            }
            other => panic!("expected Request, got {other:?}"),
        }
    }

    #[test]
    fn keyslot_offer_round_trips() {
        let original = PairingSessionMessage::KeyslotOffer(SponsorKeyslotOffer {
            space_id: SpaceId::from_str("space-42"),
            keyslot_blob: vec![0xde, 0xad, 0xbe, 0xef],
            challenge: vec![0x01; 32],
            pairing_session_id: PairingSessionId::new("sess-abc-42"),
        });

        let decoded = round_trip(original);
        match decoded {
            PairingSessionMessage::KeyslotOffer(o) => {
                assert_eq!(o.space_id.inner(), "space-42");
                assert_eq!(o.keyslot_blob, vec![0xde, 0xad, 0xbe, 0xef]);
                assert_eq!(o.challenge, vec![0x01; 32]);
                assert_eq!(o.pairing_session_id.as_str(), "sess-abc-42");
            }
            other => panic!("expected KeyslotOffer, got {other:?}"),
        }
    }

    #[test]
    fn challenge_response_round_trips() {
        let original = PairingSessionMessage::ChallengeResponse(JoinerChallengeResponse {
            encrypted_challenge: vec![0x42; 48],
        });
        let decoded = round_trip(original);
        match decoded {
            PairingSessionMessage::ChallengeResponse(c) => {
                assert_eq!(c.encrypted_challenge, vec![0x42; 48]);
            }
            other => panic!("expected ChallengeResponse, got {other:?}"),
        }
    }

    #[test]
    fn confirm_round_trips() {
        let original = PairingSessionMessage::Confirm(SponsorConfirm {
            space_id: SpaceId::from_str("space-99"),
            sender_device_id: DeviceId::new("dev-sponsor"),
            sender_device_name: "Bob's desktop".to_string(),
            sender_identity_fingerprint: sample_fingerprint(),
        });
        let decoded = round_trip(original);
        match decoded {
            PairingSessionMessage::Confirm(c) => {
                assert_eq!(c.space_id.inner(), "space-99");
                assert_eq!(c.sender_device_id.as_str(), "dev-sponsor");
                assert_eq!(c.sender_device_name, "Bob's desktop");
                assert_eq!(c.sender_identity_fingerprint, sample_fingerprint());
            }
            other => panic!("expected Confirm, got {other:?}"),
        }
    }

    #[test]
    fn reject_round_trips_all_reasons() {
        for reason in [
            PairingRejectReason::InvitationMismatch,
            PairingRejectReason::PassphraseMismatch,
            PairingRejectReason::UserRejected,
            PairingRejectReason::Internal("bad things".to_string()),
        ] {
            let original = PairingSessionMessage::Reject(PairingReject {
                reason: reason.clone(),
            });
            let decoded = round_trip(original);
            match decoded {
                PairingSessionMessage::Reject(r) => assert_eq!(r.reason, reason),
                other => panic!("expected Reject, got {other:?}"),
            }
        }
    }

    #[test]
    fn decode_rejects_future_version() {
        // v=2 envelope with any body — build by hand via postcard on our
        // own struct (fake future version).
        #[derive(Serialize)]
        struct FutureEnvelope {
            v: u8,
            body: WireBody,
        }
        let fake = FutureEnvelope {
            v: 2,
            body: WireBody::ChallengeResponse(WireJoinerChallengeResponse {
                encrypted_challenge: vec![],
            }),
        };
        let bytes = postcard::to_allocvec(&fake).unwrap();

        match decode(&bytes) {
            Err(WireDecodeError::UnsupportedVersion { got, expected }) => {
                assert_eq!(got, 2);
                assert_eq!(expected, WIRE_VERSION);
            }
            other => panic!("expected UnsupportedVersion, got {other:?}"),
        }
    }

    #[test]
    fn decode_rejects_garbage_bytes() {
        let garbage = vec![0xff; 16];
        match decode(&garbage) {
            Err(WireDecodeError::Postcard(_)) => {}
            other => panic!("expected Postcard error, got {other:?}"),
        }
    }

    #[test]
    fn decode_rejects_invalid_fingerprint_format() {
        // Manually build a request with a too-short fingerprint on the wire.
        let fake = WireEnvelope {
            v: WIRE_VERSION,
            body: WireBody::Request(WireJoinerRequest {
                invitation_code: "x".to_string(),
                device_id: "d".to_string(),
                device_name: "n".to_string(),
                identity_fingerprint: "TOO_SHORT".to_string(),
                nonce: vec![],
            }),
        };
        let bytes = postcard::to_allocvec(&fake).unwrap();

        match decode(&bytes) {
            Err(WireDecodeError::InvalidFingerprint(msg)) => {
                assert!(
                    msg.contains("expected 16 characters"),
                    "unexpected error body: {msg}"
                );
            }
            other => panic!("expected InvalidFingerprint, got {other:?}"),
        }
    }

    #[test]
    fn encoded_payload_is_binary_and_nontrivial() {
        let msg = PairingSessionMessage::ChallengeResponse(JoinerChallengeResponse {
            encrypted_challenge: vec![1, 2, 3],
        });
        let bytes = encode(&msg).unwrap();
        assert!(!bytes.is_empty());
        // Envelope version byte should be the first byte for postcard's
        // layout of `struct { v: u8, body: enum }`.
        assert_eq!(bytes[0], WIRE_VERSION);
    }
}
