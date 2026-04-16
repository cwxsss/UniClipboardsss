use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use tracing::{info, info_span, warn, Instrument};

use uc_core::ports::SettingsPort;

use super::cleanup::{check_device_quota, QuotaExceededError};

/// Standardized error messages for the file transfer pipeline.
pub mod transfer_errors {
    /// File sync is disabled on the local device.
    pub const FILE_SYNC_DISABLED: &str = "File sync is disabled on this device";

    /// Format a quota exceeded message.
    pub fn quota_exceeded(device_id: &str) -> String {
        format!("Cache quota exceeded on device {}", device_id)
    }

    /// Format a file exceeds max size message.
    pub fn file_exceeds_max_size(filename: &str, file_size_mb: u64, max_size_mb: u64) -> String {
        format!(
            "File {} ({} MB) exceeds maximum size limit ({} MB)",
            filename, file_size_mb, max_size_mb
        )
    }

    /// Format a transfer failed message.
    pub fn transfer_failed(filename: &str, reason: &str) -> String {
        format!("Transfer failed for {}: {}", filename, reason)
    }
}

/// Result of a completed inbound file transfer.
#[derive(Debug)]
pub struct InboundFileResult {
    pub transfer_id: String,
    pub file_path: PathBuf,
    pub file_size: u64,
    pub auto_pulled: bool,
}

pub struct SyncInboundFileUseCase {
    settings: Arc<dyn SettingsPort>,
    cache_dir: PathBuf,
}

impl SyncInboundFileUseCase {
    pub fn new(settings: Arc<dyn SettingsPort>, cache_dir: PathBuf) -> Self {
        Self {
            settings,
            cache_dir,
        }
    }

    /// Check if file sync is enabled in settings.
    ///
    /// Returns false when the user has disabled file sync, in which case
    /// incoming transfers should be rejected.
    pub async fn is_file_sync_enabled(&self) -> Result<bool> {
        let settings = self
            .settings
            .load()
            .await
            .context("Failed to load settings")?;
        Ok(settings.file_sync.file_sync_enabled)
    }

    /// Check if accepting a file from a peer would exceed the per-device quota.
    ///
    /// Delegates to the cleanup module's `check_device_quota` function which
    /// uses filesystem-based cache size calculation.
    pub async fn check_quota_for_transfer(
        &self,
        source_device_id: &str,
        incoming_file_size: u64,
    ) -> std::result::Result<(), QuotaExceededError> {
        check_device_quota(
            self.settings.as_ref(),
            &self.cache_dir,
            source_device_id,
            incoming_file_size,
        )
        .await
    }

    /// Check if a file should be auto-pulled based on its size.
    ///
    /// Returns true if the file size is below the small_file_threshold from settings.
    pub async fn should_auto_pull(&self, file_size: u64) -> Result<bool> {
        let settings = self
            .settings
            .load()
            .await
            .context("Failed to load settings")?;
        Ok(file_size <= settings.file_sync.small_file_threshold)
    }

    /// Check if there is enough disk space for the transfer.
    ///
    /// Returns true if available space >= required_bytes + 10MB buffer.
    pub fn check_disk_space(&self, required_bytes: u64) -> Result<bool> {
        let buffer = 10 * 1024 * 1024; // 10MB buffer
        let required_with_buffer = required_bytes.saturating_add(buffer);

        // Use fs2 or platform-specific API for disk space. For portability,
        // we use the cache_dir's filesystem stats.
        #[cfg(unix)]
        {
            let available = fs_available_space(&self.cache_dir)?;
            Ok(available >= required_with_buffer)
        }
        #[cfg(not(unix))]
        {
            // On non-Unix, we optimistically return true.
            // A more robust implementation would use platform-specific APIs.
            let _ = required_with_buffer;
            Ok(true)
        }
    }

    /// Check if adding a file would exceed the per-device cache quota.
    ///
    /// Default quota is 500MB per device (configurable via settings).
    pub async fn check_quota(&self, peer_id: &str, additional_bytes: u64) -> Result<bool> {
        let settings = self
            .settings
            .load()
            .await
            .context("Failed to load settings")?;
        let quota = settings.file_sync.file_cache_quota_per_device;

        // Calculate current usage for this peer
        let peer_cache_dir = self.cache_dir.join(peer_id);
        let current_usage = if peer_cache_dir.exists() {
            dir_size(&peer_cache_dir).unwrap_or(0)
        } else {
            0
        };

        Ok(current_usage.saturating_add(additional_bytes) <= quota)
    }

    /// Handle a completed file transfer by verifying its Blake3 hash.
    ///
    /// If the hash matches, returns success with file metadata.
    /// If the hash does not match, deletes the file and returns an error (no retry).
    pub async fn handle_transfer_complete(
        &self,
        transfer_id: &str,
        file_path: &Path,
        expected_hash: &str,
    ) -> Result<InboundFileResult> {
        async move {
            // Guard: file_sync_enabled
            let settings = self
                .settings
                .load()
                .await
                .context("Failed to load settings")?;

            if !settings.file_sync.file_sync_enabled {
                info!(
                    transfer_id = %transfer_id,
                    "File sync disabled, cleaning up received file"
                );
                // Clean up the already-transferred temp file
                cleanup_temp_file(file_path, transfer_id).await;
                bail!("{}", transfer_errors::FILE_SYNC_DISABLED);
            }

            // Read file and compute Blake3 hash
            let file_bytes = tokio::fs::read(file_path).await.with_context(|| {
                format!("Failed to read transferred file: {}", file_path.display())
            })?;

            let actual_hash = blake3::hash(&file_bytes).to_hex().to_string();

            if actual_hash != expected_hash {
                // Hash mismatch -- delete the file and fail (no retry)
                warn!(
                    transfer_id = %transfer_id,
                    expected = %expected_hash,
                    actual = %actual_hash,
                    "Hash verification failed; deleting temp file"
                );
                cleanup_temp_file(file_path, transfer_id).await;
                bail!(
                    "Hash verification failed for transfer {}: expected {}, got {}",
                    transfer_id,
                    expected_hash,
                    actual_hash
                );
            }

            let file_size = file_bytes.len() as u64;

            // Check auto-pull eligibility
            let auto_pulled = self.should_auto_pull(file_size).await.unwrap_or(false);

            info!(
                transfer_id = %transfer_id,
                file_size = file_size,
                auto_pulled = auto_pulled,
                "File transfer complete and verified"
            );

            Ok(InboundFileResult {
                transfer_id: transfer_id.to_string(),
                file_path: file_path.to_path_buf(),
                file_size,
                auto_pulled,
            })
        }
        .instrument(info_span!(
            "usecase.file_sync.sync_inbound.handle_transfer_complete",
            transfer_id = %transfer_id,
        ))
        .await
    }
}

/// Clean up a temp file on failure, logging any cleanup errors.
async fn cleanup_temp_file(path: &Path, transfer_id: &str) {
    if let Err(err) = tokio::fs::remove_file(path).await {
        if err.kind() != std::io::ErrorKind::NotFound {
            warn!(
                transfer_id = %transfer_id,
                path = %path.display(),
                error = %err,
                "Failed to clean up temp file after error"
            );
        }
    }
}

/// Calculate total size of files in a directory (non-recursive for peer cache).
fn dir_size(path: &Path) -> Result<u64> {
    let mut total = 0u64;
    if path.is_dir() {
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let meta = entry.metadata()?;
            if meta.is_file() {
                total += meta.len();
            } else if meta.is_dir() {
                total += dir_size(&entry.path()).unwrap_or(0);
            }
        }
    }
    Ok(total)
}

/// Get available disk space for a path (Unix only).
#[cfg(unix)]
fn fs_available_space(path: &Path) -> Result<u64> {
    use std::ffi::CString;
    use std::mem::MaybeUninit;

    let path_cstr = CString::new(
        path.to_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid path for statvfs"))?,
    )?;

    let mut stat = MaybeUninit::<libc::statvfs>::uninit();
    let ret = unsafe { libc::statvfs(path_cstr.as_ptr(), stat.as_mut_ptr()) };
    if ret != 0 {
        bail!("statvfs failed for {}", path.display());
    }
    let stat = unsafe { stat.assume_init() };
    Ok(stat.f_bavail as u64 * stat.f_frsize as u64)
}
