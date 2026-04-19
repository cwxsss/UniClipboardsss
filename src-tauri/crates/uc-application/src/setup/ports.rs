//! Setup-facing ports for cross-crate use-case dependencies.
//!
//! `SetupOrchestrator` / `SetupActionExecutor` historically called into
//! `uc-app::usecases::InitializeEncryption` and
//! `uc-app::usecases::AppLifecycleCoordinator`. Now that setup lives in
//! `uc-application`, we cannot reach back into `uc-app` (that would create a
//! dependency cycle). These traits hide both concrete use-cases behind narrow,
//! setup-specific interfaces; `uc-app` supplies adapters.

use uc_core::crypto::model::Passphrase;

/// ⚠️ DEPRECATED: trait 接收旧的 `uc_core::crypto::model::Passphrase`(裸 String 包装)。
///
/// 长期目标(D4): 统一改用 `uc_core::crypto::domain::Passphrase`
/// (基于 `SecretString`,drop 时 zeroize)。切换需要同步改 setup 流程
/// 中所有 Passphrase 的入参/状态字段类型——本次 Slice 3 范围太大,
/// 暂留 deprecated 标记防止后续遗忘。
#[deprecated(
    note = "use uc_core::crypto::domain::Passphrase instead of model::Passphrase; \
            see SetupInitializeEncryptionPort doc for migration plan (D4)"
)]
#[async_trait::async_trait]
pub trait SetupInitializeEncryptionPort: Send + Sync {
    async fn execute(&self, passphrase: Passphrase) -> anyhow::Result<()>;
}

#[async_trait::async_trait]
pub trait SetupAppLifecyclePort: Send + Sync {
    async fn ensure_ready(&self) -> anyhow::Result<()>;
}
