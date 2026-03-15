use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use tracing::{info, info_span, warn, Instrument};

use uc_core::ports::SettingsPort;

/// Result of a cleanup operation, reporting counts and bytes reclaimed.
#[derive(Debug, Default)]
pub struct CleanupResult {
    pub files_removed: u32,
    pub bytes_reclaimed: u64,
    pub errors: u32,
}

/// Use case: Clean up expired file cache entries.
///
/// Runs on app startup to:
/// 1. Walk the file-cache directory tree
/// 2. Remove files older than the configured retention period
/// 3. Log summary: files removed, space reclaimed
///
/// This operates on the filesystem directly (no DB repository needed)
/// since the file-cache directory structure is the source of truth
/// for cached transfer files.
pub struct CleanupExpiredFilesUseCase {
    settings: Arc<dyn SettingsPort>,
    file_cache_dir: PathBuf,
}

impl CleanupExpiredFilesUseCase {
    pub fn new(settings: Arc<dyn SettingsPort>, file_cache_dir: PathBuf) -> Self {
        Self {
            settings,
            file_cache_dir,
        }
    }

    pub async fn execute(&self) -> Result<CleanupResult> {
        let span = info_span!("usecase.cleanup_expired_files.execute");
        async {
            let settings = self.settings.load().await?;

            if !settings.file_sync.file_auto_cleanup {
                info!("File auto-cleanup disabled, skipping");
                return Ok(CleanupResult::default());
            }

            let retention_hours = settings.file_sync.file_retention_hours;
            let retention_secs = retention_hours as u64 * 3600;
            let now = std::time::SystemTime::now();

            if !self.file_cache_dir.exists() {
                info!(
                    path = %self.file_cache_dir.display(),
                    "File cache directory does not exist, nothing to clean"
                );
                return Ok(CleanupResult::default());
            }

            // Collect files to remove (filesystem walk)
            let expired_files = collect_expired_files(&self.file_cache_dir, now, retention_secs)?;

            if expired_files.is_empty() {
                info!("No expired cache files to clean up");
                return Ok(CleanupResult::default());
            }

            let mut files_removed = 0u32;
            let mut bytes_reclaimed = 0u64;
            let mut errors = 0u32;

            for (path, size) in &expired_files {
                match tokio::fs::remove_file(path).await {
                    Ok(()) => {
                        files_removed += 1;
                        bytes_reclaimed += size;
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        // File already gone, count as removed
                        files_removed += 1;
                    }
                    Err(e) => {
                        warn!(
                            path = %path.display(),
                            error = %e,
                            "Failed to delete expired cache file"
                        );
                        errors += 1;
                    }
                }
            }

            // Clean up empty peer directories after file removal
            cleanup_empty_dirs(&self.file_cache_dir).await;

            let result = CleanupResult {
                files_removed,
                bytes_reclaimed,
                errors,
            };
            info!(
                files_removed = result.files_removed,
                bytes_reclaimed_mb = result.bytes_reclaimed / (1024 * 1024),
                errors = result.errors,
                "File cache cleanup complete"
            );

            Ok(result)
        }
        .instrument(span)
        .await
    }
}

/// Collect files in the cache directory that are older than the retention period.
///
/// Returns a list of (path, file_size) tuples for expired files.
fn collect_expired_files(
    cache_dir: &Path,
    now: std::time::SystemTime,
    retention_secs: u64,
) -> Result<Vec<(PathBuf, u64)>> {
    let mut expired = Vec::new();
    collect_expired_recursive(cache_dir, now, retention_secs, &mut expired)?;
    Ok(expired)
}

fn collect_expired_recursive(
    dir: &Path,
    now: std::time::SystemTime,
    retention_secs: u64,
    out: &mut Vec<(PathBuf, u64)>,
) -> Result<()> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            warn!(
                path = %dir.display(),
                error = %e,
                "Failed to read cache directory"
            );
            return Ok(());
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                warn!(error = %e, "Failed to read directory entry");
                continue;
            }
        };

        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(e) => {
                warn!(
                    path = %entry.path().display(),
                    error = %e,
                    "Failed to read file metadata"
                );
                continue;
            }
        };

        if meta.is_dir() {
            collect_expired_recursive(&entry.path(), now, retention_secs, out)?;
        } else if meta.is_file() {
            let modified = meta.modified().unwrap_or(now);
            let age = now.duration_since(modified).unwrap_or_default();
            if age.as_secs() >= retention_secs {
                out.push((entry.path(), meta.len()));
            }
        }
    }

    Ok(())
}

/// Remove empty directories within the cache dir after cleanup.
async fn cleanup_empty_dirs(cache_dir: &Path) {
    let entries = match std::fs::read_dir(cache_dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // Only remove if empty
            if let Ok(mut contents) = std::fs::read_dir(&path) {
                if contents.next().is_none() {
                    if let Err(e) = tokio::fs::remove_dir(&path).await {
                        warn!(
                            path = %path.display(),
                            error = %e,
                            "Failed to remove empty cache directory"
                        );
                    }
                }
            }
        }
    }
}

/// Check if accepting a file would exceed the per-device cache quota.
///
/// Uses filesystem-based calculation of current cache usage for a peer.
/// Returns Ok(()) if within quota, Err with quota details if exceeded.
pub async fn check_device_quota(
    settings: &dyn SettingsPort,
    cache_dir: &Path,
    source_device_id: &str,
    incoming_file_size: u64,
) -> std::result::Result<(), QuotaExceededError> {
    let s = settings
        .load()
        .await
        .map_err(|e| QuotaExceededError::Internal(e.to_string()))?;

    let quota_bytes = s.file_sync.file_cache_quota_per_device;

    let peer_cache_dir = cache_dir.join(source_device_id);
    let current_usage = if peer_cache_dir.exists() {
        dir_size(&peer_cache_dir).unwrap_or(0)
    } else {
        0
    };

    if current_usage.saturating_add(incoming_file_size) > quota_bytes {
        return Err(QuotaExceededError::Exceeded {
            device_id: source_device_id.to_string(),
            current_usage,
            quota: quota_bytes,
            requested: incoming_file_size,
        });
    }

    Ok(())
}

/// Calculate total size of files in a directory recursively.
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

#[derive(Debug, thiserror::Error)]
pub enum QuotaExceededError {
    #[error("Cache quota exceeded for device {device_id}: {current_usage}/{quota} bytes used, requested {requested} bytes")]
    Exceeded {
        device_id: String,
        current_usage: u64,
        quota: u64,
        requested: u64,
    },
    #[error("Internal error checking quota: {0}")]
    Internal(String),
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

    fn make_settings(
        retention_hours: u32,
        auto_cleanup: bool,
        quota: u64,
    ) -> Arc<dyn SettingsPort> {
        let mut settings = Settings::default();
        settings.file_sync.file_retention_hours = retention_hours;
        settings.file_sync.file_auto_cleanup = auto_cleanup;
        settings.file_sync.file_cache_quota_per_device = quota;
        Arc::new(MockSettings { settings })
    }

    #[tokio::test]
    async fn test_cleanup_disabled() {
        let tmp = TempDir::new().unwrap();
        let settings = make_settings(24, false, 500_000_000);
        let uc = CleanupExpiredFilesUseCase::new(settings, tmp.path().to_path_buf());
        let result = uc.execute().await.unwrap();
        assert_eq!(result.files_removed, 0);
    }

    #[tokio::test]
    async fn test_cleanup_no_cache_dir() {
        let settings = make_settings(24, true, 500_000_000);
        let uc = CleanupExpiredFilesUseCase::new(settings, PathBuf::from("/nonexistent/cache"));
        let result = uc.execute().await.unwrap();
        assert_eq!(result.files_removed, 0);
    }

    #[tokio::test]
    async fn test_cleanup_removes_expired_files() {
        let tmp = TempDir::new().unwrap();
        let peer_dir = tmp.path().join("peer-1");
        std::fs::create_dir_all(&peer_dir).unwrap();

        // Create files in the cache directory
        let file1 = peer_dir.join("file1.bin");
        {
            let mut f = std::fs::File::create(&file1).unwrap();
            f.write_all(&[0u8; 1024]).unwrap();
        }
        let file2 = peer_dir.join("file2.bin");
        {
            let mut f = std::fs::File::create(&file2).unwrap();
            f.write_all(&[0u8; 512]).unwrap();
        }

        // Wait a brief moment so the files are at least 1 second old
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Retention = 0 hours means all files are expired immediately
        let settings = make_settings(0, true, 500_000_000);
        let uc = CleanupExpiredFilesUseCase::new(settings, tmp.path().to_path_buf());
        let result = uc.execute().await.unwrap();

        assert_eq!(result.files_removed, 2);
        assert_eq!(result.bytes_reclaimed, 1536);
        assert!(!file1.exists());
        assert!(!file2.exists());
    }

    #[tokio::test]
    async fn test_quota_within_limit() {
        let tmp = TempDir::new().unwrap();
        let settings = make_settings(24, true, 500_000_000);
        let result = check_device_quota(settings.as_ref(), tmp.path(), "peer-1", 100_000_000).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_quota_exceeded() {
        let tmp = TempDir::new().unwrap();
        let peer_dir = tmp.path().join("peer-1");
        std::fs::create_dir_all(&peer_dir).unwrap();

        // Write 900 bytes
        {
            let mut f = std::fs::File::create(peer_dir.join("existing.bin")).unwrap();
            f.write_all(&[0u8; 900]).unwrap();
        }

        let settings = make_settings(24, true, 1000); // 1000 byte quota
        let result = check_device_quota(settings.as_ref(), tmp.path(), "peer-1", 200).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            QuotaExceededError::Exceeded {
                current_usage,
                quota,
                requested,
                ..
            } => {
                assert_eq!(current_usage, 900);
                assert_eq!(quota, 1000);
                assert_eq!(requested, 200);
            }
            other => panic!("Expected Exceeded, got: {:?}", other),
        }
    }
}
