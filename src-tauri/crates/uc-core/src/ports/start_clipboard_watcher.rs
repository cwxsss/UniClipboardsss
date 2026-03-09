//! Clipboard watcher use case trait
//! 剪贴板监控器用例 trait

use async_trait::async_trait;

/// Error type for clipboard watcher startup failures.
#[derive(Debug, thiserror::Error)]
pub enum StartClipboardWatcherError {
    #[error("Failed to start clipboard watcher: {0}")]
    StartFailed(String),
}

/// Trait for starting the clipboard watcher.
///
/// This trait allows the AppLifecycleCoordinator to depend on the watcher
/// use case without depending on uc-platform.
#[async_trait]
pub trait StartClipboardWatcherPort: Send + Sync {
    /// Execute the use case to start the clipboard watcher.
    async fn execute(&self) -> Result<(), StartClipboardWatcherError>;
}
