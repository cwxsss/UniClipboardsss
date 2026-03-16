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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;
    use uc_core::settings::model::Settings;

    struct MockSettings {
        settings: Settings,
    }

    #[async_trait::async_trait]
    impl SettingsPort for MockSettings {
        async fn load(&self) -> anyhow::Result<Settings> {
            Ok(self.settings.clone())
        }
        async fn save(&self, _settings: &Settings) -> anyhow::Result<()> {
            Ok(())
        }
    }

    fn make_use_case(
        cache_dir: PathBuf,
        small_threshold: u64,
        quota: u64,
    ) -> SyncInboundFileUseCase {
        let mut settings = Settings::default();
        settings.file_sync.small_file_threshold = small_threshold;
        settings.file_sync.file_cache_quota_per_device = quota;

        SyncInboundFileUseCase::new(Arc::new(MockSettings { settings }), cache_dir)
    }

    fn make_use_case_disabled(cache_dir: PathBuf) -> SyncInboundFileUseCase {
        let mut settings = Settings::default();
        settings.file_sync.file_sync_enabled = false;

        SyncInboundFileUseCase::new(Arc::new(MockSettings { settings }), cache_dir)
    }

    #[tokio::test]
    async fn test_should_auto_pull_small_file() {
        let tmp = TempDir::new().unwrap();
        let uc = make_use_case(
            tmp.path().to_path_buf(),
            10 * 1024 * 1024,
            500 * 1024 * 1024,
        );
        assert!(uc.should_auto_pull(1024).await.unwrap());
    }

    #[tokio::test]
    async fn test_should_auto_pull_large_file() {
        let tmp = TempDir::new().unwrap();
        let uc = make_use_case(
            tmp.path().to_path_buf(),
            10 * 1024 * 1024,
            500 * 1024 * 1024,
        );
        // 20MB > 10MB threshold
        assert!(!uc.should_auto_pull(20 * 1024 * 1024).await.unwrap());
    }

    #[tokio::test]
    async fn test_check_quota_within_limit() {
        let tmp = TempDir::new().unwrap();
        let uc = make_use_case(tmp.path().to_path_buf(), 10_000_000, 500 * 1024 * 1024);
        // No existing files, adding 100MB should be fine
        assert!(uc.check_quota("peer-1", 100 * 1024 * 1024).await.unwrap());
    }

    #[tokio::test]
    async fn test_check_quota_exceeded() {
        let tmp = TempDir::new().unwrap();
        // Create peer cache dir with existing files totaling ~400MB
        let peer_dir = tmp.path().join("peer-1");
        std::fs::create_dir_all(&peer_dir).unwrap();

        // Write a file of ~400MB worth of size (we'll use a small file but pretend via quota)
        // Actually, let's use a small quota for testability
        let uc = make_use_case(tmp.path().to_path_buf(), 10_000_000, 1000); // 1KB quota

        // Write a 500 byte file
        let mut f = std::fs::File::create(peer_dir.join("existing.bin")).unwrap();
        f.write_all(&[0u8; 500]).unwrap();

        // Adding 600 bytes exceeds 1000 byte quota
        assert!(!uc.check_quota("peer-1", 600).await.unwrap());
    }

    #[tokio::test]
    async fn test_hash_verification_success() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("test_file.bin");
        let content = b"hello world file content";
        tokio::fs::write(&file_path, content).await.unwrap();

        let expected_hash = blake3::hash(content).to_hex().to_string();

        let uc = make_use_case(tmp.path().to_path_buf(), 10_000_000, 500_000_000);
        let result = uc
            .handle_transfer_complete("xfer-1", &file_path, &expected_hash)
            .await
            .unwrap();

        assert_eq!(result.transfer_id, "xfer-1");
        assert_eq!(result.file_size, content.len() as u64);
        // File should still exist
        assert!(file_path.exists());
    }

    #[tokio::test]
    async fn test_hash_verification_failure_deletes_file() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("bad_file.bin");
        tokio::fs::write(&file_path, b"actual content")
            .await
            .unwrap();

        let wrong_hash = "0000000000000000000000000000000000000000000000000000000000000000";

        let uc = make_use_case(tmp.path().to_path_buf(), 10_000_000, 500_000_000);
        let result = uc
            .handle_transfer_complete("xfer-2", &file_path, wrong_hash)
            .await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Hash verification failed"));
        // File should have been deleted
        assert!(!file_path.exists());
    }

    #[tokio::test]
    async fn test_is_file_sync_enabled_true() {
        let tmp = TempDir::new().unwrap();
        let uc = make_use_case(tmp.path().to_path_buf(), 10_000_000, 500_000_000);
        assert!(uc.is_file_sync_enabled().await.unwrap());
    }

    #[tokio::test]
    async fn test_is_file_sync_enabled_false() {
        let tmp = TempDir::new().unwrap();
        let uc = make_use_case_disabled(tmp.path().to_path_buf());
        assert!(!uc.is_file_sync_enabled().await.unwrap());
    }

    #[tokio::test]
    async fn test_handle_transfer_complete_rejects_when_disabled() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("received_file.bin");
        tokio::fs::write(&file_path, b"file content").await.unwrap();

        let uc = make_use_case_disabled(tmp.path().to_path_buf());
        let result = uc
            .handle_transfer_complete("xfer-disabled", &file_path, "somehash")
            .await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("File sync is disabled"));
        // Temp file should be cleaned up
        assert!(!file_path.exists());
    }

    #[tokio::test]
    async fn test_check_quota_for_transfer_within_limit() {
        let tmp = TempDir::new().unwrap();
        let uc = make_use_case(tmp.path().to_path_buf(), 10_000_000, 500_000_000);
        assert!(uc
            .check_quota_for_transfer("peer-1", 100_000_000)
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn test_check_quota_for_transfer_exceeded() {
        let tmp = TempDir::new().unwrap();
        let peer_dir = tmp.path().join("peer-1");
        std::fs::create_dir_all(&peer_dir).unwrap();

        {
            let mut f = std::fs::File::create(peer_dir.join("existing.bin")).unwrap();
            std::io::Write::write_all(&mut f, &[0u8; 900]).unwrap();
        }

        let uc = make_use_case(tmp.path().to_path_buf(), 10_000_000, 1000); // 1KB quota
        let result = uc.check_quota_for_transfer("peer-1", 200).await;
        assert!(result.is_err());
    }
}
