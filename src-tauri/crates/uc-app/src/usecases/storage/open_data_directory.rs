//! Use case for opening the application data directory in the system file manager.
//! 在系统文件管理器中打开应用数据目录的用例。

use std::sync::Arc;

use crate::app_paths::AppPaths;
use anyhow::Result;
use uc_core::ports::file_manager::FileManagerPort;

/// Use case for opening the data directory in the native file manager.
/// 在原生文件管理器中打开数据目录的用例。
pub struct OpenDataDirectory {
    storage_paths: AppPaths,
    file_manager: Arc<dyn FileManagerPort>,
}

impl OpenDataDirectory {
    pub fn new(storage_paths: AppPaths, file_manager: Arc<dyn FileManagerPort>) -> Self {
        Self {
            storage_paths,
            file_manager,
        }
    }

    #[tracing::instrument(name = "usecase.open_data_directory.execute", skip(self))]
    pub async fn execute(&self) -> Result<()> {
        let dir = &self.storage_paths.app_data_root_dir;
        self.file_manager
            .open_directory(dir)
            .map_err(anyhow::Error::from)?;

        tracing::info!(dir = %dir.display(), "Opened data directory");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_mocks::MockFileManager;
    use std::path::PathBuf;
    use uc_core::ports::file_manager::FileManagerError;

    fn test_paths() -> AppPaths {
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
    async fn opens_app_data_root_directory() {
        let opened = Arc::new(std::sync::Mutex::new(Vec::<PathBuf>::new()));
        let opened_clone = opened.clone();

        let mut fm = MockFileManager::new();
        fm.expect_open_directory().returning(move |path| {
            opened_clone.lock().unwrap().push(path.to_path_buf());
            Ok(())
        });

        let paths = test_paths();
        let expected = paths.app_data_root_dir.clone();

        let uc = OpenDataDirectory::new(paths, Arc::new(fm));
        uc.execute().await.unwrap();

        let opened = opened.lock().unwrap();
        assert_eq!(opened.len(), 1);
        assert_eq!(opened[0], expected);
    }

    #[tokio::test]
    async fn returns_error_when_file_manager_fails() {
        let mut fm = MockFileManager::new();
        fm.expect_open_directory()
            .returning(|path| Err(FileManagerError::OpenFailed(path.display().to_string())));

        let uc = OpenDataDirectory::new(test_paths(), Arc::new(fm));
        let result = uc.execute().await;
        assert!(result.is_err());
    }
}
