//! V3 envelope codec for clipboard sync (Slice 2 Phase 3 · T2).
//!
//! Wraps the raw `uc-core::network::protocol::ClipboardBinaryPayload` V3
//! codec with snapshot-level convenience:
//!
//! * [`encode_snapshot_to_v3_bytes`] — take a `SystemClipboardSnapshot`
//!   (the daemon's capture output), serialise every representation into
//!   the V3 envelope, return the plaintext `Bytes` to feed
//!   `ClipboardSyncFacade::dispatch_entry` + the dedup hash string.
//! * [`decode_v3_bytes_to_snapshot`] — inverse of the above: consume the
//!   plaintext from `IngestInboundClipboardUseCase`'s `InboundNotice` and
//!   produce a fresh `SystemClipboardSnapshot` with new
//!   `RepresentationId`s (receiver-local identity, since representations
//!   from a peer have no meaningful ID continuity).
//!
//! Why not put these inside the facade? Because
//! `ApplyInboundClipboardUseCase` (T4) decodes in the use case layer —
//! daemon's worker just hands over the opaque bytes it received, and
//! the use case owns the codec + persistence + dedup decisions.
//!
//! Phase 2 consumers (CLI `send` / `watch`) that still deal in raw text
//! bytes can keep using the `payload_version=3` marker + caller-computed
//! `content_hash` via `dispatch_entry`; Phase 3 CLI upgrades (T9/T10) go
//! through these helpers to match the daemon's wire format.

use anyhow::{anyhow, Result};
use bytes::Bytes;

use uc_core::ids::{FormatId, RepresentationId};
use uc_core::network::protocol::{BinaryRepresentation, ClipboardBinaryPayload};
use uc_core::{MimeType, ObservedClipboardRepresentation, SystemClipboardSnapshot};

/// Encode a snapshot into the V3 wire envelope and return
/// `(envelope_bytes, content_hash)`.
///
/// `content_hash` is the canonical `"blake3v1:<hex>"` string produced by
/// `snapshot.snapshot_hash()` — the same value the daemon will store in
/// `clipboard_event.snapshot_hash` after a local capture, so receiver
/// dedup via `find_entry_id_by_snapshot_hash` matches.
///
/// The envelope bytes are the application-layer plaintext; the transfer
/// cipher (`TransferCipherPort`) wraps them before they hit the wire.
pub(crate) fn encode_snapshot_to_v3_bytes(
    snapshot: &SystemClipboardSnapshot,
) -> Result<(Bytes, String)> {
    let reps = snapshot
        .representations
        .iter()
        .map(|rep| BinaryRepresentation {
            format_id: rep.format_id.as_ref().to_string(),
            mime: rep.mime.as_ref().map(|m| m.as_str().to_string()),
            data: rep.bytes.clone(),
        })
        .collect();

    let payload = ClipboardBinaryPayload {
        ts_ms: snapshot.ts_ms,
        representations: reps,
    };

    let bytes = payload
        .encode_to_vec()
        .map_err(|e| anyhow!("encode V3 envelope: {e}"))?;
    let content_hash = snapshot.snapshot_hash().to_string();

    Ok((Bytes::from(bytes), content_hash))
}

/// Decode V3 envelope bytes back into a `SystemClipboardSnapshot`.
///
/// Each `BinaryRepresentation` becomes an
/// `ObservedClipboardRepresentation` with a **freshly minted**
/// `RepresentationId`. Representation identity is local per-device —
/// the sender's ID has no meaning on the receiver, and a fresh ID keeps
/// the receiver's `ClipboardEvent` + `clipboard_snapshot_representation`
/// rows unique across re-syncs.
///
/// Caller should pass `notice.plaintext.as_ref()` from the inbound
/// notice; the decoder does not claim ownership of the buffer.
pub fn decode_v3_bytes_to_snapshot(bytes: &[u8]) -> Result<SystemClipboardSnapshot> {
    let mut cursor = bytes;
    let payload = ClipboardBinaryPayload::decode_from(&mut cursor)
        .map_err(|e| anyhow!("decode V3 envelope: {e}"))?;

    let representations = payload
        .representations
        .into_iter()
        .map(|rep| {
            ObservedClipboardRepresentation::new(
                RepresentationId::new(),
                FormatId::from(rep.format_id),
                rep.mime.map(MimeType),
                rep.data,
            )
        })
        .collect();

    Ok(SystemClipboardSnapshot {
        ts_ms: payload.ts_ms,
        representations,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_snapshot(text: &str) -> SystemClipboardSnapshot {
        SystemClipboardSnapshot {
            ts_ms: 1_700_000_000_000,
            representations: vec![ObservedClipboardRepresentation::new(
                RepresentationId::new(),
                FormatId::from("text"),
                Some(MimeType("text/plain".to_string())),
                text.as_bytes().to_vec(),
            )],
        }
    }

    /// Verdict 1 — roundtrip: encoded + decoded snapshot carries the
    /// same ts_ms, representation count, format_id/mime/bytes. The
    /// `RepresentationId` intentionally differs (receiver-local).
    #[test]
    fn roundtrip_single_text_representation() {
        let original = fixture_snapshot("hello phase3");
        let (bytes, hash) = encode_snapshot_to_v3_bytes(&original).expect("encode should succeed");
        assert!(bytes.len() > 0);
        assert!(
            hash.starts_with("blake3v1:"),
            "content_hash should be blake3v1:<hex>, got {hash}"
        );

        let decoded = decode_v3_bytes_to_snapshot(&bytes).expect("decode should succeed");
        assert_eq!(decoded.ts_ms, original.ts_ms);
        assert_eq!(decoded.representations.len(), 1);
        let rep = &decoded.representations[0];
        assert_eq!(rep.format_id.as_ref(), "text");
        assert_eq!(rep.mime.as_ref().map(|m| m.as_str()), Some("text/plain"));
        assert_eq!(rep.bytes, b"hello phase3");
    }

    /// Verdict 2 — roundtrip preserves byte-for-byte equality for
    /// multiple representations + None-mime + non-ASCII bytes.
    #[test]
    fn roundtrip_multi_rep_with_binary_and_no_mime() {
        let original = SystemClipboardSnapshot {
            ts_ms: 42,
            representations: vec![
                ObservedClipboardRepresentation::new(
                    RepresentationId::new(),
                    FormatId::from("public.png"),
                    Some(MimeType("image/png".to_string())),
                    vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0xFF, 0x00],
                ),
                ObservedClipboardRepresentation::new(
                    RepresentationId::new(),
                    FormatId::from("custom-binary"),
                    None, // no mime — exercises the [1B] has_mime=0 branch
                    b"\x00\x01\x02\xDE\xAD\xBE\xEF".to_vec(),
                ),
            ],
        };
        let (bytes, _) = encode_snapshot_to_v3_bytes(&original).expect("encode should succeed");
        let decoded = decode_v3_bytes_to_snapshot(&bytes).expect("decode should succeed");

        assert_eq!(decoded.ts_ms, 42);
        assert_eq!(decoded.representations.len(), 2);
        assert_eq!(decoded.representations[0].format_id.as_ref(), "public.png");
        assert_eq!(
            decoded.representations[0].bytes,
            vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0xFF, 0x00]
        );
        assert_eq!(
            decoded.representations[1].format_id.as_ref(),
            "custom-binary"
        );
        assert!(decoded.representations[1].mime.is_none());
    }

    /// Verdict 3 — content_hash is deterministic: same snapshot bytes →
    /// same hash. Guards against future refactors accidentally making
    /// it order-dependent or including non-content fields.
    #[test]
    fn content_hash_deterministic_across_encodes() {
        let snap_a = fixture_snapshot("same text");
        let snap_b = fixture_snapshot("same text");
        let (_, hash_a) = encode_snapshot_to_v3_bytes(&snap_a).unwrap();
        let (_, hash_b) = encode_snapshot_to_v3_bytes(&snap_b).unwrap();
        assert_eq!(hash_a, hash_b, "same content → same snapshot_hash");

        let snap_c = fixture_snapshot("different text");
        let (_, hash_c) = encode_snapshot_to_v3_bytes(&snap_c).unwrap();
        assert_ne!(hash_a, hash_c, "different content → different hash");
    }

    /// Verdict 4 — decode rejects truncated / corrupt bytes cleanly
    /// instead of panicking. The use case maps this to `DecodeFailed`
    /// and drops the inbound frame.
    #[test]
    fn decode_fails_on_truncated_bytes() {
        let original = fixture_snapshot("valid payload");
        let (bytes, _) = encode_snapshot_to_v3_bytes(&original).unwrap();
        // Truncate mid-stream — should fail in `read_exact`.
        let truncated = &bytes[..5.min(bytes.len())];
        let err =
            decode_v3_bytes_to_snapshot(truncated).expect_err("truncated payload must not decode");
        let msg = err.to_string();
        assert!(
            msg.contains("decode V3 envelope"),
            "error should mention V3 envelope context, got: {msg}"
        );
    }
}
