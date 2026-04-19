//! Auto-unlock encryption session on startup——薄 wrapper,把"静默从持久化层
//! 恢复会话"这条 startup 路径转交给 `SpaceAccessPort::try_resume_session`。
//!
//! 历史上本 usecase 自己持有 5 个 port (encryption_state / key_scope /
//! key_material / encryption / encryption_session) 做完整 7 步流程,
//! Slice 3 把它们搬到 `DefaultSpaceAccessAdapter::try_resume_session`,
//! 这里只留命令翻译 + 错误映射。

use std::sync::Arc;
use tracing::{info, info_span, Instrument};

use uc_core::{
    ids::SpaceId,
    ports::space::{SpaceAccessError, SpaceAccessPort},
};

#[derive(Debug, thiserror::Error)]
pub enum AutoUnlockError {
    #[error("space access failed: {0}")]
    SpaceAccess(#[from] SpaceAccessError),
}

pub struct AutoUnlockEncryptionSession {
    space_access: Arc<dyn SpaceAccessPort>,
}

impl AutoUnlockEncryptionSession {
    pub fn new(space_access: Arc<dyn SpaceAccessPort>) -> Self {
        Self { space_access }
    }

    pub fn from_ports(space_access: Arc<dyn SpaceAccessPort>) -> Self {
        Self::new(space_access)
    }

    /// Execute the keyring unlock flow.
    ///
    /// # Returns
    ///
    /// - `Ok(true)` - Session resumed successfully (keyring 命中)
    /// - `Ok(false)` - Encryption not initialized (no unlock needed)
    /// - `Err(_)` - Resume failed (keyring 缓存丢失 / 权限不足 / 密钥物料损坏 等)
    pub async fn execute(&self) -> Result<bool, AutoUnlockError> {
        let span = info_span!("usecase.auto_unlock_encryption_session.execute");

        async {
            info!("delegating silent session resume to SpaceAccessPort");

            // 与 InitializeEncryption 保持占位 SpaceId 一致。adapter 当前不按
            // SpaceId 路由,多空间引入时再改造。
            let space_id = SpaceId::from("space");

            match self.space_access.try_resume_session(&space_id).await? {
                Some(_active_space) => {
                    info!("session resumed via SpaceAccessPort");
                    Ok(true)
                }
                None => {
                    info!("encryption uninitialized, no session to resume");
                    Ok(false)
                }
            }
        }
        .instrument(span)
        .await
    }
}
