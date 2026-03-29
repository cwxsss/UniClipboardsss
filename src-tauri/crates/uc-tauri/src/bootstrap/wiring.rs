//! # Dependency Injection / 依赖注入模块
//!
//! ## Responsibilities / 职责
//!
//! - ✅ Create infra implementations (db, fs, secure storage) / 创建 infra 层具体实现
//! - ✅ Create platform implementations (clipboard, network) / 创建 platform 层具体实现
//! - ✅ Inject all dependencies into App / 将所有依赖注入到 App
//!
//! ## Prohibited / 禁止事项
//!
//! ❌ **No business logic / 禁止包含任何业务逻辑**
//! - Do not decide "what to do if encryption uninitialized"
//! - 不判断"如果加密未初始化就怎样"
//! - Do not handle "what to do if device not registered"
//! - 不处理"如果设备未注册就怎样"
//!
//! ❌ **No configuration validation / 禁止做配置验证**
//! - Config already loaded in config.rs
//! - 配置已在 config.rs 加载
//! - Validation should be in use case or upper layer
//! - 验证应在 use case 或上层
//!
//! ❌ **No direct concrete implementation usage / 禁止直接使用具体实现**
//! - Must inject through Port traits
//! - 必须通过 Port trait 注入
//! - Do not call implementation methods directly after App construction
//! - 不在 App 构造后直接调用实现方法
//!
//! ## Architecture Principle / 架构原则
//!
//! > **This is the only place allowed to depend on uc-infra + uc-platform + uc-app simultaneously.**
//! > **这是唯一允许同时依赖 uc-infra、uc-platform 和 uc-app 的地方。**
//! > But this privilege is only for "assembly", not for "decision making".
//! > 但这种特权仅用于"组装"，不用于"决策"。

use std::sync::Arc;
use tauri::async_runtime;
use tracing::info;

use uc_app::task_registry::TaskRegistry;
use uc_daemon_client::realtime::start_realtime_runtime;

#[cfg(test)]
use uc_app::usecases::space_access::SpaceAccessOrchestrator;
use uc_app::AppDeps;
use uc_core::ports::host_event_emitter::HostEventEmitterPort;
#[cfg(test)]
use uc_core::ports::space::ProofPort;
#[cfg(test)]
use uc_core::ports::*;
#[cfg(test)]
use uc_core::security::model::{KeySlot, KeySlotFile};
#[cfg(test)]
use uc_core::security::space_access::event::SpaceAccessEvent;
// Re-export assembly types from uc-bootstrap.
pub use uc_bootstrap::assembly::{
    get_storage_paths, resolve_pairing_config, resolve_pairing_device_name, wire_dependencies,
    wire_dependencies_with_identity_store, HostEventSetupPort, WiredDependencies, WiringError,
    WiringResult,
};

// Re-export BackgroundRuntimeDeps from uc-bootstrap (definition moved in Phase 40).
pub use uc_bootstrap::BackgroundRuntimeDeps;

/// Start background spooler and blob worker tasks.
/// 启动后台假脱机写入和 blob 物化任务。
///
/// All long-lived tasks are spawned through the `TaskRegistry` for centralized
/// lifecycle management and graceful shutdown via cooperative cancellation.
pub fn start_background_tasks(
    background: BackgroundRuntimeDeps,
    deps: &AppDeps,
    event_emitter: Arc<dyn HostEventEmitterPort>,
    daemon_connection_state: uc_daemon_client::DaemonConnectionState,
    setup_pairing_event_hub: Arc<uc_app::realtime::SetupPairingEventHub>,
    task_registry: &Arc<TaskRegistry>,
) {
    // Clones for GUI-only tasks
    let deps_settings = deps.settings.clone();
    let cleanup_file_cache_dir = background.file_cache_dir.clone();
    let blob_ports = uc_bootstrap::BlobProcessingPorts::from_app_deps(deps);

    // Spawn all long-lived tasks through the TaskRegistry for lifecycle management.
    // We use a single orchestration spawn to set up all registry tasks, since
    // registry.spawn() is async and start_background_tasks is sync.
    let registry = task_registry.clone();
    async_runtime::spawn(async move {
        // --- Shared blob processing tasks (SpoolScanner + SpoolerTask + BackgroundBlobWorker + SpoolJanitor) ---
        uc_bootstrap::spawn_blob_processing_tasks(background, blob_ports, &registry).await;

        // --- Unified realtime runtime (daemon WebSocket bridge + app consumers) ---
        start_realtime_runtime(
            daemon_connection_state,
            event_emitter.clone(),
            setup_pairing_event_hub,
            &registry,
        )
        .await;
        info!("Started unified daemon realtime runtime");

        // --- File cache cleanup (runs once at startup, fire-and-forget) ---
        {
            use tracing::warn;
            let cleanup_settings = deps_settings.clone();
            let cleanup_cache_dir = cleanup_file_cache_dir.clone();
            registry
                .spawn("file_cache_cleanup", |_token| async move {
                    let uc = uc_app::usecases::file_sync::CleanupExpiredFilesUseCase::new(
                        cleanup_settings,
                        cleanup_cache_dir,
                    );
                    match uc.execute().await {
                        Ok(result) => {
                            if result.files_removed > 0 {
                                info!(
                                    files_removed = result.files_removed,
                                    bytes_reclaimed = result.bytes_reclaimed,
                                    "Startup file cache cleanup completed"
                                );
                            }
                        }
                        Err(e) => {
                            warn!(error = %e, "Startup file cache cleanup failed (non-fatal)");
                        }
                    }
                })
                .await;
        }

        info!("All background tasks registered with TaskRegistry");
    });
}

#[derive(Clone)]
#[cfg(test)]
struct RuntimeSpaceAccessPorts {
    transport: Arc<tokio::sync::Mutex<dyn uc_core::ports::space::SpaceAccessTransportPort>>,
    proof: Arc<dyn ProofPort>,
    timer: Arc<tokio::sync::Mutex<dyn TimerPort>>,
    persistence: Arc<tokio::sync::Mutex<dyn uc_core::ports::space::PersistencePort>>,
}

#[cfg(test)]
async fn dispatch_space_access_busy_event(
    orchestrator: &SpaceAccessOrchestrator,
    runtime_ports: &RuntimeSpaceAccessPorts,
    event: SpaceAccessEvent,
    session_id: &str,
) -> Result<(), uc_app::usecases::space_access::SpaceAccessError> {
    let noop_crypto = NoopSpaceAccessCrypto;
    let mut transport = runtime_ports.transport.lock().await;
    let mut timer = runtime_ports.timer.lock().await;
    let mut store = runtime_ports.persistence.lock().await;

    orchestrator
        .dispatch(
            &mut uc_app::usecases::space_access::SpaceAccessExecutor {
                crypto: &noop_crypto,
                transport: &mut *transport,
                proof: runtime_ports.proof.as_ref(),
                timer: &mut *timer,
                store: &mut *store,
            },
            event,
            Some(session_id.to_string()),
        )
        .await
        .map(|_| ())
}

#[cfg(test)]
const BUSY_PAYLOAD_PREVIEW_MAX_CHARS: usize = 256;

#[allow(dead_code)]
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg(test)]
struct SpaceAccessBusyOfferPayload {
    kind: String,
    space_id: String,
    nonce: Vec<u8>,
    keyslot: KeySlot,
}

#[allow(dead_code)]
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg(test)]
struct SpaceAccessBusyProofPayload {
    kind: String,
    pairing_session_id: String,
    space_id: String,
    challenge_nonce: Vec<u8>,
    proof_bytes: Vec<u8>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg(test)]
struct SpaceAccessBusyResultPayload {
    kind: String,
    space_id: String,
    #[serde(default)]
    sponsor_peer_id: Option<String>,
    success: bool,
    #[serde(default)]
    deny_reason: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
#[cfg(test)]
enum SpaceAccessBusyPayload {
    Offer(SpaceAccessBusyOfferPayload),
    Proof(SpaceAccessBusyProofPayload),
    Result(SpaceAccessBusyResultPayload),
}

#[derive(Debug, thiserror::Error)]
#[cfg(test)]
enum ParseError {
    #[error("busy payload is not valid json: {source}")]
    InvalidJson {
        #[source]
        source: serde_json::Error,
    },
    #[error("busy payload missing string field `kind`")]
    MissingKind,
    #[error("busy payload kind `{kind}` is not supported")]
    UnknownKind { kind: String },
    #[error("busy payload kind `{kind}` has invalid structure: {source}")]
    InvalidStructure {
        kind: String,
        #[source]
        source: serde_json::Error,
    },
}

#[cfg(test)]
impl ParseError {
    fn payload_kind(&self) -> Option<&str> {
        match self {
            Self::UnknownKind { kind } | Self::InvalidStructure { kind, .. } => Some(kind.as_str()),
            Self::InvalidJson { .. } | Self::MissingKind => None,
        }
    }
}

#[cfg(test)]
fn parse_space_access_busy_payload(json: &str) -> Result<SpaceAccessBusyPayload, ParseError> {
    let payload: serde_json::Value =
        serde_json::from_str(json).map_err(|source| ParseError::InvalidJson { source })?;

    let kind = payload
        .get("kind")
        .and_then(serde_json::Value::as_str)
        .ok_or(ParseError::MissingKind)?
        .to_string();

    match kind.as_str() {
        "space_access_offer" => serde_json::from_value::<SpaceAccessBusyOfferPayload>(payload)
            .map(SpaceAccessBusyPayload::Offer)
            .map_err(|source| ParseError::InvalidStructure {
                kind: kind.clone(),
                source,
            }),
        "space_access_proof" => serde_json::from_value::<SpaceAccessBusyProofPayload>(payload)
            .map(SpaceAccessBusyPayload::Proof)
            .map_err(|source| ParseError::InvalidStructure {
                kind: kind.clone(),
                source,
            }),
        "space_access_result" => serde_json::from_value::<SpaceAccessBusyResultPayload>(payload)
            .map(SpaceAccessBusyPayload::Result)
            .map_err(|source| ParseError::InvalidStructure {
                kind: kind.clone(),
                source,
            }),
        _ => Err(ParseError::UnknownKind { kind }),
    }
}

#[cfg(test)]
fn extract_space_access_busy_payload_kind(json: &str) -> Option<String> {
    let payload: serde_json::Value = serde_json::from_str(json).ok()?;
    payload
        .get("kind")
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned)
}

#[cfg(test)]
fn raw_payload_preview(payload: &str) -> String {
    let mut chars = payload.chars();
    let mut preview: String = chars
        .by_ref()
        .take(BUSY_PAYLOAD_PREVIEW_MAX_CHARS)
        .collect();
    if chars.next().is_some() {
        preview.push_str("...");
    }
    preview
}

#[cfg(test)]
struct NoopSpaceAccessCrypto;

#[cfg(test)]
struct LoadedKeyslotSpaceAccessCrypto {
    keyslot_file: KeySlotFile,
}

#[cfg(test)]
impl LoadedKeyslotSpaceAccessCrypto {
    fn new(keyslot_file: KeySlotFile) -> Self {
        Self { keyslot_file }
    }
}

#[async_trait::async_trait]
#[cfg(test)]
impl uc_core::ports::space::CryptoPort for NoopSpaceAccessCrypto {
    async fn generate_nonce32(&self) -> [u8; 32] {
        [0u8; 32]
    }

    async fn export_keyslot_blob(
        &self,
        _space_id: &uc_core::ids::SpaceId,
    ) -> anyhow::Result<uc_core::security::model::KeySlot> {
        Err(anyhow::anyhow!(
            "noop crypto port cannot export keyslot blob"
        ))
    }

    async fn derive_master_key_from_keyslot(
        &self,
        _keyslot_blob: &[u8],
        _passphrase: uc_core::security::SecretString,
    ) -> anyhow::Result<uc_core::security::model::MasterKey> {
        Err(anyhow::anyhow!("noop crypto port cannot derive master key"))
    }
}

#[async_trait::async_trait]
#[cfg(test)]
impl uc_core::ports::space::CryptoPort for LoadedKeyslotSpaceAccessCrypto {
    async fn generate_nonce32(&self) -> [u8; 32] {
        [0u8; 32]
    }

    async fn export_keyslot_blob(
        &self,
        _space_id: &uc_core::ids::SpaceId,
    ) -> anyhow::Result<uc_core::security::model::KeySlot> {
        Ok(self.keyslot_file.clone().into())
    }

    async fn derive_master_key_from_keyslot(
        &self,
        _keyslot_blob: &[u8],
        _passphrase: uc_core::security::SecretString,
    ) -> anyhow::Result<uc_core::security::model::MasterKey> {
        Err(anyhow::anyhow!(
            "loaded keyslot crypto cannot derive master key in sponsor flow"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wiring_error_display() {
        let err = WiringError::DatabaseInit("connection failed".to_string());
        assert!(err.to_string().contains("Database initialization"));
        assert!(err.to_string().contains("connection failed"));
    }
}
