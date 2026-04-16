//! Centralized mockall-generated mock types for all port traits.
//!
//! This module replaces hand-written mock/noop structs scattered across test files.
//! Each mock type is generated via `mockall::mock!` for the corresponding port trait.
//!
//! Usage in unit tests:
//! ```ignore
//! use crate::test_mocks::*;
//!
//! let mut repo = MockPairedDeviceRepository::new();
//! repo.expect_list_all()
//!     .returning(|| Ok(vec![]));
//! ```
//!
//! Naming convention:
//! - Trait: `FooPort` → mock block name: `Foo` → generated type: `MockFoo`

#![allow(dead_code)]

use async_trait::async_trait;
use mockall::mock;

// =========================================================================
// Device & Pairing
// =========================================================================

mock! {
    pub PairedDeviceRepository {}

    #[async_trait]
    impl uc_core::ports::PairedDeviceRepositoryPort for PairedDeviceRepository {
        async fn get_by_peer_id(
            &self,
            peer_id: &uc_core::PeerId,
        ) -> Result<Option<uc_core::network::PairedDevice>, uc_core::ports::PairedDeviceRepositoryError>;
        async fn list_all(&self) -> Result<Vec<uc_core::network::PairedDevice>, uc_core::ports::PairedDeviceRepositoryError>;
        async fn upsert(&self, device: uc_core::network::PairedDevice) -> Result<(), uc_core::ports::PairedDeviceRepositoryError>;
        async fn set_state(
            &self,
            peer_id: &uc_core::PeerId,
            state: uc_core::network::PairingState,
        ) -> Result<(), uc_core::ports::PairedDeviceRepositoryError>;
        async fn update_last_seen(
            &self,
            peer_id: &uc_core::PeerId,
            last_seen_at: chrono::DateTime<chrono::Utc>,
        ) -> Result<(), uc_core::ports::PairedDeviceRepositoryError>;
        async fn delete(&self, peer_id: &uc_core::PeerId) -> Result<(), uc_core::ports::PairedDeviceRepositoryError>;
        async fn update_sync_settings(
            &self,
            peer_id: &uc_core::PeerId,
            settings: Option<uc_core::settings::model::SyncSettings>,
        ) -> Result<(), uc_core::ports::PairedDeviceRepositoryError>;
    }
}

mock! {
    pub PeerDirectory {}

    #[async_trait]
    impl uc_core::ports::PeerDirectoryPort for PeerDirectory {
        async fn get_discovered_peers(&self) -> anyhow::Result<Vec<uc_core::network::DiscoveredPeer>>;
        async fn get_connected_peers(&self) -> anyhow::Result<Vec<uc_core::network::ConnectedPeer>>;
        fn local_peer_id(&self) -> String;
        async fn announce_device_name(&self, device_name: String) -> anyhow::Result<()>;
    }
}

mock! {
    pub Discovery {}

    #[async_trait]
    impl uc_core::ports::DiscoveryPort for Discovery {
        async fn list_discovered_peers(&self) -> anyhow::Result<Vec<uc_core::network::DiscoveredPeer>>;
    }
}

mock! {
    pub PairingTransport {}

    #[async_trait]
    impl uc_core::ports::PairingTransportPort for PairingTransport {
        async fn open_pairing_session(&self, peer_id: String, session_id: String) -> anyhow::Result<()>;
        async fn send_pairing_on_session(&self, message: uc_core::network::PairingMessage) -> anyhow::Result<()>;
        async fn close_pairing_session(&self, session_id: String, reason: Option<String>) -> anyhow::Result<()>;
        async fn unpair_device(&self, peer_id: String) -> anyhow::Result<()>;
    }
}

mock! {
    pub NetworkControl {}

    #[async_trait]
    impl uc_core::ports::NetworkControlPort for NetworkControl {
        async fn start_network(&self) -> anyhow::Result<()>;
    }
}

mock! {
    pub DeviceIdentity {}

    impl uc_core::ports::DeviceIdentityPort for DeviceIdentity {
        fn current_device_id(&self) -> uc_core::DeviceId;
    }
}

// =========================================================================
// Security & Encryption
// =========================================================================

mock! {
    pub Encryption {}

    #[async_trait]
    impl uc_core::ports::EncryptionPort for Encryption {
        async fn derive_kek(
            &self,
            passphrase: &uc_core::security::model::Passphrase,
            salt: &[u8],
            kdf: &uc_core::security::model::KdfParams,
        ) -> Result<uc_core::security::model::Kek, uc_core::security::model::EncryptionError>;
        async fn wrap_master_key(
            &self,
            kek: &uc_core::security::model::Kek,
            master_key: &uc_core::security::model::MasterKey,
            aead: uc_core::security::model::EncryptionAlgo,
        ) -> Result<uc_core::security::model::EncryptedBlob, uc_core::security::model::EncryptionError>;
        async fn unwrap_master_key(
            &self,
            kek: &uc_core::security::model::Kek,
            wrapped: &uc_core::security::model::EncryptedBlob,
        ) -> Result<uc_core::security::model::MasterKey, uc_core::security::model::EncryptionError>;
        async fn encrypt_blob(
            &self,
            master_key: &uc_core::security::model::MasterKey,
            plaintext: &[u8],
            aad: &[u8],
            aead: uc_core::security::model::EncryptionAlgo,
        ) -> Result<uc_core::security::model::EncryptedBlob, uc_core::security::model::EncryptionError>;
        async fn decrypt_blob(
            &self,
            master_key: &uc_core::security::model::MasterKey,
            encrypted: &uc_core::security::model::EncryptedBlob,
            aad: &[u8],
        ) -> Result<Vec<u8>, uc_core::security::model::EncryptionError>;
    }
}

mock! {
    pub EncryptionSession {}

    #[async_trait]
    impl uc_core::ports::EncryptionSessionPort for EncryptionSession {
        async fn is_ready(&self) -> bool;
        async fn get_master_key(&self) -> Result<uc_core::security::model::MasterKey, uc_core::security::model::EncryptionError>;
        async fn set_master_key(&self, master_key: uc_core::security::model::MasterKey) -> Result<(), uc_core::security::model::EncryptionError>;
        async fn clear(&self) -> Result<(), uc_core::security::model::EncryptionError>;
    }
}

mock! {
    pub EncryptionState {}

    #[async_trait]
    impl uc_core::ports::security::encryption_state::EncryptionStatePort for EncryptionState {
        async fn load_state(&self) -> Result<uc_core::security::state::EncryptionState, uc_core::security::state::EncryptionStateError>;
        async fn persist_initialized(&self) -> Result<(), uc_core::security::state::EncryptionStateError>;
        async fn clear_initialized(&self) -> Result<(), uc_core::security::state::EncryptionStateError>;
    }
}

mock! {
    pub KeyScope {}

    #[async_trait]
    impl uc_core::ports::security::key_scope::KeyScopePort for KeyScope {
        async fn current_scope(&self) -> Result<uc_core::security::model::KeyScope, uc_core::ports::security::key_scope::ScopeError>;
    }
}

mock! {
    pub KeyMaterial {}

    #[async_trait]
    impl uc_core::ports::KeyMaterialPort for KeyMaterial {
        async fn load_kek(&self, scope: &uc_core::security::model::KeyScope) -> Result<uc_core::security::model::Kek, uc_core::security::model::EncryptionError>;
        async fn store_kek(&self, scope: &uc_core::security::model::KeyScope, kek: &uc_core::security::model::Kek) -> Result<(), uc_core::security::model::EncryptionError>;
        async fn delete_kek(&self, scope: &uc_core::security::model::KeyScope) -> Result<(), uc_core::security::model::EncryptionError>;
        async fn load_keyslot(&self, scope: &uc_core::security::model::KeyScope) -> Result<uc_core::security::model::KeySlot, uc_core::security::model::EncryptionError>;
        async fn store_keyslot(&self, keyslot: &uc_core::security::model::KeySlot) -> Result<(), uc_core::security::model::EncryptionError>;
        async fn delete_keyslot(&self, scope: &uc_core::security::model::KeyScope) -> Result<(), uc_core::security::model::EncryptionError>;
    }
}

mock! {
    pub SecureStorage {}

    impl uc_core::ports::SecureStoragePort for SecureStorage {
        fn get(&self, key: &str) -> Result<Option<Vec<u8>>, uc_core::ports::SecureStorageError>;
        fn set(&self, key: &str, value: &[u8]) -> Result<(), uc_core::ports::SecureStorageError>;
        fn delete(&self, key: &str) -> Result<(), uc_core::ports::SecureStorageError>;
    }
}

// =========================================================================
// Clipboard
// =========================================================================

mock! {
    pub ClipboardEntryRepository {}

    #[async_trait]
    impl uc_core::ports::ClipboardEntryRepositoryPort for ClipboardEntryRepository {
        async fn save_entry_and_selection(
            &self,
            entry: &uc_core::clipboard::ClipboardEntry,
            selection: &uc_core::ClipboardSelectionDecision,
        ) -> anyhow::Result<()>;
        async fn get_entry(&self, entry_id: &uc_core::ids::EntryId) -> anyhow::Result<Option<uc_core::clipboard::ClipboardEntry>>;
        async fn list_entries(&self, limit: usize, offset: usize) -> anyhow::Result<Vec<uc_core::clipboard::ClipboardEntry>>;
        async fn touch_entry(&self, entry_id: &uc_core::ids::EntryId, active_time_ms: i64) -> anyhow::Result<bool>;
        async fn delete_entry(&self, entry_id: &uc_core::ids::EntryId) -> anyhow::Result<()>;
    }
}

mock! {
    pub ClipboardSelectionRepository {}

    #[async_trait]
    impl uc_core::ports::ClipboardSelectionRepositoryPort for ClipboardSelectionRepository {
        async fn get_selection(&self, entry_id: &uc_core::ids::EntryId) -> anyhow::Result<Option<uc_core::clipboard::ClipboardSelectionDecision>>;
        async fn delete_selection(&self, entry_id: &uc_core::ids::EntryId) -> anyhow::Result<()>;
    }
}

mock! {
    pub ClipboardRepresentationRepository {}

    #[async_trait]
    impl uc_core::ports::ClipboardRepresentationRepositoryPort for ClipboardRepresentationRepository {
        async fn get_representation(
            &self,
            event_id: &uc_core::ids::EventId,
            representation_id: &uc_core::ids::RepresentationId,
        ) -> anyhow::Result<Option<uc_core::clipboard::PersistedClipboardRepresentation>>;
        async fn get_representation_by_id(
            &self,
            representation_id: &uc_core::ids::RepresentationId,
        ) -> anyhow::Result<Option<uc_core::clipboard::PersistedClipboardRepresentation>>;
        async fn get_representation_by_blob_id(
            &self,
            blob_id: &uc_core::BlobId,
        ) -> anyhow::Result<Option<uc_core::clipboard::PersistedClipboardRepresentation>>;
        async fn update_blob_id(
            &self,
            representation_id: &uc_core::ids::RepresentationId,
            blob_id: &uc_core::BlobId,
        ) -> anyhow::Result<()>;
        async fn update_blob_id_if_none(
            &self,
            representation_id: &uc_core::ids::RepresentationId,
            blob_id: &uc_core::BlobId,
        ) -> anyhow::Result<bool>;
        #[mockall::concretize]
        async fn update_processing_result(
            &self,
            rep_id: &uc_core::ids::RepresentationId,
            expected_states: &[uc_core::clipboard::PayloadAvailability],
            blob_id: Option<&uc_core::BlobId>,
            new_state: uc_core::clipboard::PayloadAvailability,
            last_error: Option<&str>,
        ) -> anyhow::Result<uc_core::ports::ProcessingUpdateOutcome>;
        async fn get_representations_for_event(
            &self,
            event_id: &uc_core::ids::EventId,
        ) -> anyhow::Result<Vec<uc_core::clipboard::PersistedClipboardRepresentation>>;
        async fn update_mime_type(
            &self,
            rep_id: &uc_core::ids::RepresentationId,
            mime: &uc_core::clipboard::MimeType,
        ) -> anyhow::Result<()>;
    }
}

mock! {
    pub ClipboardEventWriter {}

    #[async_trait]
    impl uc_core::ports::ClipboardEventWriterPort for ClipboardEventWriter {
        async fn insert_event(
            &self,
            event: &uc_core::clipboard::ClipboardEvent,
            representations: &Vec<uc_core::clipboard::PersistedClipboardRepresentation>,
        ) -> anyhow::Result<()>;
        async fn delete_event_and_representations(&self, event_id: &uc_core::ids::EventId) -> anyhow::Result<()>;
    }
}

mock! {
    pub SelectRepresentationPolicy {}

    impl uc_core::ports::SelectRepresentationPolicyPort for SelectRepresentationPolicy {
        fn select(&self, snapshot: &uc_core::clipboard::SystemClipboardSnapshot) -> Result<uc_core::clipboard::ClipboardSelection, uc_core::clipboard::PolicyError>;
    }
}

mock! {
    pub ClipboardRepresentationNormalizer {}

    #[async_trait]
    impl uc_core::ports::ClipboardRepresentationNormalizerPort for ClipboardRepresentationNormalizer {
        async fn normalize(
            &self,
            observed: &uc_core::clipboard::ObservedClipboardRepresentation,
        ) -> anyhow::Result<uc_core::clipboard::PersistedClipboardRepresentation>;
    }
}

mock! {
    pub RepresentationCache {}

    #[async_trait]
    impl uc_core::ports::RepresentationCachePort for RepresentationCache {
        async fn put(&self, rep_id: &uc_core::ids::RepresentationId, bytes: Vec<u8>);
        async fn get(&self, rep_id: &uc_core::ids::RepresentationId) -> Option<Vec<u8>>;
        async fn mark_completed(&self, rep_id: &uc_core::ids::RepresentationId);
        async fn mark_spooling(&self, rep_id: &uc_core::ids::RepresentationId);
        async fn remove(&self, rep_id: &uc_core::ids::RepresentationId);
    }
}

mock! {
    pub SpoolQueue {}

    #[async_trait]
    impl uc_core::ports::SpoolQueuePort for SpoolQueue {
        async fn enqueue(&self, request: uc_core::ports::SpoolRequest) -> anyhow::Result<()>;
    }
}

mock! {
    pub SystemClipboard {}

    impl uc_core::ports::SystemClipboardPort for SystemClipboard {
        fn read_snapshot(&self) -> anyhow::Result<uc_core::clipboard::SystemClipboardSnapshot>;
        fn write_snapshot(&self, snapshot: uc_core::clipboard::SystemClipboardSnapshot) -> anyhow::Result<()>;
    }
}

mock! {
    pub ClipboardChangeOrigin {}

    #[async_trait]
    impl uc_core::ports::ClipboardChangeOriginPort for ClipboardChangeOrigin {
        async fn set_next_origin(&self, origin: uc_core::ClipboardChangeOrigin, ttl: std::time::Duration);
        async fn consume_origin_or_default(&self, default_origin: uc_core::ClipboardChangeOrigin) -> uc_core::ClipboardChangeOrigin;
        async fn has_pending_origin(&self) -> bool;
        async fn remember_remote_snapshot_hash(&self, snapshot_hash: String, ttl: std::time::Duration);
        async fn remember_local_snapshot_hash(&self, snapshot_hash: String, ttl: std::time::Duration);
        async fn consume_origin_for_snapshot_or_default(&self, snapshot_hash: &str, default_origin: uc_core::ClipboardChangeOrigin) -> uc_core::ClipboardChangeOrigin;
    }
}

mock! {
    pub ClipboardPayloadResolver {}

    #[async_trait]
    impl uc_core::ports::ClipboardPayloadResolverPort for ClipboardPayloadResolver {
        async fn resolve(
            &self,
            representation: &uc_core::clipboard::PersistedClipboardRepresentation,
        ) -> anyhow::Result<uc_core::ports::ResolvedClipboardPayload>;
    }
}

mock! {
    pub SelectionResolver {}

    #[async_trait]
    impl uc_core::ports::SelectionResolverPort for SelectionResolver {
        async fn resolve_selection(
            &self,
            entry_id: &uc_core::ids::EntryId,
        ) -> anyhow::Result<(uc_core::clipboard::ClipboardEntry, uc_core::clipboard::PersistedClipboardRepresentation)>;
    }
}

mock! {
    pub ClipboardOutboundTransport {}

    #[async_trait]
    impl uc_core::ports::ClipboardOutboundTransportPort for ClipboardOutboundTransport {
        async fn send_clipboard(
            &self,
            target: &uc_core::ports::SyncTargetId,
            frame: uc_core::ports::OutboundClipboardFrame,
        ) -> Result<(), uc_core::ports::ClipboardTransportError>;
    }
}

mock! {
    pub ThumbnailRepository {}

    #[async_trait]
    impl uc_core::ports::ThumbnailRepositoryPort for ThumbnailRepository {
        async fn get_by_representation_id(
            &self,
            representation_id: &uc_core::ids::RepresentationId,
        ) -> anyhow::Result<Option<uc_core::clipboard::ThumbnailMetadata>>;
        async fn insert_thumbnail(&self, metadata: &uc_core::clipboard::ThumbnailMetadata) -> anyhow::Result<()>;
    }
}

mock! {
    pub ThumbnailGenerator {}

    #[async_trait]
    impl uc_core::ports::ThumbnailGeneratorPort for ThumbnailGenerator {
        async fn generate_thumbnail(
            &self,
            image_bytes: &[u8],
        ) -> anyhow::Result<uc_core::ports::GeneratedThumbnail>;
        async fn generate_thumbnail_from_rgba(
            &self,
            rgba_bytes: &[u8],
            width: u32,
            height: u32,
        ) -> anyhow::Result<uc_core::ports::GeneratedThumbnail>;
    }
}

// =========================================================================
// Settings & Configuration
// =========================================================================

mock! {
    pub Settings {}

    #[async_trait]
    impl uc_core::ports::SettingsPort for Settings {
        async fn load(&self) -> anyhow::Result<uc_core::settings::model::Settings>;
        async fn save(&self, settings: &uc_core::settings::model::Settings) -> anyhow::Result<()>;
    }
}

// =========================================================================
// Network Events
// =========================================================================

mock! {
    pub NetworkEvent {}

    #[async_trait]
    impl uc_core::ports::NetworkEventPort for NetworkEvent {
        async fn subscribe_events(&self) -> anyhow::Result<tokio::sync::mpsc::Receiver<uc_core::network::NetworkEvent>>;
    }
}

// =========================================================================
// Setup
// =========================================================================

mock! {
    pub SetupStatus {}

    #[async_trait]
    impl uc_core::ports::SetupStatusPort for SetupStatus {
        async fn get_status(&self) -> anyhow::Result<uc_core::setup::SetupStatus>;
        async fn set_status(&self, status: &uc_core::setup::SetupStatus) -> anyhow::Result<()>;
    }
}

mock! {
    pub SetupEvent {}

    #[async_trait]
    impl uc_core::ports::SetupEventPort for SetupEvent {
        async fn emit_setup_state_changed(&self, state: uc_core::setup::SetupState, session_id: Option<String>);
    }
}

// =========================================================================
// Space Access
// =========================================================================

mock! {
    pub SpaceAccessCrypto {}

    #[async_trait]
    impl uc_core::ports::space::CryptoPort for SpaceAccessCrypto {
        async fn generate_nonce32(&self) -> [u8; 32];
        async fn export_keyslot_blob(&self, space_id: &uc_core::ids::SpaceId) -> anyhow::Result<uc_core::security::model::KeySlot>;
        async fn derive_master_key_from_keyslot(
            &self,
            keyslot_blob: &[u8],
            passphrase: uc_core::security::SecretString,
        ) -> anyhow::Result<uc_core::security::model::MasterKey>;
    }
}

mock! {
    pub SpaceAccessProof {}

    #[async_trait]
    impl uc_core::ports::space::ProofPort for SpaceAccessProof {
        async fn build_proof(
            &self,
            pairing_session_id: &uc_core::ids::SessionId,
            space_id: &uc_core::ids::SpaceId,
            challenge_nonce: [u8; 32],
            master_key: &uc_core::security::MasterKey,
        ) -> anyhow::Result<uc_core::security::space_access::SpaceAccessProofArtifact>;
        async fn verify_proof(
            &self,
            proof: &uc_core::security::space_access::SpaceAccessProofArtifact,
            expected_nonce: [u8; 32],
        ) -> anyhow::Result<bool>;
    }
}

mock! {
    pub SpaceAccessTransport {}

    #[async_trait]
    impl uc_core::ports::space::SpaceAccessTransportPort for SpaceAccessTransport {
        async fn send_offer(&mut self, session_id: &uc_core::network::SessionId) -> anyhow::Result<()>;
        async fn send_proof(&mut self, session_id: &uc_core::network::SessionId) -> anyhow::Result<()>;
        async fn send_result(&mut self, session_id: &uc_core::network::SessionId) -> anyhow::Result<()>;
    }
}

mock! {
    pub Timer {}

    #[async_trait]
    impl uc_core::ports::TimerPort for Timer {
        async fn start(&mut self, session_id: &uc_core::ids::SessionId, ttl_secs: u64) -> anyhow::Result<()>;
        async fn stop(&mut self, session_id: &uc_core::ids::SessionId) -> anyhow::Result<()>;
    }
}

mock! {
    pub SpaceAccessPersistence {}

    #[async_trait]
    impl uc_core::ports::space::PersistencePort for SpaceAccessPersistence {
        async fn persist_joiner_access(&mut self, space_id: &uc_core::ids::SpaceId, peer_id: &str) -> anyhow::Result<()>;
        async fn persist_sponsor_access(&mut self, space_id: &uc_core::ids::SpaceId, peer_id: &str) -> anyhow::Result<()>;
    }
}

// =========================================================================
// File Transfer
// =========================================================================

mock! {
    pub FileTransferRepository {}

    #[async_trait]
    impl uc_core::ports::FileTransferRepositoryPort for FileTransferRepository {
        async fn insert_pending_transfers(&self, transfers: &[uc_core::ports::PendingInboundTransfer]) -> anyhow::Result<()>;
        async fn backfill_announce_metadata(&self, transfer_id: &str, file_size: i64, content_hash: &str) -> anyhow::Result<()>;
        async fn mark_transferring(&self, transfer_id: &str, now_ms: i64) -> anyhow::Result<bool>;
        async fn refresh_activity(&self, transfer_id: &str, now_ms: i64) -> anyhow::Result<()>;
        #[mockall::concretize]
        async fn mark_completed(&self, transfer_id: &str, content_hash: Option<&str>, now_ms: i64) -> anyhow::Result<bool>;
        async fn mark_failed(&self, transfer_id: &str, reason: &str, now_ms: i64) -> anyhow::Result<()>;
        async fn list_expired_inflight(&self, pending_cutoff_ms: i64, transferring_cutoff_ms: i64) -> anyhow::Result<Vec<uc_core::ports::ExpiredInflightTransfer>>;
        async fn bulk_fail_inflight(&self, reason: &str, now_ms: i64) -> anyhow::Result<Vec<uc_core::ports::ExpiredInflightTransfer>>;
        async fn get_entry_transfer_summary(&self, entry_id: &str) -> anyhow::Result<Option<uc_core::ports::EntryTransferSummary>>;
        async fn list_transfers_for_entry(&self, entry_id: &str) -> anyhow::Result<Vec<uc_core::ports::TrackedFileTransfer>>;
        async fn get_entry_id_for_transfer(&self, transfer_id: &str) -> anyhow::Result<Option<String>>;
    }
}

mock! {
    pub FileTransport {}

    #[async_trait]
    impl uc_core::ports::FileTransportPort for FileTransport {
        async fn send_file_announce(&self, peer_id: &str, announce: uc_core::network::protocol::FileTransferMessage) -> anyhow::Result<()>;
        async fn send_file_data(&self, peer_id: &str, data: uc_core::network::protocol::FileTransferMessage) -> anyhow::Result<()>;
        async fn send_file_complete(&self, peer_id: &str, complete: uc_core::network::protocol::FileTransferMessage) -> anyhow::Result<()>;
        async fn cancel_transfer(&self, peer_id: &str, cancel: uc_core::network::protocol::FileTransferMessage) -> anyhow::Result<()>;
        async fn send_file(
            &self,
            peer_id: &str,
            file_path: std::path::PathBuf,
            transfer_id: String,
            batch_id: Option<String>,
            batch_total: Option<u32>,
        ) -> anyhow::Result<()>;
    }
}

mock! {
    pub HostEventEmitter {}

    impl uc_core::ports::HostEventEmitterPort for HostEventEmitter {
        fn emit(&self, event: uc_core::ports::HostEvent) -> Result<(), uc_core::ports::EmitError>;
    }
}

// =========================================================================
// Storage
// =========================================================================

mock! {
    pub BlobStore {}

    #[async_trait]
    impl uc_core::ports::BlobStorePort for BlobStore {
        async fn put(&self, blob_id: &uc_core::BlobId, data: &[u8]) -> anyhow::Result<(std::path::PathBuf, Option<i64>)>;
        async fn get(&self, blob_id: &uc_core::BlobId) -> anyhow::Result<Vec<u8>>;
    }
}

mock! {
    pub CacheFs {}

    #[async_trait]
    impl uc_core::ports::cache_fs::CacheFsPort for CacheFs {
        async fn exists(&self, path: &std::path::Path) -> bool;
        async fn read_dir(&self, path: &std::path::Path) -> anyhow::Result<Vec<uc_core::ports::CacheFsDirEntry>>;
        async fn remove_dir_all(&self, path: &std::path::Path) -> anyhow::Result<()>;
        async fn remove_file(&self, path: &std::path::Path) -> anyhow::Result<()>;
        async fn dir_size(&self, path: &std::path::Path) -> anyhow::Result<u64>;
    }
}

mock! {
    pub FileManager {}

    impl uc_core::ports::FileManagerPort for FileManager {
        fn open_directory(&self, path: &std::path::Path) -> Result<(), uc_core::ports::FileManagerError>;
    }
}

// =========================================================================
// Search
// =========================================================================

mock! {
    pub SearchIndex {}

    #[async_trait]
    impl uc_core::ports::SearchIndexPort for SearchIndex {
        async fn index_entry(
            &self,
            document: uc_core::search::SearchDocument,
            postings: Vec<uc_core::search::SearchPosting>,
        ) -> Result<(), uc_core::search::SearchError>;
        async fn remove_entry(&self, entry_id: &uc_core::ids::EntryId) -> Result<(), uc_core::search::SearchError>;
        async fn search(&self, query: uc_core::search::SearchQuery) -> Result<uc_core::search::SearchResultsPage, uc_core::search::SearchError>;
        async fn rebuild(
            &self,
            entries: Vec<(uc_core::search::SearchDocument, Vec<uc_core::search::SearchPosting>)>,
            progress_tx: tokio::sync::mpsc::Sender<uc_core::search::RebuildProgress>,
        ) -> Result<(), uc_core::search::SearchError>;
        async fn get_index_meta(&self) -> Result<uc_core::search::SearchIndexMeta, uc_core::search::SearchError>;
    }
}

// =========================================================================
// Clock
// =========================================================================

mock! {
    pub Clock {}

    impl uc_core::ports::ClockPort for Clock {
        fn now_ms(&self) -> i64;
    }
}

// =========================================================================
// App Lifecycle (defined in uc-app)
// =========================================================================

mock! {
    pub LifecycleStatus {}

    #[async_trait]
    impl crate::usecases::LifecycleStatusPort for LifecycleStatus {
        async fn set_state(&self, state: crate::usecases::LifecycleState) -> anyhow::Result<()>;
        async fn get_state(&self) -> crate::usecases::LifecycleState;
    }
}

mock! {
    pub LifecycleEventEmitterMock {}

    #[async_trait]
    impl crate::usecases::LifecycleEventEmitter for LifecycleEventEmitterMock {
        async fn emit_lifecycle_event(&self, event: crate::usecases::LifecycleEvent) -> anyhow::Result<()>;
    }
}

mock! {
    pub SessionReady {}

    #[async_trait]
    impl crate::usecases::SessionReadyEmitter for SessionReady {
        async fn emit_ready(&self) -> anyhow::Result<()>;
    }
}
