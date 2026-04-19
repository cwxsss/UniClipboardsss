use std::sync::Arc;
use tracing::{info, info_span, Instrument};

use uc_core::{
    crypto::model::Passphrase,
    ids::SpaceId,
    ports::space::{SpaceAccessError, SpaceAccessPort},
};

#[derive(Debug, thiserror::Error)]
pub enum InitializeEncryptionError {
    #[error("encryption is already initialized")]
    AlreadyInitialized,

    #[error("space access failed: {0}")]
    SpaceAccess(#[from] SpaceAccessError),
}

/// 首次初始化加密会话——薄 wrapper, 把"用户输入口令 + 创建本地空间 +
/// 解锁会话"这条业务动作转交给 `SpaceAccessPort::initialize`。
///
/// 历史上本 usecase 自己持有 5 个 port（EncryptionPort / KeyMaterialPort /
/// EncryptionSessionPort / EncryptionStatePort / KeyScopePort）做完整 11 步
/// 流程。Slice 3 把这些步骤搬到 `DefaultSpaceAccessAdapter` 内部,这里只
/// 留命令翻译 + 错误映射。
///
/// Phase C 起此 usecase 只被 `uc-cli run_new_space` 直接调用; setup 流程
/// (`SetupAction::CreateEncryptedSpace`) 内部直接调 `SpaceAccessPort.initialize`,
/// 不再绕经此处(原 `SetupInitializeEncryptionPort` 适配层已删除)。
pub struct InitializeEncryption {
    space_access: Arc<dyn SpaceAccessPort>,
}

impl InitializeEncryption {
    pub fn new(space_access: Arc<dyn SpaceAccessPort>) -> Self {
        Self { space_access }
    }

    pub fn from_ports(space_access: Arc<dyn SpaceAccessPort>) -> Self {
        Self::new(space_access)
    }

    pub async fn execute(&self, passphrase: Passphrase) -> Result<(), InitializeEncryptionError> {
        let span = info_span!("usecase.initialize_encryption.execute");

        async {
            info!("delegating space initialization to SpaceAccessPort");

            // 单空间模型下用占位 SpaceId,与 setup action_executor
            // (CreateEncryptedSpace 分支) 保持一致。adapter 当前
            // 不按 SpaceId 路由,多空间引入时再改造。
            let space_id = SpaceId::from("space");
            // model::Passphrase -> domain::Passphrase 桥接
            // (domain::Passphrase 基于 SecretString, drop 时 zeroize)。
            let domain_passphrase = uc_core::crypto::domain::Passphrase::new(passphrase.0);

            self.space_access
                .initialize(&space_id, &domain_passphrase)
                .await
                .map_err(|e| match e {
                    SpaceAccessError::AlreadyInitialized => {
                        InitializeEncryptionError::AlreadyInitialized
                    }
                    other => InitializeEncryptionError::SpaceAccess(other),
                })?;

            info!("encryption initialized via SpaceAccessPort");
            Ok(())
        }
        .instrument(span)
        .await
    }
}
