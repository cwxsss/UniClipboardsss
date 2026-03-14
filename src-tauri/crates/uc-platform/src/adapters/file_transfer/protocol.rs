//! Chunked file transfer protocol.
//!
//! Handles the announce/accept/chunk/complete message flow for file transfers
//! with incremental Blake3 hash computation and verification.

use super::framing::{read_file_frame, write_file_frame, FileMessageType};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::io::{AsyncRead, AsyncWrite};
use tracing::{debug, info_span, warn, Instrument};

/// Default chunk size: 256KB.
pub const CHUNK_SIZE: usize = 256 * 1024;

/// File transfer announcement sent by the sender.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileAnnounce {
    pub transfer_id: String,
    pub filename: String,
    pub file_size: u64,
    pub blake3_hash: String,
    pub batch_id: Option<String>,
    pub batch_total: Option<u32>,
}

/// Acceptance or rejection response from the receiver.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileAcceptance {
    pub transfer_id: String,
    pub accepted: bool,
    pub reason: Option<String>,
}

/// Header prepended to each data chunk (serialized as JSON within a Chunk frame).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChunkHeader {
    pub chunk_index: u32,
    pub chunk_size: u32,
}

/// Completion message with hash for verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileComplete {
    pub transfer_id: String,
    pub blake3_hash: String,
    pub total_chunks: u32,
}

/// Compute Blake3 hash of a file.
pub async fn compute_blake3_hash(file_path: &Path) -> Result<String> {
    let data = tokio::fs::read(file_path).await?;
    let hash = blake3::hash(&data);
    Ok(hash.to_hex().to_string())
}

/// Send a file in chunks over the provided writer.
///
/// Returns the Blake3 hash of the file.
pub async fn send_file_chunked<W>(
    writer: &mut W,
    file_path: &Path,
    transfer_id: &str,
    batch_id: Option<String>,
    batch_total: Option<u32>,
    chunk_size: usize,
    progress_callback: Option<&(dyn Fn(u32, u32, u64) + Send + Sync)>,
) -> Result<String>
where
    W: AsyncWrite + Unpin,
{
    let data = tokio::fs::read(file_path)
        .await
        .map_err(|e| anyhow!("failed to read file for transfer: {}", e))?;

    let file_size = data.len() as u64;
    let filename = file_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    // Compute hash
    let mut hasher = blake3::Hasher::new();
    hasher.update(&data);
    let hash = hasher.finalize().to_hex().to_string();

    // Send announce
    let announce = FileAnnounce {
        transfer_id: transfer_id.to_string(),
        filename,
        file_size,
        blake3_hash: hash.clone(),
        batch_id,
        batch_total,
    };
    let announce_bytes = serde_json::to_vec(&announce)?;
    write_file_frame(writer, FileMessageType::Announce, &announce_bytes).await?;

    // Send chunks
    let total_chunks = ((data.len() + chunk_size - 1) / chunk_size) as u32;
    let mut bytes_sent: u64 = 0;

    for (i, chunk_data) in data.chunks(chunk_size).enumerate() {
        let header = FileChunkHeader {
            chunk_index: i as u32,
            chunk_size: chunk_data.len() as u32,
        };
        let header_bytes = serde_json::to_vec(&header)?;

        // Chunk frame payload: header JSON length (4 bytes) + header JSON + raw chunk data
        let mut payload = Vec::with_capacity(4 + header_bytes.len() + chunk_data.len());
        payload.extend_from_slice(&(header_bytes.len() as u32).to_be_bytes());
        payload.extend_from_slice(&header_bytes);
        payload.extend_from_slice(chunk_data);

        write_file_frame(writer, FileMessageType::Chunk, &payload).await?;

        bytes_sent += chunk_data.len() as u64;
        if let Some(cb) = progress_callback {
            cb(i as u32 + 1, total_chunks, bytes_sent);
        }
    }

    // Send complete
    let complete = FileComplete {
        transfer_id: transfer_id.to_string(),
        blake3_hash: hash.clone(),
        total_chunks,
    };
    let complete_bytes = serde_json::to_vec(&complete)?;
    write_file_frame(writer, FileMessageType::Complete, &complete_bytes).await?;

    Ok(hash)
}

/// Receive a file from chunks, verify hash, and atomically rename.
///
/// Returns the final file path after successful verification.
pub async fn receive_file_chunked<R>(
    reader: &mut R,
    announce: &FileAnnounce,
    cache_dir: &Path,
    progress_callback: Option<&(dyn Fn(u32, u32, u64) + Send + Sync)>,
) -> Result<PathBuf>
where
    R: AsyncRead + Unpin,
{
    let transfer_dir = cache_dir.join(&announce.transfer_id);
    tokio::fs::create_dir_all(&transfer_dir).await?;
    let tmp_path = transfer_dir.join(format!("{}.tmp", announce.transfer_id));

    // Set unix permissions on temp file after creation
    let result = receive_chunks_to_file(reader, &tmp_path, announce, progress_callback)
        .instrument(info_span!("receive_chunks", transfer_id = %announce.transfer_id))
        .await;

    match result {
        Ok(received_hash) => {
            // Verify hash
            if received_hash != announce.blake3_hash {
                // Hash mismatch - delete temp file
                let _ = tokio::fs::remove_file(&tmp_path).await;
                return Err(anyhow!(
                    "blake3 hash mismatch: expected {}, got {}",
                    announce.blake3_hash,
                    received_hash
                ));
            }

            // Sanitize filename
            let safe_filename = sanitize_filename(&announce.filename);
            let final_path = transfer_dir.join(&safe_filename);

            // Atomic rename
            tokio::fs::rename(&tmp_path, &final_path).await?;

            Ok(final_path)
        }
        Err(e) => {
            // Clean up temp file on error
            let _ = tokio::fs::remove_file(&tmp_path).await;
            Err(e)
        }
    }
}

/// Receive chunks into a temp file, returning the computed Blake3 hash.
async fn receive_chunks_to_file<R>(
    reader: &mut R,
    tmp_path: &Path,
    announce: &FileAnnounce,
    progress_callback: Option<&(dyn Fn(u32, u32, u64) + Send + Sync)>,
) -> Result<String>
where
    R: AsyncRead + Unpin,
{
    let mut hasher = blake3::Hasher::new();
    let mut file_data = Vec::with_capacity(announce.file_size as usize);
    let mut bytes_received: u64 = 0;
    let mut chunks_received: u32 = 0;

    loop {
        let frame = read_file_frame(reader).await?;
        let (msg_type, payload) = match frame {
            Some(f) => f,
            None => return Err(anyhow!("stream closed before transfer complete")),
        };

        match msg_type {
            FileMessageType::Chunk => {
                // Parse chunk: [4-byte header len][header JSON][raw data]
                if payload.len() < 4 {
                    return Err(anyhow!("chunk payload too small"));
                }
                let header_len =
                    u32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]) as usize;
                if payload.len() < 4 + header_len {
                    return Err(anyhow!("chunk payload missing header data"));
                }
                let _header: FileChunkHeader = serde_json::from_slice(&payload[4..4 + header_len])?;
                let chunk_data = &payload[4 + header_len..];

                hasher.update(chunk_data);
                file_data.extend_from_slice(chunk_data);
                bytes_received += chunk_data.len() as u64;
                chunks_received += 1;

                let estimated_total = if announce.file_size > 0 {
                    ((announce.file_size + CHUNK_SIZE as u64 - 1) / CHUNK_SIZE as u64) as u32
                } else {
                    chunks_received
                };

                if let Some(cb) = progress_callback {
                    cb(chunks_received, estimated_total, bytes_received);
                }
            }
            FileMessageType::Complete => {
                let complete: FileComplete = serde_json::from_slice(&payload)?;

                // Write all data to temp file
                tokio::fs::write(tmp_path, &file_data).await?;

                // Set unix permissions
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let perms = std::fs::Permissions::from_mode(0o600);
                    tokio::fs::set_permissions(tmp_path, perms).await?;
                }

                let computed_hash = hasher.finalize().to_hex().to_string();
                debug!(
                    transfer_id = %complete.transfer_id,
                    total_chunks = complete.total_chunks,
                    bytes = bytes_received,
                    "file receive complete"
                );
                return Ok(computed_hash);
            }
            other => {
                warn!("unexpected message type during transfer: {:?}", other);
                return Err(anyhow!("unexpected message type: {:?}", other));
            }
        }
    }
}

/// Sanitize a filename to prevent path traversal.
fn sanitize_filename(name: &str) -> String {
    name.replace("..", "_")
        .replace('/', "_")
        .replace('\\', "_")
        .replace('\0', "_")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_announce_roundtrip() {
        let announce = FileAnnounce {
            transfer_id: "xfer-1".to_string(),
            filename: "test.txt".to_string(),
            file_size: 1024,
            blake3_hash: "abc123".to_string(),
            batch_id: Some("batch-1".to_string()),
            batch_total: Some(3),
        };
        let json = serde_json::to_string(&announce).unwrap();
        let restored: FileAnnounce = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.transfer_id, "xfer-1");
        assert_eq!(restored.filename, "test.txt");
        assert_eq!(restored.file_size, 1024);
        assert_eq!(restored.batch_id, Some("batch-1".to_string()));
    }

    #[test]
    fn file_chunk_header_roundtrip() {
        let header = FileChunkHeader {
            chunk_index: 5,
            chunk_size: 262144,
        };
        let json = serde_json::to_string(&header).unwrap();
        let restored: FileChunkHeader = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.chunk_index, 5);
        assert_eq!(restored.chunk_size, 262144);
    }

    #[test]
    fn file_acceptance_roundtrip() {
        let acceptance = FileAcceptance {
            transfer_id: "xfer-1".to_string(),
            accepted: true,
            reason: None,
        };
        let json = serde_json::to_string(&acceptance).unwrap();
        let restored: FileAcceptance = serde_json::from_str(&json).unwrap();
        assert!(restored.accepted);
        assert!(restored.reason.is_none());

        let rejection = FileAcceptance {
            transfer_id: "xfer-2".to_string(),
            accepted: false,
            reason: Some("Insufficient disk space".to_string()),
        };
        let json = serde_json::to_string(&rejection).unwrap();
        let restored: FileAcceptance = serde_json::from_str(&json).unwrap();
        assert!(!restored.accepted);
        assert_eq!(restored.reason.unwrap(), "Insufficient disk space");
    }

    #[test]
    fn blake3_hash_deterministic() {
        let data = b"hello world";
        let hash1 = blake3::hash(data).to_hex().to_string();
        let hash2 = blake3::hash(data).to_hex().to_string();
        assert_eq!(hash1, hash2);
        assert!(!hash1.is_empty());
    }

    #[test]
    fn sanitize_filename_removes_traversal() {
        assert_eq!(sanitize_filename("../etc/passwd"), "__etc_passwd");
        assert_eq!(sanitize_filename("file..name.txt"), "file_name.txt");
        assert_eq!(sanitize_filename("normal.txt"), "normal.txt");
        assert_eq!(sanitize_filename("path/to\\file"), "path_to_file");
    }

    #[tokio::test]
    async fn chunked_send_receive_roundtrip() {
        let temp_dir = tempfile::tempdir().unwrap();
        let source_path = temp_dir.path().join("source.txt");
        let cache_dir = temp_dir.path().join("cache");

        // Create test file with known content
        let test_data = b"Hello, this is test data for chunked transfer!";
        tokio::fs::write(&source_path, test_data).await.unwrap();

        // Use in-memory duplex stream
        let (client, server) = tokio::io::duplex(64 * 1024);
        let (_client_read, mut client_write) = tokio::io::split(client);
        let (mut server_read, _server_write) = tokio::io::split(server);

        let source_path_clone = source_path.clone();
        let send_handle = tokio::spawn(async move {
            send_file_chunked(
                &mut client_write,
                &source_path_clone,
                "test-xfer",
                None,
                None,
                16, // Small chunk size for testing
                None,
            )
            .await
        });

        let cache_dir_clone = cache_dir.clone();
        let recv_handle = tokio::spawn(async move {
            // First read the announce frame
            let frame = read_file_frame(&mut server_read).await.unwrap().unwrap();
            assert_eq!(frame.0, FileMessageType::Announce);
            let announce: FileAnnounce = serde_json::from_slice(&frame.1).unwrap();
            assert_eq!(announce.transfer_id, "test-xfer");
            assert_eq!(announce.filename, "source.txt");

            // Receive the file
            receive_file_chunked(&mut server_read, &announce, &cache_dir_clone, None).await
        });

        let send_hash = send_handle.await.unwrap().unwrap();
        let final_path = recv_handle.await.unwrap().unwrap();

        // Verify file contents match
        let received_data = tokio::fs::read(&final_path).await.unwrap();
        assert_eq!(received_data, test_data);

        // Verify the final path has the expected name pattern: cache/test-xfer/source.txt
        assert_eq!(
            final_path.file_name().unwrap().to_str().unwrap(),
            "source.txt"
        );
        assert_eq!(
            final_path
                .parent()
                .unwrap()
                .file_name()
                .unwrap()
                .to_str()
                .unwrap(),
            "test-xfer"
        );

        // Verify hash was computed
        assert!(!send_hash.is_empty());
    }

    #[tokio::test]
    async fn hash_mismatch_deletes_temp_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cache_dir = temp_dir.path().join("cache");
        tokio::fs::create_dir_all(&cache_dir).await.unwrap();

        // Create a fake announce with wrong hash
        let announce = FileAnnounce {
            transfer_id: "bad-hash-xfer".to_string(),
            filename: "test.txt".to_string(),
            file_size: 5,
            blake3_hash: "definitely_wrong_hash".to_string(),
            batch_id: None,
            batch_total: None,
        };

        // Build a stream with chunk + complete frames
        let mut stream_data = Vec::new();
        let chunk_data = b"hello";
        let header = FileChunkHeader {
            chunk_index: 0,
            chunk_size: chunk_data.len() as u32,
        };
        let header_bytes = serde_json::to_vec(&header).unwrap();
        let mut chunk_payload = Vec::new();
        chunk_payload.extend_from_slice(&(header_bytes.len() as u32).to_be_bytes());
        chunk_payload.extend_from_slice(&header_bytes);
        chunk_payload.extend_from_slice(chunk_data);
        write_file_frame(&mut stream_data, FileMessageType::Chunk, &chunk_payload)
            .await
            .unwrap();

        let complete = FileComplete {
            transfer_id: "bad-hash-xfer".to_string(),
            blake3_hash: "definitely_wrong_hash".to_string(),
            total_chunks: 1,
        };
        let complete_bytes = serde_json::to_vec(&complete).unwrap();
        write_file_frame(&mut stream_data, FileMessageType::Complete, &complete_bytes)
            .await
            .unwrap();

        let mut cursor = &stream_data[..];
        let result = receive_file_chunked(&mut cursor, &announce, &cache_dir, None).await;

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("hash mismatch"));

        // Verify temp file was cleaned up
        let tmp_path = cache_dir.join("bad-hash-xfer").join("bad-hash-xfer.tmp");
        assert!(!tmp_path.exists());
    }

    #[tokio::test]
    async fn atomic_rename_on_success() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cache_dir = temp_dir.path().join("cache");

        let test_data = b"success data";
        let hash = blake3::hash(test_data).to_hex().to_string();

        let announce = FileAnnounce {
            transfer_id: "rename-xfer".to_string(),
            filename: "result.dat".to_string(),
            file_size: test_data.len() as u64,
            blake3_hash: hash.clone(),
            batch_id: None,
            batch_total: None,
        };

        // Build stream
        let mut stream_data = Vec::new();
        let header = FileChunkHeader {
            chunk_index: 0,
            chunk_size: test_data.len() as u32,
        };
        let header_bytes = serde_json::to_vec(&header).unwrap();
        let mut chunk_payload = Vec::new();
        chunk_payload.extend_from_slice(&(header_bytes.len() as u32).to_be_bytes());
        chunk_payload.extend_from_slice(&header_bytes);
        chunk_payload.extend_from_slice(test_data);
        write_file_frame(&mut stream_data, FileMessageType::Chunk, &chunk_payload)
            .await
            .unwrap();

        let complete = FileComplete {
            transfer_id: "rename-xfer".to_string(),
            blake3_hash: hash,
            total_chunks: 1,
        };
        let complete_bytes = serde_json::to_vec(&complete).unwrap();
        write_file_frame(&mut stream_data, FileMessageType::Complete, &complete_bytes)
            .await
            .unwrap();

        let mut cursor = &stream_data[..];
        let final_path = receive_file_chunked(&mut cursor, &announce, &cache_dir, None)
            .await
            .unwrap();

        // Verify .tmp file does NOT exist
        let tmp_path = cache_dir.join("rename-xfer").join("rename-xfer.tmp");
        assert!(!tmp_path.exists());

        // Verify final file exists with correct name: cache/rename-xfer/result.dat
        assert!(final_path.exists());
        assert_eq!(
            final_path.file_name().unwrap().to_str().unwrap(),
            "result.dat"
        );
        assert_eq!(
            final_path
                .parent()
                .unwrap()
                .file_name()
                .unwrap()
                .to_str()
                .unwrap(),
            "rename-xfer"
        );

        // Verify contents
        let contents = tokio::fs::read(&final_path).await.unwrap();
        assert_eq!(contents, test_data);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn unix_permissions_on_received_file() {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = tempfile::tempdir().unwrap();
        let cache_dir = temp_dir.path().join("cache");

        let test_data = b"permission test data";
        let hash = blake3::hash(test_data).to_hex().to_string();

        let announce = FileAnnounce {
            transfer_id: "perm-xfer".to_string(),
            filename: "secret.dat".to_string(),
            file_size: test_data.len() as u64,
            blake3_hash: hash.clone(),
            batch_id: None,
            batch_total: None,
        };

        let mut stream_data = Vec::new();
        let header = FileChunkHeader {
            chunk_index: 0,
            chunk_size: test_data.len() as u32,
        };
        let header_bytes = serde_json::to_vec(&header).unwrap();
        let mut chunk_payload = Vec::new();
        chunk_payload.extend_from_slice(&(header_bytes.len() as u32).to_be_bytes());
        chunk_payload.extend_from_slice(&header_bytes);
        chunk_payload.extend_from_slice(test_data);
        write_file_frame(&mut stream_data, FileMessageType::Chunk, &chunk_payload)
            .await
            .unwrap();

        let complete = FileComplete {
            transfer_id: "perm-xfer".to_string(),
            blake3_hash: hash,
            total_chunks: 1,
        };
        let complete_bytes = serde_json::to_vec(&complete).unwrap();
        write_file_frame(&mut stream_data, FileMessageType::Complete, &complete_bytes)
            .await
            .unwrap();

        let mut cursor = &stream_data[..];
        let final_path = receive_file_chunked(&mut cursor, &announce, &cache_dir, None)
            .await
            .unwrap();

        // The file was written with 0o600 permissions during receive,
        // then renamed. Verify the final file permissions.
        let metadata = tokio::fs::metadata(&final_path).await.unwrap();
        let mode = metadata.permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "file should have 0600 permissions");
    }

    #[tokio::test]
    async fn test_large_file_multi_chunk_roundtrip() {
        let temp_dir = tempfile::tempdir().unwrap();
        let source_path = temp_dir.path().join("large_source.bin");
        let cache_dir = temp_dir.path().join("cache");

        // Create a 1MB file filled with deterministic pseudo-random data (repeating 0..255)
        let file_size: usize = 1_048_576;
        let test_data: Vec<u8> = (0..file_size).map(|i| (i % 256) as u8).collect();
        tokio::fs::write(&source_path, &test_data).await.unwrap();

        // Compute expected blake3 hash from the source data independently
        let expected_hash = blake3::hash(&test_data).to_hex().to_string();

        // Use duplex stream with buffer large enough for a full chunk + framing overhead
        let (client, server) = tokio::io::duplex(CHUNK_SIZE + 4096);
        let (_client_read, mut client_write) = tokio::io::split(client);
        let (mut server_read, _server_write) = tokio::io::split(server);

        let source_path_clone = source_path.clone();
        let send_handle = tokio::spawn(async move {
            send_file_chunked(
                &mut client_write,
                &source_path_clone,
                "large-xfer",
                Some("batch-large".to_string()),
                Some(1),
                CHUNK_SIZE, // Use the real 256KB chunk size
                None,
            )
            .await
        });

        let cache_dir_clone = cache_dir.clone();
        let recv_handle = tokio::spawn(async move {
            // Read the announce frame first
            let frame = read_file_frame(&mut server_read).await.unwrap().unwrap();
            assert_eq!(frame.0, FileMessageType::Announce);
            let announce: FileAnnounce = serde_json::from_slice(&frame.1).unwrap();
            assert_eq!(announce.transfer_id, "large-xfer");
            assert_eq!(announce.file_size, 1_048_576);
            assert_eq!(announce.filename, "large_source.bin");

            // Receive the file
            receive_file_chunked(&mut server_read, &announce, &cache_dir_clone, None).await
        });

        let send_hash = send_handle.await.unwrap().unwrap();
        let final_path = recv_handle.await.unwrap().unwrap();

        // Verify received file is exactly 1MB
        let received_data = tokio::fs::read(&final_path).await.unwrap();
        assert_eq!(received_data.len(), file_size);

        // Verify file content matches byte-for-byte
        assert_eq!(received_data, test_data);

        // Verify blake3 hashes match (sender hash, independent hash, and received file hash)
        let received_hash = blake3::hash(&received_data).to_hex().to_string();
        assert_eq!(send_hash, expected_hash);
        assert_eq!(received_hash, expected_hash);

        // Verify send_hash is non-empty
        assert!(!send_hash.is_empty());
    }
}
