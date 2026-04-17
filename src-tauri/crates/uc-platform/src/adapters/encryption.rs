//! In-memory encryption session port implementation
//! 内存加密会话端口实现

use async_trait::async_trait;
use std::sync::{Arc, Mutex};
use tracing::{debug, debug_span};
use uc_core::ports::EncryptionSessionPort;
use uc_core::security::model::{EncryptionError, MasterKey};

#[async_trait]
impl EncryptionSessionPort for InMemoryEncryptionSessionPort {
    async fn is_ready(&self) -> bool {
        let state = self.state.lock().expect("lock state");
        state.master_key.is_some()
    }

    async fn get_master_key(&self) -> Result<MasterKey, EncryptionError> {
        let state = self.state.lock().expect("lock state");
        state
            .master_key
            .as_ref()
            .cloned()
            .ok_or(EncryptionError::NotInitialized)
    }

    async fn set_master_key(&self, master_key: MasterKey) -> Result<(), EncryptionError> {
        let span = debug_span!("platform.encryption.set_master_key");
        span.in_scope(|| {
            let mut state = self.state.lock().expect("lock state");
            // Replace old key - MasterKey will be dropped and zeroized automatically
            // 替换旧密钥 - MasterKey 将被丢弃并自动零化
            state.master_key = Some(master_key);
            debug!("Master key set successfully");
            Ok(())
        })
    }

    async fn clear(&self) -> Result<(), EncryptionError> {
        let span = debug_span!("platform.encryption.clear");
        span.in_scope(|| {
            let mut state = self.state.lock().expect("lock state");
            // Drop old key - MasterKey will be zeroized automatically
            // 丢弃旧密钥 - MasterKey 将自动零化
            state.master_key = None;
            debug!("Master key cleared");
            Ok(())
        })
    }
}

/// In-memory encryption session port implementation
/// 内存加密会话端口实现
///
/// This implementation maintains an in-memory master key for basic functionality.
/// 此实现维护内存中的主密钥以实现基本功能。
///
/// # Current Limitations / 当前限制
///
/// Phase 2 (Development):
/// - Keys are stored in-memory only / 密钥仅存储在内存中
/// - Keys are lost on app restart / 应用重启后密钥丢失
/// - No persistence to secure storage / 未持久化到安全存储
///
/// Future Enhancement (Phase 3+):
/// - Persist master key to system keyring / 将主密钥持久化到系统密钥环
/// - Implement key rotation / 实现密钥轮换
/// - Add session timeout / 添加会话超时
///
/// # Security / 安全性
///
/// The current implementation provides:
/// 当前实现提供：
/// - Thread-safe access via Arc<Mutex<>> / 通过 Arc<Mutex<>> 实现线程安全访问
/// - Automatic key zeroization on drop / 丢弃时自动密钥零化（通过 MasterKey Drop impl）
/// - No disk writes / 无磁盘写入
///
#[derive(Clone)]
pub struct InMemoryEncryptionSessionPort {
    state: Arc<Mutex<EncryptionSessionState>>,
}

#[derive(Debug)]
struct EncryptionSessionState {
    master_key: Option<MasterKey>,
}

impl InMemoryEncryptionSessionPort {
    /// Create a new in-memory encryption session
    /// 创建新的内存加密会话
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(EncryptionSessionState { master_key: None })),
        }
    }
}

impl Default for InMemoryEncryptionSessionPort {
    fn default() -> Self {
        Self::new()
    }
}
