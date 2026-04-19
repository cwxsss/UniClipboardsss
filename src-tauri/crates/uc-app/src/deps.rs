//! # Application Dependencies / 应用依赖
//!
//! This module defines the dependency grouping for App construction.
//! 此模块定义 App 构造的依赖分组。
//!
//! **Note / 注意**: This is NOT a Builder pattern.
//! **这不是 Builder 模式。**
//! - No build steps / 无构建步骤
//! - No default values / 无默认值
//! - No hidden logic / 无隐藏逻辑
//! - Just parameter grouping / 仅用于参数打包

use std::sync::Arc;
use tokio::sync::mpsc;
use uc_core::blob::ports::{BlobReaderPort, BlobWriterPort};
use uc_core::ids::RepresentationId;
use uc_core::ports::clipboard::{
    ClipboardChangeOriginPort, ClipboardPayloadResolverPort, ClipboardRepresentationNormalizerPort,
    RepresentationCachePort, SpoolQueuePort, SystemClipboardPort, ThumbnailGeneratorPort,
    ThumbnailRepositoryPort,
};
use uc_core::ports::file_transport::FileTransportPort;
use uc_core::ports::search::search_index::SearchIndexPort;
use uc_core::ports::search::search_key::SearchKeyDerivationPort;
use uc_core::ports::search::search_pipeline::SearchPipelinePort;
use uc_core::ports::*;
use uc_core::MemberRepositoryPort;

/// Focused network capability bundle for dependency injection.
/// 用于依赖注入的网络能力聚合。
pub struct NetworkPorts {
    /// Outbound clipboard transport capability (`Arc<dyn ClipboardOutboundTransportPort>`).
    /// 出站剪贴板传输能力（`Arc<dyn ClipboardOutboundTransportPort>`）。
    pub clipboard_outbound: Arc<dyn ClipboardOutboundTransportPort>,
    /// Inbound clipboard transport capability (`Arc<dyn ClipboardInboundTransportPort>`).
    /// 入站剪贴板传输能力（`Arc<dyn ClipboardInboundTransportPort>`）。
    pub clipboard_inbound: Arc<dyn ClipboardInboundTransportPort>,
    /// Peer directory capability (`Arc<dyn PeerDirectoryPort>`).
    /// 对等端目录能力（`Arc<dyn PeerDirectoryPort>`）。
    pub peers: Arc<dyn PeerDirectoryPort>,
    /// Pairing transport capability (`Arc<dyn PairingTransportPort>`).
    /// 配对传输能力（`Arc<dyn PairingTransportPort>`）。
    pub pairing: Arc<dyn PairingTransportPort>,
    /// Network event subscription capability (`Arc<dyn NetworkEventPort>`).
    /// 网络事件订阅能力（`Arc<dyn NetworkEventPort>`）。
    pub events: Arc<dyn NetworkEventPort>,
    /// File transfer transport capability (`Arc<dyn FileTransportPort>`).
    /// 文件传输能力（`Arc<dyn FileTransportPort>`）。
    pub file_transfer: Arc<dyn FileTransportPort>,
    /// File-transfer domain event inbound stream from the platform layer.
    /// 文件传输领域事件入站流（由平台层产出）。
    pub file_transfer_events: Arc<dyn uc_core::file_transfer::FileTransferEventInboundPort>,
}

/// Clipboard-domain ports bundle.
/// 剪贴板领域端口组。
pub struct ClipboardPorts {
    pub clipboard: Arc<dyn PlatformClipboardPort>,
    pub system_clipboard: Arc<dyn SystemClipboardPort>,
    pub clipboard_entry_repo: Arc<dyn ClipboardEntryRepositoryPort>,
    pub clipboard_event_repo: Arc<dyn ClipboardEventWriterPort>,
    pub representation_repo: Arc<dyn ClipboardRepresentationRepositoryPort>,
    pub representation_normalizer: Arc<dyn ClipboardRepresentationNormalizerPort>,
    pub selection_repo: Arc<dyn ClipboardSelectionRepositoryPort>,
    pub representation_policy: Arc<dyn SelectRepresentationPolicyPort>,
    pub representation_cache: Arc<dyn RepresentationCachePort>,
    pub spool_queue: Arc<dyn SpoolQueuePort>,
    pub clipboard_change_origin: Arc<dyn ClipboardChangeOriginPort>,
    pub worker_tx: mpsc::Sender<RepresentationId>,
    pub payload_resolver: Arc<dyn ClipboardPayloadResolverPort>,
}

/// Security-domain ports bundle.
/// 安全领域端口组。
pub struct SecurityPorts {
    pub encryption: Arc<dyn EncryptionPort>,
    pub encryption_session: Arc<dyn EncryptionSessionPort>,
    pub encryption_state: Arc<dyn uc_core::ports::security::encryption_state::EncryptionStatePort>,
    pub key_scope: Arc<dyn uc_core::ports::security::key_scope::KeyScopePort>,
    pub secure_storage: Arc<dyn SecureStoragePort>,
    pub key_material: Arc<dyn KeyMaterialPort>,
    /// 单一空间访问 port——initialize / unlock / try_resume_session /
    /// verify_keychain_access / derive_subkey 等业务动作的统一入口。
    /// Slice 3 起所有 usecase 都通过此 port 访问会话生命周期与密钥派生,
    /// 上面 5 个 port (encryption / key_material / encryption_session /
    /// encryption_state / key_scope) 仅作为本 adapter 的内部依赖,
    /// 计划在 Slice 3 末尾整组移除。
    pub space_access: Arc<dyn uc_core::ports::space::SpaceAccessPort>,
    /// Argon2 PIN hasher for pairing.
    pub pin_hasher: Arc<dyn uc_core::ports::security::PinHasherPort>,
    /// Short pairing-code derivation.
    pub short_code: Arc<dyn uc_core::ports::security::ShortCodeGeneratorPort>,
    /// Identity-fingerprint factory used by pairing.
    pub fingerprint: Arc<dyn uc_core::ports::security::IdentityFingerprintFactoryPort>,
}

/// Device-domain ports bundle (includes pairing).
/// 设备领域端口组（含配对）。
pub struct DevicePorts {
    pub device_identity: Arc<dyn DeviceIdentityPort>,
    /// Authoritative repository of admitted space members (phase 4b PR-4：
    /// `paired_device_repo` 已下线，成员身份与同步偏好的唯一持久层)。
    pub member_repo: Arc<dyn MemberRepositoryPort>,
}

/// Storage-domain ports bundle (blobs, thumbnails, file transfer tracking).
/// 存储领域端口组（Blob、缩略图、文件传输追踪）。
pub struct StoragePorts {
    pub blob_store: Arc<dyn BlobReaderPort>,
    pub blob_writer: Arc<dyn BlobWriterPort>,
    pub thumbnail_repo: Arc<dyn ThumbnailRepositoryPort>,
    pub thumbnail_generator: Arc<dyn ThumbnailGeneratorPort>,
    pub file_transfer_repo: Arc<dyn uc_core::ports::FileTransferRepositoryPort>,
}

/// Search-domain ports bundle.
///
/// Groups the three search infrastructure pieces that must travel together:
/// the index port (query + CRUD), the key derivation port (HMAC term tags),
/// and the pipeline port (tokenization + text extraction). Keeping them in
/// one bundle prevents uc-daemon code from constructing these pieces ad hoc.
pub struct SearchPorts {
    /// Encrypted search index: query, index_entry, remove_entry, rebuild.
    pub search_index: Arc<dyn SearchIndexPort>,
    /// HMAC search key derivation (profile-scoped, HKDF-SHA256).
    pub search_key_derivation: Arc<dyn SearchKeyDerivationPort>,
    /// Tokenization + text extraction pipeline used for building search documents.
    pub search_pipeline: Arc<dyn SearchPipelinePort>,
}

/// System-domain ports bundle (clock, hash, cache filesystem).
/// 系统领域端口组（时钟、哈希、缓存文件系统）。
pub struct SystemPorts {
    pub clock: Arc<dyn ClockPort>,
    pub hash: Arc<dyn ContentHashPort>,
    pub cache_fs: Arc<dyn uc_core::ports::cache_fs::CacheFsPort>,
}

/// Application dependency grouping (non-Builder, just parameter grouping)
/// 应用依赖分组（非 Builder，仅参数打包）
///
/// **NOT a Builder pattern** - this is just a struct to group parameters.
/// **不是 Builder 模式** - 这只是一个打包参数的结构体。
///
/// All dependencies are required - no defaults, no optional fields.
/// 所有依赖都是必需的 - 无默认值，无可选字段。
pub struct AppDeps {
    /// Clipboard-domain ports / 剪贴板领域端口
    pub clipboard: ClipboardPorts,
    /// Security-domain ports / 安全领域端口
    pub security: SecurityPorts,
    /// Device-domain ports (includes pairing) / 设备领域端口（含配对）
    pub device: DevicePorts,
    /// Network ports bundle (unchanged) / 网络端口组（不变）
    pub network_ports: Arc<NetworkPorts>,
    /// Network control (cross-cutting) / 网络控制（横切关注）
    pub network_control: Arc<dyn NetworkControlPort>,
    /// Setup status (setup-specific) / 设置状态（设置流程专用）
    pub setup_status: Arc<dyn SetupStatusPort>,
    /// Storage-domain ports / 存储领域端口
    pub storage: StoragePorts,
    /// Settings (cross-cutting) / 设置（横切关注）
    pub settings: Arc<dyn SettingsPort>,
    /// System-domain ports / 系统领域端口
    pub system: SystemPorts,
    /// Search-domain ports (index, key derivation, pipeline) / 搜索领域端口
    pub search: SearchPorts,
}
