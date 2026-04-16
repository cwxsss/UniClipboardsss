//! Chunked file transfer protocol.
//!
//! Handles the announce/accept/chunk/complete message flow for file transfers
//! with incremental Blake3 hash computation and verification.

use super::framing::{read_file_frame, write_file_frame, FileMessageType};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::time::Instant;
use tracing::{debug, info, info_span, instrument, warn, Instrument};

/// Default chunk size: 1MB.
pub const CHUNK_SIZE: usize = 1024 * 1024;
const STREAMING_CHUNK_HEADER_BYTES: usize = 8;

/// File transfer announcement sent by the sender.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileAnnounce {
    pub transfer_id: String,
    pub filename: String,
    pub file_size: u64,
    pub chunk_size: u32,
    pub blake3_hash: String,
    pub batch_id: Option<String>,
    pub batch_total: Option<u32>,
}

/// Streaming file transfer announcement for protocol v2.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileAnnounceV2 {
    pub transfer_id: String,
    pub filename: String,
    pub file_size: u64,
    pub chunk_size: u32,
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

#[derive(Debug, Clone, Copy, Default)]
pub struct LegacyFileTransferProtocol;

#[derive(Debug, Clone, Copy, Default)]
pub struct StreamingFileTransferProtocol;

#[derive(Debug, Clone, Copy, Default)]
struct ChunkTransferEngine;

#[derive(Debug, Clone, Copy)]
enum ChunkEncoding {
    LegacyJsonHeader,
    StreamingBinaryHeader,
}

struct ReceivedHash {
    computed_hash: String,
    completed_hash: String,
}

/// Compute Blake3 hash of a file.
async fn compute_blake3_hash(file_path: &Path) -> Result<String> {
    let mut file = tokio::fs::File::open(file_path).await?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0u8; CHUNK_SIZE];

    loop {
        let bytes_read = file.read(&mut buffer).await?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    Ok(hasher.finalize().to_hex().to_string())
}

impl LegacyFileTransferProtocol {
    pub async fn send_file<W>(
        &self,
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
        let hash = compute_blake3_hash(file_path).await?;
        let (mut file, file_size, filename) = open_file_transfer_source(file_path).await?;

        let announce = FileAnnounce {
            transfer_id: transfer_id.to_string(),
            filename,
            file_size,
            chunk_size: chunk_size as u32,
            blake3_hash: hash.clone(),
            batch_id,
            batch_total,
        };
        let announce_bytes = serde_json::to_vec(&announce)?;
        write_file_frame(writer, FileMessageType::Announce, &announce_bytes).await?;

        let streamed_hash = ChunkTransferEngine::send_stream(
            writer,
            &mut file,
            &announce.transfer_id,
            announce.file_size,
            chunk_size,
            ChunkEncoding::LegacyJsonHeader,
            progress_callback,
        )
        .await?;

        debug_assert_eq!(streamed_hash, hash);
        Ok(streamed_hash)
    }

    pub async fn receive_file<R>(
        &self,
        reader: &mut R,
        announce: &FileAnnounce,
        cache_dir: &Path,
        progress_callback: Option<&(dyn Fn(u32, u32, u64) + Send + Sync)>,
    ) -> Result<PathBuf>
    where
        R: AsyncRead + Unpin,
    {
        ChunkTransferEngine::receive_file(
            reader,
            &announce.transfer_id,
            &announce.filename,
            announce.file_size,
            announce.chunk_size,
            Some(&announce.blake3_hash),
            cache_dir,
            ChunkEncoding::LegacyJsonHeader,
            progress_callback,
        )
        .await
    }
}

impl StreamingFileTransferProtocol {
    pub async fn send_file<W>(
        &self,
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
        let (mut file, file_size, filename) = open_file_transfer_source(file_path).await?;

        let announce = FileAnnounceV2 {
            transfer_id: transfer_id.to_string(),
            filename,
            file_size,
            chunk_size: chunk_size as u32,
            batch_id,
            batch_total,
        };
        let announce_bytes = serde_json::to_vec(&announce)?;
        write_file_frame(writer, FileMessageType::Announce, &announce_bytes).await?;

        ChunkTransferEngine::send_stream(
            writer,
            &mut file,
            &announce.transfer_id,
            announce.file_size,
            chunk_size,
            ChunkEncoding::StreamingBinaryHeader,
            progress_callback,
        )
        .await
    }

    pub async fn receive_file<R>(
        &self,
        reader: &mut R,
        announce: &FileAnnounceV2,
        cache_dir: &Path,
        progress_callback: Option<&(dyn Fn(u32, u32, u64) + Send + Sync)>,
    ) -> Result<PathBuf>
    where
        R: AsyncRead + Unpin,
    {
        ChunkTransferEngine::receive_file(
            reader,
            &announce.transfer_id,
            &announce.filename,
            announce.file_size,
            announce.chunk_size,
            None,
            cache_dir,
            ChunkEncoding::StreamingBinaryHeader,
            progress_callback,
        )
        .await
    }
}

impl ChunkTransferEngine {
    #[instrument(
        name = "file_transfer.send_stream",
        level = "info",
        skip(writer, reader, progress_callback),
        fields(
            transfer_id = %transfer_id,
            file_size,
            chunk_size
        )
    )]
    async fn send_stream<W, R>(
        writer: &mut W,
        reader: &mut R,
        transfer_id: &str,
        file_size: u64,
        chunk_size: usize,
        chunk_encoding: ChunkEncoding,
        progress_callback: Option<&(dyn Fn(u32, u32, u64) + Send + Sync)>,
    ) -> Result<String>
    where
        W: AsyncWrite + Unpin,
        R: AsyncRead + Unpin,
    {
        let total_chunks = if file_size == 0 {
            0
        } else {
            file_size.div_ceil(chunk_size as u64) as u32
        };
        let mut bytes_sent: u64 = 0;
        let mut chunk_index: u32 = 0;
        let mut hasher = blake3::Hasher::new();
        let mut buffer = vec![0u8; chunk_size];
        let started_at = Instant::now();
        let mut file_read_elapsed_ns: u128 = 0;
        let mut network_write_elapsed_ns: u128 = 0;
        let mut complete_frame_elapsed_ns: u128 = 0;
        let mut flush_elapsed_ns: u128 = 0;

        loop {
            let read_started_at = Instant::now();
            let bytes_read = reader.read(&mut buffer).await?;
            file_read_elapsed_ns += read_started_at.elapsed().as_nanos();
            if bytes_read == 0 {
                break;
            }

            let chunk_data = &buffer[..bytes_read];
            hasher.update(chunk_data);
            let write_started_at = Instant::now();
            Self::write_chunk_frame(
                writer,
                chunk_encoding,
                chunk_index,
                bytes_read as u32,
                chunk_data,
            )
            .await?;
            network_write_elapsed_ns += write_started_at.elapsed().as_nanos();

            bytes_sent += bytes_read as u64;
            chunk_index += 1;
            if let Some(cb) = progress_callback {
                cb(chunk_index, total_chunks.max(chunk_index), bytes_sent);
            }
        }

        let hash = hasher.finalize().to_hex().to_string();
        let complete = FileComplete {
            transfer_id: transfer_id.to_string(),
            blake3_hash: hash.clone(),
            total_chunks: chunk_index,
        };
        let complete_bytes = serde_json::to_vec(&complete)?;
        let complete_started_at = Instant::now();
        write_file_frame(writer, FileMessageType::Complete, &complete_bytes).await?;
        complete_frame_elapsed_ns += complete_started_at.elapsed().as_nanos();
        let flush_started_at = Instant::now();
        writer.flush().await?;
        flush_elapsed_ns += flush_started_at.elapsed().as_nanos();

        let elapsed_ms = started_at.elapsed().as_millis() as u64;
        info!(
            transfer_id = %transfer_id,
            file_size,
            total_chunks = chunk_index,
            chunk_size,
            elapsed_ms,
            file_read_elapsed_ms = nanos_to_ms(file_read_elapsed_ns),
            network_write_elapsed_ms = nanos_to_ms(network_write_elapsed_ns),
            complete_frame_elapsed_ms = nanos_to_ms(complete_frame_elapsed_ns),
            flush_elapsed_ms = nanos_to_ms(flush_elapsed_ns),
            avg_mbps = average_mbps(bytes_sent, elapsed_ms),
            "file transfer stream sent"
        );

        Ok(hash)
    }

    async fn receive_file<R>(
        reader: &mut R,
        transfer_id: &str,
        filename: &str,
        file_size: u64,
        announced_chunk_size: u32,
        announce_hash: Option<&str>,
        cache_dir: &Path,
        chunk_encoding: ChunkEncoding,
        progress_callback: Option<&(dyn Fn(u32, u32, u64) + Send + Sync)>,
    ) -> Result<PathBuf>
    where
        R: AsyncRead + Unpin,
    {
        let transfer_dir = cache_dir.join(transfer_id);
        tokio::fs::create_dir_all(&transfer_dir).await?;
        let tmp_path = transfer_dir.join(format!("{}.tmp", transfer_id));

        let result = Self::receive_chunks_to_file(
            reader,
            &tmp_path,
            transfer_id,
            file_size,
            announced_chunk_size,
            chunk_encoding,
            progress_callback,
        )
        .instrument(info_span!("receive_chunks", transfer_id = %transfer_id))
        .await;

        match result {
            Ok(received_hash) => {
                if let Some(expected_hash) = announce_hash {
                    if received_hash.completed_hash != expected_hash {
                        let _ = tokio::fs::remove_file(&tmp_path).await;
                        return Err(anyhow!(
                            "blake3 hash mismatch: expected {}, got {}",
                            expected_hash,
                            received_hash.completed_hash
                        ));
                    }
                }

                if received_hash.computed_hash != received_hash.completed_hash {
                    let _ = tokio::fs::remove_file(&tmp_path).await;
                    return Err(anyhow!(
                        "blake3 hash mismatch: expected {}, got {}",
                        received_hash.completed_hash,
                        received_hash.computed_hash
                    ));
                }

                let safe_filename = sanitize_filename(filename);
                let final_path = transfer_dir.join(&safe_filename);
                tokio::fs::rename(&tmp_path, &final_path).await?;
                Ok(final_path)
            }
            Err(e) => {
                let _ = tokio::fs::remove_file(&tmp_path).await;
                Err(e)
            }
        }
    }

    #[instrument(
        name = "file_transfer.receive_chunks",
        level = "info",
        skip(reader, progress_callback),
        fields(
            transfer_id = %transfer_id,
            file_size,
            tmp_path = %tmp_path.display()
        )
    )]
    async fn receive_chunks_to_file<R>(
        reader: &mut R,
        tmp_path: &Path,
        transfer_id: &str,
        file_size: u64,
        announced_chunk_size: u32,
        chunk_encoding: ChunkEncoding,
        progress_callback: Option<&(dyn Fn(u32, u32, u64) + Send + Sync)>,
    ) -> Result<ReceivedHash>
    where
        R: AsyncRead + Unpin,
    {
        let mut hasher = blake3::Hasher::new();
        let mut file = tokio::fs::File::create(tmp_path).await?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            tokio::fs::set_permissions(tmp_path, perms).await?;
        }

        let mut bytes_received: u64 = 0;
        let mut chunks_received: u32 = 0;
        let started_at = Instant::now();
        let mut frame_read_elapsed_ns: u128 = 0;
        let mut file_write_elapsed_ns: u128 = 0;
        let mut finalize_sync_elapsed_ns: u128 = 0;

        loop {
            let read_started_at = Instant::now();
            let frame = read_file_frame(reader).await?;
            frame_read_elapsed_ns += read_started_at.elapsed().as_nanos();
            let (msg_type, payload) = match frame {
                Some(f) => f,
                None => return Err(anyhow!("stream closed before transfer complete")),
            };

            match msg_type {
                FileMessageType::Chunk => {
                    let (_chunk_index, chunk_data) =
                        Self::decode_chunk_payload(chunk_encoding, &payload)?;

                    hasher.update(chunk_data);
                    let write_started_at = Instant::now();
                    file.write_all(chunk_data).await?;
                    file_write_elapsed_ns += write_started_at.elapsed().as_nanos();
                    bytes_received += chunk_data.len() as u64;
                    chunks_received += 1;

                    let estimated_total = if file_size > 0 && announced_chunk_size > 0 {
                        file_size.div_ceil(announced_chunk_size as u64) as u32
                    } else {
                        chunks_received
                    };

                    if let Some(cb) = progress_callback {
                        cb(chunks_received, estimated_total, bytes_received);
                    }
                }
                FileMessageType::Complete => {
                    let complete: FileComplete = serde_json::from_slice(&payload)?;
                    let finalize_started_at = Instant::now();
                    file.flush().await?;
                    file.sync_data().await?;
                    finalize_sync_elapsed_ns += finalize_started_at.elapsed().as_nanos();

                    let elapsed_ms = started_at.elapsed().as_millis() as u64;
                    let computed_hash = hasher.finalize().to_hex().to_string();
                    info!(
                        transfer_id = %transfer_id,
                        file_size,
                        bytes_received,
                        total_chunks = complete.total_chunks,
                        chunks_received,
                        elapsed_ms,
                        frame_read_elapsed_ms = nanos_to_ms(frame_read_elapsed_ns),
                        file_write_elapsed_ms = nanos_to_ms(file_write_elapsed_ns),
                        finalize_sync_elapsed_ms = nanos_to_ms(finalize_sync_elapsed_ns),
                        avg_mbps = average_mbps(bytes_received, elapsed_ms),
                        "file transfer stream received"
                    );
                    debug!(
                        transfer_id = %transfer_id,
                        total_chunks = complete.total_chunks,
                        bytes = bytes_received,
                        "file receive complete"
                    );
                    return Ok(ReceivedHash {
                        computed_hash,
                        completed_hash: complete.blake3_hash,
                    });
                }
                other => {
                    warn!("unexpected message type during transfer: {:?}", other);
                    return Err(anyhow!("unexpected message type: {:?}", other));
                }
            }
        }
    }

    async fn write_chunk_frame<W>(
        writer: &mut W,
        chunk_encoding: ChunkEncoding,
        chunk_index: u32,
        chunk_size: u32,
        chunk_data: &[u8],
    ) -> Result<()>
    where
        W: AsyncWrite + Unpin,
    {
        match chunk_encoding {
            ChunkEncoding::LegacyJsonHeader => {
                let header = FileChunkHeader {
                    chunk_index,
                    chunk_size,
                };
                let header_bytes = serde_json::to_vec(&header)?;
                let mut payload = Vec::with_capacity(4 + header_bytes.len() + chunk_data.len());
                payload.extend_from_slice(&(header_bytes.len() as u32).to_be_bytes());
                payload.extend_from_slice(&header_bytes);
                payload.extend_from_slice(chunk_data);
                write_file_frame(writer, FileMessageType::Chunk, &payload).await
            }
            ChunkEncoding::StreamingBinaryHeader => {
                let payload_len = STREAMING_CHUNK_HEADER_BYTES + chunk_data.len();
                if payload_len > super::framing::MAX_FILE_FRAME_BYTES {
                    return Err(anyhow!(
                        "file frame payload too large: {} > {}",
                        payload_len,
                        super::framing::MAX_FILE_FRAME_BYTES
                    ));
                }

                writer.write_all(&[FileMessageType::Chunk as u8]).await?;
                writer
                    .write_all(&(payload_len as u32).to_be_bytes())
                    .await?;
                writer.write_all(&chunk_index.to_be_bytes()).await?;
                writer.write_all(&chunk_size.to_be_bytes()).await?;
                writer.write_all(chunk_data).await?;
                Ok(())
            }
        }
    }

    fn decode_chunk_payload<'a>(
        chunk_encoding: ChunkEncoding,
        payload: &'a [u8],
    ) -> Result<(u32, &'a [u8])> {
        match chunk_encoding {
            ChunkEncoding::LegacyJsonHeader => {
                if payload.len() < 4 {
                    return Err(anyhow!("chunk payload too small"));
                }
                let header_len =
                    u32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]) as usize;
                if payload.len() < 4 + header_len {
                    return Err(anyhow!("chunk payload missing header data"));
                }
                let header: FileChunkHeader = serde_json::from_slice(&payload[4..4 + header_len])?;
                let chunk_data = &payload[4 + header_len..];
                if chunk_data.len() != header.chunk_size as usize {
                    return Err(anyhow!(
                        "chunk payload size mismatch: expected {}, got {}",
                        header.chunk_size,
                        chunk_data.len()
                    ));
                }
                Ok((header.chunk_index, chunk_data))
            }
            ChunkEncoding::StreamingBinaryHeader => {
                if payload.len() < STREAMING_CHUNK_HEADER_BYTES {
                    return Err(anyhow!("streaming chunk payload too small"));
                }
                let chunk_index =
                    u32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]);
                let chunk_size =
                    u32::from_be_bytes([payload[4], payload[5], payload[6], payload[7]]) as usize;
                let chunk_data = &payload[STREAMING_CHUNK_HEADER_BYTES..];
                if chunk_data.len() != chunk_size {
                    return Err(anyhow!(
                        "streaming chunk payload size mismatch: expected {}, got {}",
                        chunk_size,
                        chunk_data.len()
                    ));
                }
                Ok((chunk_index, chunk_data))
            }
        }
    }
}

async fn open_file_transfer_source(file_path: &Path) -> Result<(tokio::fs::File, u64, String)> {
    let file = tokio::fs::File::open(file_path)
        .await
        .map_err(|e| anyhow!("failed to open file for transfer: {}", e))?;
    let file_size = file
        .metadata()
        .await
        .map_err(|e| anyhow!("failed to read file metadata for transfer: {}", e))?
        .len();
    let filename = file_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();
    Ok((file, file_size, filename))
}

/// Sanitize a filename to prevent path traversal.
fn sanitize_filename(name: &str) -> String {
    name.replace("..", "_")
        .replace('/', "_")
        .replace('\\', "_")
        .replace('\0', "_")
}

fn average_mbps(bytes: u64, elapsed_ms: u64) -> f64 {
    if elapsed_ms == 0 {
        return 0.0;
    }

    let bits = (bytes as f64) * 8.0;
    let seconds = (elapsed_ms as f64) / 1000.0;
    bits / seconds / 1_000_000.0
}

fn nanos_to_ms(elapsed_ns: u128) -> u64 {
    (elapsed_ns / 1_000_000) as u64
}
