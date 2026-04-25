#![allow(deprecated)] // wires the frozen libp2p PairingTransportPort; replaced in Slice 5

//! # Pure Dependency Assembly
//!
//! This module contains all pure dependency construction functions that have
//! zero Tauri imports. It is the heart of the `uc-bootstrap` composition root.
//!
//! ## What lives here
//!
//! - `WiredDependencies` struct (output of the wiring process)
//! - `BackgroundRuntimeDeps` struct (background worker dependencies)
//! - `HostEventSetupPort` adapter (pure, no Tauri types)
//! - All infrastructure and platform layer construction functions
//! - `wire_dependencies`, `get_storage_paths`, `resolve_pairing_device_name`, etc.
//!
//! ## Architecture Principle
//!
//! > **Zero tauri imports in this file.**

use std::path::PathBuf;
use std::sync::Arc;

use tracing::{info, warn};

use uc_app::deps::{NetworkPorts, SearchPorts};
use uc_app::shared::host_event::{HostEvent, HostEventEmitterPort, SetupHostEvent};
use uc_app::usecases::ResolveConnectionPolicy;
use uc_app::{AppDeps, ClipboardPorts, DevicePorts, SecurityPorts, StoragePorts, SystemPorts};
use uc_application::pairing::PairingConfig;
use uc_application::setup::SetupEventPort;
use uc_core::blob::ports::{BlobReaderPort, BlobWriterPort};
use uc_core::clipboard::SelectRepresentationPolicyV1;
use uc_core::config::AppConfig;
use uc_core::ids::RepresentationId;
use uc_core::ports::blob::BlobReferenceRepositoryPort;
use uc_core::ports::clipboard::{
    ClipboardChangeOriginPort, ClipboardRepresentationNormalizerPort, RepresentationCachePort,
    SpoolQueuePort, SpoolRequest,
};
use uc_core::ports::*;
use uc_core::settings::model::Settings;
use uc_infra::blob::{BlobRepositoryPort, BlobStorePort, BlobWriter, FilesystemBlobStore};
use uc_infra::clipboard::{
    clipboard_change_origin, init_clipboard_change_origin, new_in_memory_change_origin,
    ClipboardPayloadResolver, ClipboardRepresentationNormalizer, DurableSpoolQueue,
    InfraThumbnailGenerator, RepresentationCache, SpoolManager,
};
use uc_infra::config::ClipboardStorageConfig;
use uc_infra::db::executor::DieselSqliteExecutor;
use uc_infra::db::mappers::{
    blob_mapper::BlobRowMapper, clipboard_entry_mapper::ClipboardEntryRowMapper,
    clipboard_event_mapper::ClipboardEventRowMapper,
    clipboard_selection_mapper::ClipboardSelectionRowMapper,
    peer_address_mapper::PeerAddressRowMapper,
    snapshot_representation_mapper::RepresentationRowMapper,
    space_member_mapper::SpaceMemberRowMapper, trusted_peer_mapper::TrustedPeerRowMapper,
};
use uc_infra::db::pool::{init_db_pool, DbPool};
use uc_infra::db::repositories::{
    DieselBlobReferenceRepository, DieselBlobRepository, DieselClipboardEntryRepository,
    DieselClipboardEventRepository, DieselClipboardRepresentationRepository,
    DieselClipboardSelectionRepository, DieselFileTransferRepository, DieselPeerAddressRepository,
    DieselSpaceMemberRepository, DieselThumbnailRepository, DieselTrustedPeerRepository,
};
use uc_infra::device::LocalDeviceIdentity;
use uc_infra::fs::key_slot_store::JsonKeySlotStore;
use uc_infra::search::{HkdfSearchKeyDerivation, SearchPipeline, SqliteSearchIndex};
use uc_infra::security::{
    Argon2PinHasher, Blake3Hasher, DecryptingClipboardRepresentationRepository, EncryptedBlobStore,
    EncryptingClipboardEventWriter, InMemorySession, KeyMaterialStore,
    Sha256IdentityFingerprintFactory, Sha256ShortCodeGenerator,
};
use uc_infra::settings::repository::FileSettingsRepository;
use uc_infra::{FileSetupStatusRepository, SystemClock};
use uc_platform::adapters::{DisabledPairingTransport, Libp2pNetworkAdapter, PairingRuntimeOwner};
use uc_platform::app_dirs::DirsAppDirsAdapter;
use uc_platform::clipboard::{LocalClipboard, NoopSystemClipboard};
use uc_platform::identity_store::FileIdentityStore;
use uc_platform::ports::{AppDirsPort, IdentityStorePort};

use tokio::sync::mpsc;

/// Result type for wiring operations
pub type WiringResult<T> = Result<T, WiringError>;

/// Errors during dependency injection
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

/// Background runtime components that must be started after async runtime is ready.
pub struct BackgroundRuntimeDeps {
    pub libp2p_network: Arc<Libp2pNetworkAdapter>,
    pub representation_cache: Arc<RepresentationCache>,
    pub spool_manager: Arc<SpoolManager>,
    /// Sender side of the legacy spool channel. Kept alive so that `SpoolerTask`
    /// (which drains `spool_rx`) does not immediately exit when no senders remain.
    /// `DurableSpoolQueue` bypasses this channel and writes to disk directly, so
    /// `spool_tx` is never actually used to send messages in normal operation.
    pub spool_tx: mpsc::Sender<SpoolRequest>,
    pub spool_rx: mpsc::Receiver<SpoolRequest>,
    pub worker_rx: mpsc::Receiver<RepresentationId>,
    pub spool_dir: PathBuf,
    pub file_cache_dir: PathBuf,
    pub spool_ttl_days: u64,
    pub worker_retry_max_attempts: u32,
    pub worker_retry_backoff_ms: u64,
    /// Event-sourced file transfer lifecycle: durable store + host-event
    /// publisher + 6 use cases, plus the sweep/reconcile runtime tasks.
    /// Holds a clone of the shared emitter_cell so it automatically sees
    /// emitter swaps (LoggingEventEmitter → DaemonApiEventEmitter).
    pub file_transfer_lifecycle: Arc<crate::file_transfer_lifecycle::FileTransferLifecycle>,
    /// Single write boundary for all programmatic clipboard writes.
    /// Centralises guard-registration + write + cleanup-on-error.
    pub clipboard_write_coordinator: Arc<uc_app::usecases::ClipboardWriteCoordinator>,
}

/// Fully wired dependencies plus background runtime components.
pub struct WiredDependencies {
    pub deps: AppDeps,
    pub background: BackgroundRuntimeDeps,
    /// Shared emitter cell created at wire time with the initial `LoggingHostEventEmitter`.
    /// Callers (GUI bootstrap, non-GUI bootstrap) use this same cell so that
    /// all consumers — CoreRuntime, SetupOrchestrator, and FileTransferOrchestrator —
    /// see the same emitter after any swap.
    pub emitter_cell: Arc<std::sync::RwLock<Arc<dyn HostEventEmitterPort>>>,
    /// Trusted-peer repository surfaced at the bootstrap boundary so the
    /// GUI / daemon builders can build the singleton `TrustPeerOrchestrator`
    /// without threading it through `AppDeps` (which is retiring together
    /// with uc-app). Scheduled to move into `uc-application` wiring
    /// infrastructure once uc-app is gone.
    pub trusted_peer_repo: Arc<dyn uc_core::TrustedPeerRepositoryPort>,
    /// Slice 2 Phase 1 · T5：peer address 仓库，由
    /// [`crate::space_setup::build_space_setup_assembly`] 注入 `SpaceSetupFacade`，
    /// 用于配对完成后 best-effort 写对端传输地址 blob。跟
    /// `trusted_peer_repo` 同样绕开 `AppDeps`：消费者在 uc-application 里。
    pub peer_addr_repo: Arc<dyn uc_core::ports::PeerAddressRepositoryPort>,
    /// Slice 3 Phase 1:iroh-blobs store 目录。由 `SpaceSetupAssembly`
    /// 装配 iroh blob handler 时使用。
    pub iroh_blob_store_dir: PathBuf,
    /// Slice 3 Phase 1:明文 hash → 密文 digest 去重缓存。
    pub blob_reference_repo: Arc<dyn BlobReferenceRepositoryPort>,
}

/// HostEventEmitterPort adapter that emits setup state changes to frontend listeners.
///
/// Uses Arc<RwLock<...>> shared cell so that HostEventSetupPort always reads the
/// current emitter after bootstrap swaps it from LoggingEventEmitter to DaemonApiEventEmitter.
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
        state: uc_application::setup::SetupState,
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

/// Infrastructure layer implementations
struct InfraLayer {
    // Clipboard repositories
    #[allow(dead_code)]
    clipboard_entry_repo: Arc<dyn ClipboardEntryRepositoryPort>,
    clipboard_event_repo: Arc<dyn ClipboardEventWriterPort>,
    representation_repo: Arc<dyn ClipboardRepresentationRepositoryPort>,
    selection_repo: Arc<dyn ClipboardSelectionRepositoryPort>,

    // Membership repository (phase 4b PR-4 起成为唯一持久成员层).
    member_repo: Arc<dyn uc_core::MemberRepositoryPort>,

    // Trusted-peer repository — authoritative write path from phase 0.4.2.
    // Drives `TrustPeerOrchestrator` at the pairing handler's PersistPairedDevice
    // boundary, replacing the previous `paired_device` upsert + `space_member`
    // shadow-write.
    trusted_peer_repo: Arc<dyn uc_core::TrustedPeerRepositoryPort>,

    // Slice 2 Phase 1 · T5：peer address 仓库。pairing 收尾点 best-effort
    // 写入对端传输地址，供 F1 `ensure_reachable_all` 直接拨号。
    peer_addr_repo: Arc<dyn uc_core::ports::PeerAddressRepositoryPort>,

    // Slice 3 Phase 1:明文 hash → 密文 digest 去重缓存。
    blob_reference_repo: Arc<dyn BlobReferenceRepositoryPort>,

    // Blob storage
    blob_repository: Arc<dyn BlobRepositoryPort>,
    thumbnail_repo: Arc<dyn ThumbnailRepositoryPort>,
    thumbnail_generator: Arc<dyn ThumbnailGeneratorPort>,

    // Security services
    key_material: Arc<KeyMaterialStore>,

    // Settings
    settings_repo: Arc<dyn SettingsPort>,

    // Setup status
    setup_status: Arc<dyn SetupStatusPort>,

    // System services
    clock: Arc<dyn ClockPort>,
    hash: Arc<dyn ContentHashPort>,

    // File transfer tracking (projection/read-model port).
    file_transfer_repo: Arc<dyn uc_core::ports::FileTransferRepositoryPort>,

    // File transfer durable event store + receiver-side projection updater.
    //
    // Exposed as the concrete type because receiver-side code calls
    // `seed_receiver_context`, which is not part of the `FileTransferEventStorePort`
    // surface on purpose (entry_id / cached_path are receiver-local concerns).
    file_transfer_store: Arc<crate::file_transfer_lifecycle::FileTransferEventStore>,
}

/// Platform layer implementations
pub struct PlatformLayer {
    // System clipboard
    pub clipboard: Arc<dyn PlatformClipboardPort>,
    pub system_clipboard: Arc<dyn SystemClipboardPort>,

    // Secure storage
    pub secure_storage: Arc<dyn SecureStoragePort>,

    // Network operations
    pub network_ports: Arc<NetworkPorts>,

    // libp2p network adapter (concrete)
    pub libp2p_network: Arc<Libp2pNetworkAdapter>,

    // Device identity
    pub device_identity: Arc<dyn DeviceIdentityPort>,

    // Clipboard representation normalizer
    pub representation_normalizer: Arc<dyn ClipboardRepresentationNormalizerPort>,

    // Blob writer
    pub blob_writer: Arc<dyn BlobWriterPort>,

    // Blob store (encrypted) — exposed to use cases as a read-only port.
    pub blob_store: Arc<dyn BlobReaderPort>,

    // 进程内会话——uc-infra 内部 adapter (SpaceAccessAdapter / BlobCipherAdapter /
    // TransferCipherAdapter / EncryptedBlobStore) 共享同一份 Arc。具体类型,
    // 不再走 EncryptionSessionPort trait dyn 间接层。
    pub session: Arc<InMemorySession>,

    // Current profile
    pub current_profile: Arc<dyn uc_core::ports::security::current_profile::CurrentProfilePort>,
}

/// Create SQLite database connection pool
pub fn create_db_pool(db_path: &PathBuf) -> WiringResult<DbPool> {
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

    let member_repo_impl =
        DieselSpaceMemberRepository::new(Arc::clone(&db_executor), SpaceMemberRowMapper);
    let member_repo: Arc<dyn uc_core::MemberRepositoryPort> = Arc::new(member_repo_impl);

    let trusted_peer_repo_impl =
        DieselTrustedPeerRepository::new(Arc::clone(&db_executor), TrustedPeerRowMapper);
    let trusted_peer_repo: Arc<dyn uc_core::TrustedPeerRepositoryPort> =
        Arc::new(trusted_peer_repo_impl);

    let peer_addr_repo_impl =
        DieselPeerAddressRepository::new(Arc::clone(&db_executor), PeerAddressRowMapper);
    let peer_addr_repo: Arc<dyn uc_core::ports::PeerAddressRepositoryPort> =
        Arc::new(peer_addr_repo_impl);

    let blob_reference_repo: Arc<dyn BlobReferenceRepositoryPort> =
        Arc::new(DieselBlobReferenceRepository::new(Arc::clone(&db_executor)));

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

    let key_material = Arc::new(KeyMaterialStore::new(
        secure_storage_for_key_material,
        keyslot_store,
    ));

    let settings_repo: Arc<dyn SettingsPort> = Arc::new(FileSettingsRepository::new(settings_path));

    let setup_status: Arc<dyn SetupStatusPort> =
        Arc::new(FileSetupStatusRepository::with_defaults(vault_path.clone()));

    let clock: Arc<dyn ClockPort> = Arc::new(SystemClock);
    let hash: Arc<dyn ContentHashPort> = Arc::new(Blake3Hasher);

    let selection_repo_impl = DieselClipboardSelectionRepository::new(Arc::clone(&db_executor));
    let selection_repo: Arc<dyn ClipboardSelectionRepositoryPort> = Arc::new(selection_repo_impl);

    let file_transfer_repo: Arc<dyn uc_core::ports::FileTransferRepositoryPort> =
        Arc::new(DieselFileTransferRepository::new(Arc::clone(&db_executor)));

    let file_transfer_store = Arc::new(
        uc_infra::file_transfer::SqliteReceiverFileTransferStore::new(Arc::clone(&db_executor)),
    );

    let infra = InfraLayer {
        clipboard_entry_repo,
        clipboard_event_repo,
        representation_repo,
        selection_repo,
        member_repo,
        trusted_peer_repo,
        peer_addr_repo,
        blob_reference_repo,
        blob_repository,
        thumbnail_repo,
        thumbnail_generator,
        key_material,
        settings_repo,
        setup_status,
        clock,
        hash,
        file_transfer_repo,
        file_transfer_store,
    };

    Ok(infra)
}

/// Create platform layer implementations
fn build_network_ports(
    libp2p_network: Arc<Libp2pNetworkAdapter>,
    pairing_runtime_owner: PairingRuntimeOwner,
) -> Arc<NetworkPorts> {
    let pairing: Arc<dyn PairingTransportPort> = match pairing_runtime_owner {
        PairingRuntimeOwner::CurrentProcess => libp2p_network.clone(),
        PairingRuntimeOwner::ExternalDaemon => Arc::new(DisabledPairingTransport),
    };

    Arc::new(NetworkPorts {
        clipboard_outbound: libp2p_network.clone(),
        clipboard_inbound: libp2p_network.clone(),
        peers: libp2p_network.clone(),
        pairing,
        events: libp2p_network.clone(),
        file_transfer: libp2p_network.clone(),
        file_transfer_events: libp2p_network.clone(),
    })
}

pub fn create_platform_layer(
    secure_storage: Arc<dyn SecureStoragePort>,
    config_dir: &PathBuf,
    blob_repository: Arc<dyn BlobRepositoryPort>,
    member_repo: Arc<dyn uc_core::MemberRepositoryPort>,
    clock: Arc<dyn ClockPort>,
    storage_config: Arc<ClipboardStorageConfig>,
    identity_store: Arc<dyn IdentityStorePort>,
    file_cache_dir: PathBuf,
    pairing_runtime_owner: PairingRuntimeOwner,
) -> WiringResult<PlatformLayer> {
    // Slice 1 CLI commands (init/invite/join) do not touch the system
    // clipboard, but a non-bundled CLI launched from a shell lacks the
    // WindowServer / AppKit context that `clipboard-rs` assumes, so
    // `LocalClipboard::new()` panics inside `+[NSPasteboard generalPasteboard]`.
    // When `UC_DISABLE_SYSTEM_CLIPBOARD=1` is set we skip the real
    // adapter entirely and wire in `NoopSystemClipboard`. The CLI sets
    // this variable before bootstrap; GUI / daemon paths leave it unset
    // and get the real adapter.
    let (clipboard, system_clipboard): (
        Arc<dyn PlatformClipboardPort>,
        Arc<dyn SystemClipboardPort>,
    ) = if std::env::var_os("UC_DISABLE_SYSTEM_CLIPBOARD").is_some() {
        tracing::info!(
            "UC_DISABLE_SYSTEM_CLIPBOARD set; substituting NoopSystemClipboard \
             (any clipboard capture / write is a no-op)"
        );
        let noop: Arc<NoopSystemClipboard> = Arc::new(NoopSystemClipboard);
        (noop.clone(), noop)
    } else {
        let clipboard_impl = LocalClipboard::new().map_err(|e| {
            WiringError::ClipboardInit(format!("Failed to create clipboard: {}", e))
        })?;
        let clipboard_impl = Arc::new(clipboard_impl);
        (clipboard_impl.clone(), clipboard_impl)
    };

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

    // 进程内会话: uc-infra adapter 共享的具体类型,替换历史
    // InMemoryEncryptionSessionPort + EncryptionSessionPort trait dyn 间接层。
    let session = Arc::new(InMemorySession::new());
    let policy_resolver = Arc::new(ResolveConnectionPolicy::new(member_repo.clone()));
    let libp2p_network = Arc::new(
        Libp2pNetworkAdapter::new(
            identity_store,
            policy_resolver,
            file_cache_dir,
            pairing_runtime_owner,
        )
        .map_err(|e| {
            WiringError::NetworkInit(format!("Failed to initialize libp2p identity: {e}"))
        })?,
    );
    info!(peer_id = %libp2p_network.local_peer_id(), "Loaded libp2p identity");
    let network_ports = build_network_ports(libp2p_network.clone(), pairing_runtime_owner);

    let encrypted_blob_store =
        Arc::new(EncryptedBlobStore::new(blob_store.clone(), session.clone()));

    // BlobWriter needs the put-side (BlobStorePort); use cases need only the
    // read-side (BlobReaderPort). Both views point at the same concrete
    // EncryptedBlobStore instance.
    let encrypted_blob_store_for_writer: Arc<dyn BlobStorePort> = encrypted_blob_store.clone();
    let blob_writer: Arc<dyn BlobWriterPort> = Arc::new(BlobWriter::new(
        encrypted_blob_store_for_writer,
        blob_repository,
        clock,
    ));
    let blob_store_reader: Arc<dyn BlobReaderPort> = encrypted_blob_store;

    let current_profile: Arc<dyn uc_core::ports::security::current_profile::CurrentProfilePort> =
        Arc::new(uc_infra::security::DefaultCurrentProfile::new());

    Ok(PlatformLayer {
        clipboard,
        system_clipboard,
        secure_storage,
        network_ports,
        libp2p_network,
        device_identity,
        representation_normalizer,
        blob_writer,
        blob_store: blob_store_reader,
        session,
        current_profile,
    })
}

/// Resolves the application's default directories for storing data and configuration.
pub fn get_default_app_dirs() -> WiringResult<uc_core::app_dirs::AppDirs> {
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

/// Build `AppPaths` from platform dirs and config overrides.
pub fn resolve_app_paths(
    platform_dirs: &uc_core::app_dirs::AppDirs,
    config: &AppConfig,
) -> WiringResult<uc_app::app_paths::AppPaths> {
    let mut paths = uc_app::app_paths::AppPaths::from_app_dirs(platform_dirs);

    let is_in_memory_db = config.database_path.as_os_str() == ":memory:";

    if is_in_memory_db {
        paths.db_path = config.database_path.clone();
    } else if !config.database_path.as_os_str().is_empty() {
        if config.database_path.is_absolute() {
            // Absolute path: use as-is. In production the path is already inside
            // app_data_root_dir; tests use temp dirs and need the full path respected.
            paths.db_path = config.database_path.clone();
        } else {
            let db_file_name = config
                .database_path
                .file_name()
                .map(|name| name.to_os_string())
                .unwrap_or_else(|| std::ffi::OsString::from("uniclipboard.db"));
            paths.db_path = paths.app_data_root_dir.join(db_file_name);
        }
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
                paths.vault_dir = paths.app_data_root_dir.join(relative);
            } else {
                paths.vault_dir = apply_profile_suffix(configured_vault_root);
            }
        }
    }

    Ok(paths)
}

pub fn apply_profile_suffix(path: PathBuf) -> PathBuf {
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
    pairing_runtime_owner: PairingRuntimeOwner,
) -> WiringResult<WiredDependencies> {
    wire_dependencies_with_identity_store(config, None, pairing_runtime_owner)
}

/// Wires dependencies with a caller-provided identity store.
///
/// This is primarily intended for tests or environments without system secure storage.
pub fn wire_dependencies_with_identity_store(
    config: &AppConfig,
    identity_store: Option<Arc<dyn IdentityStorePort>>,
    pairing_runtime_owner: PairingRuntimeOwner,
) -> WiringResult<WiredDependencies> {
    let platform_dirs = get_default_app_dirs()?;
    let paths = resolve_app_paths(&platform_dirs, config)?;

    let db_path = paths.db_path;
    let db_pool = create_db_pool(&db_path)?;
    // Clone pool before infra layer consumes it — search bundle needs the same pool.
    let db_pool_for_search = db_pool.clone();

    let vault_path = paths.vault_dir;
    let settings_path = paths.settings_path;

    let secure_storage =
        uc_platform::secure_storage::create_default_secure_storage_in_app_data_root(
            paths.app_data_root_dir.clone(),
        )
        .map_err(|e| WiringError::SecureStorageInit(e.to_string()))?;

    let identity_store = identity_store.unwrap_or_else(|| {
        Arc::new(FileIdentityStore::new(paths.app_data_root_dir.clone()))
            as Arc<dyn IdentityStorePort>
    });

    let infra = create_infra_layer(db_pool, &vault_path, &settings_path, secure_storage.clone())?;

    let storage_config = Arc::new(ClipboardStorageConfig::defaults());
    let platform = create_platform_layer(
        secure_storage,
        &vault_path,
        infra.blob_repository.clone(),
        infra.member_repo.clone(),
        infra.clock.clone(),
        storage_config.clone(),
        identity_store,
        paths.file_cache_dir.clone(),
        pairing_runtime_owner,
    )?;

    // SpaceAccessPort——单一会话/密钥访问入口。adapter 自管 KeyMaterialStore +
    // InMemorySession + CurrentProfilePort,V1 AEAD 走 v1_aead helper。
    // Phase C 起不再依赖 EncryptionStatePort (已物理删除);adapter 用
    // `key_material.keyslot_exists()` 判断是否已初始化。
    let space_access: Arc<dyn uc_core::ports::space::SpaceAccessPort> =
        Arc::new(uc_infra::security::DefaultSpaceAccessAdapter::new(
            infra.key_material.clone(),
            platform.current_profile.clone(),
            platform.session.clone(),
        ));

    // Wire the search bundle (Phase 92).
    let search_key_derivation: Arc<dyn SearchKeyDerivationPort> = Arc::new(
        HkdfSearchKeyDerivation::new(space_access.clone(), platform.current_profile.clone()),
    );
    let search_index: Arc<dyn SearchIndexPort> = Arc::new(SqliteSearchIndex::new(
        db_pool_for_search,
        platform.current_profile.clone(),
        search_key_derivation.clone(),
    ));
    let search_pipeline = Arc::new(SearchPipeline::new());

    // BlobCipherPort——4 个 decorator 共享的业务 AEAD adapter。
    let blob_cipher: Arc<dyn uc_core::ports::security::BlobCipherPort> = Arc::new(
        uc_infra::security::BlobCipherAdapter::new(platform.session.clone()),
    );

    // TransferCipherPort——sync_outbound / sync_inbound 通过此 port 加解密
    // 网络字节,与 BlobCipherPort 共享同一个 InMemorySession。
    let transfer_cipher: Arc<dyn uc_core::ports::security::TransferCipherPort> = Arc::new(
        uc_infra::clipboard::TransferCipherAdapter::new(platform.session.clone()),
    );

    // Wrap ports with encryption decorators
    let encrypting_event_writer: Arc<dyn ClipboardEventWriterPort> =
        Arc::new(EncryptingClipboardEventWriter::new(
            infra.clipboard_event_repo.clone(),
            blob_cipher.clone(),
        ));

    let decrypting_rep_repo: Arc<dyn ClipboardRepresentationRepositoryPort> =
        Arc::new(DecryptingClipboardRepresentationRepository::new(
            infra.representation_repo.clone(),
            blob_cipher.clone(),
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
    let (worker_tx, worker_rx) = mpsc::channel::<RepresentationId>(100);

    // DurableSpoolQueue writes bytes to disk synchronously before returning,
    // ensuring spool files survive process exits. The in-memory MpscSpoolQueue
    // used previously only enqueued bytes into a channel; if the app exited
    // before SpoolerTask drained the channel, the bytes were permanently lost.
    let spool_queue: Arc<dyn SpoolQueuePort> = Arc::new(DurableSpoolQueue::new(
        spool_manager.clone(),
        worker_tx.clone(),
    ));

    let origin_impl = new_in_memory_change_origin();
    init_clipboard_change_origin(origin_impl.clone());
    let clipboard_change_origin =
        clipboard_change_origin().expect("clipboard_change_origin not initialized");

    // Extract the concrete file-transfer store before moving the rest of InfraLayer
    // into AppDeps — it is not exposed through uc-app ports (the use cases see it
    // as `Arc<dyn FileTransferEventStorePort>`), so it travels via BackgroundRuntimeDeps.
    let file_transfer_store_arc = Arc::clone(&infra.file_transfer_store);

    // Clone the trusted-peer repository handle before moving `infra` into
    // `AppDeps` below — the builders (build_gui_app / build_daemon_app) need
    // it to construct the `TrustPeerOrchestrator` singleton (D19). We do not
    // thread it through `AppDeps` because uc-app is retiring (D13) and
    // the repository is consumed solely by uc-application wiring.
    let trusted_peer_repo_for_wiring = Arc::clone(&infra.trusted_peer_repo);
    // Same pattern for `peer_addr_repo` — Slice 2 Phase 1 wiring consumer
    // is `SpaceSetupFacade`, which lives in uc-application, not uc-app.
    let peer_addr_repo_for_wiring = Arc::clone(&infra.peer_addr_repo);
    let blob_reference_repo_for_wiring = Arc::clone(&infra.blob_reference_repo);
    let iroh_blob_store_dir_for_wiring =
        apply_profile_suffix(paths.app_data_root_dir.join("iroh-blobs"));

    // Create payload resolver for resolving staged/processing payloads
    let payload_resolver: Arc<dyn ClipboardPayloadResolverPort> =
        Arc::new(ClipboardPayloadResolver::new(
            representation_cache.clone(),
            spool_manager.clone(),
            worker_tx.clone(),
        ));

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
            payload_resolver,
        },
        security: SecurityPorts {
            current_profile: platform.current_profile,
            secure_storage: platform.secure_storage,
            space_access: space_access.clone(),
            blob_cipher: blob_cipher.clone(),
            transfer_cipher: transfer_cipher.clone(),
            pin_hasher: Arc::new(Argon2PinHasher),
            short_code: Arc::new(Sha256ShortCodeGenerator),
            fingerprint: Arc::new(Sha256IdentityFingerprintFactory),
        },
        device: DevicePorts {
            device_identity: platform.device_identity,
            member_repo: infra.member_repo,
        },
        network_ports: platform.network_ports,
        network_control: platform.libp2p_network.clone(),
        setup_status: infra.setup_status,
        storage: StoragePorts {
            blob_store: platform.blob_store,
            blob_writer: platform.blob_writer,
            thumbnail_repo: infra.thumbnail_repo,
            thumbnail_generator: infra.thumbnail_generator,
            file_transfer_repo: infra.file_transfer_repo,
        },
        settings: infra.settings_repo,
        system: SystemPorts {
            clock: infra.clock,
            hash: infra.hash,
            cache_fs: Arc::new(uc_infra::fs::TokioCacheFsAdapter::new()),
        },
        search: SearchPorts {
            search_index,
            search_key_derivation,
            search_pipeline,
        },
    };

    // Create shared emitter cell at wire time using the logging placeholder.
    // All consumers (CoreRuntime, SetupOrchestrator, FileTransferOrchestrator)
    // hold a clone of this cell and automatically see the emitter after any swap.
    let initial_emitter: Arc<dyn HostEventEmitterPort> =
        Arc::new(crate::non_gui_runtime::LoggingHostEventEmitter);
    let emitter_cell: Arc<std::sync::RwLock<Arc<dyn HostEventEmitterPort>>> =
        Arc::new(std::sync::RwLock::new(initial_emitter));

    let file_transfer_lifecycle = Arc::new(
        crate::file_transfer_lifecycle::build_file_transfer_lifecycle(
            Arc::clone(&file_transfer_store_arc),
            emitter_cell.clone(),
            deps.storage.file_transfer_repo.clone(),
            deps.system.clock.clone(),
        ),
    );

    let clipboard_write_coordinator = build_clipboard_write_coordinator(
        deps.clipboard.system_clipboard.clone(),
        deps.clipboard.clipboard_change_origin.clone(),
    );

    Ok(WiredDependencies {
        deps,
        trusted_peer_repo: trusted_peer_repo_for_wiring,
        peer_addr_repo: peer_addr_repo_for_wiring,
        iroh_blob_store_dir: iroh_blob_store_dir_for_wiring,
        blob_reference_repo: blob_reference_repo_for_wiring,
        background: BackgroundRuntimeDeps {
            libp2p_network: platform.libp2p_network.clone(),
            representation_cache,
            spool_manager,
            spool_tx,
            spool_rx,
            worker_rx,
            spool_dir,
            file_cache_dir: paths.file_cache_dir.clone(),
            spool_ttl_days: storage_config.spool_ttl_days,
            worker_retry_max_attempts: storage_config.worker_retry_max_attempts,
            worker_retry_backoff_ms: storage_config.worker_retry_backoff_ms,
            file_transfer_lifecycle,
            clipboard_write_coordinator,
        },
        emitter_cell,
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

// ---------------------------------------------------------------------------
// SetupAssemblyPorts — network/external adapter ports for SetupOrchestrator
// ---------------------------------------------------------------------------

use tokio::sync::Mutex as TokioMutex;
use uc_app::usecases::{
    DeviceAnnouncer, LifecycleEventEmitter, LifecycleStatusPort, SessionReadyEmitter,
};
use uc_application::pairing::PairingFacade;
use uc_application::setup::{SetupFacade, SetupPairingFacadePort};
use uc_application::space_access::SpaceAccessFacade;
use uc_core::ports::space::SpaceAccessTransportPort;
use uc_core::ports::TimerPort;
use uc_core::TrustedPeerRepositoryPort;

/// Bundle of network/external adapter ports needed to assemble the SetupOrchestrator.
///
/// Replaces SetupRuntimePorts from runtime.rs. Contains ONLY network/external
/// adapter ports that the caller (main.rs/wiring.rs) provides and that are NOT
/// shared with AppRuntime or CoreRuntime. All shared/dual-use values
/// (emitter_cell, lifecycle_status, session_ready_emitter) are separate
/// parameters to build_setup_facade(), ensuring with_setup() can pass
/// the SAME instance to both the orchestrator and AppRuntime/CoreRuntime.
pub struct SetupAssemblyPorts {
    pub setup_pairing_facade: Arc<dyn SetupPairingFacadePort>,
    pub space_access_facade: Arc<SpaceAccessFacade>,
    pub device_announcer: Option<Arc<dyn DeviceAnnouncer>>,
    pub lifecycle_emitter: Arc<dyn LifecycleEventEmitter>,
    /// Trusted-peer repository (0.4.3): consumed by
    /// `SpaceAccessPersistenceAdapter` to verify pairing-side persistence
    /// before space_access promotes the peer. Same Arc as the one injected
    /// into `TrustPeerOrchestrator` in bootstrap, shared across the
    /// process.
    pub trusted_peer_repo: Arc<dyn TrustedPeerRepositoryPort>,
}

impl SetupAssemblyPorts {
    /// Create a bundle wired to the production network/lifecycle adapters.
    pub fn from_network(
        pairing_facade: Arc<PairingFacade>,
        space_access_facade: Arc<SpaceAccessFacade>,
        device_announcer: Option<Arc<dyn DeviceAnnouncer>>,
        lifecycle_emitter: Arc<dyn LifecycleEventEmitter>,
        trusted_peer_repo: Arc<dyn TrustedPeerRepositoryPort>,
    ) -> Self {
        Self {
            setup_pairing_facade: pairing_facade,
            space_access_facade,
            device_announcer,
            lifecycle_emitter,
            trusted_peer_repo,
        }
    }

    /// Create placeholder ports for tests. All ports are noop implementations.
    /// Used by AppRuntime::new() for command tests that don't exercise setup flow.
    ///
    /// NOTE: Shared state (emitter_cell, lifecycle_status, clipboard_integration_mode)
    /// and with_setup()-constructed adapters (session_ready_emitter) are NOT created
    /// here — they are created by AppRuntime::new() / with_setup() and passed
    /// separately to build_setup_facade().
    pub fn placeholder(_deps: &uc_app::AppDeps) -> Self {
        struct NoopSetupPairingFacade;

        #[async_trait::async_trait]
        impl SetupPairingFacadePort for NoopSetupPairingFacade {
            async fn subscribe(
                &self,
            ) -> anyhow::Result<
                tokio::sync::mpsc::Receiver<uc_application::pairing::PairingDomainEvent>,
            > {
                let (_tx, rx) = tokio::sync::mpsc::channel(1);
                Ok(rx)
            }

            async fn initiate_pairing(&self, _peer_id: String) -> anyhow::Result<String> {
                Err(anyhow::anyhow!(
                    "setup pairing facade placeholder cannot initiate pairing"
                ))
            }

            async fn accept_pairing(&self, _session_id: &str) -> anyhow::Result<()> {
                Ok(())
            }

            async fn reject_pairing(&self, _session_id: &str) -> anyhow::Result<()> {
                Ok(())
            }

            async fn cancel_pairing(&self, _session_id: &str) -> anyhow::Result<()> {
                Ok(())
            }

            async fn verify_pairing(
                &self,
                _session_id: &str,
                _pin_matches: bool,
            ) -> anyhow::Result<()> {
                Ok(())
            }
        }

        struct NoopTrustedPeerRepository;

        #[async_trait::async_trait]
        impl TrustedPeerRepositoryPort for NoopTrustedPeerRepository {
            async fn get(
                &self,
                _peer_device_id: &uc_core::DeviceId,
            ) -> Result<Option<uc_core::TrustedPeer>, uc_core::TrustedPeerError> {
                Ok(None)
            }

            async fn list(&self) -> Result<Vec<uc_core::TrustedPeer>, uc_core::TrustedPeerError> {
                Ok(Vec::new())
            }

            async fn save(
                &self,
                _trusted_peer: &uc_core::TrustedPeer,
            ) -> Result<(), uc_core::TrustedPeerError> {
                Ok(())
            }

            async fn remove(
                &self,
                _peer_device_id: &uc_core::DeviceId,
            ) -> Result<bool, uc_core::TrustedPeerError> {
                Ok(false)
            }
        }

        Self {
            setup_pairing_facade: Arc::new(NoopSetupPairingFacade),
            space_access_facade: Arc::new(SpaceAccessFacade::new()),
            device_announcer: None,
            lifecycle_emitter: Arc::new(uc_app::usecases::LoggingLifecycleEventEmitter),
            trusted_peer_repo: Arc::new(NoopTrustedPeerRepository),
        }
    }
}

/// Constructs a `ClipboardWriteCoordinator` — the single write boundary for all
/// programmatic clipboard writes.
///
/// Centralises the guard-registration + write + cleanup-on-error pattern
/// (previously duplicated across restore_clipboard_selection, sync_inbound, copy_file_to_clipboard).
pub fn build_clipboard_write_coordinator(
    system_clipboard: Arc<dyn uc_core::ports::clipboard::SystemClipboardPort>,
    clipboard_change_origin: Arc<dyn ClipboardChangeOriginPort>,
) -> Arc<uc_app::usecases::ClipboardWriteCoordinator> {
    Arc::new(uc_app::usecases::ClipboardWriteCoordinator::new(
        system_clipboard,
        clipboard_change_origin,
    ))
}

/// Build the `SetupFacade` with all required adapters.
///
/// This is the single composition point for setup (RNTM-05 / phase B.4).
/// Returns the phase-B `SetupFacade`; the internal `SetupOrchestrator` is
/// hidden per `uc-application/AGENTS.md` §11.4.
pub fn build_setup_facade(
    deps: &uc_app::AppDeps,
    ports: SetupAssemblyPorts,
    lifecycle_status: Arc<dyn LifecycleStatusPort>,
    emitter_cell: Arc<std::sync::RwLock<Arc<dyn HostEventEmitterPort>>>,
    session_ready_emitter: Arc<dyn SessionReadyEmitter>,
) -> Arc<SetupFacade> {
    use uc_app::usecases::{
        AppLifecycleCoordinator, AppLifecycleCoordinatorDeps, StartNetworkAfterUnlock,
    };

    // Phase C: `InitializeEncryption` usecase 保留给 uc-cli run_new_space 入口;
    // setup flow action_executor 现在直接调 `SpaceAccessPort.initialize`,不再绕
    // `SetupInitializeEncryptionPort` 适配层(已删除)。

    let start_network = Arc::new(StartNetworkAfterUnlock::from_port(
        deps.network_control.clone(),
    ));
    let app_lifecycle = Arc::new(AppLifecycleCoordinator::from_deps(
        AppLifecycleCoordinatorDeps {
            network: start_network,
            announcer: ports.device_announcer,
            emitter: session_ready_emitter,
            status: lifecycle_status,
            lifecycle_emitter: ports.lifecycle_emitter,
        },
    ));
    let space_access_port: Arc<dyn uc_core::ports::space::SpaceAccessPort> =
        deps.security.space_access.clone();
    let transport_port: Arc<TokioMutex<dyn SpaceAccessTransportPort>> = Arc::new(TokioMutex::new(
        uc_application::space_access::SpaceAccessNetworkAdapter::new(
            deps.network_ports.pairing.clone(),
            ports.space_access_facade.context_handle(),
        ),
    ));
    let proof_port: Arc<dyn uc_core::ports::space::ProofPort> = Arc::new(
        uc_application::space_access::HmacProofAdapter::new_with_space_access(
            deps.security.space_access.clone(),
        ),
    );
    let timer_port: Arc<TokioMutex<dyn TimerPort>> =
        Arc::new(TokioMutex::new(uc_infra::time::Timer::new()));
    let persistence_port = Arc::new(TokioMutex::new(
        uc_application::space_access::SpaceAccessPersistenceAdapter::new(
            ports.trusted_peer_repo.clone(),
        ),
    ));
    let setup_event_port = Arc::new(HostEventSetupPort::new(emitter_cell));

    Arc::new(SetupFacade::new(
        deps.setup_status.clone(),
        app_lifecycle,
        ports.setup_pairing_facade,
        setup_event_port,
        ports.space_access_facade,
        deps.network_control.clone(),
        space_access_port,
        deps.network_ports.pairing.clone(),
        transport_port,
        proof_port,
        timer_port,
        persistence_port,
    ))
}
