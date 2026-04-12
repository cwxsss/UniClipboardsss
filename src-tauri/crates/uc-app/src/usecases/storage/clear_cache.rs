//! Use case for clearing the application cache directory.
//! 清除应用缓存目录的用例。

use std::sync::Arc;

use crate::app_paths::AppPaths;
use anyhow::Result;
use uc_core::ports::cache_fs::CacheFsPort;

/// Use case for clearing cache directory contents.
/// 清除缓存目录内容的用例。
pub struct ClearCache {
    storage_paths: AppPaths,
    cache_fs: Arc<dyn CacheFsPort>,
}

impl ClearCache {
    pub fn new(storage_paths: AppPaths, cache_fs: Arc<dyn CacheFsPort>) -> Self {
        Self {
            storage_paths,
            cache_fs,
        }
    }

    /// Clears cache directory contents and returns the number of bytes freed.
    /// 清除缓存目录内容并返回释放的字节数。
    #[tracing::instrument(name = "usecase.clear_cache.execute", skip(self))]
    pub async fn execute(&self) -> Result<u64> {
        let paths = &self.storage_paths;
        let size_before = self.cache_fs.dir_size(&paths.cache_dir).await?;

        if self.cache_fs.exists(&paths.cache_dir).await {
            let entries = self
                .cache_fs
                .read_dir(&paths.cache_dir)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to read cache dir: {}", e))?;

            for entry in entries {
                if entry.is_dir {
                    if let Err(e) = self.cache_fs.remove_dir_all(&entry.path).await {
                        tracing::warn!(path = %entry.path.display(), error = %e, "Failed to remove cache subdirectory");
                    }
                } else if let Err(e) = self.cache_fs.remove_file(&entry.path).await {
                    tracing::warn!(path = %entry.path.display(), error = %e, "Failed to remove cache file");
                }
            }
        }

        let size_after = self.cache_fs.dir_size(&paths.cache_dir).await?;
        let freed = size_before.saturating_sub(size_after);

        tracing::info!(freed_bytes = freed, "Cache cleared");
        Ok(freed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_mocks::MockCacheFs;
    use std::path::PathBuf;
    use uc_core::ports::CacheFsDirEntry;

    fn test_storage_paths() -> AppPaths {
        AppPaths {
            db_path: PathBuf::from("/tmp/test-data/uniclipboard.db"),
            vault_dir: PathBuf::from("/tmp/test-data/vault"),
            settings_path: PathBuf::from("/tmp/test-data/settings.json"),
            logs_dir: PathBuf::from("/tmp/test-data/logs"),
            cache_dir: PathBuf::from("/tmp/test-cache"),
            file_cache_dir: PathBuf::from("/tmp/test-cache/file-cache"),
            spool_dir: PathBuf::from("/tmp/test-cache/spool"),
            app_data_root_dir: PathBuf::from("/tmp/test-data"),
        }
    }

    #[tokio::test]
    async fn execute_returns_freed_bytes_when_cache_exists() {
        let mut cache_fs = MockCacheFs::new();
        // exists → true
        cache_fs.expect_exists().returning(|_| true);
        // read_dir → two entries
        cache_fs.expect_read_dir().returning(|_| {
            Ok(vec![
                CacheFsDirEntry {
                    path: PathBuf::from("/tmp/test-cache/subdir"),
                    is_dir: true,
                },
                CacheFsDirEntry {
                    path: PathBuf::from("/tmp/test-cache/file.tmp"),
                    is_dir: false,
                },
            ])
        });
        cache_fs.expect_remove_dir_all().returning(|_| Ok(()));
        cache_fs.expect_remove_file().returning(|_| Ok(()));
        // dir_size: first call returns 1024, second call returns 0
        cache_fs.expect_dir_size().once().returning(|_| Ok(1024));
        cache_fs.expect_dir_size().once().returning(|_| Ok(0));

        let uc = ClearCache::new(test_storage_paths(), Arc::new(cache_fs));
        let freed = uc.execute().await.unwrap();
        assert_eq!(freed, 1024);
    }

    #[tokio::test]
    async fn execute_returns_zero_when_cache_dir_missing() {
        let mut cache_fs = MockCacheFs::new();
        cache_fs.expect_exists().returning(|_| false);
        // dir_size: called twice, both return 0 (no entries to clear)
        cache_fs.expect_dir_size().once().returning(|_| Ok(0));
        cache_fs.expect_dir_size().once().returning(|_| Ok(0));

        let uc = ClearCache::new(test_storage_paths(), Arc::new(cache_fs));
        let freed = uc.execute().await.unwrap();
        assert_eq!(freed, 0);
    }
}
