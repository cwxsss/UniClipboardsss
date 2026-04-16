use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_with::{base64::Base64, serde_as};

/// Payload version for ClipboardMessage.encrypted_content.
/// V3 is the only supported version. V1/V2 have been removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(into = "u8", try_from = "u8")]
pub enum ClipboardPayloadVersion {
    /// V3: Binary multi-representation payload (V3 chunked AEAD with optional zstd compression)
    V3 = 3,
}

impl Default for ClipboardPayloadVersion {
    fn default() -> Self {
        Self::V3
    }
}

impl From<ClipboardPayloadVersion> for u8 {
    fn from(v: ClipboardPayloadVersion) -> u8 {
        v as u8
    }
}

impl TryFrom<u8> for ClipboardPayloadVersion {
    type Error = String;
    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            3 => Ok(Self::V3),
            other => Err(format!("unknown ClipboardPayloadVersion: {other}")),
        }
    }
}

/// Mapping between a file transfer and its original filename.
/// Carried in clipboard sync so the receiver can pre-compute local cache paths
/// before the file transfer completes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileTransferMapping {
    pub transfer_id: String,
    pub filename: String,
}

/// Clipboard content broadcast via network.
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardMessage {
    pub id: String,
    pub content_hash: String,
    /// Binary payload. For V3: binary chunked format (UC3 header + compressed chunks).
    /// Uses base64 encoding in JSON for compact representation.
    #[serde_as(as = "Base64")]
    pub encrypted_content: Vec<u8>,
    pub timestamp: DateTime<Utc>,
    pub origin_device_id: String,
    pub origin_device_name: String,
    /// Payload format version. Required in deserialization to reject messages with missing version.
    pub payload_version: ClipboardPayloadVersion,
    /// Flow correlation ID from the originating capture pipeline.
    /// Defaults to None for backward compatibility with older peers.
    #[deprecated(
        note = "Phase 87: replaced by W3C traceparent field. Do not read or write. Scheduled for full removal in a future protocol cleanup phase."
    )]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin_flow_id: Option<String>,
    /// W3C traceparent header for cross-device distributed tracing (Phase 87).
    /// Defaults to None for backward compatibility with older peers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub traceparent: Option<String>,
    /// File transfer mappings for cross-platform path rewriting.
    /// When present, the receiver rewrites file paths to local cache locations.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub file_transfers: Vec<FileTransferMapping>,
}
