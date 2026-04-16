use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::anyhow;
use async_trait::async_trait;
use chrono::Utc;
use mockall::mock;
use uc_app::usecases::clipboard::sync_outbound::SyncOutboundClipboardUseCase;
use uc_core::ids::{FormatId, RepresentationId};
use uc_core::network::{ConnectedPeer, DiscoveredPeer, PairedDevice, PairingState};
use uc_core::ports::{
    ClipboardOutboundTransportPort, ClipboardTransportError, DeviceIdentityPort,
    EncryptionSessionPort, OutboundClipboardFrame, PairedDeviceRepositoryError,
    PairedDeviceRepositoryPort, PeerDirectoryPort, SettingsPort, SyncTargetId, SystemClipboardPort,
    TransferCryptoError, TransferPayloadEncryptorPort,
};
use uc_core::security::model::MasterKey;
use uc_core::settings::model::Settings;
use uc_core::{
    DeviceId, MimeType, ObservedClipboardRepresentation, PeerId, SystemClipboardSnapshot,
};

mock! {
    SystemClipboard {}

    impl SystemClipboardPort for SystemClipboard {
        fn read_snapshot(&self) -> anyhow::Result<SystemClipboardSnapshot>;
        fn write_snapshot(&self, snapshot: SystemClipboardSnapshot) -> anyhow::Result<()>;
    }
}

mock! {
    ClipboardOutboundTransport {}

    #[async_trait]
    impl ClipboardOutboundTransportPort for ClipboardOutboundTransport {
        async fn send_clipboard(
            &self,
            target: &SyncTargetId,
            frame: OutboundClipboardFrame,
        ) -> Result<(), ClipboardTransportError>;
    }
}

mock! {
    PeerDirectory {}

    #[async_trait]
    impl PeerDirectoryPort for PeerDirectory {
        async fn get_discovered_peers(&self) -> anyhow::Result<Vec<DiscoveredPeer>>;
        async fn get_connected_peers(&self) -> anyhow::Result<Vec<ConnectedPeer>>;
        fn local_peer_id(&self) -> String;
        async fn announce_device_name(&self, device_name: String) -> anyhow::Result<()>;
    }
}

mock! {
    EncryptionSession {}

    #[async_trait]
    impl EncryptionSessionPort for EncryptionSession {
        async fn is_ready(&self) -> bool;
        async fn get_master_key(&self) -> Result<MasterKey, uc_core::security::model::EncryptionError>;
        async fn set_master_key(&self, master_key: MasterKey) -> Result<(), uc_core::security::model::EncryptionError>;
        async fn clear(&self) -> Result<(), uc_core::security::model::EncryptionError>;
    }
}

mock! {
    DeviceIdentity {}

    impl DeviceIdentityPort for DeviceIdentity {
        fn current_device_id(&self) -> DeviceId;
    }
}

mock! {
    AppSettings {}

    #[async_trait]
    impl SettingsPort for AppSettings {
        async fn load(&self) -> anyhow::Result<uc_core::settings::model::Settings>;
        async fn save(&self, settings: &uc_core::settings::model::Settings) -> anyhow::Result<()>;
    }
}

mock! {
    PairedDeviceRepository {}

    #[async_trait]
    impl PairedDeviceRepositoryPort for PairedDeviceRepository {
        async fn get_by_peer_id(
            &self,
            peer_id: &PeerId,
        ) -> Result<Option<PairedDevice>, PairedDeviceRepositoryError>;
        async fn list_all(&self) -> Result<Vec<PairedDevice>, PairedDeviceRepositoryError>;
        async fn upsert(&self, device: PairedDevice) -> Result<(), PairedDeviceRepositoryError>;
        async fn set_state(
            &self,
            peer_id: &PeerId,
            state: PairingState,
        ) -> Result<(), PairedDeviceRepositoryError>;
        async fn update_last_seen(
            &self,
            peer_id: &PeerId,
            last_seen_at: chrono::DateTime<chrono::Utc>,
        ) -> Result<(), PairedDeviceRepositoryError>;
        async fn delete(&self, peer_id: &PeerId) -> Result<(), PairedDeviceRepositoryError>;
        async fn update_sync_settings(
            &self,
            peer_id: &PeerId,
            settings: Option<uc_core::settings::model::SyncSettings>,
        ) -> Result<(), PairedDeviceRepositoryError>;
    }
}

mock! {
    TransferPayloadEncryptor {}

    impl TransferPayloadEncryptorPort for TransferPayloadEncryptor {
        fn encrypt(
            &self,
            master_key: &MasterKey,
            plaintext: &[u8],
        ) -> Result<Vec<u8>, TransferCryptoError>;
    }
}

fn passthrough_encryptor_mock() -> Arc<MockTransferPayloadEncryptor> {
    let mut encryptor = MockTransferPayloadEncryptor::new();
    encryptor
        .expect_encrypt()
        .times(0..)
        .returning(|_, plaintext| Ok(plaintext.to_vec()));
    Arc::new(encryptor)
}

fn noop_clipboard_mock() -> Arc<MockSystemClipboard> {
    let mut clipboard = MockSystemClipboard::new();
    clipboard
        .expect_read_snapshot()
        .times(0..)
        .returning(|| Ok(text_snapshot()));
    clipboard
        .expect_write_snapshot()
        .times(0..)
        .returning(|_| Ok(()));
    Arc::new(clipboard)
}

fn noop_transport_mock() -> Arc<MockClipboardOutboundTransport> {
    let mut transport = MockClipboardOutboundTransport::new();
    transport
        .expect_send_clipboard()
        .times(0..)
        .returning(|_, _| Ok(()));
    Arc::new(transport)
}

fn noop_peer_directory_mock() -> Arc<MockPeerDirectory> {
    let mut directory = MockPeerDirectory::new();
    directory
        .expect_get_discovered_peers()
        .times(0..)
        .returning(|| Ok(vec![]));
    directory
        .expect_get_connected_peers()
        .times(0..)
        .returning(|| Ok(vec![]));
    directory
        .expect_local_peer_id()
        .times(0..)
        .returning(|| "local-peer".to_string());
    directory
        .expect_announce_device_name()
        .times(0..)
        .returning(|_| Ok(()));
    Arc::new(directory)
}

fn ready_encryption_session_mock() -> Arc<MockEncryptionSession> {
    let mut session = MockEncryptionSession::new();
    session.expect_is_ready().times(0..).returning(|| true);
    session
        .expect_get_master_key()
        .times(0..)
        .returning(|| Ok(MasterKey([7; 32])));
    session
        .expect_set_master_key()
        .times(0..)
        .returning(|_| Ok(()));
    session.expect_clear().times(0..).returning(|| Ok(()));
    Arc::new(session)
}

fn static_device_identity_mock() -> Arc<MockDeviceIdentity> {
    let mut identity = MockDeviceIdentity::new();
    identity
        .expect_current_device_id()
        .times(0..)
        .returning(|| DeviceId::new("device-a"));
    Arc::new(identity)
}

fn configurable_settings_mock(
    initial: Settings,
    expected_load_calls: usize,
) -> (Arc<MockAppSettings>, Arc<Mutex<Settings>>) {
    let state = Arc::new(Mutex::new(initial));
    let mut settings = MockAppSettings::new();
    let state_for_load = Arc::clone(&state);
    settings
        .expect_load()
        .times(expected_load_calls)
        .returning(move || Ok(state_for_load.lock().expect("settings lock").clone()));
    (Arc::new(settings), state)
}

fn failing_settings_mock(expected_load_calls: usize) -> Arc<MockAppSettings> {
    let mut settings = MockAppSettings::new();
    settings
        .expect_load()
        .times(expected_load_calls)
        .returning(|| Err(anyhow!("forced settings load failure")));
    Arc::new(settings)
}

fn paired_repo_mock(
    devices: Vec<PairedDevice>,
    assert_no_mutation: bool,
) -> Arc<MockPairedDeviceRepository> {
    let devices_by_peer = Arc::new(
        devices
            .into_iter()
            .map(|device| (device.peer_id.as_str().to_string(), device))
            .collect::<HashMap<_, _>>(),
    );
    let mut repo = MockPairedDeviceRepository::new();
    let devices_for_get = Arc::clone(&devices_by_peer);
    repo.expect_get_by_peer_id()
        .times(0..)
        .returning(move |peer_id| Ok(devices_for_get.get(peer_id.as_str()).cloned()));
    let devices_for_list = Arc::clone(&devices_by_peer);
    repo.expect_list_all()
        .times(0..)
        .returning(move || Ok(devices_for_list.values().cloned().collect()));
    if assert_no_mutation {
        repo.expect_upsert().times(0);
        repo.expect_set_state().times(0);
        repo.expect_update_last_seen().times(0);
        repo.expect_delete().times(0);
        repo.expect_update_sync_settings().times(0);
    }
    Arc::new(repo)
}

fn build_use_case(
    settings: Arc<dyn SettingsPort>,
    paired_repo: Arc<dyn PairedDeviceRepositoryPort>,
) -> SyncOutboundClipboardUseCase {
    SyncOutboundClipboardUseCase::new(
        noop_clipboard_mock(),
        noop_transport_mock(),
        noop_peer_directory_mock(),
        ready_encryption_session_mock(),
        static_device_identity_mock(),
        settings,
        passthrough_encryptor_mock(),
        paired_repo,
    )
}

fn peer(peer_id: &str) -> DiscoveredPeer {
    DiscoveredPeer {
        peer_id: peer_id.to_string(),
        device_name: Some(format!("Device {peer_id}")),
        device_id: None,
        addresses: vec![],
        discovered_at: Utc::now(),
        last_seen: Utc::now(),
        is_paired: true,
    }
}

fn peers(ids: &[&str]) -> Vec<DiscoveredPeer> {
    ids.iter().map(|id| peer(id)).collect()
}

fn text_snapshot() -> SystemClipboardSnapshot {
    SystemClipboardSnapshot {
        ts_ms: 1_700_000_000_000,
        representations: vec![ObservedClipboardRepresentation::new(
            RepresentationId::new(),
            FormatId::from("text"),
            Some(MimeType("text/plain".to_string())),
            b"hello".to_vec(),
        )],
    }
}

fn with_global_auto_sync(enabled: bool) -> Settings {
    let mut settings = Settings::default();
    settings.sync.auto_sync = enabled;
    settings
}

fn paired_device(peer_id: &str, auto_sync: bool) -> PairedDevice {
    let mut sync_settings = Settings::default().sync;
    sync_settings.auto_sync = auto_sync;

    PairedDevice {
        peer_id: PeerId::from(peer_id),
        pairing_state: PairingState::Trusted,
        identity_fingerprint: format!("fp-{peer_id}"),
        paired_at: Utc::now(),
        last_seen_at: None,
        device_name: format!("Device {peer_id}"),
        sync_settings: Some(sync_settings),
    }
}

fn peer_ids(peers: &[DiscoveredPeer]) -> Vec<String> {
    peers.iter().map(|p| p.peer_id.clone()).collect()
}

#[tokio::test]
async fn sync_outbound_global_toggle() {
    let (settings, _settings_state) = configurable_settings_mock(with_global_auto_sync(false), 1);
    let repo = paired_repo_mock(vec![], true);
    let use_case = build_use_case(settings, repo);
    let input_peers = peers(&["peer-a", "peer-b", "peer-c"]);

    let result = use_case
        .apply_sync_policy(&input_peers, &text_snapshot())
        .await;

    assert!(
        result.is_empty(),
        "global auto_sync=false must block all peers"
    );
}

#[tokio::test]
async fn sync_outbound_global_override() {
    let (settings, _settings_state) = configurable_settings_mock(with_global_auto_sync(false), 1);
    let repo = paired_repo_mock(
        vec![paired_device("peer-a", true), paired_device("peer-b", true)],
        true,
    );
    let use_case = build_use_case(settings, repo);
    let input_peers = peers(&["peer-a", "peer-b"]);

    let result = use_case
        .apply_sync_policy(&input_peers, &text_snapshot())
        .await;

    assert!(
        result.is_empty(),
        "global auto_sync=false must override per-device auto_sync=true"
    );
}

#[tokio::test]
async fn sync_outbound_global_enabled() {
    let (settings, _settings_state) = configurable_settings_mock(with_global_auto_sync(true), 1);
    let repo = paired_repo_mock(
        vec![
            paired_device("peer-a", true),
            paired_device("peer-b", false),
        ],
        true,
    );
    let use_case = build_use_case(settings, repo);
    let input_peers = peers(&["peer-a", "peer-b", "peer-c"]);

    let result = use_case
        .apply_sync_policy(&input_peers, &text_snapshot())
        .await;

    assert_eq!(
        peer_ids(&result),
        vec!["peer-a".to_string(), "peer-c".to_string()]
    );
}

#[tokio::test]
async fn sync_outbound_settings_fallback() {
    let settings = failing_settings_mock(1);
    let repo = paired_repo_mock(
        vec![
            paired_device("peer-a", false),
            paired_device("peer-b", false),
        ],
        true,
    );
    let use_case = build_use_case(settings, repo);
    let input_peers = peers(&["peer-a", "peer-b", "peer-c"]);

    let result = use_case
        .apply_sync_policy(&input_peers, &text_snapshot())
        .await;

    assert_eq!(peer_ids(&result), vec!["peer-a", "peer-b", "peer-c"]);
}

#[tokio::test]
async fn sync_outbound_no_device_mutation() {
    let (settings, settings_state) = configurable_settings_mock(with_global_auto_sync(false), 2);
    let repo = paired_repo_mock(
        vec![
            paired_device("peer-a", true),
            paired_device("peer-b", false),
        ],
        true,
    );
    let use_case = build_use_case(settings.clone(), repo);
    let input_peers = peers(&["peer-a", "peer-b"]);

    let first = use_case
        .apply_sync_policy(&input_peers, &text_snapshot())
        .await;
    assert!(first.is_empty());

    settings_state.lock().expect("settings lock").sync.auto_sync = true;
    let _second = use_case
        .apply_sync_policy(&input_peers, &text_snapshot())
        .await;
}

#[tokio::test]
async fn sync_outbound_resume() {
    let (settings, settings_state) = configurable_settings_mock(with_global_auto_sync(false), 2);
    let repo = paired_repo_mock(
        vec![
            paired_device("peer-a", true),
            paired_device("peer-b", false),
            paired_device("peer-c", true),
        ],
        true,
    );
    let use_case = build_use_case(settings.clone(), repo);
    let input_peers = peers(&["peer-a", "peer-b", "peer-c"]);

    let blocked = use_case
        .apply_sync_policy(&input_peers, &text_snapshot())
        .await;
    assert!(
        blocked.is_empty(),
        "global auto_sync=false must block outbound sync"
    );

    settings_state.lock().expect("settings lock").sync.auto_sync = true;
    let resumed = use_case
        .apply_sync_policy(&input_peers, &text_snapshot())
        .await;
    assert_eq!(
        peer_ids(&resumed),
        vec!["peer-a".to_string(), "peer-c".to_string()]
    );
}
