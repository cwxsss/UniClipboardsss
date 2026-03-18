//! # Pure Dependency Assembly / 纯依赖组装模块
//!
//! This module contains all pure dependency construction functions that have
//! zero Tauri imports. It is structurally ready for extraction to a standalone
//! `uc-bootstrap` crate in Phase 40.
//!
//! ## What lives here / 本模块包含
//!
//! - `WiredDependencies` struct (output of the wiring process)
//! - `HostEventSetupPort` adapter (pure, no Tauri types)
//! - All infrastructure and platform layer construction functions
//! - `wire_dependencies`, `get_storage_paths`, `resolve_pairing_device_name`, etc.
//!
//! ## What does NOT live here / 不在本模块
//!
//! - `BackgroundRuntimeDeps` (lives in wiring.rs — event-loop-side)
//! - `start_background_tasks` and all `run_*_loop` functions (Tauri runtime)
//! - Any function that uses `tauri::` types
//!
//! ## Architecture Principle / 架构原则
//!
//! > **Zero tauri imports in this file — enforced by CI lint.**
//! > **本文件零 tauri 导入 — 由 CI lint 强制执行。**

use std::path::PathBuf;
use std::sync::Arc;

use tracing::{info, warn};

use uc_app::deps::NetworkPorts;
use uc_app::usecases::{PairingConfig, ResolveConnectionPolicy};
use uc_app::{AppDeps, ClipboardPorts, DevicePorts, SecurityPorts, StoragePorts, SystemPorts};
use uc_core::clipboard::SelectRepresentationPolicyV1;
use uc_core::config::AppConfig;
use uc_core::ids::RepresentationId;
use uc_core::ports::clipboard::{
    ClipboardChangeOriginPort, ClipboardRepresentationNormalizerPort, RepresentationCachePort,
    SpoolQueuePort, SpoolRequest,
};
use uc_core::ports::host_event_emitter::{HostEvent, HostEventEmitterPort, SetupHostEvent};
use uc_core::ports::SetupEventPort;
use uc_core::ports::*;
use uc_core::settings::model::Settings;
use uc_infra::blob::BlobWriter;
use uc_infra::clipboard::{
    ClipboardRepresentationNormalizer, InMemoryClipboardChangeOrigin, InfraThumbnailGenerator,
    MpscSpoolQueue, RepresentationCache, SpoolManager,
};
use uc_infra::config::ClipboardStorageConfig;
use uc_infra::db::executor::DieselSqliteExecutor;
use uc_infra::db::mappers::{
    blob_mapper::BlobRowMapper, clipboard_entry_mapper::ClipboardEntryRowMapper,
    clipboard_event_mapper::ClipboardEventRowMapper,
    clipboard_selection_mapper::ClipboardSelectionRowMapper, device_mapper::DeviceRowMapper,
    paired_device_mapper::PairedDeviceRowMapper,
    snapshot_representation_mapper::RepresentationRowMapper,
};
use uc_infra::db::pool::{init_db_pool, DbPool};
use uc_infra::db::repositories::{
    DieselBlobRepository, DieselClipboardEntryRepository, DieselClipboardEventRepository,
    DieselClipboardRepresentationRepository, DieselClipboardSelectionRepository,
    DieselDeviceRepository, DieselFileTransferRepository, DieselPairedDeviceRepository,
    DieselThumbnailRepository,
};
use uc_infra::device::LocalDeviceIdentity;
use uc_infra::fs::key_slot_store::JsonKeySlotStore;
use uc_infra::security::{
    Blake3Hasher, DecryptingClipboardRepresentationRepository, DefaultKeyMaterialService,
    EncryptedBlobStore, EncryptingClipboardEventWriter, EncryptionRepository,
    FileEncryptionStateRepository,
};
use uc_infra::settings::repository::FileSettingsRepository;
use uc_infra::{FileSetupStatusRepository, SystemClock};
use uc_platform::adapters::{
    FilesystemBlobStore, InMemoryEncryptionSessionPort, InMemoryWatcherControl,
    Libp2pNetworkAdapter,
};
use uc_platform::app_dirs::DirsAppDirsAdapter;
use uc_platform::clipboard::LocalClipboard;
use uc_platform::identity_store::FileIdentityStore;
use uc_platform::ports::{AppDirsPort, IdentityStorePort, WatcherControlPort};
use uc_platform::runtime::event_bus::PlatformCommandSender;

use tokio::sync::mpsc;

/// Result type for wiring operations
pub type WiringResult<T> = Result<T, WiringError>;

/// Errors during dependency injection
/// 依赖注入错误（基础设施初始化失败）
#[derive(Debug, thiserror::Error)]
pub enum WiringError {
    #[error("Database initialization failed: {0}")]
    DatabaseInit(String),

    #[error("Secure storage initialization failed: {0}")]
    SecureStorageInit(String),

    #[error("Clipboard initialization failed: {0}")]
    ClipboardInit(String),

    #[error("Network initialization failed: {0}")]
    NetworkInit(String),

    #[error("Blob storage initialization failed: {0}")]
    BlobStorageInit(String),

    #[error("Settings repository initialization failed: {0}")]
    SettingsInit(String),

    #[error("Configuration initialization failed: {0}")]
    ConfigInit(String),

    #[error("Thumbnail generator initialization failed: {0}")]
    ThumbnailInit(String),
}

/// Fully wired dependencies plus background runtime components.
/// 已完成依赖连接与后台运行组件的组合。
pub struct WiredDependencies {
    pub deps: AppDeps,
    pub background: super::wiring::BackgroundRuntimeDeps,
    pub watcher_control: Arc<dyn WatcherControlPort>,
}

/// HostEventEmitterPort adapter that emits setup state changes to frontend listeners.
///
/// Uses Arc<RwLock<...>> shared cell so that HostEventSetupPort always reads the
/// current emitter after bootstrap swaps it from LoggingEventEmitter to TauriEventEmitter.
/// This eliminates the stale emitter bug described in STATE.md Known Bugs.
#[derive(Clone)]
pub struct HostEventSetupPort {
    emitter_cell: Arc<std::sync::RwLock<Arc<dyn HostEventEmitterPort>>>,
}

impl HostEventSetupPort {
    pub fn new(emitter_cell: Arc<std::sync::RwLock<Arc<dyn HostEventEmitterPort>>>) -> Self {
        Self { emitter_cell }
    }
}

#[async_trait::async_trait]
impl SetupEventPort for HostEventSetupPort {
    async fn emit_setup_state_changed(
        &self,
        state: uc_core::setup::SetupState,
        session_id: Option<String>,
    ) {
        let emitter = self
            .emitter_cell
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .clone();
        if let Err(err) = emitter.emit(HostEvent::Setup(SetupHostEvent::StateChanged {
            state,
            session_id,
        })) {
            warn!(error = %err, "Failed to emit setup-state-changed");
        }
    }
}

/// Infrastructure layer implementations / 基础设施层实现
struct InfraLayer {
    // Clipboard repositories / 剪贴板仓库
    #[allow(dead_code)]
    clipboard_entry_repo: Arc<dyn ClipboardEntryRepositoryPort>,
    clipboard_event_repo: Arc<dyn ClipboardEventWriterPort>,
    representation_repo: Arc<dyn ClipboardRepresentationRepositoryPort>,
    selection_repo: Arc<dyn ClipboardSelectionRepositoryPort>,

    // Device repository / 设备仓库
    device_repo: Arc<dyn DeviceRepositoryPort>,

    // Pairing repository / 配对仓库
    paired_device_repo: Arc<dyn PairedDeviceRepositoryPort>,

    // Blob storage / Blob 存储
    blob_repository: Arc<dyn BlobRepositoryPort>,
    thumbnail_repo: Arc<dyn ThumbnailRepositoryPort>,
    thumbnail_generator: Arc<dyn ThumbnailGeneratorPort>,

    // Security services / 安全服务
    key_material: Arc<dyn KeyMaterialPort>,
    encryption: Arc<dyn EncryptionPort>,
    encryption_state: Arc<dyn uc_core::ports::security::encryption_state::EncryptionStatePort>,

    // Settings / 设置
    settings_repo: Arc<dyn SettingsPort>,

    // Setup status / 设置状态
    setup_status: Arc<dyn SetupStatusPort>,

    // System services / 系统服务
    clock: Arc<dyn ClockPort>,
    hash: Arc<dyn ContentHashPort>,

    // File transfer tracking / 文件传输追踪
    file_transfer_repo: Arc<dyn uc_core::ports::FileTransferRepositoryPort>,
}

/// Platform layer implementations / 平台层实现
pub(crate) struct PlatformLayer {
    // System clipboard / 系统剪贴板
    pub(crate) clipboard: Arc<dyn PlatformClipboardPort>,
    pub(crate) system_clipboard: Arc<dyn SystemClipboardPort>,

    // Secure storage / 安全存储
    pub(crate) secure_storage: Arc<dyn SecureStoragePort>,

    // Network operations / 网络操作
    pub(crate) network_ports: Arc<NetworkPorts>,

    // libp2p network adapter (concrete)
    pub(crate) libp2p_network: Arc<Libp2pNetworkAdapter>,

    // Device identity / 设备身份
    pub(crate) device_identity: Arc<dyn DeviceIdentityPort>,

    // Clipboard representation normalizer / 剪贴板表示规范化器
    pub(crate) representation_normalizer: Arc<dyn ClipboardRepresentationNormalizerPort>,

    // Blob writer / Blob 写入器
    pub(crate) blob_writer: Arc<dyn BlobWriterPort>,

    // Blob store / Blob 存储（加密装饰后）
    pub(crate) blob_store: Arc<dyn BlobStorePort>,

    // Encryption session / 加密会话
    pub(crate) encryption_session: Arc<dyn EncryptionSessionPort>,

    // Watcher control / 监控器控制
    pub(crate) watcher_control: Arc<dyn WatcherControlPort>,

    // Key scope / 密钥范围
    pub(crate) key_scope: Arc<dyn uc_core::ports::security::key_scope::KeyScopePort>,
}

/// Create SQLite database connection pool
pub(crate) fn create_db_pool(db_path: &PathBuf) -> WiringResult<DbPool> {
    if db_path.as_os_str() != ":memory:" {
        if let Some(parent) = db_path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(|e| {
                WiringError::DatabaseInit(format!("Failed to create DB directory: {}", e))
            })?;
        }
    }

    let db_url = db_path
        .to_str()
        .ok_or_else(|| WiringError::DatabaseInit("Invalid database path".to_string()))?;

    init_db_pool(db_url)
        .map_err(|e| WiringError::DatabaseInit(format!("Failed to initialize DB: {}", e)))
}

/// Check if a file starts with the UCBL binary format magic bytes.
/// V2 blobs use magic [0x55, 0x43, 0x42, 0x4C] ("UCBL").
fn is_v2_blob(path: &std::path::Path) -> bool {
    const UCBL_MAGIC: [u8; 4] = [0x55, 0x43, 0x42, 0x4C];
    std::fs::File::open(path)
        .and_then(|mut f| {
            use std::io::Read;
            let mut buf = [0u8; 4];
            f.read_exact(&mut buf)?;
            Ok(buf == UCBL_MAGIC)
        })
        .unwrap_or(false)
}

/// Create infrastructure layer implementations
fn create_infra_layer(
    db_pool: DbPool,
    vault_path: &PathBuf,
    settings_path: &PathBuf,
    secure_storage: Arc<dyn SecureStoragePort>,
) -> WiringResult<InfraLayer> {
    let db_executor = Arc::new(DieselSqliteExecutor::new(db_pool));

    let entry_row_mapper = ClipboardEntryRowMapper;
    let selection_row_mapper = ClipboardSelectionRowMapper;
    let device_row_mapper = DeviceRowMapper;
    let paired_device_row_mapper = PairedDeviceRowMapper;
    let blob_row_mapper = BlobRowMapper;
    let _representation_row_mapper = RepresentationRowMapper;

    let entry_repo = DieselClipboardEntryRepository::new(
        Arc::clone(&db_executor),
        entry_row_mapper,
        selection_row_mapper,
        ClipboardEntryRowMapper, // ZST - can instantiate again
    );
    let clipboard_entry_repo: Arc<dyn ClipboardEntryRepositoryPort> = Arc::new(entry_repo);

    let event_row_mapper = ClipboardEventRowMapper;
    let clipboard_event_repo_impl = DieselClipboardEventRepository::new(
        Arc::clone(&db_executor),
        event_row_mapper,
        RepresentationRowMapper,
    );
    let clipboard_event_repo: Arc<dyn ClipboardEventWriterPort> =
        Arc::new(clipboard_event_repo_impl);

    let rep_repo = DieselClipboardRepresentationRepository::new(Arc::clone(&db_executor));
    let representation_repo: Arc<dyn ClipboardRepresentationRepositoryPort> = Arc::new(rep_repo);

    let dev_repo = DieselDeviceRepository::new(Arc::clone(&db_executor), device_row_mapper);
    let device_repo: Arc<dyn DeviceRepositoryPort> = Arc::new(dev_repo);

    let paired_repo =
        DieselPairedDeviceRepository::new(Arc::clone(&db_executor), paired_device_row_mapper);
    let paired_device_repo: Arc<dyn PairedDeviceRepositoryPort> = Arc::new(paired_repo);

    let blob_repo = DieselBlobRepository::new(
        Arc::clone(&db_executor),
        blob_row_mapper,
        BlobRowMapper, // ZST - can instantiate again
    );
    let blob_repository: Arc<dyn BlobRepositoryPort> = Arc::new(blob_repo);

    let thumbnail_repo_impl = DieselThumbnailRepository::new(Arc::clone(&db_executor));
    let thumbnail_repo: Arc<dyn ThumbnailRepositoryPort> = Arc::new(thumbnail_repo_impl);
    let thumbnail_generator =
        InfraThumbnailGenerator::new(128).map_err(|e| WiringError::ThumbnailInit(e.to_string()))?;
    let thumbnail_generator: Arc<dyn ThumbnailGeneratorPort> = Arc::new(thumbnail_generator);

    let secure_storage_for_key_material = Arc::clone(&secure_storage);

    let keyslot_store = JsonKeySlotStore::new(vault_path.clone());
    let keyslot_store: Arc<dyn uc_infra::fs::key_slot_store::KeySlotStore> =
        Arc::new(keyslot_store);

    let key_material_service =
        DefaultKeyMaterialService::new(secure_storage_for_key_material, keyslot_store);
    let key_material: Arc<dyn KeyMaterialPort> = Arc::new(key_material_service);

    let encryption: Arc<dyn EncryptionPort> = Arc::new(EncryptionRepository);

    let encryption_state: Arc<dyn uc_core::ports::security::encryption_state::EncryptionStatePort> =
        Arc::new(FileEncryptionStateRepository::new(vault_path.clone()));

    let settings_repo: Arc<dyn SettingsPort> = Arc::new(FileSettingsRepository::new(settings_path));

    let setup_status: Arc<dyn SetupStatusPort> =
        Arc::new(FileSetupStatusRepository::with_defaults(vault_path.clone()));

    let clock: Arc<dyn ClockPort> = Arc::new(SystemClock);
    let hash: Arc<dyn ContentHashPort> = Arc::new(Blake3Hasher);

    let selection_repo_impl = DieselClipboardSelectionRepository::new(Arc::clone(&db_executor));
    let selection_repo: Arc<dyn ClipboardSelectionRepositoryPort> = Arc::new(selection_repo_impl);

    let file_transfer_repo: Arc<dyn uc_core::ports::FileTransferRepositoryPort> =
        Arc::new(DieselFileTransferRepository::new(Arc::clone(&db_executor)));

    let infra = InfraLayer {
        clipboard_entry_repo,
        clipboard_event_repo,
        representation_repo,
        selection_repo,
        device_repo,
        paired_device_repo,
        blob_repository,
        thumbnail_repo,
        thumbnail_generator,
        key_material,
        encryption,
        encryption_state,
        settings_repo,
        setup_status,
        clock,
        hash,
        file_transfer_repo,
    };

    Ok(infra)
}

/// Create platform layer implementations
pub(crate) fn create_platform_layer(
    secure_storage: Arc<dyn SecureStoragePort>,
    config_dir: &PathBuf,
    platform_cmd_tx: PlatformCommandSender,
    encryption: Arc<dyn EncryptionPort>,
    blob_repository: Arc<dyn BlobRepositoryPort>,
    paired_device_repo: Arc<dyn PairedDeviceRepositoryPort>,
    clock: Arc<dyn ClockPort>,
    storage_config: Arc<ClipboardStorageConfig>,
    identity_store: Arc<dyn IdentityStorePort>,
    file_cache_dir: PathBuf,
) -> WiringResult<PlatformLayer> {
    let clipboard_impl = LocalClipboard::new()
        .map_err(|e| WiringError::ClipboardInit(format!("Failed to create clipboard: {}", e)))?;
    let clipboard_impl = Arc::new(clipboard_impl);
    let clipboard: Arc<dyn PlatformClipboardPort> = clipboard_impl.clone();
    let system_clipboard: Arc<dyn SystemClipboardPort> = clipboard_impl;

    let device_identity = LocalDeviceIdentity::load_or_create(config_dir.clone()).map_err(|e| {
        WiringError::SettingsInit(format!("Failed to create device identity: {}", e))
    })?;
    let device_identity: Arc<dyn DeviceIdentityPort> = Arc::new(device_identity);

    let blob_store_dir = config_dir.join("blobs");

    // Purge old blob files after V2 migration (old JSON format files are incompatible
    // with the new UCBL binary format). Uses a sentinel file so this only runs once.
    let sentinel = blob_store_dir.join(".v2_migrated");
    if blob_store_dir.exists() && !sentinel.exists() {
        match std::fs::read_dir(&blob_store_dir) {
            Ok(entries) => {
                let mut purged = 0u64;
                let mut errors = 0u64;
                for entry_result in entries {
                    let entry = match entry_result {
                        Ok(e) => e,
                        Err(e) => {
                            tracing::warn!(error = %e, "Failed to read directory entry during V2 migration");
                            errors += 1;
                            continue;
                        }
                    };
                    if entry.path().is_file() {
                        let path = entry.path();
                        if path.file_name().map_or(false, |n| n == ".v2_migrated") {
                            continue;
                        }
                        if is_v2_blob(&path) {
                            continue;
                        }
                        if let Err(e) = std::fs::remove_file(&path) {
                            tracing::warn!(
                                path = %path.display(),
                                error = %e,
                                "Failed to purge old blob file"
                            );
                            errors += 1;
                        } else {
                            purged += 1;
                        }
                    }
                }
                if purged > 0 {
                    tracing::info!(
                        count = purged,
                        "Purged old blob files (V2 format migration)"
                    );
                }

                if errors == 0 {
                    if let Err(e) = std::fs::File::create(&sentinel) {
                        tracing::warn!(error = %e, "Failed to create V2 migration sentinel");
                    }
                } else {
                    tracing::warn!(
                        errors = errors,
                        "Skipping V2 migration sentinel: {} errors during cleanup, will retry next startup",
                        errors
                    );
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to read blob directory for cleanup");
            }
        }
    }

    let blob_store: Arc<dyn BlobStorePort> = Arc::new(FilesystemBlobStore::new(blob_store_dir));

    let representation_normalizer: Arc<dyn ClipboardRepresentationNormalizerPort> =
        Arc::new(ClipboardRepresentationNormalizer::new(storage_config));

    let encryption_session: Arc<dyn EncryptionSessionPort> =
        Arc::new(InMemoryEncryptionSessionPort::new());
    let policy_resolver = Arc::new(ResolveConnectionPolicy::new(paired_device_repo.clone()));
    let transfer_decryptor: Arc<dyn TransferPayloadDecryptorPort> =
        Arc::new(uc_infra::clipboard::TransferPayloadDecryptorAdapter);
    let transfer_encryptor: Arc<dyn TransferPayloadEncryptorPort> =
        Arc::new(uc_infra::clipboard::TransferPayloadEncryptorAdapter);
    let libp2p_network = Arc::new(
        Libp2pNetworkAdapter::new(
            identity_store,
            policy_resolver,
            encryption_session.clone(),
            transfer_decryptor,
            transfer_encryptor,
            file_cache_dir,
        )
        .map_err(|e| {
            WiringError::NetworkInit(format!("Failed to initialize libp2p identity: {e}"))
        })?,
    );
    info!(peer_id = %libp2p_network.local_peer_id(), "Loaded libp2p identity");
    let network_ports = Arc::new(NetworkPorts {
        clipboard: libp2p_network.clone(),
        peers: libp2p_network.clone(),
        pairing: libp2p_network.clone(),
        events: libp2p_network.clone(),
        file_transfer: libp2p_network.clone(),
    });

    let encrypted_blob_store: Arc<dyn BlobStorePort> = Arc::new(EncryptedBlobStore::new(
        blob_store.clone(),
        encryption,
        encryption_session.clone(),
    ));

    let blob_writer: Arc<dyn BlobWriterPort> = Arc::new(BlobWriter::new(
        encrypted_blob_store.clone(),
        blob_repository,
        clock,
    ));

    let watcher_control: Arc<dyn WatcherControlPort> =
        Arc::new(InMemoryWatcherControl::new(platform_cmd_tx));

    let key_scope: Arc<dyn uc_core::ports::security::key_scope::KeyScopePort> =
        Arc::new(uc_platform::key_scope::DefaultKeyScope::new());

    Ok(PlatformLayer {
        clipboard,
        system_clipboard,
        secure_storage,
        network_ports,
        libp2p_network,
        device_identity,
        representation_normalizer,
        blob_writer,
        blob_store: encrypted_blob_store,
        encryption_session,
        watcher_control,
        key_scope,
    })
}

/// Resolves the application's default directories for storing data and configuration.
pub(crate) fn get_default_app_dirs() -> WiringResult<uc_core::app_dirs::AppDirs> {
    let adapter = DirsAppDirsAdapter::new();
    adapter
        .get_app_dirs()
        .map_err(|e| WiringError::ConfigInit(e.to_string()))
}

/// Get resolved storage paths from configuration.
pub fn get_storage_paths(
    config: &uc_core::config::AppConfig,
) -> WiringResult<uc_app::app_paths::AppPaths> {
    let platform_dirs = get_default_app_dirs()?;
    resolve_app_paths(&platform_dirs, config)
}

/// Resolve the effective `AppDirs` by applying config overrides.
pub(crate) fn resolve_app_dirs(
    platform_dirs: &uc_core::app_dirs::AppDirs,
    config: &AppConfig,
) -> uc_core::app_dirs::AppDirs {
    let is_in_memory_db = config.database_path.as_os_str() == ":memory:";
    let config_overrides_root = !config.database_path.as_os_str().is_empty() && !is_in_memory_db;

    if config_overrides_root {
        let raw_root = config
            .database_path
            .parent()
            .unwrap_or(&config.database_path)
            .to_path_buf();
        let abs_root = if raw_root.is_relative() {
            std::env::current_dir().unwrap_or_default().join(&raw_root)
        } else {
            raw_root
        };
        let app_data_root = apply_profile_suffix(abs_root);
        let app_cache_root = app_data_root.join("cache");
        uc_core::app_dirs::AppDirs {
            app_data_root,
            app_cache_root,
        }
    } else {
        platform_dirs.clone()
    }
}

/// Build `AppPaths` from platform dirs and config overrides.
pub(crate) fn resolve_app_paths(
    platform_dirs: &uc_core::app_dirs::AppDirs,
    config: &AppConfig,
) -> WiringResult<uc_app::app_paths::AppPaths> {
    let resolved_dirs = resolve_app_dirs(platform_dirs, config);
    let mut paths = uc_app::app_paths::AppPaths::from_app_dirs(&resolved_dirs);

    let is_in_memory_db = config.database_path.as_os_str() == ":memory:";

    if is_in_memory_db {
        paths.db_path = config.database_path.clone();
    } else if !config.database_path.as_os_str().is_empty() {
        let db_file_name = config
            .database_path
            .file_name()
            .map(|name| name.to_os_string())
            .unwrap_or_else(|| std::ffi::OsString::from("uniclipboard.db"));
        paths.db_path = paths.app_data_root.join(db_file_name);
    }

    if !config.vault_key_path.as_os_str().is_empty() {
        let configured_vault_root = config
            .vault_key_path
            .parent()
            .unwrap_or(&config.vault_key_path)
            .to_path_buf();

        if config.database_path.as_os_str().is_empty() {
            paths.vault_dir = apply_profile_suffix(configured_vault_root);
        } else {
            let configured_db_root = config
                .database_path
                .parent()
                .unwrap_or(&config.database_path)
                .to_path_buf();

            if configured_vault_root.starts_with(&configured_db_root) {
                let relative = configured_vault_root
                    .strip_prefix(&configured_db_root)
                    .unwrap_or(std::path::Path::new(""));
                paths.vault_dir = paths.app_data_root.join(relative);
            } else {
                paths.vault_dir = apply_profile_suffix(configured_vault_root);
            }
        }
    }

    Ok(paths)
}

pub(crate) fn apply_profile_suffix(path: PathBuf) -> PathBuf {
    let profile = match std::env::var("UC_PROFILE") {
        Ok(value) if !value.is_empty() => value.replace('/', "_").replace('\\', "_"),
        _ => return path,
    };

    let file_name = match path.file_name().and_then(|name| name.to_str()) {
        Some(name) => name.to_string(),
        None => return path,
    };

    let mut updated = path;
    updated.set_file_name(format!("{file_name}_{profile}"));
    updated
}

/// Wires and constructs the application's dependency graph, returning ready-to-use dependencies.
pub fn wire_dependencies(
    config: &AppConfig,
    platform_cmd_tx: PlatformCommandSender,
) -> WiringResult<WiredDependencies> {
    wire_dependencies_with_identity_store(config, platform_cmd_tx, None)
}

/// Wires dependencies with a caller-provided identity store.
///
/// This is primarily intended for tests or environments without system secure storage.
pub fn wire_dependencies_with_identity_store(
    config: &AppConfig,
    platform_cmd_tx: PlatformCommandSender,
    identity_store: Option<Arc<dyn IdentityStorePort>>,
) -> WiringResult<WiredDependencies> {
    let platform_dirs = get_default_app_dirs()?;
    let paths = resolve_app_paths(&platform_dirs, config)?;

    let db_path = paths.db_path;
    let db_pool = create_db_pool(&db_path)?;

    let vault_path = paths.vault_dir;
    let settings_path = paths.settings_path;

    let secure_storage =
        uc_platform::secure_storage::create_default_secure_storage_in_app_data_root(
            paths.app_data_root.clone(),
        )
        .map_err(|e| WiringError::SecureStorageInit(e.to_string()))?;

    let identity_store = identity_store.unwrap_or_else(|| {
        Arc::new(FileIdentityStore::new(paths.app_data_root.clone())) as Arc<dyn IdentityStorePort>
    });

    let infra = create_infra_layer(db_pool, &vault_path, &settings_path, secure_storage.clone())?;

    let storage_config = Arc::new(ClipboardStorageConfig::defaults());
    let platform = create_platform_layer(
        secure_storage,
        &vault_path,
        platform_cmd_tx,
        infra.encryption.clone(),
        infra.blob_repository.clone(),
        infra.paired_device_repo.clone(),
        infra.clock.clone(),
        storage_config.clone(),
        identity_store,
        paths.file_cache_dir.clone(),
    )?;

    // Wrap ports with encryption decorators
    let encrypting_event_writer: Arc<dyn ClipboardEventWriterPort> =
        Arc::new(EncryptingClipboardEventWriter::new(
            infra.clipboard_event_repo.clone(),
            infra.encryption.clone(),
            platform.encryption_session.clone(),
        ));

    let decrypting_rep_repo: Arc<dyn ClipboardRepresentationRepositoryPort> =
        Arc::new(DecryptingClipboardRepresentationRepository::new(
            infra.representation_repo.clone(),
            infra.encryption.clone(),
            platform.encryption_session.clone(),
        ));

    // Create background processing components
    let representation_cache = Arc::new(RepresentationCache::new(
        storage_config.cache_max_entries,
        storage_config.cache_max_bytes,
    ));
    let representation_cache_port: Arc<dyn RepresentationCachePort> = representation_cache.clone();

    let spool_dir = paths.spool_dir.clone();
    let spool_manager = Arc::new(
        SpoolManager::new(spool_dir.clone(), storage_config.spool_max_bytes)
            .map_err(|e| WiringError::BlobStorageInit(format!("Failed to create spool: {}", e)))?,
    );

    let (spool_tx, spool_rx) = mpsc::channel::<SpoolRequest>(100);
    let spool_queue: Arc<dyn SpoolQueuePort> = Arc::new(MpscSpoolQueue::new(spool_tx));
    let (worker_tx, worker_rx) = mpsc::channel::<RepresentationId>(100);

    let clipboard_change_origin: Arc<dyn ClipboardChangeOriginPort> =
        Arc::new(InMemoryClipboardChangeOrigin::new());

    let deps = AppDeps {
        clipboard: ClipboardPorts {
            clipboard: platform.clipboard,
            system_clipboard: platform.system_clipboard,
            clipboard_entry_repo: infra.clipboard_entry_repo,
            clipboard_event_repo: encrypting_event_writer,
            representation_repo: decrypting_rep_repo,
            representation_normalizer: platform.representation_normalizer,
            selection_repo: infra.selection_repo,
            representation_policy: Arc::new(SelectRepresentationPolicyV1::new()),
            representation_cache: representation_cache_port,
            spool_queue,
            clipboard_change_origin,
            worker_tx,
        },
        security: SecurityPorts {
            encryption: infra.encryption,
            encryption_session: platform.encryption_session,
            encryption_state: infra.encryption_state,
            key_scope: platform.key_scope,
            secure_storage: platform.secure_storage,
            key_material: infra.key_material,
        },
        device: DevicePorts {
            device_repo: infra.device_repo,
            device_identity: platform.device_identity,
            paired_device_repo: infra.paired_device_repo,
        },
        network_ports: platform.network_ports,
        network_control: platform.libp2p_network.clone(),
        setup_status: infra.setup_status,
        storage: StoragePorts {
            blob_store: platform.blob_store,
            blob_repository: infra.blob_repository,
            blob_writer: platform.blob_writer,
            thumbnail_repo: infra.thumbnail_repo,
            thumbnail_generator: infra.thumbnail_generator,
            file_transfer_repo: infra.file_transfer_repo,
        },
        settings: infra.settings_repo,
        system: SystemPorts {
            clock: infra.clock,
            hash: infra.hash,
            file_manager: Arc::new(uc_platform::file_manager::NativeFileManagerAdapter::new()),
            cache_fs: Arc::new(uc_infra::fs::TokioCacheFsAdapter::new()),
        },
    };

    Ok(WiredDependencies {
        deps,
        background: super::wiring::BackgroundRuntimeDeps {
            libp2p_network: platform.libp2p_network.clone(),
            representation_cache,
            spool_manager,
            spool_rx,
            worker_rx,
            spool_dir,
            file_cache_dir: paths.file_cache_dir.clone(),
            spool_ttl_days: storage_config.spool_ttl_days,
            worker_retry_max_attempts: storage_config.worker_retry_max_attempts,
            worker_retry_backoff_ms: storage_config.worker_retry_backoff_ms,
        },
        watcher_control: platform.watcher_control,
    })
}

const DEFAULT_PAIRING_DEVICE_NAME: &str = "Uniclipboard Device";

pub async fn resolve_pairing_device_name(settings: Arc<dyn SettingsPort>) -> String {
    match settings.load().await {
        Ok(settings) => {
            let name = settings.general.device_name.unwrap_or_default();
            if name.trim().is_empty() {
                DEFAULT_PAIRING_DEVICE_NAME.to_string()
            } else {
                name
            }
        }
        Err(err) => {
            warn!(error = %err, "Failed to load settings for pairing device name");
            DEFAULT_PAIRING_DEVICE_NAME.to_string()
        }
    }
}

pub async fn resolve_pairing_config(settings: Arc<dyn SettingsPort>) -> PairingConfig {
    match settings.load().await {
        Ok(settings) => PairingConfig::from_settings(&settings),
        Err(err) => {
            warn!(error = %err, "Failed to load settings for pairing config");
            PairingConfig::from_settings(&Settings::default())
        }
    }
}
