//! Binary-encoded file transfer protocol messages.
//!
//! Defines the message types exchanged between peers during file transfer
//! over libp2p. Uses the same `std::io::Read/Write` binary codec pattern
//! as `clipboard_payload_v3.rs`.
//!
//! # Binary Layout
//! ```text
//! [1B]  discriminant tag (0..5)
//! Variant-specific fields follow (length-prefixed strings, raw data, etc.)
//! ```

use std::io::{Read, Write};

/// Maximum filename length in bytes.
const MAX_FILENAME_LEN: usize = 1_024;
/// Maximum chunk data size in bytes (256 MiB).
const MAX_CHUNK_SIZE: usize = 256 * 1024 * 1024;
/// Maximum reason/message string length in bytes.
const MAX_REASON_LEN: usize = 1_024;
/// Maximum content hash string length in bytes.
const MAX_HASH_LEN: usize = 256;
/// Maximum transfer_id / batch_id string length in bytes.
const MAX_ID_LEN: usize = 256;

/// Messages exchanged between peers during a file transfer session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileTransferMessage {
    /// Sender announces intent to transfer a file.
    Announce {
        filename: String,
        file_size: u64,
        content_hash: String,
        batch_id: Option<String>,
    },
    /// Receiver accepts the announced transfer.
    Accept { transfer_id: String },
    /// A chunk of file data.
    Data { chunk_index: u32, data: Vec<u8> },
    /// Sender signals transfer completion with a verification hash.
    Complete { content_hash: String },
    /// Either side cancels the transfer.
    Cancel { reason: String },
    /// Protocol-level error.
    Error { code: u16, message: String },
}

// Discriminant tags
const TAG_ANNOUNCE: u8 = 0;
const TAG_ACCEPT: u8 = 1;
const TAG_DATA: u8 = 2;
const TAG_COMPLETE: u8 = 3;
const TAG_CANCEL: u8 = 4;
const TAG_ERROR: u8 = 5;

impl FileTransferMessage {
    /// Encode this message into binary format.
    pub fn encode_to<W: Write>(&self, w: &mut W) -> std::io::Result<()> {
        match self {
            Self::Announce {
                filename,
                file_size,
                content_hash,
                batch_id,
            } => {
                w.write_all(&[TAG_ANNOUNCE])?;
                write_string_u16(w, filename, MAX_FILENAME_LEN, "filename")?;
                w.write_all(&file_size.to_le_bytes())?;
                write_string_u16(w, content_hash, MAX_HASH_LEN, "content_hash")?;
                // Optional batch_id: 1 byte flag + string
                match batch_id {
                    Some(id) => {
                        w.write_all(&[1u8])?;
                        write_string_u16(w, id, MAX_ID_LEN, "batch_id")?;
                    }
                    None => {
                        w.write_all(&[0u8])?;
                    }
                }
            }
            Self::Accept { transfer_id } => {
                w.write_all(&[TAG_ACCEPT])?;
                write_string_u16(w, transfer_id, MAX_ID_LEN, "transfer_id")?;
            }
            Self::Data { chunk_index, data } => {
                w.write_all(&[TAG_DATA])?;
                w.write_all(&chunk_index.to_le_bytes())?;
                if data.len() > MAX_CHUNK_SIZE {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        format!(
                            "data length {} exceeds maximum {}",
                            data.len(),
                            MAX_CHUNK_SIZE
                        ),
                    ));
                }
                let data_len = u32::try_from(data.len()).map_err(|_| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        format!("data length {} cannot fit u32", data.len()),
                    )
                })?;
                w.write_all(&data_len.to_le_bytes())?;
                w.write_all(data)?;
            }
            Self::Complete { content_hash } => {
                w.write_all(&[TAG_COMPLETE])?;
                write_string_u16(w, content_hash, MAX_HASH_LEN, "content_hash")?;
            }
            Self::Cancel { reason } => {
                w.write_all(&[TAG_CANCEL])?;
                write_string_u16(w, reason, MAX_REASON_LEN, "reason")?;
            }
            Self::Error { code, message } => {
                w.write_all(&[TAG_ERROR])?;
                w.write_all(&code.to_le_bytes())?;
                write_string_u16(w, message, MAX_REASON_LEN, "message")?;
            }
        }
        Ok(())
    }

    /// Convenience method: encode to a new `Vec<u8>`.
    pub fn encode_to_vec(&self) -> std::io::Result<Vec<u8>> {
        let mut buf = Vec::new();
        self.encode_to(&mut buf)?;
        Ok(buf)
    }

    /// Decode a message from binary format.
    pub fn decode_from<R: Read>(r: &mut R) -> std::io::Result<Self> {
        let mut tag = [0u8; 1];
        r.read_exact(&mut tag)?;

        match tag[0] {
            TAG_ANNOUNCE => {
                let filename = read_string_u16(r, MAX_FILENAME_LEN, "filename")?;
                let mut size_buf = [0u8; 8];
                r.read_exact(&mut size_buf)?;
                let file_size = u64::from_le_bytes(size_buf);
                let content_hash = read_string_u16(r, MAX_HASH_LEN, "content_hash")?;
                let mut has_batch = [0u8; 1];
                r.read_exact(&mut has_batch)?;
                let batch_id = match has_batch[0] {
                    1 => Some(read_string_u16(r, MAX_ID_LEN, "batch_id")?),
                    0 => None,
                    other => {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            format!("invalid has_batch_id flag: expected 0 or 1, got {other}"),
                        ));
                    }
                };
                Ok(Self::Announce {
                    filename,
                    file_size,
                    content_hash,
                    batch_id,
                })
            }
            TAG_ACCEPT => {
                let transfer_id = read_string_u16(r, MAX_ID_LEN, "transfer_id")?;
                Ok(Self::Accept { transfer_id })
            }
            TAG_DATA => {
                let mut idx_buf = [0u8; 4];
                r.read_exact(&mut idx_buf)?;
                let chunk_index = u32::from_le_bytes(idx_buf);
                let mut len_buf = [0u8; 4];
                r.read_exact(&mut len_buf)?;
                let data_len = u32::from_le_bytes(len_buf) as usize;
                if data_len > MAX_CHUNK_SIZE {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("data_len {data_len} exceeds maximum {MAX_CHUNK_SIZE}"),
                    ));
                }
                let mut data = vec![0u8; data_len];
                r.read_exact(&mut data)?;
                Ok(Self::Data { chunk_index, data })
            }
            TAG_COMPLETE => {
                let content_hash = read_string_u16(r, MAX_HASH_LEN, "content_hash")?;
                Ok(Self::Complete { content_hash })
            }
            TAG_CANCEL => {
                let reason = read_string_u16(r, MAX_REASON_LEN, "reason")?;
                Ok(Self::Cancel { reason })
            }
            TAG_ERROR => {
                let mut code_buf = [0u8; 2];
                r.read_exact(&mut code_buf)?;
                let code = u16::from_le_bytes(code_buf);
                let message = read_string_u16(r, MAX_REASON_LEN, "message")?;
                Ok(Self::Error { code, message })
            }
            other => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("invalid FileTransferMessage discriminant tag: {other}"),
            )),
        }
    }
}

/// Write a length-prefixed string (u16 LE length + UTF-8 bytes).
fn write_string_u16<W: Write>(
    w: &mut W,
    s: &str,
    max_len: usize,
    field_name: &str,
) -> std::io::Result<()> {
    let bytes = s.as_bytes();
    if bytes.len() > max_len {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!(
                "{field_name} length {} exceeds maximum {max_len}",
                bytes.len()
            ),
        ));
    }
    let len = u16::try_from(bytes.len()).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{field_name} length {} cannot fit u16", bytes.len()),
        )
    })?;
    w.write_all(&len.to_le_bytes())?;
    w.write_all(bytes)?;
    Ok(())
}

/// Read a length-prefixed string (u16 LE length + UTF-8 bytes).
fn read_string_u16<R: Read>(
    r: &mut R,
    max_len: usize,
    field_name: &str,
) -> std::io::Result<String> {
    let mut len_buf = [0u8; 2];
    r.read_exact(&mut len_buf)?;
    let len = u16::from_le_bytes(len_buf) as usize;
    if len > max_len {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("{field_name} length {len} exceeds maximum {max_len}"),
        ));
    }
    let mut bytes = vec![0u8; len];
    r.read_exact(&mut bytes)?;
    String::from_utf8(bytes).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("invalid UTF-8 in {field_name}: {e}"),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn round_trip(msg: &FileTransferMessage) -> FileTransferMessage {
        let encoded = msg.encode_to_vec().expect("encode failed");
        FileTransferMessage::decode_from(&mut Cursor::new(encoded)).expect("decode failed")
    }

    #[test]
    fn round_trip_announce_with_batch() {
        let msg = FileTransferMessage::Announce {
            filename: "report.pdf".to_string(),
            file_size: 1_048_576,
            content_hash: "sha256:abc123".to_string(),
            batch_id: Some("batch-001".to_string()),
        };
        assert_eq!(round_trip(&msg), msg);
    }

    #[test]
    fn round_trip_announce_without_batch() {
        let msg = FileTransferMessage::Announce {
            filename: "photo.jpg".to_string(),
            file_size: 5_000_000,
            content_hash: "sha256:def456".to_string(),
            batch_id: None,
        };
        assert_eq!(round_trip(&msg), msg);
    }

    #[test]
    fn round_trip_accept() {
        let msg = FileTransferMessage::Accept {
            transfer_id: "xfer-42".to_string(),
        };
        assert_eq!(round_trip(&msg), msg);
    }

    #[test]
    fn round_trip_data() {
        let msg = FileTransferMessage::Data {
            chunk_index: 7,
            data: vec![0xDE, 0xAD, 0xBE, 0xEF],
        };
        assert_eq!(round_trip(&msg), msg);
    }

    #[test]
    fn round_trip_data_empty() {
        let msg = FileTransferMessage::Data {
            chunk_index: 0,
            data: vec![],
        };
        assert_eq!(round_trip(&msg), msg);
    }

    #[test]
    fn round_trip_complete() {
        let msg = FileTransferMessage::Complete {
            content_hash: "sha256:final".to_string(),
        };
        assert_eq!(round_trip(&msg), msg);
    }

    #[test]
    fn round_trip_cancel() {
        let msg = FileTransferMessage::Cancel {
            reason: "user cancelled".to_string(),
        };
        assert_eq!(round_trip(&msg), msg);
    }

    #[test]
    fn round_trip_error() {
        let msg = FileTransferMessage::Error {
            code: 500,
            message: "internal error".to_string(),
        };
        assert_eq!(round_trip(&msg), msg);
    }

    #[test]
    fn reject_oversized_filename() {
        let msg = FileTransferMessage::Announce {
            filename: "x".repeat(MAX_FILENAME_LEN + 1),
            file_size: 0,
            content_hash: "h".to_string(),
            batch_id: None,
        };
        let err = msg.encode_to_vec().unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
        assert!(err.to_string().contains("filename"));
    }

    #[test]
    fn reject_oversized_data() {
        let msg = FileTransferMessage::Data {
            chunk_index: 0,
            data: vec![0u8; MAX_CHUNK_SIZE + 1],
        };
        let err = msg.encode_to_vec().unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
        assert!(err.to_string().contains("data length"));
    }

    #[test]
    fn reject_oversized_reason() {
        let msg = FileTransferMessage::Cancel {
            reason: "r".repeat(MAX_REASON_LEN + 1),
        };
        let err = msg.encode_to_vec().unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
        assert!(err.to_string().contains("reason"));
    }

    #[test]
    fn reject_oversized_error_message() {
        let msg = FileTransferMessage::Error {
            code: 1,
            message: "m".repeat(MAX_REASON_LEN + 1),
        };
        let err = msg.encode_to_vec().unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
        assert!(err.to_string().contains("message"));
    }

    #[test]
    fn reject_invalid_discriminant() {
        let buf = vec![99u8]; // invalid tag
        let err = FileTransferMessage::decode_from(&mut Cursor::new(buf)).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("discriminant"));
    }

    #[test]
    fn reject_oversized_filename_on_decode() {
        // Craft: tag=0 (Announce) + filename_len > MAX
        let mut buf = Vec::new();
        buf.push(TAG_ANNOUNCE);
        let big_len = (MAX_FILENAME_LEN + 1) as u16;
        buf.extend_from_slice(&big_len.to_le_bytes());
        let err = FileTransferMessage::decode_from(&mut Cursor::new(buf)).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("filename"));
    }

    #[test]
    fn reject_oversized_data_on_decode() {
        // Craft: tag=2 (Data) + chunk_index + data_len > MAX
        let mut buf = Vec::new();
        buf.push(TAG_DATA);
        buf.extend_from_slice(&0u32.to_le_bytes()); // chunk_index
        let big_len = (MAX_CHUNK_SIZE as u32) + 1;
        buf.extend_from_slice(&big_len.to_le_bytes());
        let err = FileTransferMessage::decode_from(&mut Cursor::new(buf)).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("data_len"));
    }

    #[test]
    fn reject_invalid_batch_id_flag() {
        let mut buf = Vec::new();
        buf.push(TAG_ANNOUNCE);
        // filename "x"
        buf.extend_from_slice(&1u16.to_le_bytes());
        buf.push(b'x');
        // file_size
        buf.extend_from_slice(&0u64.to_le_bytes());
        // content_hash "h"
        buf.extend_from_slice(&1u16.to_le_bytes());
        buf.push(b'h');
        // invalid has_batch_id flag
        buf.push(2u8);
        let err = FileTransferMessage::decode_from(&mut Cursor::new(buf)).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("has_batch_id"));
    }
}
