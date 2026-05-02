//! Wire codec for the clipboard sync protocol (Slice 2 Phase 2).
//!
//! ## Frame layout
//!
//! ```text
//! sender -> receiver (one bi-stream, one direction):
//!   [magic(1) | header_len_be(4) | header_bytes | payload_len_be(4) | ciphertext | FIN]
//!
//! receiver -> sender (same bi-stream, reverse direction):
//!   [ack_code(1) | FIN]
//! ```
//!
//! * `magic` = [`CLIPBOARD_MAGIC`] — a fixed sentinel so bytes arriving on a
//!   mis-routed ALPN are rejected early, before postcard even runs.
//! * Header is postcard-encoded [`WireHeader`] mirroring
//!   [`ClipboardHeader`](uc_core::ports::ClipboardHeader). Version on the
//!   wire is **this** codec's version (starts at
//!   `ClipboardHeader::CURRENT_VERSION = 1`); it is independent of the
//!   pairing `WIRE_VERSION` so clipboard changes don't drag pairing along.
//! * `header_len` / `payload_len` are big-endian `u32`. Receiver caps them
//!   against [`MAX_HEADER_SIZE`] / [`MAX_PAYLOAD_SIZE`] before allocating
//!   to prevent an unbounded allocation from a hostile peer.
//! * Ack code is one byte — see [`AckCode`]. The sender FIN signals payload
//!   done; the receiver FIN (after the ack byte) signals ack done.
//!
//! ## Why not postcard the whole frame?
//!
//! postcard does not natively describe "arbitrary bytes followed by more
//! arbitrary bytes with a terminator." Explicit length prefixes are
//! simpler, testable in isolation, and align with the pairing wire codec's
//! length-prefixed framing (see `uc-infra/src/pairing/wire.rs`).
//!
//! ## Stream I/O
//!
//! [`write_frame`] / [`read_frame`] abstract over `tokio::io::AsyncRead +
//! AsyncWrite`. iroh's `SendStream` / `RecvStream` already satisfy those
//! bounds so the adapter hands raw stream halves in; the tests here use
//! [`tokio::io::duplex`] bidirectional pipes for the same contract.

use std::convert::TryFrom;

use bytes::Bytes;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use uc_core::ports::ClipboardHeader;

// ============================================================================
// Constants
// ============================================================================

/// Sentinel byte identifying a clipboard sync frame at the head of the
/// stream. Distinct from the pairing codec (no magic byte) so a
/// mis-routed connection fails fast instead of drifting into postcard.
pub const CLIPBOARD_MAGIC: u8 = 0xC1;

/// Hard ceiling on the postcard-encoded header size. A typical header is
/// ~200 bytes; 4 KiB leaves headroom for future optional fields without
/// inviting oversized allocations from a hostile peer.
pub const MAX_HEADER_SIZE: u32 = 4 * 1024;

/// Hard ceiling on ciphertext payload size. 2 MiB covers Phase 2's text /
/// small payload scope; larger content will use the Slice 3 blob path and
/// a `blob_refs` header field. A sender that produces a payload larger
/// than this gets an encoder-side error; a peer that claims one in the
/// length prefix is rejected before allocation.
pub const MAX_PAYLOAD_SIZE: u32 = 2 * 1024 * 1024;

// ============================================================================
// Ack
// ============================================================================

/// One-byte ack emitted by the receiver after successfully consuming the
/// frame (or rejecting it at the wire boundary).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AckCode {
    Accepted = 0x01,
    DuplicateIgnored = 0x02,
    /// Adapter-level rejection — e.g. unknown peer, bad header, oversized
    /// payload. Application-level dedupe uses [`AckCode::DuplicateIgnored`].
    Rejected = 0xFF,
}

impl AckCode {
    pub fn as_byte(self) -> u8 {
        self as u8
    }
}

impl TryFrom<u8> for AckCode {
    type Error = InvalidAckByte;
    fn try_from(byte: u8) -> Result<Self, Self::Error> {
        match byte {
            0x01 => Ok(AckCode::Accepted),
            0x02 => Ok(AckCode::DuplicateIgnored),
            0xFF => Ok(AckCode::Rejected),
            other => Err(InvalidAckByte(other)),
        }
    }
}

#[derive(Debug, Error)]
#[error("unknown clipboard ack byte: 0x{0:02X}")]
pub struct InvalidAckByte(pub u8);

// ============================================================================
// Wire types
// ============================================================================

/// Postcard-serialized header — infra-local mirror of
/// [`ClipboardHeader`](uc_core::ports::ClipboardHeader). Kept separate from
/// the core type so `uc-core` stays free of `serde` derives on port
/// structs (see `uc-infra/AGENTS.md` §4.2).
#[derive(Serialize, Deserialize, Debug)]
struct WireHeader {
    version: u8,
    content_hash: String,
    captured_at_ms: i64,
    origin_device_id: String,
    origin_device_name: String,
    payload_version: u8,
}

// ============================================================================
// Errors
// ============================================================================

#[derive(Debug, Error)]
pub enum WireEncodeError {
    #[error("postcard encode failed: {0}")]
    Postcard(#[from] postcard::Error),
    #[error("header too large: {size} bytes (max {max})")]
    HeaderTooLarge { size: usize, max: u32 },
    #[error("payload too large: {size} bytes (max {max})")]
    PayloadTooLarge { size: usize, max: u32 },
    #[error("stream io: {0}")]
    Io(std::io::Error),
}

#[derive(Debug, Error)]
pub enum WireDecodeError {
    #[error("postcard decode failed: {0}")]
    Postcard(#[from] postcard::Error),
    #[error("unsupported clipboard wire version {got} (this build understands {expected})")]
    UnsupportedVersion { got: u8, expected: u8 },
    #[error("bad magic byte: got 0x{got:02X} (expected 0x{expected:02X})")]
    BadMagic { got: u8, expected: u8 },
    #[error("header size {size} exceeds maximum {max}")]
    HeaderTooLarge { size: u32, max: u32 },
    #[error("payload size {size} exceeds maximum {max}")]
    PayloadTooLarge { size: u32, max: u32 },
    #[error("stream io: {0}")]
    Io(std::io::Error),
}

// ============================================================================
// Public API — pure byte codec
// ============================================================================

/// Serialize a [`ClipboardHeader`] for the wire. Does not include the
/// magic byte or length prefix — callers typically run this once and hand
/// the bytes (plus the payload) to [`write_frame`].
pub fn encode_header(header: &ClipboardHeader) -> Result<Vec<u8>, WireEncodeError> {
    let wire = WireHeader {
        version: header.version,
        content_hash: header.content_hash.clone(),
        captured_at_ms: header.captured_at_ms,
        origin_device_id: header.origin_device_id.clone(),
        origin_device_name: header.origin_device_name.clone(),
        payload_version: header.payload_version,
    };
    let bytes = postcard::to_allocvec(&wire)?;
    if bytes.len() > MAX_HEADER_SIZE as usize {
        return Err(WireEncodeError::HeaderTooLarge {
            size: bytes.len(),
            max: MAX_HEADER_SIZE,
        });
    }
    Ok(bytes)
}

/// Deserialize a header from its postcard byte form, then enforce this
/// codec's version contract.
pub fn decode_header(bytes: &[u8]) -> Result<ClipboardHeader, WireDecodeError> {
    let wire: WireHeader = postcard::from_bytes(bytes)?;
    if wire.version != ClipboardHeader::CURRENT_VERSION {
        return Err(WireDecodeError::UnsupportedVersion {
            got: wire.version,
            expected: ClipboardHeader::CURRENT_VERSION,
        });
    }
    Ok(ClipboardHeader {
        version: wire.version,
        content_hash: wire.content_hash,
        captured_at_ms: wire.captured_at_ms,
        origin_device_id: wire.origin_device_id,
        origin_device_name: wire.origin_device_name,
        payload_version: wire.payload_version,
    })
}

// ============================================================================
// Public API — stream I/O
// ============================================================================

/// Serialize + send one clipboard frame: magic | header_len | header |
/// payload_len | payload. The caller is responsible for closing the send
/// half after this returns (so the peer's final read hits EOF cleanly).
pub async fn write_frame<W: AsyncWrite + Unpin>(
    send: &mut W,
    header: &ClipboardHeader,
    payload: &Bytes,
) -> Result<(), WireEncodeError> {
    if payload.len() > MAX_PAYLOAD_SIZE as usize {
        return Err(WireEncodeError::PayloadTooLarge {
            size: payload.len(),
            max: MAX_PAYLOAD_SIZE,
        });
    }
    let header_bytes = encode_header(header)?;
    let header_len = header_bytes.len() as u32; // bounded by MAX_HEADER_SIZE
    let payload_len = payload.len() as u32;

    send.write_all(&[CLIPBOARD_MAGIC])
        .await
        .map_err(WireEncodeError::Io)?;
    send.write_all(&header_len.to_be_bytes())
        .await
        .map_err(WireEncodeError::Io)?;
    send.write_all(&header_bytes)
        .await
        .map_err(WireEncodeError::Io)?;
    send.write_all(&payload_len.to_be_bytes())
        .await
        .map_err(WireEncodeError::Io)?;
    send.write_all(payload).await.map_err(WireEncodeError::Io)?;
    Ok(())
}

/// Decoded frame handed back to the receiver adapter.
#[derive(Debug)]
pub struct ReadFrame {
    pub header: ClipboardHeader,
    pub ciphertext: Bytes,
}

/// Read one clipboard frame from a stream, validating magic + size caps
/// **before** allocating the header / payload buffers.
pub async fn read_frame<R: AsyncRead + Unpin>(recv: &mut R) -> Result<ReadFrame, WireDecodeError> {
    let mut magic_buf = [0u8; 1];
    recv.read_exact(&mut magic_buf)
        .await
        .map_err(WireDecodeError::Io)?;
    if magic_buf[0] != CLIPBOARD_MAGIC {
        return Err(WireDecodeError::BadMagic {
            got: magic_buf[0],
            expected: CLIPBOARD_MAGIC,
        });
    }

    let mut len_buf = [0u8; 4];
    recv.read_exact(&mut len_buf)
        .await
        .map_err(WireDecodeError::Io)?;
    let header_len = u32::from_be_bytes(len_buf);
    if header_len > MAX_HEADER_SIZE {
        return Err(WireDecodeError::HeaderTooLarge {
            size: header_len,
            max: MAX_HEADER_SIZE,
        });
    }
    let mut header_bytes = vec![0u8; header_len as usize];
    recv.read_exact(&mut header_bytes)
        .await
        .map_err(WireDecodeError::Io)?;
    let header = decode_header(&header_bytes)?;

    recv.read_exact(&mut len_buf)
        .await
        .map_err(WireDecodeError::Io)?;
    let payload_len = u32::from_be_bytes(len_buf);
    if payload_len > MAX_PAYLOAD_SIZE {
        return Err(WireDecodeError::PayloadTooLarge {
            size: payload_len,
            max: MAX_PAYLOAD_SIZE,
        });
    }
    let mut payload = vec![0u8; payload_len as usize];
    recv.read_exact(&mut payload)
        .await
        .map_err(WireDecodeError::Io)?;

    Ok(ReadFrame {
        header,
        ciphertext: Bytes::from(payload),
    })
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::duplex;

    fn sample_header() -> ClipboardHeader {
        ClipboardHeader {
            version: ClipboardHeader::CURRENT_VERSION,
            content_hash: "a".repeat(64),
            captured_at_ms: 1_700_000_000_000,
            origin_device_id: "dev-alpha".to_string(),
            origin_device_name: "Alpha Laptop".to_string(),
            payload_version: 3,
        }
    }

    async fn round_trip(
        header: &ClipboardHeader,
        payload: &Bytes,
    ) -> Result<ReadFrame, WireDecodeError> {
        let (mut client, mut server) = duplex(64 * 1024);
        // Sender finishes its write before shutting the half so the
        // reader's read_exact sees a complete frame. In the real adapter
        // the `finish()` call on iroh SendStream plays the same role.
        let h = header.clone();
        let p = payload.clone();
        let send_task = tokio::spawn(async move {
            write_frame(&mut client, &h, &p).await.expect("write frame");
            client.shutdown().await.expect("shutdown client");
        });
        let frame = read_frame(&mut server).await?;
        send_task.await.unwrap();
        Ok(frame)
    }

    /// 1. 正常 round-trip — header + payload recover byte-for-byte.
    #[tokio::test]
    async fn write_then_read_round_trips_header_and_payload() {
        let header = sample_header();
        let payload = Bytes::from(vec![0xDE, 0xAD, 0xBE, 0xEF, 0x01, 0x02, 0x03, 0x04]);

        let recovered = round_trip(&header, &payload).await.expect("round trip");

        assert_eq!(recovered.header, header);
        assert_eq!(recovered.ciphertext, payload);
    }

    /// 2. Bad magic byte — receiver rejects before attempting to decode
    /// the postcard header or allocate buffers.
    #[tokio::test]
    async fn read_frame_rejects_bad_magic_byte() {
        let (mut client, mut server) = duplex(64);
        let send_task = tokio::spawn(async move {
            // Wrong magic, then some plausible garbage after.
            client.write_all(&[0x00, 0x00, 0x00, 0x00, 0x01]).await.ok();
            client.shutdown().await.ok();
        });

        let err = read_frame(&mut server)
            .await
            .expect_err("bad magic must be rejected");
        match err {
            WireDecodeError::BadMagic { got, expected } => {
                assert_eq!(got, 0x00);
                assert_eq!(expected, CLIPBOARD_MAGIC);
            }
            other => panic!("expected BadMagic, got {other:?}"),
        }
        send_task.await.unwrap();
    }

    /// 3. Header length exceeds MAX_HEADER_SIZE — rejected before the
    /// receiver allocates the header buffer (protects against an oversized
    /// allocation from a hostile peer).
    #[tokio::test]
    async fn read_frame_rejects_oversized_header_length() {
        let (mut client, mut server) = duplex(64);
        let oversized = MAX_HEADER_SIZE + 1;
        let send_task = tokio::spawn(async move {
            client.write_all(&[CLIPBOARD_MAGIC]).await.ok();
            client.write_all(&oversized.to_be_bytes()).await.ok();
            client.shutdown().await.ok();
        });

        let err = read_frame(&mut server)
            .await
            .expect_err("oversized header length must be rejected");
        match err {
            WireDecodeError::HeaderTooLarge { size, max } => {
                assert_eq!(size, oversized);
                assert_eq!(max, MAX_HEADER_SIZE);
            }
            other => panic!("expected HeaderTooLarge, got {other:?}"),
        }
        send_task.await.unwrap();
    }

    /// 4. Payload length exceeds MAX_PAYLOAD_SIZE — rejected at the length
    /// prefix, not by attempting a huge Vec allocation.
    #[tokio::test]
    async fn read_frame_rejects_oversized_payload_length() {
        let header = sample_header();
        let header_bytes = encode_header(&header).unwrap();
        let oversized = MAX_PAYLOAD_SIZE + 1;

        let (mut client, mut server) = duplex(64 * 1024);
        let header_len = header_bytes.len() as u32;
        let send_task = tokio::spawn(async move {
            client.write_all(&[CLIPBOARD_MAGIC]).await.ok();
            client.write_all(&header_len.to_be_bytes()).await.ok();
            client.write_all(&header_bytes).await.ok();
            client.write_all(&oversized.to_be_bytes()).await.ok();
            client.shutdown().await.ok();
        });

        let err = read_frame(&mut server)
            .await
            .expect_err("oversized payload length must be rejected");
        match err {
            WireDecodeError::PayloadTooLarge { size, max } => {
                assert_eq!(size, oversized);
                assert_eq!(max, MAX_PAYLOAD_SIZE);
            }
            other => panic!("expected PayloadTooLarge, got {other:?}"),
        }
        send_task.await.unwrap();
    }

    /// 5. Truncated stream — peer drops after magic + partial header length
    /// prefix. `read_exact` surfaces the EOF as an Io error, not a panic.
    #[tokio::test]
    async fn read_frame_surfaces_truncation_as_io_error() {
        let (mut client, mut server) = duplex(64);
        let send_task = tokio::spawn(async move {
            // Magic OK, but only 2 of the 4 header-length bytes.
            client.write_all(&[CLIPBOARD_MAGIC, 0x00, 0x00]).await.ok();
            client.shutdown().await.ok();
        });

        let err = read_frame(&mut server)
            .await
            .expect_err("truncated prefix must surface as error");
        match err {
            WireDecodeError::Io(_) => {}
            other => panic!("expected Io error for truncation, got {other:?}"),
        }
        send_task.await.unwrap();
    }

    /// 6. Unsupported wire version — forge a header at version+1 and
    /// confirm the decoder rejects explicitly instead of interpreting it
    /// as the current schema.
    #[tokio::test]
    async fn decode_rejects_future_header_version() {
        let future = WireHeader {
            version: ClipboardHeader::CURRENT_VERSION + 1,
            content_hash: "stub".to_string(),
            captured_at_ms: 0,
            origin_device_id: "d".to_string(),
            origin_device_name: "n".to_string(),
            payload_version: 3,
        };
        let bytes = postcard::to_allocvec(&future).unwrap();

        match decode_header(&bytes) {
            Err(WireDecodeError::UnsupportedVersion { got, expected }) => {
                assert_eq!(got, ClipboardHeader::CURRENT_VERSION + 1);
                assert_eq!(expected, ClipboardHeader::CURRENT_VERSION);
            }
            other => panic!("expected UnsupportedVersion, got {other:?}"),
        }
    }

    /// Ack codec sanity — the three defined variants round-trip through
    /// the byte boundary, and an unknown byte returns an error rather
    /// than mapping to a surprise variant.
    #[test]
    fn ack_code_round_trip_and_rejects_unknown_byte() {
        for code in [
            AckCode::Accepted,
            AckCode::DuplicateIgnored,
            AckCode::Rejected,
        ] {
            let byte = code.as_byte();
            let decoded = AckCode::try_from(byte).expect("known byte decodes");
            assert_eq!(decoded, code);
        }
        let unknown = AckCode::try_from(0x7B);
        assert!(
            matches!(unknown, Err(InvalidAckByte(0x7B))),
            "got {unknown:?}"
        );
    }
}
