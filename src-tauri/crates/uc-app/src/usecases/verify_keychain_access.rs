//! Verify whether macOS Keychain "Always Allow" permission has been granted.
//!
//! 薄 wrapper, 把"探测 keyring 是否能在静默下读出 KEK"这条 macOS 引导
//! 流程转交给 `SpaceAccessPort::verify_keychain_access`。
//!
//! 历史上本 usecase 自己持有 KeyMaterialPort + KeyScopePort 做权限探测,
//! Slice 3 把"探测 + 错误分类"全部搬到 `DefaultSpaceAccessAdapter` 内部,
//! 这里只留命令翻译。

use std::sync::Arc;
use tracing::{info, info_span, Instrument};

use uc_core::ports::space::{SpaceAccessError, SpaceAccessPort};

#[derive(Debug, thiserror::Error)]
pub enum VerifyKeychainError {
    #[error("KEK not found: encryption may not be properly initialized")]
    KekNotFound,

    #[error("space access failed: {0}")]
    SpaceAccess(#[from] SpaceAccessError),
}

pub struct VerifyKeychainAccess {
    space_access: Arc<dyn SpaceAccessPort>,
}

impl VerifyKeychainAccess {
    pub fn new(space_access: Arc<dyn SpaceAccessPort>) -> Self {
        Self { space_access }
    }

    pub fn from_ports(space_access: Arc<dyn SpaceAccessPort>) -> Self {
        Self::new(space_access)
    }

    /// Execute the keychain access verification.
    ///
    /// # Returns
    ///
    /// - `Ok(true)` — Keychain access succeeded silently (Always Allow granted)
    /// - `Ok(false)` — Permission denied or transient keyring error
    /// - `Err(KekNotFound)` — Encryption uninitialized (KEK 不在 keyring 里)
    /// - `Err(SpaceAccess)` — 其它不可恢复错误
    pub async fn execute(&self) -> Result<bool, VerifyKeychainError> {
        let span = info_span!("usecase.verify_keychain_access.execute");

        async {
            info!("delegating keychain access probe to SpaceAccessPort");

            self.space_access
                .verify_keychain_access()
                .await
                .map_err(|e| match e {
                    SpaceAccessError::NotInitialized => VerifyKeychainError::KekNotFound,
                    other => VerifyKeychainError::SpaceAccess(other),
                })
        }
        .instrument(span)
        .await
    }
}
