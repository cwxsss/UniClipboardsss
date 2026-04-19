//! Port interfaces for the application layer
//!
//! Ports define the contract between the application logic (use cases)
//! and infrastructure implementations. This follows Hexagonal Architecture
//! principles, allowing the core business logic to remain independent of
//! external dependencies.
//!
//! ## Port Placement Guidelines
//!
//! Before adding a new port to `uc-core/ports`, ask yourself three questions:
//!
//! 1. **Does this port represent a business capability?**
//! 2. **Will it be depended upon by multiple use cases or domains?**
//! 3. **Is it implemented by the infrastructure or platform layer?**
//!
//! If all three answers are **yes**, place it in `uc-core/ports`.
//! Otherwise, place it in the relevant `domain` submodule.

pub mod cache_fs;
pub mod clipboard;
mod clipboard_change_handler;
mod clipboard_event;
mod clock;
pub mod connection_policy;
pub mod device_identity;
mod discovery;
pub mod errors;
pub mod file_transfer_repository;
pub mod file_transport;
mod hash;
pub mod network_control;
pub mod network_events;
pub mod pairing_transport;
pub mod peer_directory;
pub mod search;
pub mod security;
pub mod settings;
pub mod setup;
pub mod space;
mod timer;

pub use cache_fs::{CacheFsPort, DirEntry as CacheFsDirEntry};
pub use clipboard_event::*;
pub use clock::*;
pub use connection_policy::{ConnectionPolicyResolverError, ConnectionPolicyResolverPort};
pub use discovery::DiscoveryPort;
pub use hash::*;
pub use timer::TimerPort;

pub use clipboard::*;
pub use clipboard_change_handler::ClipboardChangeHandler;
pub use device_identity::DeviceIdentityPort;
pub use errors::AppDirsError;
pub use file_transfer_repository::{
    compute_aggregate_status, EntryTransferSummary, ExpiredInflightTransfer,
    FileTransferRepositoryPort, NoopFileTransferRepositoryPort, PendingInboundTransfer,
    TrackedFileTransfer, TrackedFileTransferStatus,
};
pub use file_transport::{FileTransportPort, NoopFileTransportPort};
pub use network_control::NetworkControlPort;
pub use network_events::NetworkEventPort;
pub use pairing_transport::PairingTransportPort;
pub use peer_directory::PeerDirectoryPort;
pub use search::search_index::SearchIndexPort;
pub use search::search_key::SearchKeyDerivationPort;
pub use security::encryption::EncryptionPort;
pub use security::encryption_session::EncryptionSessionPort;
pub use security::key_material::KeyMaterialPort;
pub use security::secure_storage::{SecureStorageError, SecureStoragePort};
pub use security::transfer_cipher::{TransferCipherError, TransferCipherPort};
pub use security::transfer_crypto::{
    TransferCryptoError, TransferPayloadDecryptorPort, TransferPayloadEncryptorPort,
};
pub use settings::{SettingsMigrationPort, SettingsPort};
pub use setup::SetupStatusPort;
