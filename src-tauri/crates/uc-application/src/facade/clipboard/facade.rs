//! Slice 2 Phase 2 · T9 — `ClipboardSyncFacade` implementation.
//!
//! Thin wrapper over the two use cases. Job is:
//!
//! * Hold `Arc<DispatchClipboardEntryUseCase>` and
//!   `Arc<IngestInboundClipboardUseCase>`, so the facade controls lifetime.
//! * Translate between public (`pub`) and internal (`pub(crate)`) types so
//!   AGENTS.md §11.4 stays intact (external crates never touch the
//!   underlying use case structs).
//! * Expose `spawn_ingest_loop` for bootstrap to call right after F1
//!   `auto_start_network` succeeds — same lifecycle hook pattern as Phase
//!   1's `ensure_reachable_all` but for the clipboard receiver.

use std::sync::Arc;

use bytes::Bytes;
use tokio::sync::broadcast;
use tracing::instrument;

use uc_core::ids::DeviceId;
use uc_core::ports::security::TransferCipherPort;
use uc_core::ports::{
    ClipboardDispatchPort, ClipboardReceiverPort, ClockPort, DeviceIdentityPort, DispatchAck,
    LocalIdentityPort, PeerAddressRepositoryPort, PresencePort, SettingsPort,
};

use crate::usecases::clipboard_sync::{
    DispatchClipboardEntryInput, DispatchClipboardEntryUseCase, DispatchOutcome, DispatchPerTarget,
    DispatchSyncError, InboundAction as UcInboundAction, InboundClipboardNotice as UcInboundNotice,
    IngestInboundClipboardUseCase, IngestSpawnHandle,
};

/// Construction bundle, mirrors `MemberRosterDeps` pattern so bootstrap
/// wiring stays consistent across facades.
pub struct ClipboardSyncDeps {
    pub peer_addr_repo: Arc<dyn PeerAddressRepositoryPort>,
    pub presence: Arc<dyn PresencePort>,
    pub transfer_cipher: Arc<dyn TransferCipherPort>,
    pub clipboard_dispatch: Arc<dyn ClipboardDispatchPort>,
    pub clipboard_receiver: Arc<dyn ClipboardReceiverPort>,
    pub device_identity: Arc<dyn DeviceIdentityPort>,
    pub local_identity: Arc<dyn LocalIdentityPort>,
    pub settings: Arc<dyn SettingsPort>,
    pub clock: Arc<dyn ClockPort>,
}

/// Public-facing input to a dispatch pass. Mirrors the use case's own
/// struct but lives in the facade layer for stability.
#[derive(Debug, Clone)]
pub struct DispatchEntryInput {
    pub plaintext: Bytes,
    pub content_hash: String,
    pub payload_version: u8,
}

/// Public-facing per-target report.
#[derive(Debug, Clone)]
pub struct DispatchEntryPerTarget {
    pub device_id: DeviceId,
    pub outcome: Result<DispatchAck, String>,
}

/// Public-facing aggregate report. Counts + per-target detail, mirroring
/// the internal `DispatchOutcome`.
#[derive(Debug, Clone)]
pub struct DispatchEntryOutcome {
    pub content_hash: String,
    pub per_target: Vec<DispatchEntryPerTarget>,
    pub total_accepted: usize,
    pub total_duplicate: usize,
    pub total_offline: usize,
    pub total_errored: usize,
    pub at_ms: i64,
}

/// Public-facing error type. Collapses the internal variants onto the
/// subset meaningful to external callers.
#[derive(Debug, thiserror::Error)]
pub enum ClipboardSyncError {
    #[error("encryption session not unlocked")]
    LockedSpace,
    #[error("transfer cipher failure: {0}")]
    CipherFailure(String),
    #[error("peer address repository: {0}")]
    Repository(String),
    #[error("local identity lookup: {0}")]
    LocalIdentity(String),
}

impl From<DispatchSyncError> for ClipboardSyncError {
    fn from(err: DispatchSyncError) -> Self {
        match err {
            DispatchSyncError::LockedSpace => ClipboardSyncError::LockedSpace,
            DispatchSyncError::CipherFailure(msg) => ClipboardSyncError::CipherFailure(msg),
            DispatchSyncError::Repository(msg) => ClipboardSyncError::Repository(msg),
            DispatchSyncError::LocalIdentity(msg) => ClipboardSyncError::LocalIdentity(msg),
        }
    }
}

/// Public view of one inbound clipboard delivery.
#[derive(Debug, Clone)]
pub struct InboundNotice {
    pub from_device: DeviceId,
    pub content_hash: String,
    pub plaintext: Bytes,
    pub action: InboundAction,
    pub at_ms: i64,
}

/// Public mirror of the internal `InboundAction` enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InboundAction {
    NewEntry,
    DuplicateIgnored,
}

/// Public re-export of the ingest spawn handle. Drop or call `abort()`
/// to stop the background loop.
pub struct IngestHandle {
    inner: IngestSpawnHandle,
}

impl IngestHandle {
    pub fn abort(&self) {
        self.inner.abort();
    }
}

/// Clipboard sync facade — the single public entry point for Slice 2
/// Phase 2.
pub struct ClipboardSyncFacade {
    dispatch_uc: Arc<DispatchClipboardEntryUseCase>,
    ingest_uc: Arc<IngestInboundClipboardUseCase>,
}

impl ClipboardSyncFacade {
    pub fn new(deps: ClipboardSyncDeps) -> Self {
        let dispatch_uc = Arc::new(DispatchClipboardEntryUseCase::new(
            Arc::clone(&deps.peer_addr_repo),
            Arc::clone(&deps.presence),
            Arc::clone(&deps.transfer_cipher),
            Arc::clone(&deps.clipboard_dispatch),
            Arc::clone(&deps.device_identity),
            Arc::clone(&deps.local_identity),
            Arc::clone(&deps.settings),
            Arc::clone(&deps.clock),
        ));
        let ingest_uc = Arc::new(IngestInboundClipboardUseCase::new(
            Arc::clone(&deps.clipboard_receiver),
            Arc::clone(&deps.transfer_cipher),
            Arc::clone(&deps.clock),
        ));
        Self {
            dispatch_uc,
            ingest_uc,
        }
    }

    /// Fan out one plaintext payload to every online paired peer.
    #[instrument(skip_all, fields(content_hash = %input.content_hash))]
    pub async fn dispatch_entry(
        &self,
        input: DispatchEntryInput,
    ) -> Result<DispatchEntryOutcome, ClipboardSyncError> {
        let internal = self
            .dispatch_uc
            .execute(DispatchClipboardEntryInput {
                plaintext: input.plaintext,
                content_hash: input.content_hash.clone(),
                payload_version: input.payload_version,
            })
            .await?;
        Ok(lift_outcome(internal))
    }

    /// Subscribe to the inbound-notice broadcast. CLI `watch` / future
    /// daemon subscribers attach here.
    pub fn subscribe_inbound_notices(&self) -> broadcast::Receiver<InboundNotice> {
        // Bridge from the internal broadcast to the public-type broadcast
        // via a relay task. This keeps public types independent of
        // `usecases::*` renames while still letting lagging subscribers
        // recover per broadcast semantics.
        let (public_tx, public_rx) = broadcast::channel(64);
        let mut internal_rx = self.ingest_uc.subscribe_notices();
        tokio::spawn(async move {
            loop {
                match internal_rx.recv().await {
                    Ok(internal) => {
                        let lifted = lift_notice(internal);
                        if public_tx.send(lifted).is_err() {
                            // No public subscribers — keep consuming so
                            // the internal broadcast doesn't lag us.
                            continue;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });
        public_rx
    }

    /// Spawn the ingest background loop. Caller owns the returned handle;
    /// dropping it (or `abort()`) terminates the loop. Typically called by
    /// bootstrap after F1 `auto_start_network` completes.
    pub fn spawn_ingest_loop(&self) -> IngestHandle {
        let inner = Arc::clone(&self.ingest_uc).spawn_run();
        IngestHandle { inner }
    }
}

// ---------------------------------------------------------------------------
// Private mappers — keep internal / public types pinned together.
// ---------------------------------------------------------------------------

fn lift_outcome(internal: DispatchOutcome) -> DispatchEntryOutcome {
    DispatchEntryOutcome {
        content_hash: internal.content_hash,
        per_target: internal
            .per_target
            .into_iter()
            .map(lift_per_target)
            .collect(),
        total_accepted: internal.total_accepted,
        total_duplicate: internal.total_duplicate,
        total_offline: internal.total_offline,
        total_errored: internal.total_errored,
        at_ms: internal.at_ms,
    }
}

fn lift_per_target(internal: DispatchPerTarget) -> DispatchEntryPerTarget {
    DispatchEntryPerTarget {
        device_id: internal.device_id,
        outcome: internal.outcome,
    }
}

fn lift_notice(internal: UcInboundNotice) -> InboundNotice {
    InboundNotice {
        from_device: internal.from_device,
        content_hash: internal.content_hash,
        plaintext: internal.plaintext,
        action: match internal.action {
            UcInboundAction::NewEntry => InboundAction::NewEntry,
            UcInboundAction::DuplicateIgnored => InboundAction::DuplicateIgnored,
        },
        at_ms: internal.at_ms,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashMap;
    use std::time::Duration;

    use async_trait::async_trait;
    use tokio::sync::Mutex;
    use uc_core::ports::security::TransferCipherError;
    use uc_core::ports::{
        ClipboardDispatchError, ClipboardHeader, DispatchAck, InboundClipboard, LocalIdentityError,
        PeerAddressError, PeerAddressRecord, PresenceError, PresenceEvent, ReachabilityState,
        SyncPayload,
    };
    use uc_core::security::IdentityFingerprint;
    use uc_core::settings::model::Settings;

    // ---- doubles --------------------------------------------------------

    #[derive(Default)]
    struct MemRepo {
        inner: Mutex<HashMap<String, PeerAddressRecord>>,
    }
    #[async_trait]
    impl PeerAddressRepositoryPort for MemRepo {
        async fn get(
            &self,
            device: &DeviceId,
        ) -> Result<Option<PeerAddressRecord>, PeerAddressError> {
            Ok(self.inner.lock().await.get(device.as_str()).cloned())
        }
        async fn upsert(&self, record: &PeerAddressRecord) -> Result<(), PeerAddressError> {
            self.inner
                .lock()
                .await
                .insert(record.device_id.as_str().to_string(), record.clone());
            Ok(())
        }
        async fn list(&self) -> Result<Vec<PeerAddressRecord>, PeerAddressError> {
            Ok(self.inner.lock().await.values().cloned().collect())
        }
        async fn remove(&self, device: &DeviceId) -> Result<(), PeerAddressError> {
            self.inner.lock().await.remove(device.as_str());
            Ok(())
        }
    }

    struct StaticPresence(HashMap<String, ReachabilityState>);
    #[async_trait]
    impl PresencePort for StaticPresence {
        async fn ensure_reachable(
            &self,
            _device: &DeviceId,
        ) -> Result<ReachabilityState, PresenceError> {
            Ok(ReachabilityState::Online)
        }
        async fn current_state(&self, device: &DeviceId) -> ReachabilityState {
            self.0
                .get(device.as_str())
                .copied()
                .unwrap_or(ReachabilityState::Unknown)
        }
        fn subscribe(&self) -> broadcast::Receiver<PresenceEvent> {
            let (tx, rx) = broadcast::channel(1);
            std::mem::forget(tx);
            rx
        }
    }

    struct EchoCipher;
    #[async_trait]
    impl TransferCipherPort for EchoCipher {
        async fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, TransferCipherError> {
            Ok(plaintext.to_vec())
        }
        async fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, TransferCipherError> {
            Ok(ciphertext.to_vec())
        }
    }

    #[derive(Default)]
    struct OkDispatch {
        called: Mutex<usize>,
    }
    #[async_trait]
    impl ClipboardDispatchPort for OkDispatch {
        async fn dispatch(
            &self,
            _target: &DeviceId,
            _header: &ClipboardHeader,
            _payload: SyncPayload,
        ) -> Result<DispatchAck, ClipboardDispatchError> {
            *self.called.lock().await += 1;
            Ok(DispatchAck::Accepted)
        }
    }

    struct FakeReceiver {
        tx: broadcast::Sender<InboundClipboard>,
    }
    impl FakeReceiver {
        fn new() -> Self {
            let (tx, _) = broadcast::channel(16);
            Self { tx }
        }
        fn publish(&self, inbound: InboundClipboard) {
            let _ = self.tx.send(inbound);
        }
    }
    #[async_trait]
    impl ClipboardReceiverPort for FakeReceiver {
        fn subscribe(&self) -> broadcast::Receiver<InboundClipboard> {
            self.tx.subscribe()
        }
    }

    struct FixedDevice(DeviceId);
    impl DeviceIdentityPort for FixedDevice {
        fn current_device_id(&self) -> DeviceId {
            self.0.clone()
        }
    }
    struct StubLocalIdentity;
    #[async_trait]
    impl LocalIdentityPort for StubLocalIdentity {
        async fn create(&self) -> Result<IdentityFingerprint, LocalIdentityError> {
            Ok(IdentityFingerprint::from_raw_string("AAAABBBBCCCCDDDD").unwrap())
        }
        async fn ensure(&self) -> Result<IdentityFingerprint, LocalIdentityError> {
            self.create().await
        }
        async fn get_current_fingerprint(
            &self,
        ) -> Result<Option<IdentityFingerprint>, LocalIdentityError> {
            Ok(Some(
                IdentityFingerprint::from_raw_string("AAAABBBBCCCCDDDD").unwrap(),
            ))
        }
    }
    struct StubSettings;
    #[async_trait]
    impl SettingsPort for StubSettings {
        async fn load(&self) -> anyhow::Result<Settings> {
            Ok(Settings::default())
        }
        async fn save(&self, _: &Settings) -> anyhow::Result<()> {
            Ok(())
        }
    }
    struct FixedClock(i64);
    impl ClockPort for FixedClock {
        fn now_ms(&self) -> i64 {
            self.0
        }
    }

    fn make_deps(
        peer_addr_repo: Arc<dyn PeerAddressRepositoryPort>,
        presence: Arc<dyn PresencePort>,
        dispatch: Arc<dyn ClipboardDispatchPort>,
        receiver: Arc<dyn ClipboardReceiverPort>,
    ) -> ClipboardSyncDeps {
        ClipboardSyncDeps {
            peer_addr_repo,
            presence,
            transfer_cipher: Arc::new(EchoCipher),
            clipboard_dispatch: dispatch,
            clipboard_receiver: receiver,
            device_identity: Arc::new(FixedDevice(DeviceId::new("self"))),
            local_identity: Arc::new(StubLocalIdentity),
            settings: Arc::new(StubSettings),
            clock: Arc::new(FixedClock(1_700_000_000_000)),
        }
    }

    // ---- verdicts -------------------------------------------------------

    /// Verdict 1 — `dispatch_entry` delegates to the inner use case and
    /// returns a public outcome. Counts match the dispatch adapter's Ok
    /// ack.
    #[tokio::test]
    async fn dispatch_entry_returns_public_outcome_for_online_peer() {
        let repo = Arc::new(MemRepo::default());
        repo.upsert(&PeerAddressRecord {
            device_id: DeviceId::new("peer-a"),
            addr_blob: vec![0xAA; 8],
            observed_at: chrono::Utc::now(),
        })
        .await
        .unwrap();
        let presence = Arc::new(StaticPresence(
            [("peer-a".to_string(), ReachabilityState::Online)]
                .into_iter()
                .collect(),
        ));
        let dispatch = Arc::new(OkDispatch::default());
        let receiver = Arc::new(FakeReceiver::new());
        let facade =
            ClipboardSyncFacade::new(make_deps(repo, presence, dispatch.clone(), receiver));

        let outcome = facade
            .dispatch_entry(DispatchEntryInput {
                plaintext: Bytes::from_static(b"hello"),
                content_hash: "abc".to_string(),
                payload_version: 3,
            })
            .await
            .expect("dispatch ok");
        assert_eq!(outcome.total_accepted, 1);
        assert_eq!(outcome.per_target.len(), 1);
        assert_eq!(*dispatch.called.lock().await, 1);
    }

    /// Verdict 2 — `subscribe_inbound_notices` bridges the internal
    /// broadcast to a public one. Publishing an inbound on the underlying
    /// receiver yields a `InboundNotice` with the decrypted plaintext and
    /// public `InboundAction::NewEntry`.
    #[tokio::test]
    async fn subscribe_inbound_notices_bridges_internal_to_public_type() {
        let repo = Arc::new(MemRepo::default());
        let presence = Arc::new(StaticPresence(HashMap::new()));
        let dispatch = Arc::new(OkDispatch::default());
        let receiver = Arc::new(FakeReceiver::new());
        let facade = ClipboardSyncFacade::new(make_deps(
            repo,
            presence,
            dispatch,
            receiver.clone() as Arc<dyn ClipboardReceiverPort>,
        ));
        let mut notices = facade.subscribe_inbound_notices();
        let _ingest = facade.spawn_ingest_loop();

        tokio::time::sleep(Duration::from_millis(30)).await;

        receiver.publish(InboundClipboard {
            peer_device_id: DeviceId::new("peer-x"),
            header: ClipboardHeader {
                version: ClipboardHeader::CURRENT_VERSION,
                content_hash: "xx".repeat(32),
                captured_at_ms: 42,
                origin_device_id: "peer-x".to_string(),
                origin_device_name: "Peer X".to_string(),
                payload_version: 3,
            },
            ciphertext: Bytes::from_static(b"hello"),
        });

        let notice = tokio::time::timeout(Duration::from_secs(2), notices.recv())
            .await
            .expect("notice arrives")
            .expect("sender alive");
        assert_eq!(notice.from_device.as_str(), "peer-x");
        assert_eq!(notice.plaintext, Bytes::from_static(b"hello"));
        assert_eq!(notice.action, InboundAction::NewEntry);
    }

    /// Verdict 3 — `spawn_ingest_loop` returns a handle that aborts the
    /// background task on drop, so bootstrap shutdown is clean without an
    /// explicit `.abort()` call.
    #[tokio::test]
    async fn spawn_ingest_handle_drops_clean() {
        let repo = Arc::new(MemRepo::default());
        let presence = Arc::new(StaticPresence(HashMap::new()));
        let dispatch = Arc::new(OkDispatch::default());
        let receiver = Arc::new(FakeReceiver::new());
        let facade = ClipboardSyncFacade::new(make_deps(
            repo,
            presence,
            dispatch,
            receiver.clone() as Arc<dyn ClipboardReceiverPort>,
        ));
        {
            let _handle = facade.spawn_ingest_loop();
            // Let the loop subscribe…
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        // Handle dropped. Sleep briefly so any panic would surface.
        tokio::time::sleep(Duration::from_millis(20)).await;
        // If the task were still alive it would still be holding a subscribe
        // handle on the receiver; but we can't easily observe that from
        // outside. The real assertion is "no panic, no leaked task" — we
        // rely on `Drop` calling `abort` on the `JoinHandle`.
    }
}
