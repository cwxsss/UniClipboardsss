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
