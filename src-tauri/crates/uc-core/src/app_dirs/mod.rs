use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppDirs {
    pub app_data_root: PathBuf,
    pub app_cache_root: PathBuf,
}
